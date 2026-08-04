//! Pure perception state and cross-system threat contract.

use bevy_ecs::prelude::*;
use bevy_math::Vec3;

/// How aware an enemy is of a threat, `0.0..=1.0`.
#[derive(Component, Default)]
pub struct Awareness(pub f32);

impl Awareness {
    pub const ALERTED: f32 = 1.0;
    pub const SUSPICIOUS: f32 = 0.35;

    pub fn is_alerted(&self) -> bool {
        self.0 >= Self::ALERTED
    }

    pub fn is_suspicious(&self) -> bool {
        self.0 >= Self::SUSPICIOUS
    }
}

/// An unmistakable threat aimed at one enemy.
#[derive(Message, Debug, Clone, Copy)]
pub struct DirectThreatMessage {
    pub enemy: Entity,
    pub threat_position: Vec3,
}
