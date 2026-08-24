//! World composition: what the level contains and how a piece of it is built.
//!
//! The ground itself — the heightfield, its per-cell meaning, the authored
//! markers and the clock — lives in `bof_simulation::world`. What stays here is
//! composition, which is the binary's job: [`layout`] declares *what* the level
//! contains, [`spawn`] turns a spec into entities (mesh + collider + markers),
//! [`forest`] scatters trees, and [`day_night`] draws the sky the clock implies.
//! Only this layer may touch both crates at once, which is exactly why building
//! a collider and a mesh in the same function belongs here and nowhere else.

use bevy::prelude::*;

pub mod crags;
pub mod day_night;
pub mod debris;
pub mod forest;
pub mod layout;
// Herramienta de autoría: esculpe la meseta y reescribe el nivel en disco. No
// es código de juego —el `.ron` es el nivel— así que vive con los tests.
#[cfg(test)]
pub mod plateau;
mod spawn;

pub use forest::TreeKind;

pub use bof_domain::props::PropKind;
pub use bof_simulation::world::{
    GameLayer, Ladder, NonClimbable, PRACTICE_TARGET_HP, PracticeTarget, Stairs, Surface, Terrain,
    TerrainAccess, TerrainKind, TerrainSnapshot,
};

pub struct WorldPlugin;

/// Presentation state that other render-facing systems consume in the same
/// frame. Keeping this boundary explicit prevents consumers from observing the
/// previous frame's light while the clock has already advanced.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum WorldPresentationSet {
    Lighting,
}

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        // El gemelo de simulación lo instala `SimulationPlugin`, no esto: Bevy
        // panickea ante un plugin repetido, y desde 6.8 el crate arma su propio
        // grafo. Este plugin es sólo la mitad de presentación y composición.
        //
        // Scene content, not startup content (`crate::scene`): the sky belongs
        // to every walkable scene, the graybox layout only to the scenes that
        // declare it. Everything spawned here carries `DespawnOnExit`, so
        // leaving the state tears it down with no cleanup system to keep in
        // sync. One registration per scene, all driven by the same table
        // (`crate::scene::SCENES`): adding a scene is a row there, not a branch
        // here.
        for id in crate::scene::SceneId::ALL {
            app.add_systems(
                OnEnter(AppState::Scene(id)),
                (layout::setup_sky, day_night::setup_moon_light)
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
                    crags::setup_crags.run_if(crate::scene::scene_has(|c| c.crags)),
                    debris::setup_debris.run_if(crate::scene::scene_has(|c| c.crags)),
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
            Update,
            (
                day_night::apply_sun.in_set(WorldPresentationSet::Lighting),
                day_night::place_sky_discs,
                day_night::apply_cascade_config,
                day_night::apply_shadow_map_size,
            ),
        );
    }
}

use crate::scene::AppState;
