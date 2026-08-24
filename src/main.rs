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
mod movement_tuning;
mod perf;
mod presentation;
mod scene;
mod sfx;
mod visuals;
mod world;

use avian3d::prelude::*;
use bevy::prelude::*;
use bof_simulation::SimulationPlugin;

/// **El editor y el juego son contextos distintos, y se separan acá.**
///
/// `BOF_MODE` no apaga el editor: decide si existe. En modo juego
/// `EditorPlugin` no se instala, así que no hay recursos de autoría, ni HUD de
/// pincel, ni F5 — el juego sólo tiene sus herramientas de diagnóstico (F1, F3,
/// F7). Un modo que sólo desactivara sistemas dejaría el estado igual y la
/// tecla viva; no instalarlo es lo que hace la separación real.
fn main() -> AppExit {
    let mode = scene::configured_app_mode();
    let mut app = App::new();
    app.insert_resource(mode);
    app.add_plugins(DefaultPlugins)
        // `presentation::theme::ThemePlugin` (dentro de `PresentationPlugin`,
        // más abajo) inserta el `UiTheme` real, parchado con la paleta del
        // juego — tiene que registrarse después de `FeathersPlugins` para
        // pisar el tema oscuro genérico que trae por default.
        .add_plugins(bevy::feathers::FeathersPlugins)
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
            scene::ScenePlugin,
        ));
    // Después de `DefaultPlugins`: el parser reporta lo que aplicó y lo que
    // rechazó, y antes de esa línea el `LogPlugin` de Bevy todavía no existe —
    // los avisos se perderían justo cuando más importan.
    app.insert_resource(movement_tuning::configured_tuning());
    if mode == scene::AppMode::Editor {
        app.add_plugins(editor::EditorPlugin);
    }
    app.run()
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
