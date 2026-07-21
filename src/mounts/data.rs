use bevy::prelude::*;

/// Per-horse charge phase. The generation changes on every activation so a
/// target can be hit once per charge and becomes eligible again after rearm.
#[derive(Component, Debug, Clone, Copy)]
pub struct HorseCharge {
    pub active: bool,
    pub generation: u64,
    pub previous_position: Vec3,
}

impl HorseCharge {
    pub const fn new(position: Vec3) -> Self {
        Self {
            active: false,
            generation: 0,
            previous_position: position,
        }
    }
}

#[derive(Component)]
pub struct Horse;

#[derive(Component, Clone, Copy)]
pub struct MountedOn(pub Entity);

#[derive(Component, Clone, Copy, Default)]
pub struct RiddenBy(pub Option<Entity>);

#[derive(Component, Clone, Copy, Default)]
pub struct HorseOwner(pub Option<Entity>);

#[derive(Component)]
pub struct PendingHorseDespawn;

#[derive(Message, Clone, Copy)]
pub enum MountDebugRequest {
    ToggleHorse,
}

#[derive(Message, Clone, Copy)]
pub enum MountTransitionRequest {
    Mount {
        rider: Entity,
        horse: Entity,
    },
    Dismount {
        rider: Entity,
        horse: Entity,
        forced: bool,
    },
}
