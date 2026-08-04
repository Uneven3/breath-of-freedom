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
mod debug;
mod editor;
mod input;
mod inventory;
mod perf;
mod presentation;
mod scene;
mod sfx;
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
            camera::CameraPlugin,
            visuals::VisualsPlugin,
            debug::DebugPlugin,
            presentation::PresentationPlugin,
            sfx::SfxPlugin,
            perf::PerfPlugin,
            editor::EditorPlugin,
            scene::ScenePlugin,
        ))
        .run();
}
