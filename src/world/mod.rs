//! World: static geometry types, world-owned game rules, and the level.
//!
//! Three layers, so growing the map never grows the mechanism:
//! - [`mod@self`] — marker/data types (`Stairs`, `Ladder`, `GameLayer`) and
//!   world-owned rules (practice-target death).
//! - [`spawn`] — how a piece of geometry becomes entities (mesh + collider +
//!   markers). Knows nothing about the actual level.
//! - [`layout`] — *what* the level contains, as declarative data plus the few
//!   derived shapes (curved stairs, ramps). This is the only file that changes
//!   when authoring the map, and the natural seam for a future asset-file
//!   loader (RON/GLTF) to replace.

use avian3d::prelude::*;
use bevy::prelude::*;

pub mod day_night;
pub mod forest;
pub mod layout;
mod spawn;
pub mod terrain;

pub use forest::TreeKind;
pub use terrain::{Terrain, terrain_file};

use crate::scene::AppState;

/// Authored uniform straight stair segment. Curved stairs are composed from
/// adjacent one-step segments with independently oriented trigger volumes.
#[derive(Component, Debug, Clone)]
pub struct Stairs {
    pub base: Vec3,
    pub top: Vec3,
    pub step_count: i32,
    pub step_depth: f32,
    pub step_rise: f32,
    pub trigger_center: Vec3,
    pub trigger_half_extents: Vec3,
    pub trigger_rotation: Quat,
}

/// Authored ladder marker.
#[derive(Component, Debug, Clone)]
pub struct Ladder {
    pub bottom: Vec3,
    pub top: Vec3,
    /// Where the controlled body's center is held while attached.
    pub body_anchor: Vec3,
    /// Horizontal normal pointing away from the supporting wall.
    pub outward_normal: Vec3,
    pub trigger_center: Vec3,
    pub trigger_half_extents: Vec3,
}

/// Marks world geometry that supports a ladder but must not start wall-climb.
/// Ledge sensing still sees it for Mantle and Vault.
#[derive(Component)]
pub struct NonClimbable;

/// The ground surface a piece of world geometry presents to whoever stands on
/// it. World owns it because it is a property of the substrate; `movement`'s
/// ground probe reads it off the hit entity into `GroundFacts`, and `sfx`
/// turns the recorded surface into a footstep sound (§20).
#[derive(Component, Clone, Copy, Debug)]
pub struct Surface(pub crate::asset_pipeline::schema::SurfaceKind);

/// Game-wide physics layers. Static world geometry spawns without a
/// `CollisionLayers` component, which leaves it on `Default` (layer 0);
/// movement actors declare membership in `Actor` (see
/// `movement::bundles::KinematicActorBundle`). Physical contacts are
/// unaffected — bodies still collide across layers. What layers buy us is
/// *selective sensing*: a spatial query opts into what it can see via
/// `SpatialQueryFilter::from_mask`, e.g. ledge sensing masks to `Default` so
/// no actor reads another actor's capsule as a climbable wall.
#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
pub enum GameLayer {
    #[default]
    Default,
    Actor,
}

impl Ladder {
    pub fn contains(&self, p: Vec3) -> bool {
        let d = (p - self.trigger_center).abs();
        d.x <= self.trigger_half_extents.x
            && d.y <= self.trigger_half_extents.y
            && d.z <= self.trigger_half_extents.z
    }
}

/// A destructible archery/melee practice target (owner: World — its death
/// reaction lives here, per `docs/ARCHITECTURE.md`).
#[derive(Component)]
pub struct PracticeTarget;

pub(crate) const PRACTICE_TARGET_HP: f32 = 30.0;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<day_night::TimeOfDay>();
        app.add_message::<day_night::TimeOfDayRequest>();
        // Scene content, not startup content (`crate::scene`): the sky and the
        // ground belong to every walkable scene, the graybox layout only to
        // `Playing`. Everything spawned here carries `DespawnOnExit`, so leaving
        // the state tears it down with no cleanup system to keep in sync.
        // One registration per scene, all driven by the same table
        // (`crate::scene::SCENES`): the sky and the ground are in every scene,
        // and each optional piece is gated by what that scene's row declares.
        // Adding a scene is a row there, not a branch here.
        for id in crate::scene::SceneId::ALL {
            app.add_systems(
                OnEnter(AppState::Scene(id)),
                (
                    layout::setup_sky,
                    terrain::setup_terrain,
                    day_night::setup_moon_light,
                )
                    .in_set(crate::scene::SceneBuild::Ground),
            );
            app.add_systems(
                OnEnter(AppState::Scene(id)),
                (
                    layout::setup_course.run_if(crate::scene::scene_has(|c| c.course)),
                    layout::setup_stairs.run_if(crate::scene::scene_has(|c| c.stairs)),
                    layout::setup_targets.run_if(crate::scene::scene_has(|c| c.targets)),
                    layout::setup_pickups.run_if(crate::scene::scene_has(|c| c.pickups)),
                    layout::setup_forest.run_if(crate::scene::scene_has(|c| c.forest)),
                )
                    // `Actors`, not `Ground`: every one of these reads the
                    // terrain to sit on it, and in `Ground` the terrain is a
                    // queued command that has not spawned yet. That phase split
                    // already existed for the player; the graybox never used it,
                    // which is why sculpting under the course left it hanging.
                    .in_set(crate::scene::SceneBuild::Actors),
            );
        }
        app.add_systems(
            FixedUpdate,
            (day_night::apply_time_requests, day_night::advance_time).chain(),
        );
        app.add_systems(
            Update,
            (
                day_night::apply_sun,
                day_night::place_sky_discs,
                day_night::apply_cascade_config,
                day_night::apply_shadow_map_size,
            ),
        );
        // In `FixedUpdate`, not `Update`: avian syncs collider AABBs and the
        // query pipeline in `FixedPostUpdate`, so rebuilding here (earlier in the
        // same fixed tick) lands the new shape in that tick's physics instead of
        // trailing it by a frame or more — which is why sculpted ground used to
        // sometimes not collide.
        app.add_systems(FixedUpdate, terrain::rebuild_terrain_collider);
        app.add_systems(
            FixedUpdate,
            despawn_dead_targets.after(crate::health::HealthSet::Apply),
        );
    }
}

fn despawn_dead_targets(
    mut commands: Commands,
    mut deaths: MessageReader<crate::health::DeathMessage>,
    targets: Query<Option<&Name>, With<PracticeTarget>>,
) {
    for death in deaths.read() {
        let Ok(name) = targets.get(death.entity) else {
            continue;
        };
        info!(
            "[world] {} destroyed",
            name.map(Name::as_str).unwrap_or("practice target")
        );
        commands.entity(death.entity).despawn();
    }
}
