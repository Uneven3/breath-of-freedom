//! Per-actor capsule dimensions used by movement simulation.

use avian3d::prelude::*;
use bevy::prelude::*;

/// Semantic capsule measurements for a kinematic movement actor.
///
/// `Collider` remains the physical shape used by Avian. This component keeps
/// the measurements Movement needs for feet, ledges, ladders, stairs, and the
/// Sneak collider transition.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BodyDimensions {
    pub radius: f32,
    /// Capsule cylinder length while standing, excluding hemispheres.
    pub standing_capsule_length: f32,
    /// Capsule cylinder length while crouched, excluding hemispheres.
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

    pub fn standing_collider(self) -> Collider {
        Collider::capsule(self.radius, self.standing_capsule_length)
    }

    pub fn crouched_collider(self) -> Collider {
        Collider::capsule(self.radius, self.crouched_capsule_length)
    }
}
