//! Pure projectile state, messages and schedule contracts.

use avian3d::prelude::*;
use bevy::prelude::*;

pub(super) const ARROW_STEALTH_MULT: f32 = 4.0;
pub(super) const ARROW_KNOCKBACK: f32 = 2.5;
pub(super) const FLIGHT_TTL_SECS: f32 = 8.0;
pub(super) const STUCK_TTL_SECS: f32 = 4.0;
pub(super) const ARROW_POOL_SIZE: u8 = 64;

#[derive(Message, Debug, Clone, Copy)]
pub struct SpawnProjectileMessage {
    pub shooter: Entity,
    pub origin: Vec3,
    pub velocity: Vec3,
    pub damage: f32,
}

#[derive(Component)]
pub struct Arrow {
    pub(super) active: bool,
    pub(super) velocity: Vec3,
    pub(super) shooter: Entity,
    pub(super) damage: f32,
    pub(super) remaining: f32,
    pub(super) stuck: bool,
    pub(super) trail_timer: f32,
    pub(super) filter: SpatialQueryFilter,
}

impl Arrow {
    pub(super) fn pooled() -> Self {
        let mut filter = SpatialQueryFilter::default();
        filter.excluded_entities.reserve(1);
        Self {
            active: false,
            velocity: Vec3::ZERO,
            shooter: Entity::PLACEHOLDER,
            damage: 0.0,
            remaining: 0.0,
            stuck: false,
            trail_timer: 0.0,
            filter,
        }
    }

    pub(super) fn deactivate(&mut self) {
        self.active = false;
        self.remaining = 0.0;
        self.stuck = false;
        self.trail_timer = 0.0;
    }
}

#[derive(Component)]
pub(super) struct ArrowPoolSlot(pub u8);

#[derive(Message, Clone, Copy)]
pub(super) struct ArrowTrailMessage(pub Vec3);

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProjectilesSet {
    Simulate,
}
