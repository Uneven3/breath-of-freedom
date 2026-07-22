//! Pure state for authoritative simulation-time control.

use bevy::prelude::*;

/// Remaining hitstop measured in real seconds while virtual time is paused.
#[derive(Resource, Default)]
pub(super) struct Hitstop {
    pub remaining: f32,
}
