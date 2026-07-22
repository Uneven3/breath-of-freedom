//! Pooled ballistic projectiles with simulation/presentation separation.

use bevy::prelude::*;

mod data;
mod presentation;
mod simulation;

pub use data::{ProjectilesSet, SpawnProjectileMessage};

pub struct ProjectilesPlugin;

impl Plugin for ProjectilesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpawnProjectileMessage>();
        app.add_message::<data::ArrowTrailMessage>();
        app.add_systems(Startup, (presentation::init_assets, simulation::init_pool));
        app.configure_sets(
            FixedUpdate,
            ProjectilesSet::Simulate.after(crate::combat::CombatSet::EmitConstraints),
        );
        app.add_systems(
            FixedUpdate,
            (simulation::spawn_arrows, simulation::fly_arrows)
                .chain()
                .in_set(ProjectilesSet::Simulate),
        );
        app.add_systems(
            Update,
            (
                presentation::sync_visuals,
                presentation::spawn_trails,
                presentation::tick_trails,
            )
                .chain(),
        );
    }
}
