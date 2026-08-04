//! Simulation lifecycle for the domain-owned sensor trace.

use bevy_ecs::prelude::*;

pub use bof_domain::movement::diag::{CastKind, CastTrace};

/// Clears the trace right before the services sense the world.
pub fn clear_cast_trace(mut trace: ResMut<CastTrace>) {
    trace.records.clear();
}
