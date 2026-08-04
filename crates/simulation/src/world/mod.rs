//! World: the ground itself and the semantics authored geometry presents.
//!
//! What lives here is what simulation reads to move a body: the heightfield and
//! its per-cell meaning, plus the marker types that say what a piece of geometry
//! *is* (a staircase, a ladder, an unclimbable wall, the surface underfoot).
//! How that geometry is built and drawn is composition and stays with the app —
//! `world::layout` decides what the level contains, `world::spawn` turns a spec
//! into entities, and the binary is the only layer allowed to see both crates.

use bevy_app::{App, FixedUpdate, Plugin};
use bevy_ecs::prelude::*;
use bevy_log::info;
use bevy_math::prelude::*;

pub mod day_night;
pub mod terrain;
pub mod terrain_kind;

pub use terrain::{Terrain, TerrainAccess, TerrainSnapshot, spawn_terrain};
pub use terrain_kind::TerrainKind;

pub use crate::physics::GameLayer;
pub use bof_domain::world::WORLD_SIZE;

use crate::health::{DeathMessage, HealthSet};

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
pub struct Surface(pub bof_domain::asset_pipeline::schema::SurfaceKind);

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

pub const PRACTICE_TARGET_HP: f32 = 30.0;

/// Installs the ground: terrain collider upkeep and world-owned death rules.
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        // In `FixedUpdate`, not `Update`: avian syncs collider AABBs and the
        // query pipeline in `FixedPostUpdate`, so rebuilding here (earlier in the
        // same fixed tick) lands the new shape in that tick's physics instead of
        // trailing it by a frame or more — which is why sculpted ground used to
        // sometimes not collide.
        app.add_plugins(day_night::DayNightClockPlugin);
        app.add_systems(FixedUpdate, terrain::rebuild_terrain_collider);
        app.add_systems(FixedUpdate, despawn_dead_targets.after(HealthSet::Apply));
    }
}

fn despawn_dead_targets(
    mut commands: Commands,
    mut deaths: MessageReader<DeathMessage>,
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
