//! Movement — the pipeline lives in `bof_simulation`; what stays here are the
//! sensors that still read the app's world.
//!
//! The Broker (intents → LOD → sense → propose → arbitrate → tick) and every
//! motor moved to `bof_simulation::movement`. The four services below publish
//! the fact components those motors consume, and they read `TerrainAccess` plus
//! authored geometry (`Stairs`, `Ladder`, `NonClimbable`, `Surface`), which have
//! not crossed the crate boundary yet. They follow `world` in `CRATES.md` 6.7,
//! and this plugin disappears with them.

use bevy::prelude::*;

pub mod services;

pub use bof_domain::movement::{Actor, ActorId, BodyVelocity, Player};
pub use bof_simulation::movement::MovementSet;

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bof_simulation::movement::MovementMotorsPlugin);

        app.add_systems(
            FixedUpdate,
            (
                services::ground::ground_service,
                services::ledge::ledge_service,
                services::stairs::stairs_service,
                services::ladder::ladder_service,
            )
                .in_set(MovementSet::SenseWorld),
        );
        // After the motors move the body, before attachments follow it: an actor
        // that ended up inside the terrain is lifted back onto the surface. The
        // downward probe cannot catch this — it finds ground right there and
        // reports the body comfortably grounded while it sits under the floor.
        app.add_systems(
            FixedUpdate,
            services::ground::lift_actors_out_of_terrain
                .after(MovementSet::TickActiveMotor)
                .before(MovementSet::SyncAttachments),
        );
    }
}

#[cfg(test)]
mod determinism_tests;

/// Compatibility paths for the simulation-owned movement modules. They keep the
/// app's `crate::movement::X` call sites working while the remaining modules
/// migrate; each disappears with its last consumer.
pub mod abilities {
    pub use bof_simulation::movement::abilities::*;
}
pub mod attachment {
    pub use bof_simulation::movement::attachment::*;
}
#[cfg(test)]
pub mod attachment_systems {
    pub use bof_simulation::movement::attachment_systems::*;
}
pub mod body {
    pub use bof_simulation::movement::body::*;
}
pub mod brain {
    pub use bof_simulation::movement::brain::*;
}
pub mod bundles {
    pub use bof_simulation::movement::bundles::*;
}
pub mod constraints {
    pub use bof_simulation::movement::constraints::*;
}
pub mod control {
    pub use bof_simulation::movement::control::*;
}
pub mod diag {
    pub use bof_simulation::movement::diag::*;
}
pub mod facing {
    pub use bof_simulation::movement::facing::*;
}
pub mod facts {
    pub use bof_simulation::movement::facts::*;
}
pub mod intents {
    pub use bof_simulation::movement::intents::*;
}
pub mod link {
    pub use bof_simulation::movement::link::*;
}
pub mod lod {
    pub use bof_simulation::movement::lod::*;
}
pub mod motor_common {
    pub use bof_simulation::movement::motor_common::*;
}
pub mod motors {
    pub use bof_simulation::movement::motors::*;
}
pub mod probe_data {
    pub use bof_simulation::movement::probe_data::*;
}
pub mod proposal {
    pub use bof_simulation::movement::proposal::*;
}
pub mod sensing {
    pub use bof_simulation::movement::sensing::*;
}
pub mod stamina {
    pub use bof_simulation::movement::stamina::*;
}
pub mod state {
    pub use bof_simulation::movement::state::*;
}
