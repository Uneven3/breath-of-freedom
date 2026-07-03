use bevy::prelude::Component;

/// Component attached to actors to track the last logged continuous SFX state.
#[derive(Component, Debug, Clone)]
pub struct ContinuousSfxTracker {
    pub last_speed: f32,
    pub last_stamina: f32,
}
