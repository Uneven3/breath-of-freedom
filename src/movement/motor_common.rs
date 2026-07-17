//! Shared motor helpers.
//!
//! Every motor uses these (`apply_locomotion_rotation`, `move_toward`) plus the
//! `move_and_slide` call every motor makes. The slide is Avian's `MoveAndSlide`
//! system param; we wrap it so each motor calls one function — the only place
//! the body is moved (see `docs/ARCHITECTURE.md`).

use avian3d::prelude::*;
use bevy::prelude::*;
use std::f32::consts::PI;
use std::time::Duration;

use super::BodyVelocity;
use super::abilities::GroundDriveProfile;
use super::body::BodyDimensions;
use super::facts::{BodyContact, GroundFacts};
use super::intents::Intents;
use super::stamina::Stamina;
use super::state::LocomotionState;

/// A surface counts as floor if its normal is within this dot of straight up.
/// `cos(60°) = 0.5` matches `GroundService`'s `max_slope_angle_deg = 60`.
/// Shared with `services::ground` (the downward grounded probe) as the single
/// 60° slope source of truth.
pub const FLOOR_MIN_UP_DOT: f32 = 0.5;
/// A surface counts as wall if it is nearly vertical (`|n.y|` small).
const WALL_MAX_UP_DOT: f32 = 0.2;

/// Step `from` toward `to` by at most `delta`.
pub fn move_toward(from: f32, to: f32, delta: f32) -> f32 {
    if (to - from).abs() <= delta {
        to
    } else {
        from + (to - from).signum() * delta
    }
}

/// Slerp the body's yaw toward its planar move direction. Movement is planar,
/// so we rotate purely about Y (cheaper and stable vs a full `looking_to`).
pub fn apply_locomotion_rotation(transform: &mut Transform, move_dir: Vec2, dt: f32, speed: f32) {
    if move_dir.length_squared() <= 0.01 {
        return;
    }
    let dir = Vec3::new(move_dir.x, 0.0, move_dir.y).normalize_or_zero();
    // Yaw so that the body's forward (-Z) points along `dir`: solving R_y(θ)·(-Z) = dir.
    let yaw = (-dir.x).atan2(-dir.z);
    let target = Quat::from_rotation_y(yaw);
    let t = (speed * dt).clamp(0.0, 1.0);
    transform.rotation = transform.rotation.slerp(target, t);
}

/// Move the kinematic body along `velocity` for one fixed step, sliding along
/// colliders. Updates `transform.translation`, classifies wall contacts into
/// `contact`, and returns the post-slide ("projected") velocity to store for next
/// frame.
pub fn body_move_and_slide(
    mas: &MoveAndSlide,
    entity: Entity,
    collider: &Collider,
    transform: &mut Transform,
    velocity: Vec3,
    delta: Duration,
    contact: &mut BodyContact,
) -> Vec3 {
    let filter = SpatialQueryFilter::from_excluded_entities([entity]);
    let mut on_wall = false;
    let mut wall_normal = Vec3::ZERO;

    let out = mas.move_and_slide(
        collider,
        transform.translation,
        transform.rotation,
        velocity,
        delta,
        &MoveAndSlideConfig::default(),
        &filter,
        |hit| {
            let n: Vec3 = Vec3::from(*hit.normal);
            if n.y.abs() < WALL_MAX_UP_DOT {
                on_wall = true;
                wall_normal = n;
            }
            MoveAndSlideHitResponse::Accept
        },
    );

    transform.translation = out.position;
    contact.on_wall = on_wall;
    contact.wall_normal = wall_normal;
    out.projected_velocity
}

/// How far below the body to look when re-attaching to a downward slope.
/// Generous enough to cover one tick's worth of horizontal movement dropping
/// off a steep-but-still-floor slope; still short enough not to snap onto
/// unrelated geometry far below (e.g. mid-air over a pit).
const GROUND_SNAP_DISTANCE: f32 = 0.4;

/// Below this gap, the body already rests on the floor (collision skin
/// margin, not a real gap) — skip the correction. Without this, the pull
/// below fires on *every* tick (see why in the doc comment), and a
/// straight-down pull on a non-flat normal (a slope, or anywhere but dead
/// center on a sphere) nudges the contact point sideways each time —
/// visible as "sliding while standing still" on curved/inclined ground.
const GROUND_SNAP_EPSILON: f32 = 0.02;

/// Pull a grounded body down onto a slope its purely-horizontal move this
/// tick didn't reach.
///
/// Walk/Sprint/Sneak zero `velocity.y` every tick (flat-floor locomotion), so
/// `body_move_and_slide`'s sweep is a horizontal-only vector. On a *descending*
/// slope that sweep can clear the surface entirely instead of following it
/// down — and, per `ground.rs`'s own doc comment, `contact.grounded` reads
/// false almost every tick *regardless of slope* (a zero/horizontal-only
/// sweep rarely re-touches the floor), so this runs continuously rather than
/// only on the descending-slope case it targets. Without this snap at all,
/// the body floats forward in a straight line for a few ticks on a downhill
/// until `GroundFacts`'s short downward probe (`ground.rs`, 0.2 units)
/// finally notices it's airborne and `Fall` takes over — the "walks off the
/// top of a downhill slope" feel bug. Call this right after
/// `body_move_and_slide` in any flat-floor motor's `tick`.
///
/// Skips when `contact.on_wall`: a wall-classified hit this tick means we
/// just bumped into something in front of us (a stair riser, a curb) rather
/// than walked off a continuous slope — without this guard the downward cast
/// finds the obstacle's top within `GROUND_SNAP_DISTANCE` and auto-climbs it,
/// which reads as "sliding uphill" on approach and produces a jerky handoff
/// into motors (like Stairs) that expect to own that climb themselves.
pub fn snap_to_ground(
    mas: &MoveAndSlide,
    entity: Entity,
    collider: &Collider,
    transform: &mut Transform,
    contact: &BodyContact,
) {
    if contact.on_wall {
        return;
    }

    let filter = SpatialQueryFilter::from_excluded_entities([entity]);
    let Some(hit) = mas.spatial_query.cast_shape(
        collider,
        transform.translation,
        transform.rotation,
        Dir3::NEG_Y,
        &ShapeCastConfig::from_max_distance(GROUND_SNAP_DISTANCE),
        &filter,
    ) else {
        return;
    };

    // `normal1` is already in world space (avian docs) — no rotation needed.
    let normal = hit.normal1;
    if normal.y <= FLOOR_MIN_UP_DOT {
        return;
    }

    if hit.distance > GROUND_SNAP_EPSILON {
        transform.translation.y -= hit.distance;
    }
}

/// Redirect a planar (horizontal) velocity along the floor plane, keeping its
/// magnitude — walking a slope moves at full speed *tangentially* (BotW
/// style). Sweeping the raw horizontal vector into an incline instead makes
/// `move_and_slide` re-project it every tick, taxing speed by `sin²(slope)`
/// per tick — the "stuck at the foot of the ramp" crawl.
pub fn align_with_floor(planar: Vec3, floor_normal: Vec3) -> Vec3 {
    let speed = planar.length();
    if speed <= f32::EPSILON {
        return planar;
    }
    let tangent = (planar - floor_normal * planar.dot(floor_normal)).normalize_or_zero();
    if tangent == Vec3::ZERO {
        return planar;
    }
    tangent * speed
}

/// Advance one actor through a flat-ground locomotion mode.
pub struct GroundDriveStep<'a> {
    pub entity: Entity,
    pub collider: &'a Collider,
    pub transform: &'a mut Transform,
    pub velocity: &'a mut BodyVelocity,
    pub intents: &'a Intents,
    pub stamina: Option<&'a mut Stamina>,
    pub contact: &'a mut BodyContact,
    pub ground: &'a GroundFacts,
    pub state: LocomotionState,
}

pub fn ground_drive_step(
    mut step: GroundDriveStep,
    active: LocomotionState,
    mas: &MoveAndSlide,
    time: &Time,
    params: &GroundDriveProfile,
) {
    if step.state != active {
        return;
    }

    let dt = time.delta_secs();
    let move_dir = Vec3::new(
        step.intents.planar.direction.x,
        0.0,
        step.intents.planar.direction.y,
    )
    .normalize_or_zero();
    let (mut next_velocity, turn_rate) = drive_planar_velocity(
        step.velocity.0,
        move_dir,
        step.transform.forward().as_vec3(),
        step.intents.planar.strength,
        dt,
        params,
    );
    if move_dir != Vec3::ZERO {
        apply_locomotion_rotation(step.transform, step.intents.planar.direction, dt, turn_rate);
    }
    // Flat-ground motors own velocity.y: bookkeeping stays planar…
    next_velocity.y = 0.0;
    let planar_velocity = next_velocity;
    // …but the sweep follows the floor plane, so slopes move at full speed
    // tangentially instead of paying a projection tax every tick.
    if step.ground.grounded {
        next_velocity = align_with_floor(next_velocity, step.ground.floor_normal);
    }

    if let Some(stamina) = step.stamina.as_mut() {
        if params.stamina_per_sec >= 0.0 {
            stamina.recover(params.stamina_per_sec * dt);
        } else {
            stamina.drain(-params.stamina_per_sec * dt);
        }
    }

    let projected_velocity = body_move_and_slide(
        mas,
        step.entity,
        step.collider,
        step.transform,
        next_velocity,
        time.delta(),
        step.contact,
    );
    // A floor sweep can hit a ramp's sharp lower edge before its downward
    // probe sees the ramp. `move_and_slide` projects against that corner,
    // which is correct for displacement but must not erase the motor's planar
    // target speed. Preserve it unless an actual wall stopped us.
    step.velocity.0 = if step.contact.on_wall {
        projected_velocity
    } else {
        planar_velocity
    };
    snap_to_ground(
        mas,
        step.entity,
        step.collider,
        step.transform,
        step.contact,
    );
    // Flat-ground motors are strictly planar: discard the tangential Y the
    // slide projected onto ramps. Leaving it in `BodyVelocity` made
    // `GroundService`'s ascend check read slope-walking as "launching off the
    // floor" (the Walk<->Fall flicker on the test ramp).
    step.velocity.0.y = 0.0;
}

pub(crate) fn drive_planar_velocity(
    current: Vec3,
    desired: Vec3,
    facing: Vec3,
    strength: f32,
    dt: f32,
    profile: &GroundDriveProfile,
) -> (Vec3, f32) {
    let planar = Vec3::new(current.x, 0.0, current.z);
    let speed = planar.length();
    let speed_factor = (speed / profile.max_forward_speed.max(f32::EPSILON)).clamp(0.0, 1.0);
    let turn_rate = profile.turn_rate_at_zero_speed
        + (profile.turn_rate_at_max_speed - profile.turn_rate_at_zero_speed) * speed_factor;
    if desired == Vec3::ZERO {
        return (
            Vec3::new(
                move_toward(current.x, 0.0, profile.coast_deceleration * dt),
                current.y,
                move_toward(current.z, 0.0, profile.coast_deceleration * dt),
            ),
            turn_rate,
        );
    }
    let reversing = facing.dot(desired) < -0.15;
    let target_speed = if reversing {
        profile.max_reverse_speed
    } else {
        profile.max_forward_speed
    };
    let acceleration = if reversing {
        profile.reverse_acceleration
    } else {
        profile.forward_acceleration
    };
    let alignment = planar.normalize_or_zero().dot(desired);
    let rate = if speed > 0.0 && alignment < 0.0 {
        profile.brake_deceleration
    } else {
        acceleration
    };
    let target = desired * target_speed * strength.clamp(0.0, 1.0);
    let raw = Vec3::new(
        move_toward(current.x, target.x, rate * dt),
        current.y,
        move_toward(current.z, target.z, rate * dt),
    );
    let aligned = Vec3::new(raw.x, 0.0, raw.z).lerp(
        desired * Vec3::new(raw.x, 0.0, raw.z).length(),
        (profile.velocity_alignment_rate * dt).clamp(0.0, 1.0),
    );
    let loss = (1.0 - alignment.clamp(0.0, 1.0)) * profile.turning_speed_loss * dt;
    (
        Vec3::new(
            aligned.x * (1.0 - loss).max(0.0),
            raw.y,
            aligned.z * (1.0 - loss).max(0.0),
        ),
        turn_rate,
    )
}

/// Position-lerped arc shared by Mantle and AutoVault: smoothstep from `start`
/// to `target` with a sinusoidal height bump.
#[derive(Default)]
pub struct KinematicArc {
    pub running: bool,
    elapsed: f32,
    duration: f32,
    start: Vec3,
    target: Vec3,
}

impl KinematicArc {
    pub fn begin(&mut self, start: Vec3, target: Vec3, duration: f32) {
        self.start = start;
        self.target = target;
        self.duration = duration;
        self.elapsed = 0.0;
        self.running = true;
    }

    /// Advance by `dt` and return the next body position; on the final step
    /// this lands exactly on `target` and clears `running`.
    pub fn step(&mut self, dt: f32, arc_height: f32) -> Vec3 {
        self.elapsed = (self.elapsed + dt).min(self.duration);
        let raw = self.elapsed / self.duration;
        if raw >= 1.0 {
            self.running = false;
            return self.target;
        }
        let mut next = self.start.lerp(self.target, smoothstep(raw));
        next.y += (raw * PI).sin() * arc_height;
        next
    }
}

/// `smoothstep(0, 1, x)` = x²(3 − 2x).
fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Keep the climb/wall-jump cap this far below a detected ledge lip, forcing a
/// Mantle instead of letting the body float over the edge.
pub const LEDGE_TOP_OFFSET: f32 = 0.33;

/// Soft ceiling shared by Climb and WallJump: cap upward motion just below the
/// ledge lip (`lip_height` > 0 means the down-cast found the ledge top).
/// Returns true while the body is pinned at the cap.
pub fn clip_below_ledge_lip(
    transform: &mut Transform,
    v: &mut Vec3,
    lip_height: f32,
    body: BodyDimensions,
    dt: f32,
) -> bool {
    if lip_height <= 0.0 || v.y <= 0.0 {
        return false;
    }
    let feet_y = transform.translation.y - body.standing_half_height();
    let max_y = feet_y + lip_height - LEDGE_TOP_OFFSET;
    if transform.translation.y >= max_y {
        v.y = 0.0;
        transform.translation.y = max_y;
        true
    } else {
        // Don't overshoot the cap within a single tick.
        let max_safe = (max_y - transform.translation.y) / dt;
        if v.y > max_safe {
            v.y = max_safe;
        }
        false
    }
}

/// Wall normal used to launch off a climbed wall (WallJump / EdgeLeap): prefer
/// the sensed climb normal, fall back to the last wall contact, then to the
/// body's back.
pub fn launch_normal(
    climb_normal: Option<Vec3>,
    contact: &BodyContact,
    transform: &Transform,
) -> Vec3 {
    climb_normal.unwrap_or_else(|| {
        if contact.on_wall {
            -contact.wall_normal
        } else {
            transform.rotation * Vec3::Z
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::abilities::GroundMovement;

    /// The 20° test ramp's surface normal (rises toward +X).
    fn ramp_normal() -> Vec3 {
        Vec3::new(
            -20.0_f32.to_radians().sin(),
            20.0_f32.to_radians().cos(),
            0.0,
        )
    }

    #[test]
    fn align_on_flat_floor_is_identity() {
        let v = Vec3::new(5.0, 0.0, 0.0);
        assert!((align_with_floor(v, Vec3::Y) - v).length() < 1e-5);
    }

    #[test]
    fn align_uphill_keeps_full_speed_and_rises() {
        let out = align_with_floor(Vec3::new(5.0, 0.0, 0.0), ramp_normal());
        assert!((out.length() - 5.0).abs() < 1e-4, "speed must be preserved");
        assert!(out.y > 0.0, "moving into the incline must climb");
    }

    #[test]
    fn align_downhill_keeps_full_speed_and_descends() {
        let out = align_with_floor(Vec3::new(-5.0, 0.0, 0.0), ramp_normal());
        assert!((out.length() - 5.0).abs() < 1e-4);
        assert!(out.y < 0.0, "moving away from the incline must descend");
    }

    #[test]
    fn align_zero_velocity_is_untouched() {
        assert_eq!(align_with_floor(Vec3::ZERO, ramp_normal()), Vec3::ZERO);
    }

    #[test]
    fn player_and_horse_profiles_have_distinct_deterministic_response() {
        let input = Vec3::NEG_Z;
        let facing = Vec3::NEG_Z;
        let player = drive_planar_velocity(
            Vec3::ZERO,
            input,
            facing,
            1.0,
            0.1,
            &GroundMovement::PLAYER.drive,
        )
        .0;
        let horse = drive_planar_velocity(
            Vec3::ZERO,
            input,
            facing,
            1.0,
            0.1,
            &GroundMovement::HORSE.drive,
        )
        .0;
        assert_eq!(
            player,
            drive_planar_velocity(
                Vec3::ZERO,
                input,
                facing,
                1.0,
                0.1,
                &GroundMovement::PLAYER.drive
            )
            .0
        );
        assert!(
            player.length() > horse.length(),
            "player preset preserves its quicker initial response"
        );
    }

    #[test]
    fn drive_distinguishes_coast_brake_reverse_and_high_speed_turning() {
        let profile = GroundMovement::HORSE.drive;
        let moving = Vec3::new(0.0, 0.0, -8.0);
        let coast = drive_planar_velocity(moving, Vec3::ZERO, Vec3::NEG_Z, 0.0, 0.1, &profile).0;
        let brake = drive_planar_velocity(moving, Vec3::Z, Vec3::NEG_Z, 1.0, 0.1, &profile).0;
        let reverse = drive_planar_velocity(Vec3::ZERO, Vec3::Z, Vec3::NEG_Z, 1.0, 1.0, &profile).0;
        let (_, fast_turn) =
            drive_planar_velocity(moving, Vec3::X, Vec3::NEG_Z, 1.0, 0.1, &profile);
        assert!(brake.length() < coast.length());
        assert!(reverse.length() <= profile.max_reverse_speed);
        assert!(fast_turn < profile.turn_rate_at_zero_speed);
    }
}
