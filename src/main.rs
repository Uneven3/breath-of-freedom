//! Breath of Freedom.
//!
//! Architecture: Brain → Intents → Broker → Motors / Services → Body,
//! expressed as ordered `FixedUpdate` system sets, with Avian3d providing the
//! kinematic `move_and_slide` and spatial queries (see
//! `docs/architecture/movement.md`).

mod camera;
mod debug;
mod movement;
mod presentation;
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
        .add_plugins((
            world::WorldPlugin,
            movement::MovementPlugin,
            camera::CameraPlugin,
            visuals::VisualsPlugin,
            debug::DebugPlugin,
            sfx::SfxPlugin,
        ))
        .add_message::<presentation::cues::CueMessage>()
        .run();
}
