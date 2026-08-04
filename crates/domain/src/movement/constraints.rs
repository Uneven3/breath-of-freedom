//! Requests applied by Movement on behalf of other simulation systems.

use bevy_ecs::prelude::*;
use bevy_math::Vec3;

/// A one-shot velocity impulse on an actor body.
#[derive(Message, Debug, Clone, Copy)]
pub struct BodyImpulseMessage {
    pub entity: Entity,
    pub impulse: Vec3,
}

/// Per-actor constraint facts for this tick, derived from the messages that
/// other domains emit. Motors read this like any other fact; only
/// `apply_locomotion_constraints` writes it.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct LocomotionConstraintFacts {
    pub forbid_sprint: bool,
}
