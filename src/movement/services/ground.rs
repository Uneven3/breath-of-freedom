//! Ground service — downward shape-cast grounded probe.
//!
//! Avian's `MoveAndSlide` has no floor snap, so deriving "grounded" from whether the
//! body collided *while moving this tick* flickers on flat ground: Walk zeroes vertical
//! velocity, so the swept move is horizontal-only and never re-touches the floor →
//! grounded flips false → Fall → gravity → contact → grounded true → Walk → repeat.
//!
//! Instead we cast the player's collider straight down a short distance every tick and
//! classify the hit — the idiom from avian's `kinematic_character_3d` example
//! `update_grounded`. This decouples "standing on ground" from "moved into ground" and
//! reads the body's *current* transform (no one-frame contact latency).

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::diag::CastTrace;
use crate::movement::facts::GroundFacts;
use crate::movement::motor_common::FLOOR_MIN_UP_DOT;
use crate::movement::sensing::GroundSensing;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity};

/// Suppress grounding only while *genuinely launching off* the floor (m/s).
/// During a jump's first ticks the body is still within probe range of the
/// floor; without this guard `grounded` would stay true and Walk
/// (PlayerRequested) would out-arbitrate the post-impulse state and zero the
/// upward velocity, cancelling the jump.
///
/// "Ascending" requires **both** conditions (see `is_ascending`):
///
/// - `velocity.y > ε` — rising in world space. Alone it false-positives on
///   the tangential +Y that `move_and_slide` leaves in `BodyVelocity` while
///   walking uphill.
/// - `velocity · floor_normal > ε` — moving away from the surface. Alone it
///   false-positives while walking *downhill*: the flat-ground motors keep
///   velocity horizontal (`v.y = 0`), and horizontal motion points away from
///   a tilted normal (`dot = |v|·sin(slope)`, ≈1.7 at walk speed on 20°) even
///   though `snap_to_ground` keeps the body attached. It also trips on edge
///   normals when the probe clips a box corner entering a ramp.
///
/// A real jump satisfies both by a wide margin (`v.y = 5.5`); slope walking
/// satisfies at most one.
/// Pure grounded-suppression decision (unit-tested below).
fn is_ascending(velocity: Vec3, floor_normal: Vec3, ascend_epsilon: f32) -> bool {
    velocity.y > ascend_epsilon && velocity.dot(floor_normal) > ascend_epsilon
}

type ServiceQuery<'a> = (
    Entity,
    &'a Transform,
    &'a Collider,
    &'a BodyVelocity,
    &'a GroundSensing,
    &'a mut GroundFacts,
    &'a LocomotionState,
);

pub fn ground_service(
    mut q: Query<ServiceQuery, With<Actor>>,
    spatial: SpatialQuery,
    mut trace: ResMut<CastTrace>,
) {
    for (entity, transform, collider, velocity, sensing, mut facts, state) in &mut q {
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        let hit = spatial.cast_shape(
            collider,
            transform.translation,
            transform.rotation,
            Dir3::NEG_Y,
            &ShapeCastConfig::from_max_distance(sensing.probe_distance),
            &filter,
        );
        trace.record_shape(
            entity,
            "ground_probe",
            transform.translation,
            Vec3::NEG_Y,
            sensing.probe_distance,
            hit.map(|h| (h.point1, h.normal1)),
        );

        // A hit counts as floor only if its normal is within the 60° slope limit.
        // `normal1` is already in world space (avian docs) — no rotation needed.
        let floor_normal = hit.and_then(|hit| {
            let normal = hit.normal1;
            (normal.y > FLOOR_MIN_UP_DOT).then_some(normal)
        });

        // Irrelevant when `floor_normal` is `None` (`grounded` is false either way
        // via the `&&` below), so the `Vec3::Y` fallback here is just "some finite
        // value".
        let normal = floor_normal.unwrap_or(Vec3::Y);
        facts.grounded =
            floor_normal.is_some() && !is_ascending(velocity.0, normal, sensing.ascend_epsilon);
        facts.floor_normal = normal;
        // Diagnostic decomposition for the debug HUD/logs.
        facts.probe_hit = hit.is_some();
        facts.slope_ok = floor_normal.is_some();
        facts.ascend_dot = if floor_normal.is_some() {
            velocity.0.dot(normal)
        } else {
            0.0
        };

        // Stairs motor handles Y-snap between treads; the downward probe can
        // miss the gap between steps, flickering grounded=false while the body
        // is actually supported.  Force grounded when the stairs motor is
        // active (reads previous frame’s state, which is correct).
        if *state == LocomotionState::Stairs && !facts.grounded {
            facts.grounded = true;
        }
    }
}

#[cfg(test)]
mod tests {
    //! The velocity/normal pairs come from real play-session logs (2026-07-13):
    //! the slope-flicker regressions this check used to cause.
    use super::*;

    /// The 20° test ramp's surface normal.
    fn ramp_normal() -> Vec3 {
        Vec3::new(-0.34, 0.94, 0.0).normalize()
    }

    #[test]
    fn walking_downhill_stays_grounded() {
        // [t000805] vel=(-4.99,0.00,-0.26): horizontal velocity points away
        // from the tilted normal (dot ≈ 1.7) but the body never rises.
        assert!(!is_ascending(
            Vec3::new(-4.99, 0.0, -0.26),
            ramp_normal(),
            GroundSensing::PLAYER.ascend_epsilon,
        ));
    }

    #[test]
    fn walking_uphill_tangential_stays_grounded() {
        // Slide-projected tangential velocity has real +Y but moves along the
        // surface, not away from it (dot ≈ 0).
        let tangential = Vec3::new(2.35, 0.85, 0.0); // ≈ 2.5 m/s along a 20° incline
        assert!(!is_ascending(
            tangential,
            ramp_normal(),
            GroundSensing::PLAYER.ascend_epsilon,
        ));
    }

    #[test]
    fn ramp_edge_corner_normal_stays_grounded() {
        // [t000514] hit a box corner entering the ramp (n=(-0.67,0.75,0)).
        // The logged velocity carried a +2.49 tangential Y residual from the
        // slide; the flat-ground motors now zero it (`ground_locomotion_step`),
        // so the service sees planar velocity and the corner dot is negative.
        let corner = Vec3::new(-0.67, 0.75, 0.0).normalize();
        assert!(!is_ascending(
            Vec3::new(2.58, 0.0, -0.29),
            corner,
            GroundSensing::PLAYER.ascend_epsilon,
        ));
    }

    #[test]
    fn jump_impulse_suppresses_grounding() {
        // Jump sets v.y = 5.5: rising in world space AND away from the floor.
        assert!(is_ascending(
            Vec3::new(0.0, 5.5, 0.0),
            Vec3::Y,
            GroundSensing::PLAYER.ascend_epsilon,
        ));
        // Also while jumping on the ramp, moving downhill.
        assert!(is_ascending(
            Vec3::new(-4.99, 5.5, -0.26),
            ramp_normal(),
            GroundSensing::PLAYER.ascend_epsilon,
        ));
    }
}
