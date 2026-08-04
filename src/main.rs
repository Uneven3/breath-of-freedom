//! Breath of Freedom.
//!
//! Architecture: Brain → Intents → Broker → Motors / Services → Body,
//! expressed as ordered `FixedUpdate` system sets, with Avian3d providing the
//! kinematic `move_and_slide` and spatial queries (see
//! `docs/ARCHITECTURE.md`).

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::float_cmp,
        clippy::unwrap_used,
        reason = "tests assert exact authored constants and may panic on broken fixtures"
    )
)]

mod asset_pipeline;
mod camera;
mod combat;
mod debug;
mod editor;
mod enemies;
mod health;
mod input;
mod interaction;
mod inventory;
mod mounts;
mod movement;
mod perf;
mod player;
mod presentation;
mod projectiles;
mod scene;
mod sfx;
mod time_control;
mod visuals;
mod world;

use avian3d::prelude::*;
use bevy::prelude::*;
use bof_simulation::SimulationPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(SimulationPlugin)
        // Collider-wireframe rendering; starts disabled, toggled with F1
        // (see `debug.rs`).
        .add_plugins(PhysicsDebugPlugin)
        .add_plugins(asset_pipeline::AssetPipelinePlugin)
        .add_plugins((
            world::WorldPlugin,
            input::InputPlugin,
            movement::MovementPlugin,
            mounts::MountsPlugin,
            combat::CombatPlugin,
            projectiles::ProjectilesPlugin,
            health::HealthPlugin,
            inventory::InventoryPlugin,
            enemies::EnemiesPlugin,
            player::PlayerPlugin,
            camera::CameraPlugin,
            visuals::VisualsPlugin,
            debug::DebugPlugin,
            presentation::PresentationPlugin,
            sfx::SfxPlugin,
        ))
        // Separate call: `add_plugins` tuples cap at 15 elements.
        .add_plugins((
            perf::PerfPlugin,
            interaction::InteractionPlugin,
            time_control::TimeControlPlugin,
            editor::EditorPlugin,
            scene::ScenePlugin,
        ))
        .run();
}
