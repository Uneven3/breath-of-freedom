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
use crate::movement::lod::SensingLod;
use crate::movement::motor_common::FLOOR_MIN_UP_DOT;
use crate::movement::sensing::GroundSensing;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity};
use crate::world::{GameLayer, Surface, Terrain};

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
    Option<&'a SensingLod>,
);

pub fn ground_service(
    mut q: Query<
        ServiceQuery,
        (
            With<Actor>,
            With<crate::movement::attachment::LocomotionEnabled>,
        ),
    >,
    spatial: SpatialQuery,
    surfaces: Query<&Surface>,
    terrains: Query<&Terrain>,
    mut trace: ResMut<CastTrace>,
) {
    for (entity, transform, collider, velocity, sensing, mut facts, state, lod) in &mut q {
        if SensingLod::skips(lod) {
            continue;
        }
        // Actors live exclusively on `Actor`; the world-only mask also
        // excludes `entity` without building an EntityHashSet every tick.
        let filter = SpatialQueryFilter::from_mask(GameLayer::Default);
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
        // The surface the probe stands on, for presentation (footstep audio).
        //
        // Two sources, because the ground is two different kinds of thing. A box
        // or a stair tread is uniform, so it carries one authored `Surface`. The
        // terrain is not: it is 320 m of painted cells, so it answers per contact
        // point through its semantic layer — which is the whole reason that layer
        // exists. Terrain first, since it is the entity that would otherwise need
        // a `Surface` component lying about the other 16k cells.
        facts.surface = hit
            .and_then(|h| match terrains.get(h.entity) {
                Ok(terrain) => Some(terrain.kind_at(h.point1.xz()).surface()),
                Err(_) => surfaces.get(h.entity).ok().map(|surface| surface.0),
            })
            .unwrap_or_default();
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

    #[test]
    fn ground_probe_sees_world_but_not_actor_bodies() {
        let filter = SpatialQueryFilter::from_mask(GameLayer::Default);
        assert!(filter.test(
            Entity::PLACEHOLDER,
            CollisionLayers::new(GameLayer::Default, LayerMask::ALL)
        ));
        assert!(!filter.test(
            Entity::PLACEHOLDER,
            CollisionLayers::new(GameLayer::Actor, LayerMask::ALL)
        ));
    }

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

/// Actors under their own locomotion — mounted riders are placed by their mount,
/// so they are not ours to correct.
type WalkingActors<'w, 's> = Query<
    'w,
    's,
    (&'static mut Transform, &'static Collider),
    (
        With<Actor>,
        With<crate::movement::attachment::LocomotionEnabled>,
    ),
>;

/// How deep the body may sit inside the terrain before it is lifted out, in
/// metres. Not zero: the probe and the motors legitimately keep the capsule a
/// hair inside the surface, and correcting that every tick would fight them.
const MAX_TERRAIN_PENETRATION: f32 = 0.05;

/// Lift an actor that ended up **inside** the terrain back onto its surface.
///
/// A heightfield is a one-sided surface with a thin collision margin, so a body
/// can end up under it — sculpted up beneath its feet, dropped in fast, or
/// spawned into a hill loaded from disk. Nothing rescued it once that happened:
/// the downward probe finds surface right there and reports `grounded=ON` with
/// `slope_ok=ON`, so from the simulation's point of view the actor is standing
/// comfortably, several centimetres below the ground it is standing on. That is
/// exactly the state the screenshot caught.
///
/// Runs after the motors move the body, so it corrects the position they
/// produced rather than the one they started from. Reads the collider's own
/// scaled shape, so it is right whether the capsule is standing or crouched.
pub fn lift_actors_out_of_terrain(
    terrain: Query<&crate::world::Terrain>,
    mut actors: WalkingActors,
) {
    let Ok(terrain) = terrain.single() else {
        return;
    };
    for (mut transform, collider) in &mut actors {
        let local_aabb = collider.shape_scaled().compute_local_aabb();
        let feet = transform.translation.y + local_aabb.mins.y;
        let ground = terrain.height_at(transform.translation.xz());
        let penetration = ground - feet;
        if penetration > MAX_TERRAIN_PENETRATION {
            transform.translation.y += penetration;
        }
    }
}

#[cfg(test)]
mod terrain_clearance_tests {
    use super::*;
    use crate::movement::attachment::LocomotionEnabled;
    use crate::world::Terrain;

    /// Runs the lift system once over a world holding one terrain and one actor.
    fn settle(body_y: f32, sculpt: Option<f32>) -> f32 {
        let mut app = App::new();
        let mut terrain = Terrain::flat_for_test();
        if let Some(height) = sculpt {
            terrain.raise_area(Vec2::ZERO, 30.0, height);
        }
        app.world_mut().spawn(terrain);
        let actor = app
            .world_mut()
            .spawn((
                Actor,
                LocomotionEnabled,
                Transform::from_xyz(0.0, body_y, 0.0),
                Collider::capsule(0.5, 1.0),
            ))
            .id();
        app.add_systems(Update, lift_actors_out_of_terrain);
        app.update();
        app.world().get::<Transform>(actor).unwrap().translation.y
    }

    #[test]
    fn a_body_buried_in_a_hill_is_lifted_onto_it() {
        // The screenshot's state: standing inside the ground, probe reporting
        // grounded, nothing rescuing the body.
        let ground = 8.0;
        let settled = settle(1.5, Some(ground));
        assert!(
            settled > ground,
            "body at {settled} should be lifted above ground {ground}"
        );
    }

    #[test]
    fn a_body_standing_on_flat_ground_is_not_nudged() {
        // Capsule half-height is 1.0 (0.5 radius + 0.5 half-length), so a body
        // at y = 1.0 rests exactly on a flat floor and must be left alone.
        let settled = settle(1.0, None);
        assert!(
            (settled - 1.0).abs() < 0.001,
            "a resting body moved to {settled}"
        );
    }

    #[test]
    fn a_body_in_the_air_is_not_pulled_down() {
        let settled = settle(20.0, None);
        assert!((settled - 20.0).abs() < 0.001, "airborne body moved");
    }
}
