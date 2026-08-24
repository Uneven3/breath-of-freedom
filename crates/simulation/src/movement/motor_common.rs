//! Shared motor helpers.
//!
//! Every motor uses these (`apply_locomotion_rotation`, `move_toward`) plus the
//! `move_and_slide` call every motor makes. The slide is Avian's `MoveAndSlide`
//! system param; we wrap it so each motor calls one function — the only place
//! the body is moved (see `docs/ARCHITECTURE.md`).

use avian3d::prelude::*;
use bevy_ecs::prelude::*;
use bevy_math::prelude::*;
use bevy_time::prelude::*;
use bevy_transform::prelude::*;
use std::time::Duration;

use super::BodyVelocity;
use super::abilities::GroundDriveProfile;
use super::body::BodyDimensions;
use super::facts::{BodyContact, GroundFacts};
use super::intents::Intents;
use super::stamina::Stamina;
use super::state::LocomotionState;
use crate::physics::GameLayer;

/// The steepest slope a body can stand on, in degrees. **45°**, which is what
/// Unity and Unreal ship as their default walkable angle.
///
/// It was 60° while the ground was a flat graybox, where the value never came
/// up. On sculpted relief it did: measured on the canyon, 68% of everything
/// past the limit sat in the 60-70° band, so the line fell right where the
/// terrain piles up.
pub(crate) const WALKABLE_LIMIT_DEG: f32 = 45.0;

/// A surface counts as floor if its normal is within this dot of straight up.
/// The single definition of "you can stand here", shared by the grounded probe,
/// the ledge sensor, the ground snap and the dismount search.
///
/// Read it through [`is_walkable_floor`], never by comparing directly: the four
/// call sites used to spell the comparison themselves and two of them used `<`
/// where the others used `>`, so a face at exactly the limit was neither
/// standable nor climbable. Harmless at 60°, where nothing landed on it
/// exactly; not at 45°, which is the value the test fixtures produce.
pub(crate) const FLOOR_MIN_UP_DOT: f32 = 0.70710677;

const _: () = {
    // El dot y los grados son el mismo umbral escrito dos veces, y `cos` no es
    // `const`: si alguien mueve uno sin el otro, el build lo dice.
    let cos_of_limit = 0.70710677_f32;
    assert!(WALKABLE_LIMIT_DEG == 45.0 && FLOOR_MIN_UP_DOT == cos_of_limit);
};
/// Whether a surface with this normal can be stood on.
pub(crate) fn is_walkable_floor(normal: Vec3) -> bool {
    normal.y >= FLOOR_MIN_UP_DOT
}

/// A surface counts as wall if it is nearly vertical (`|n.y|` small).
const WALL_MAX_UP_DOT: f32 = 0.2;

/// Step `from` toward `to` by at most `delta`.
pub(crate) fn move_toward(from: f32, to: f32, delta: f32) -> f32 {
    if (to - from).abs() <= delta {
        to
    } else {
        from + (to - from).signum() * delta
    }
}

/// Slerp the body's yaw toward its planar move direction. Movement is planar,
/// so we rotate purely about Y (cheaper and stable vs a full `looking_to`).
pub(crate) fn apply_locomotion_rotation(
    transform: &mut Transform,
    move_dir: Vec2,
    dt: f32,
    speed: f32,
) {
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
pub(crate) fn body_move_and_slide(
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
pub(crate) fn snap_to_ground(
    mas: &MoveAndSlide,
    collider: &Collider,
    transform: &mut Transform,
    contact: &BodyContact,
) {
    if contact.on_wall {
        return;
    }

    // World-only sensing prevents another actor capsule becoming a floor.
    let filter = SpatialQueryFilter::from_mask(GameLayer::Default);
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
    if !is_walkable_floor(normal) {
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
///
/// **Sólo la componente de subida se realinea.** Renormalizar el vector entero
/// convertía la desviación en deriva: contra una cara al límite caminable la
/// proyección mide la mitad, así que el factor de reescalado duplicaba lo que
/// iba de costado. Medido el 2026-08-23 caminando paralelo al acantilado, con
/// una dirección de input fija de `(1.00, 0.00, 0.05)`: en once ticks el avance
/// en X se fue a cero y todo el desplazamiento terminó siendo lateral. Lo que
/// va por la curva de nivel es exactamente lo que pidió el jugador y no se toca.
pub(crate) fn align_with_floor(planar: Vec3, floor_normal: Vec3) -> Vec3 {
    let Ok(uphill) = Dir3::new(Vec3::new(-floor_normal.x, 0.0, -floor_normal.z)) else {
        return planar;
    };
    let climbing_speed = planar.dot(*uphill);
    let along_contour = planar - *uphill * climbing_speed;
    let uphill_tangent = (*uphill - floor_normal * uphill.dot(floor_normal)).normalize_or_zero();
    along_contour + uphill_tangent * climbing_speed
}

/// Advance one actor through a flat-ground locomotion mode.
pub(crate) struct GroundDriveStep<'a> {
    pub entity: Entity,
    pub collider: &'a Collider,
    pub transform: &'a mut Transform,
    pub velocity: &'a mut BodyVelocity,
    pub intents: &'a Intents,
    pub stamina: Option<&'a mut Stamina>,
    pub contact: &'a mut BodyContact,
    pub ground: &'a GroundFacts,
    pub state: LocomotionState,
    /// Whether the body rotates toward its move direction. `false` under
    /// lock-on/aim, where `resolve_facing` owns the yaw.
    pub face_move: bool,
}

pub(crate) fn ground_drive_step(
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
    if move_dir != Vec3::ZERO && step.face_move {
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
    snap_to_ground(mas, step.collider, step.transform, step.contact);
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

pub use bof_domain::movement::motor_state::KinematicArc;

/// Keep the climb/wall-jump cap this far below a detected ledge lip, forcing a
/// Mantle instead of letting the body float over the edge.
pub(crate) const LEDGE_TOP_OFFSET: f32 = 0.33;

/// Soft ceiling shared by Climb and WallJump: cap upward motion just below the
/// ledge lip (`lip_height` > 0 means the down-cast found the ledge top).
/// Returns true while the body is pinned at the cap.
pub(crate) fn clip_below_ledge_lip(
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
pub(crate) fn launch_normal(
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

    /// **La deriva del 2026-08-23.** Caminando casi de frente contra la
    /// pendiente, renormalizar el vector entero duplicaba la desviación
    /// lateral, y a los pocos ticks el jugador terminaba caminando paralelo al
    /// acantilado en vez de hacia donde apuntaba.
    #[test]
    fn align_never_amplifies_the_sideways_component() {
        let planar = Vec3::new(5.0, 0.0, 0.25);
        let out = align_with_floor(planar, ramp_normal());
        assert!(
            (out.z - planar.z).abs() < 1e-4,
            "lo que va por la curva de nivel es del jugador: {} debería seguir siendo {}",
            out.z,
            planar.z
        );
    }

    /// Caminar exactamente por la curva de nivel no sube, no baja y no se
    /// desvía: la pendiente no tiene nada que decir sobre esa dirección.
    #[test]
    fn align_along_the_contour_is_identity() {
        let planar = Vec3::new(0.0, 0.0, 5.0);
        let out = align_with_floor(planar, ramp_normal());
        assert!((out - planar).length() < 1e-4);
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
