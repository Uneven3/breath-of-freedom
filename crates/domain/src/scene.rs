use bevy_ecs::prelude::*;

/// Content whose lifetime belongs to whichever playable scene creates it.
///
/// Simulation declares the lifetime without knowing the app's state enum. The
/// app layer binds this marker to Bevy's state-specific cleanup component.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SceneScoped;
