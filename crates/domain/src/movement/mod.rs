use bevy_ecs::prelude::Component;
use bevy_math::Vec3;

pub mod abilities;
pub mod body;
pub mod diag;
pub mod facing;
pub mod facts;
pub mod intents;
pub mod probe_data;
pub mod proposal;
pub mod sensing;
pub mod stamina;
pub mod state;

/// Marker for the local player entity.
#[derive(Component)]
pub struct Player;

/// Marker for any movement-capable simulation actor.
#[derive(Component)]
pub struct Actor;

/// Stable authored simulation identity; never derived from Bevy's transient
/// entity allocation order.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActorId(u32);

impl ActorId {
    pub const PLAYER: Self = Self(1);
    pub const HORSE: Self = Self(2);
    pub const BOKOBO_MELEE: Self = Self(100);
    pub const BOKOBO_ARCHER: Self = Self(101);
    pub const TRAVERSAL_PROBE: Self = Self(1_000);

    pub const fn authored(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Velocity integrated by Movement's kinematic controller.
#[derive(Component, Default)]
pub struct BodyVelocity(pub Vec3);
