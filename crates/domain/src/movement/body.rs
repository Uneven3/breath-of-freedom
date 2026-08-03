use bevy_ecs::prelude::Component;

/// Semantic capsule measurements shared across simulation and presentation.
/// Physical collider construction stays in the simulation crate.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BodyDimensions {
    pub radius: f32,
    pub standing_capsule_length: f32,
    pub crouched_capsule_length: f32,
}

impl BodyDimensions {
    pub const PLAYER: Self = Self {
        radius: 0.5,
        standing_capsule_length: 1.0,
        crouched_capsule_length: 0.2,
    };

    pub fn standing_half_height(self) -> f32 {
        self.radius + self.standing_capsule_length / 2.0
    }

    pub fn crouched_half_height(self) -> f32 {
        self.radius + self.crouched_capsule_length / 2.0
    }
}
