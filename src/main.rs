//! Breath of Freedom.
//!
//! Architecture: Brain → Intents → Broker → Motors / Services → Body,
//! expressed as ordered `FixedUpdate` system sets, with Avian3d providing the
//! kinematic `move_and_slide` and spatial queries (see
//! `docs/ARCHITECTURE.md`).

mod camera;
mod combat;
mod debug;
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
mod proposal;
mod sfx;
mod visuals;
mod world;

use avian3d::prelude::*;
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PhysicsPlugins::default())
        // Collider-wireframe rendering; starts disabled, toggled with F1
        // (see `debug.rs`).
        .add_plugins(PhysicsDebugPlugin)
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
        .add_plugins((perf::PerfPlugin, interaction::InteractionPlugin))
        .run();
}
