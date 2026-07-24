//! Pooled ballistic projectiles with simulation/presentation separation.

use bevy::prelude::*;

mod data;
mod simulation;

// `Arrow`/`ArrowTrailMessage` are the read-only contract the visual layer
// consumes; the disposable arrow meshes live in `visuals::arrows` so no
// simulation module depends on presentation (§20).
pub use data::{Arrow, ArrowTrailMessage, ProjectilesSet, SpawnProjectileMessage};

pub struct ProjectilesPlugin;

impl Plugin for ProjectilesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpawnProjectileMessage>();
        app.add_message::<ArrowTrailMessage>();
        app.add_systems(Startup, simulation::init_pool);
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
    }
}
