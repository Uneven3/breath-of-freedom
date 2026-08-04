//! Outcomes published by Combat and consumed across crate boundaries.

use bevy_ecs::prelude::*;
use bevy_math::Vec3;

/// Published when an accepted attack produces impact feedback.
#[derive(Message, Debug, Clone, Copy)]
pub struct HitImpactMessage {
    pub target: Entity,
    pub attacker: Entity,
    pub position: Vec3,
    pub damage: f32,
    pub critical: bool,
    pub melee: bool,
}

/// Published the tick an arrow leaves the string.
#[derive(Message, Debug, Clone, Copy)]
pub struct BowFiredMessage {
    pub shooter: Entity,
    /// Draw charge at release, `0.0..=1.0`.
    pub charge: f32,
}
