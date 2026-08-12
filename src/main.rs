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

fn main() -> AppExit {
    App::new()
        .add_plugins(DefaultPlugins)
        // Spike (2026-08-12): probar el patrón real de bevy_feathers —
        // BSN/spawn_scene — sobre un solo botón antes de comprometer el hub
        // entero. Ver `presentation::debug_ui`. El tema oscuro es explícito
        // porque el resto del hub ya es oscuro (`presentation::theme`); sin
        // esto Feathers cae a lo que declare `UiTheme::default`.
        .add_plugins(bevy::feathers::FeathersPlugins)
        .insert_resource(bevy::feathers::theme::UiTheme(
            bevy::feathers::dark_theme::create_dark_theme(),
        ))
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
        .run()
}

#[cfg(test)]
mod entrypoint_tests {
    use super::*;

    /// Returning `()` silently turns every `AppExit::Error` emitted by an
    /// automation into process exit code 0. The return type is the bridge to
    /// Rust's `Termination` implementation for `AppExit`.
    #[test]
    fn the_entrypoint_propagates_the_apps_exit_status() {
        let _: fn() -> AppExit = main;
    }
}
