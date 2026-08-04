//! Requests applied by Movement on behalf of other simulation systems.

use bevy_ecs::prelude::*;
use bevy_math::Vec3;

/// A one-shot velocity impulse on an actor body.
#[derive(Message, Debug, Clone, Copy)]
pub struct BodyImpulseMessage {
    pub entity: Entity,
    pub impulse: Vec3,
}
