//! Aim motor — bow drawn while the aim button is held.
//!
//! `propose` keeps `Aiming` alive while `wants_aim` holds (release = silence
//! → `Idle`); charging accumulates `DrawStrength` while attack is held during
//! `Aiming`, and releasing attack fires an arrow whose speed and damage scale
//! with the charge.
//!
//! Firing follows the two-phase third-person standard: a ray from the
//! camera-aligned shoulder pivot finds the crosshair's world target, then the
//! arrow leaves the bow socket converging on that point. Both the pivot and
//! the socket are pure simulation data (§20), no camera read; the camera and
//! the bow visual import the same constants so alignment holds by
//! construction.

use avian3d::prelude::*;
use bevy_ecs::prelude::*;
use bevy_math::prelude::*;
use bevy_time::prelude::*;
use bevy_transform::prelude::*;

use crate::combat::context::effective_bow;
use crate::combat::context_data::{BowProfile, CombatContext, MountedCombatProfile};
use crate::combat::intent::CombatIntents;
use crate::combat::proposal::{CombatProposalBuffer, Priority, TransitionProposal, weight};
use crate::combat::state::CombatState;
use crate::movement::Actor;
use crate::projectiles::SpawnProjectileMessage;
use bof_domain::input::frame::ControlOrientation;

pub use bof_domain::combat::messages::BowFiredMessage;

pub use bof_domain::combat::state::BOW_SOCKET_LOCAL;
/// Aim shoulder pivot the crosshair ray passes through, owned by Combat
/// (the aim line is simulation — §20). `camera.rs` imports both so the
/// aim-mode camera orbit matches this ray exactly: pivot height over the
/// body center, and the rightward offset.
///
/// Eye height of the rig (~1.7 m over the feet, body center is ~1.0 m up).
/// Keeping this close to `BOW_SOCKET_LOCAL` keeps the convergence between
/// the crosshair line and the arrow's actual path shallow — a large gap
/// makes arrows fly visibly diagonal and clip cover the camera sees over.
pub use bof_domain::combat::state::{AIM_PIVOT_HEIGHT, AIM_SHOULDER_OFFSET};
/// Forward nudge so the arrow doesn't spawn intersecting the body capsule.
const ARROW_MUZZLE_FORWARD: f32 = 0.3;
/// Max distance of the crosshair aim ray; with no hit inside this range the
/// arrow flies toward the point at max range on the aim line.
const AIM_RAY_MAX_RANGE: f32 = 120.0;
/// Aim targets projecting closer than this ahead of the socket fire straight
/// along the aim line instead of converging, so point-blank shots never get a
/// degenerate or backward direction.
const CONVERGENCE_MIN_DISTANCE: f32 = 1.5;
/// Blockers this close to the target itself are not cover — they are
/// practically the target (or its immediate surroundings) and the arrow
/// should just fly and resolve naturally.
const TARGET_BACKOFF: f32 = 0.5;
/// Stamina drain rate per second when pulling/holding the bowstring.
const DRAW_STAMINA_DRAIN_PER_SEC: f32 = 14.0;
/// Delay (seconds) after firing before you can draw or shoot again.
const RELOAD_COOLDOWN_SECS: f32 = 0.6;

pub use bof_domain::combat::state::DrawStrength;

/// Per-actor deterministic stream for bow spread.
///
/// The seed is authored with the actor archetype, never derived from `Entity`
/// or wall-clock time. Keeping the stream on the actor also makes simultaneous
/// shooters independent without coupling results to ECS query order.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShotSpreadRng(u32);

impl ShotSpreadRng {
    pub const PLAYER: Self = Self::seeded(0xB0F0_0001);
    pub const BOKOBO_ARCHER: Self = Self::seeded(0xB0F0_A001);

    pub const fn seeded(seed: u32) -> Self {
        Self(seed)
    }

    fn next_unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0 as f32 / u32::MAX as f32
    }
}

type ProposeQuery<'a> = (
    &'a CombatIntents,
    &'a CombatState,
    &'a mut CombatProposalBuffer,
);

/// Draw from `Idle`, keep drawn while held. A melee start the same tick
/// out-arbitrates the draw (`weight::ATTACK_CHAIN > weight::AIM`); an attack
/// press while already drawn starts charging, handled by `tick_draw_strength`
/// and `shoot_drawn_arrow`.
pub fn propose(mut q: Query<ProposeQuery, With<Actor>>) {
    for (intents, state, mut buffer) in &mut q {
        if !intents.wants_aim {
            continue;
        }
        if matches!(*state, CombatState::Idle | CombatState::Aiming) {
            let _ = buffer.push(TransitionProposal::new(
                CombatState::Aiming,
                Priority::PlayerRequested,
                weight::AIM,
                "aim",
            ));
        }
    }
}

type DrawQuery<'a> = (
    &'a CombatIntents,
    &'a CombatState,
    &'a mut DrawStrength,
    Option<&'a mut crate::movement::stamina::Stamina>,
    Option<&'a CombatContext>,
    Option<&'a MountedCombatProfile>,
);

/// `TickActiveMotor`: accumulate charge while attack is held during `Aiming`.
/// Resets on any other state.
pub fn tick_draw_strength(time: Res<Time>, mut q: Query<DrawQuery, With<Actor>>) {
    let dt = time.delta_secs();
    for (intents, state, mut draw, mut stamina, context, mounted_profile) in &mut q {
        // Cooldown ticks down at all times
        if draw.cooldown > 0.0 {
            draw.cooldown = (draw.cooldown - dt).max(0.0);
        }

        if *state != CombatState::Aiming {
            draw.factor = 0.0;
            draw.charging = false;
            draw.tuning = None;
            continue;
        }

        // Only allow drawing/charging if not on cooldown and has stamina left
        let has_stamina = stamina.as_ref().is_none_or(|s| !s.is_exhausted());
        let can_charge = draw.cooldown <= 0.0 && has_stamina;

        if intents.attack.held && can_charge {
            if draw.tuning.is_none() {
                draw.tuning = Some(effective_bow(context, mounted_profile));
            }
            draw.charging = true;
            let tuning = draw.tuning.unwrap_or(BowProfile::ON_FOOT);
            draw.factor = (draw.factor + dt / tuning.draw_time_secs).min(1.0);

            if let Some(ref mut s) = stamina {
                s.drain(DRAW_STAMINA_DRAIN_PER_SEC * dt);
            }
        } else {
            draw.charging = false;
        }
    }
}

type ShootQuery<'a> = (
    Entity,
    &'a Transform,
    &'a ControlOrientation,
    &'a CombatIntents,
    &'a CombatState,
    &'a mut DrawStrength,
    &'a mut ShotSpreadRng,
    Option<&'a crate::movement::stamina::Stamina>,
    Option<&'a CombatContext>,
    Option<&'a MountedCombatProfile>,
    Option<&'a crate::movement::facing::FacingSource>,
);

/// Height above the target's origin the lock-on aim points at (torso, not feet).
const LOCK_AIM_TARGET_HEIGHT: f32 = 0.9;

/// A control orientation that points the aim from the shooter straight at the
/// locked target, so a locked bow auto-aims instead of following the (now
/// camera-decoupled) mouse look.
fn lock_aim_orientation(shooter: Vec3, target: Vec3) -> ControlOrientation {
    let to = (target + Vec3::Y * LOCK_AIM_TARGET_HEIGHT) - (shooter + Vec3::Y * AIM_PIVOT_HEIGHT);
    let len = to.length().max(1e-4);
    ControlOrientation {
        yaw: (-to.x).atan2(-to.z),
        pitch: (to.y / len).asin().clamp(-1.2, 1.2),
    }
}

/// While `Aiming`, releasing the attack button (held last tick but not this
/// tick, with charge > 0) fires the arrow.
pub fn shoot_drawn_arrow(
    mut q: Query<ShootQuery, With<Actor>>,
    targets: Query<&Transform>,
    mut spawns: MessageWriter<SpawnProjectileMessage>,
    mut fired: MessageWriter<BowFiredMessage>,
    spatial: SpatialQuery,
) {
    for (
        shooter,
        transform,
        orientation,
        intents,
        state,
        mut draw,
        mut spread_rng,
        stamina,
        context,
        mounted_profile,
        facing,
    ) in &mut q
    {
        if *state != CombatState::Aiming {
            continue;
        }

        // Cannot fire if reload/notch cooldown is still active
        if draw.cooldown > 0.0 {
            continue;
        }

        // Fire triggers:
        // 1. Released attack button while we had some charge accumulated.
        // 2. Press and release inside one fixed tick: the edge arrives with
        //    the button already up and no charge — fire at minimum charge
        //    instead of losing the shot.
        // 3. Stamina ran out while drawing, forcing release.
        let released = !intents.attack.held && draw.factor > 0.0;
        let tap = intents.attack.pressed && !intents.attack.held;
        let stamina_exhausted = draw.charging && stamina.is_some_and(|s| s.is_exhausted());

        let fire = released || tap || stamina_exhausted;
        if !fire {
            continue;
        }

        let factor = draw.factor;
        let profile = draw
            .tuning
            .unwrap_or_else(|| effective_bow(context, mounted_profile));
        let speed = lerp(profile.speed_min, profile.speed_max, factor);
        let damage = lerp(profile.damage_min, profile.damage_max, factor);

        // When locked on, the bow aims at the target, not the mouse look (which
        // the lock-on camera has decoupled from the view). Orientation points at
        // the target so the socket/aim line are consistent.
        let locked_target = match facing {
            Some(crate::movement::facing::FacingSource::LockOn(target)) => {
                targets.get(*target).ok().map(|t| t.translation)
            }
            _ => None,
        };
        let locked_aim = locked_target.map(|p| lock_aim_orientation(transform.translation, p));
        let orientation = locked_aim.as_ref().unwrap_or(orientation);

        let aim_dir = aim_direction(orientation);

        // Two-phase aim: the crosshair ray (from the camera-aligned pivot)
        // resolves the world target, and the arrow leaves the bow socket
        // converging on it. Point-blank targets, or any cover blocking the
        // bow's line that the crosshair already cleared, fall back to the
        // aim line itself — if the player can see it, they can shoot it.
        let pivot = aim_pivot(transform, orientation);
        let socket = bow_socket(transform, orientation);
        // Locked: converge straight on the target torso so the shoulder-offset
        // socket does not bias the shot sideways. Free: raycast the crosshair.
        let target = match locked_target {
            Some(p) => p + Vec3::Y * LOCK_AIM_TARGET_HEIGHT,
            None => aim_target(&spatial, shooter, pivot, aim_dir),
        };
        let blocked = bow_line_blocked(&spatial, shooter, socket, target, aim_dir);
        let (launch_origin, direction) = launch_pose(pivot, socket, target, aim_dir, blocked);

        // Accuracy spread calculation: Bannerlord style
        // Maximum spread angle of ~8.5 degrees (0.15 rad) at zero charge,
        // shrinking down to 0.0 degrees (100% accurate) at full charge.
        let spread = (1.0 - factor) * 0.15;
        let perturbed_direction = perturb_direction(direction, spread, &mut spread_rng);

        let origin = launch_origin + perturbed_direction * ARROW_MUZZLE_FORWARD;

        spawns.write(SpawnProjectileMessage {
            shooter,
            origin,
            velocity: perturbed_direction * speed,
            damage,
        });
        fired.write(BowFiredMessage {
            shooter,
            charge: factor,
        });

        draw.factor = 0.0;
        draw.charging = false;
        draw.tuning = None;
        draw.cooldown = RELOAD_COOLDOWN_SECS;
    }
}

fn perturb_direction(direction: Vec3, spread: f32, rng: &mut ShotSpreadRng) -> Vec3 {
    if spread <= 0.001 {
        return direction;
    }
    let mut right = direction.cross(Vec3::Y);
    if right.length_squared() < 0.001 {
        right = direction.cross(Vec3::Z);
    }
    let right = right.normalize();
    let up = right.cross(direction).normalize();
    let r1 = rng.next_unit() * 2.0 - 1.0;
    let r2 = rng.next_unit() * 2.0 - 1.0;
    (direction + right * r1 * spread + up * r2 * spread).normalize()
}

/// The camera-aligned shoulder pivot the crosshair ray starts from.
fn aim_pivot(transform: &Transform, orientation: &ControlOrientation) -> Vec3 {
    let shoulder = Quat::from_rotation_y(orientation.yaw) * Vec3::X * AIM_SHOULDER_OFFSET;
    transform.translation + Vec3::Y * AIM_PIVOT_HEIGHT + shoulder
}

/// World position of the bow socket the arrow spawns at.
fn bow_socket(transform: &Transform, orientation: &ControlOrientation) -> Vec3 {
    transform.translation + Quat::from_rotation_y(orientation.yaw) * BOW_SOCKET_LOCAL
}

/// The crosshair's world target: first hit along the aim ray (ignoring the
/// shooter), or the point at max range when the ray hits nothing.
fn aim_target(spatial: &SpatialQuery, shooter: Entity, pivot: Vec3, aim_dir: Vec3) -> Vec3 {
    let filter = SpatialQueryFilter::from_excluded_entities([shooter]);
    let Ok(direction) = Dir3::new(aim_dir) else {
        return pivot + aim_dir * AIM_RAY_MAX_RANGE;
    };
    match spatial.cast_ray(pivot, direction, AIM_RAY_MAX_RANGE, true, &filter) {
        Some(hit) => pivot + aim_dir * hit.distance,
        None => pivot + aim_dir * AIM_RAY_MAX_RANGE,
    }
}

/// Direction from the bow socket to the crosshair target, so the trajectory
/// converges on the point the player is looking at. Targets projecting closer
/// than `CONVERGENCE_MIN_DISTANCE` ahead of the socket (including behind it)
/// fall back to the aim line.
fn converge_direction(socket: Vec3, target: Vec3, aim_dir: Vec3) -> Vec3 {
    let to_target = target - socket;
    if to_target.dot(aim_dir) < CONVERGENCE_MIN_DISTANCE {
        return aim_dir;
    }
    to_target.normalize()
}

/// Whether anything blocks the bow's own line to the crosshair target. The
/// crosshair ray already cleared the sight line up to the target, so any
/// blocker here is something the camera sees past but the bow does not
/// (stair edge, wall lip, parkour box) — checked over the whole flight path,
/// backed off the target itself so the target never counts as cover.
fn bow_line_blocked(
    spatial: &SpatialQuery,
    shooter: Entity,
    socket: Vec3,
    target: Vec3,
    aim_dir: Vec3,
) -> bool {
    let direction = converge_direction(socket, target, aim_dir);
    let check_distance = socket.distance(target) - TARGET_BACKOFF;
    if check_distance <= 0.0 {
        return false;
    }
    let Ok(direction) = Dir3::new(direction) else {
        return false;
    };
    let filter = SpatialQueryFilter::from_excluded_entities([shooter]);
    spatial
        .cast_ray(socket, direction, check_distance, true, &filter)
        .is_some()
}

/// Where the arrow launches. Normally from the bow socket, converging on the
/// crosshair target; when near cover blocks the bow's line the crosshair
/// already cleared, from the aim pivot straight down the aim line — the shot
/// always goes where the player sees.
fn launch_pose(
    pivot: Vec3,
    socket: Vec3,
    target: Vec3,
    aim_dir: Vec3,
    bow_line_blocked: bool,
) -> (Vec3, Vec3) {
    if bow_line_blocked {
        return (pivot, aim_dir);
    }
    (socket, converge_direction(socket, target, aim_dir))
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// The camera-forward implied by the control orientation (same rotation the
/// camera rig builds: yaw then pitch, looking down -Z).
pub(crate) fn aim_direction(orientation: &ControlOrientation) -> Vec3 {
    (Quat::from_rotation_y(orientation.yaw) * Quat::from_rotation_x(orientation.pitch))
        * Vec3::NEG_Z
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::RunSystemOnce;

    #[test]
    fn aim_direction_matches_yaw_and_pitch() {
        let level = aim_direction(&ControlOrientation {
            yaw: 0.0,
            pitch: 0.0,
        });
        assert!((level - Vec3::NEG_Z).length() < 1e-5);

        let up = aim_direction(&ControlOrientation {
            yaw: 0.0,
            pitch: std::f32::consts::FRAC_PI_4,
        });
        assert!(up.y > 0.5, "positive pitch must aim upward, got {up}");

        let turned = aim_direction(&ControlOrientation {
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: 0.0,
        });
        assert!(
            turned.x < -0.99,
            "quarter yaw must aim along -X, got {turned}"
        );
    }

    #[test]
    fn convergence_points_from_socket_to_the_crosshair_target() {
        let socket = Vec3::new(0.35, 1.15, -0.55);
        let target = Vec3::new(0.0, 2.5, -30.0);
        let aim_dir = Vec3::NEG_Z;

        let direction = converge_direction(socket, target, aim_dir);

        let expected = (target - socket).normalize();
        assert!(
            (direction - expected).length() < 1e-5,
            "far targets must converge socket→target, got {direction}"
        );
        assert!(
            (direction.length() - 1.0).abs() < 1e-5,
            "direction must be normalized"
        );
    }

    #[test]
    fn blocked_bow_line_launches_from_the_aim_pivot_instead() {
        let pivot = Vec3::new(0.72, 1.7, 0.0);
        let socket = Vec3::new(0.35, 1.4, -0.55);
        let target = Vec3::new(0.0, 1.7, -40.0);
        let aim_dir = Vec3::NEG_Z;

        let (origin, direction) = launch_pose(pivot, socket, target, aim_dir, true);
        assert_eq!(origin, pivot, "blocked bow must fire from the aim line");
        assert_eq!(direction, aim_dir);

        let (origin, direction) = launch_pose(pivot, socket, target, aim_dir, false);
        assert_eq!(origin, socket, "clear bow fires from the socket");
        assert!(
            (direction - (target - socket).normalize()).length() < 1e-5,
            "clear bow converges on the target"
        );
    }

    #[test]
    fn point_blank_target_falls_back_to_the_aim_line() {
        let socket = Vec3::new(0.35, 1.15, -0.55);
        let aim_dir = Vec3::NEG_Z;

        // Wall right in front: closer along the aim line than the minimum.
        let close = socket + aim_dir * (CONVERGENCE_MIN_DISTANCE * 0.5);
        assert_eq!(converge_direction(socket, close, aim_dir), aim_dir);

        // Degenerate: the crosshair hit something behind the socket.
        let behind = socket - aim_dir * 2.0;
        assert_eq!(converge_direction(socket, behind, aim_dir), aim_dir);
    }

    #[test]
    fn aiming_holds_while_wanted_and_yields_to_a_melee_start() {
        let mut world = World::new();
        let drawing = world
            .spawn((
                Actor,
                CombatIntents {
                    wants_aim: true,
                    ..Default::default()
                },
                CombatState::Idle,
                CombatProposalBuffer::default(),
            ))
            .id();
        let mid_swing = world
            .spawn((
                Actor,
                CombatIntents {
                    wants_aim: true,
                    ..Default::default()
                },
                CombatState::Windup,
                CombatProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

        let proposals: Vec<_> = world
            .entity(drawing)
            .get::<CombatProposalBuffer>()
            .unwrap()
            .iter()
            .cloned()
            .collect();
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].target_state, CombatState::Aiming);
        assert!(
            proposals[0].override_weight < weight::ATTACK_CHAIN,
            "a same-tick melee start must beat the draw"
        );

        assert!(
            world
                .entity(mid_swing)
                .get::<CombatProposalBuffer>()
                .unwrap()
                .iter()
                .next()
                .is_none(),
            "the bow must not interrupt a committed swing"
        );
    }

    #[test]
    fn draw_strength_scales_speed_and_damage() {
        let profile = BowProfile::ON_FOOT;
        assert!((lerp(profile.speed_min, profile.speed_max, 0.0) - profile.speed_min).abs() < 1e-5);
        assert!((lerp(profile.speed_min, profile.speed_max, 1.0) - profile.speed_max).abs() < 1e-5);
        let half = lerp(profile.damage_min, profile.damage_max, 0.5);
        assert!(
            half > profile.damage_min && half < profile.damage_max,
            "half charge must be between min and max, got {half}"
        );
    }

    #[test]
    fn spread_is_reproducible_and_independent_per_authored_seed() {
        let mut first_run = ShotSpreadRng::seeded(42);
        let mut second_run = ShotSpreadRng::seeded(42);
        let mut other_actor = ShotSpreadRng::seeded(43);

        let first: Vec<Vec3> = (0..8)
            .map(|_| perturb_direction(Vec3::NEG_Z, 0.15, &mut first_run))
            .collect();
        let replay: Vec<Vec3> = (0..8)
            .map(|_| perturb_direction(Vec3::NEG_Z, 0.15, &mut second_run))
            .collect();
        let independent: Vec<Vec3> = (0..8)
            .map(|_| perturb_direction(Vec3::NEG_Z, 0.15, &mut other_actor))
            .collect();

        assert_eq!(first, replay);
        assert_ne!(first, independent);
    }

    #[test]
    fn full_charge_does_not_advance_the_spread_stream() {
        let mut rng = ShotSpreadRng::seeded(42);
        let before = rng;
        assert_eq!(perturb_direction(Vec3::NEG_Z, 0.0, &mut rng), Vec3::NEG_Z);
        assert_eq!(rng, before);
    }

    #[test]
    fn active_draw_keeps_mounted_tuning_after_context_switch() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Messages<crate::combat::context::SetMountedCombatMessage>>();
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(0.1));
        let actor = world
            .spawn((
                Actor,
                CombatIntents {
                    attack: crate::combat::intent::AttackIntent {
                        pressed: true,
                        held: true,
                    },
                    ..Default::default()
                },
                CombatState::Aiming,
                DrawStrength::default(),
                CombatContext::default(),
                MountedCombatProfile::HORSE,
            ))
            .id();
        world.write_message(crate::combat::context::SetMountedCombatMessage {
            actor,
            mounted: true,
        });
        world
            .run_system_once(crate::combat::context::apply_mounted_context)
            .unwrap();
        world.run_system_once(tick_draw_strength).unwrap();
        let mounted_tuning = MountedCombatProfile::HORSE.bow;
        assert_eq!(
            world.entity(actor).get::<DrawStrength>().unwrap().tuning,
            Some(mounted_tuning)
        );

        world.write_message(crate::combat::context::SetMountedCombatMessage {
            actor,
            mounted: false,
        });
        world
            .run_system_once(crate::combat::context::apply_mounted_context)
            .unwrap();
        world.run_system_once(tick_draw_strength).unwrap();
        assert_eq!(
            world.entity(actor).get::<DrawStrength>().unwrap().tuning,
            Some(mounted_tuning)
        );
    }
}
