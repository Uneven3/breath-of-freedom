//! Aim motor — bow drawn while the aim button is held.
//!
//! `propose` keeps `Aiming` alive while `wants_aim` holds (release = silence
//! → `Idle`); charging accumulates `DrawStrength` while attack is held during
//! `Aiming`, and releasing attack fires an arrow whose speed and damage scale
//! with the charge. The arrow spawns from the camera position so it visually
//! leaves the crosshair, not the player's body.

use bevy::prelude::*;

use crate::camera::CameraRig;
use crate::combat::intent::CombatIntents;
use crate::combat::proposal::{CombatProposalBuffer, Priority, TransitionProposal, weight};
use crate::combat::state::CombatState;
use crate::input::frame::ControlOrientation;
use crate::movement::Actor;
use crate::projectiles::SpawnProjectileMessage;

/// Arrow speed at zero charge (instant tap-fire).
const ARROW_SPEED_MIN: f32 = 30.0;
/// Arrow speed at full charge.
const ARROW_SPEED_MAX: f32 = 95.0;
/// Arrow damage at zero charge.
const ARROW_DAMAGE_MIN: f32 = 6.0;
/// Arrow damage at full charge.
const ARROW_DAMAGE_MAX: f32 = 18.0;
/// Seconds to reach full charge.
const DRAW_TIME_SECS: f32 = 1.4;
/// Body-relative muzzle for non-aim shots (AI actors without a camera).
const ARROW_MUZZLE_UP: f32 = 0.4;
const ARROW_MUZZLE_FORWARD: f32 = 0.6;
/// Stamina drain rate per second when pulling/holding the bowstring.
const DRAW_STAMINA_DRAIN_PER_SEC: f32 = 14.0;
/// Delay (seconds) after firing before you can draw or shoot again.
const RELOAD_COOLDOWN_SECS: f32 = 0.6;

/// Per-actor draw charge. Resets when leaving `Aiming`. Presentation reads
/// this to contract the crosshair, tint the bow, etc.
#[derive(Component, Default)]
pub struct DrawStrength {
    /// 0.0 = uncharged, 1.0 = full draw.
    pub factor: f32,
    /// Whether the player is actively holding the attack button to charge.
    pub charging: bool,
    /// Set true for the single tick the arrow is released — lets presentation
    /// systems react (camera shake, muzzle flash).
    pub just_fired: bool,
    /// Delay after firing before you can notch and pull another arrow.
    pub cooldown: f32,
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
);

/// `TickActiveMotor`: accumulate charge while attack is held during `Aiming`.
/// Resets on any other state.
pub fn tick_draw_strength(time: Res<Time>, mut q: Query<DrawQuery, With<Actor>>) {
    let dt = time.delta_secs();
    for (intents, state, mut draw, mut stamina) in &mut q {
        draw.just_fired = false;
        
        // Cooldown ticks down at all times
        if draw.cooldown > 0.0 {
            draw.cooldown = (draw.cooldown - dt).max(0.0);
        }

        if *state != CombatState::Aiming {
            draw.factor = 0.0;
            draw.charging = false;
            continue;
        }

        // Only allow drawing/charging if not on cooldown and has stamina left
        let has_stamina = stamina.as_ref().is_none_or(|s| !s.is_exhausted());
        let can_charge = draw.cooldown <= 0.0 && has_stamina;

        if intents.attack.held && can_charge {
            draw.charging = true;
            draw.factor = (draw.factor + dt / DRAW_TIME_SECS).min(1.0);
            
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
    Option<&'a crate::movement::stamina::Stamina>,
);

/// While `Aiming`, releasing the attack button (held last tick but not this
/// tick = `!pressed` with charge > 0) fires the arrow.
pub fn shoot_drawn_arrow(
    mut q: Query<ShootQuery, With<Actor>>,
    cam: Option<Single<(&Transform, &CameraRig)>>,
    mut spawns: MessageWriter<SpawnProjectileMessage>,
    time: Res<Time>,
) {
    for (shooter, transform, orientation, intents, state, mut draw, stamina) in &mut q {
        if *state != CombatState::Aiming {
            continue;
        }

        // Cannot fire if reload/notch cooldown is still active
        if draw.cooldown > 0.0 {
            continue;
        }

        // Fire triggers:
        // 1. Released attack button while we had some charge accumulated.
        // 2. Stamina ran out while drawing, forcing release.
        let stamina_exhausted = draw.charging && stamina.is_some_and(|s| s.is_exhausted());
        let released = !intents.attack.held && draw.factor > 0.0;

        let fire = released || stamina_exhausted;
        if !fire {
            continue;
        }

        let factor = draw.factor;
        let speed = lerp(ARROW_SPEED_MIN, ARROW_SPEED_MAX, factor);
        let damage = lerp(ARROW_DAMAGE_MIN, ARROW_DAMAGE_MAX, factor);

        let direction = aim_direction(orientation);

        // Accuracy spread calculation: Bannerlord style
        // Maximum spread angle of ~8.5 degrees (0.15 rad) at zero charge,
        // shrinking down to 0.0 degrees (100% accurate) at full charge.
        let spread = (1.0 - factor) * 0.15;
        let perturbed_direction = if spread > 0.001 {
            // Perpendicular vectors to perturb along
            let mut right = direction.cross(Vec3::Y);
            if right.length_squared() < 0.001 {
                right = direction.cross(Vec3::Z);
            }
            let right = right.normalize();
            let up = right.cross(direction).normalize();

            // Pseudo-random LCG based on time
            let mut seed = (time.elapsed_secs_f64().fract() * 100000.0) as u32;
            let r1 = next_random(&mut seed) * 2.0 - 1.0;
            let r2 = next_random(&mut seed) * 2.0 - 1.0;

            (direction + right * r1 * spread + up * r2 * spread).normalize()
        } else {
            direction
        };

        // In aim mode with a camera present, spawn from the camera position
        // so the arrow visually leaves the crosshair. For AI actors (no
        // camera), fall back to the body-relative muzzle.
        let origin = match &cam {
            Some(cam_single) => {
                let (cam_tf, rig) = **cam_single;
                if rig.aim_blend > 0.5 {
                    cam_tf.translation + perturbed_direction * 0.3
                  } else {
                      body_muzzle(transform, perturbed_direction)
                  }
            }
            None => body_muzzle(transform, perturbed_direction),
        };

        spawns.write(SpawnProjectileMessage {
            shooter,
            origin,
            velocity: perturbed_direction * speed,
            damage,
        });

        draw.just_fired = true;
        draw.factor = 0.0;
        draw.charging = false;
        draw.cooldown = RELOAD_COOLDOWN_SECS;
    }
}

fn next_random(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
    (*seed as f32) / (u32::MAX as f32)
}

fn body_muzzle(transform: &Transform, direction: Vec3) -> Vec3 {
    transform.translation + Vec3::Y * ARROW_MUZZLE_UP + direction * ARROW_MUZZLE_FORWARD
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
    use bevy::ecs::system::RunSystemOnce;

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
    fn aiming_holds_while_wanted_and_yields_to_a_melee_start() {
        let mut world = World::new();
        let drawing = world
            .spawn((
                Actor,
                CombatIntents {
                    wants_aim: true,
                    ..default()
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
                    ..default()
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
        assert!((lerp(ARROW_SPEED_MIN, ARROW_SPEED_MAX, 0.0) - ARROW_SPEED_MIN).abs() < 1e-5);
        assert!((lerp(ARROW_SPEED_MIN, ARROW_SPEED_MAX, 1.0) - ARROW_SPEED_MAX).abs() < 1e-5);
        let half = lerp(ARROW_DAMAGE_MIN, ARROW_DAMAGE_MAX, 0.5);
        assert!(
            half > ARROW_DAMAGE_MIN && half < ARROW_DAMAGE_MAX,
            "half charge must be between min and max, got {half}"
        );
    }
}
