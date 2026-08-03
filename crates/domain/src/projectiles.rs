use bevy_ecs::prelude::{Component, Entity, Message};
use bevy_math::Vec3;

#[derive(Message, Debug, Clone, Copy)]
pub struct SpawnProjectileMessage {
    pub shooter: Entity,
    pub origin: Vec3,
    pub velocity: Vec3,
    pub damage: f32,
}

/// Pool lifecycle visible to presentation without exposing the physics query
/// filter or ballistic state owned by simulation.
#[derive(Component, Default)]
pub struct ProjectileState {
    active: bool,
}

impl ProjectileState {
    pub fn active(&self) -> bool {
        self.active
    }

    pub fn activate(&mut self) {
        self.active = true;
    }

    pub fn deactivate(&mut self) {
        self.active = false;
    }
}

#[derive(Message, Clone, Copy)]
pub struct ArrowTrailMessage(pub Vec3);
