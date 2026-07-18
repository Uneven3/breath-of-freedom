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
pub mod layout;
mod spawn;

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
        app.add_systems(Startup, layout::setup_world);
        app.add_systems(FixedUpdate, day_night::advance_time);
        app.add_systems(Update, day_night::apply_sun);
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
