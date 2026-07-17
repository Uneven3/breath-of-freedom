use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;

use super::control::ControlMask;

#[derive(Message, Debug, Clone, Copy)]
pub enum ActorLinkRequestMessage {
    Attach {
        controller: Entity,
        controlled: Entity,
        local_pose: Transform,
        mask: ControlMask,
    },
    Detach {
        controller: Entity,
        controlled: Entity,
        world_pose: Transform,
        inherited_velocity: Vec3,
        safety: DetachSafety,
        force: bool,
    },
    Neutralize {
        actor: Entity,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetachSafety {
    Validated,
    NeedsRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorLinkRejection {
    MissingController,
    MissingControlled,
    SelfLink,
    ControllerBusy,
    ControlledBusy,
    ChainOrCycle,
    InconsistentLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorLinkStatus {
    Accepted,
    Rejected(ActorLinkRejection),
}

#[derive(Message, Debug, Clone, Copy)]
pub struct ActorLinkResultMessage {
    pub request: ActorLinkRequestMessage,
    pub status: ActorLinkStatus,
}

#[derive(Resource, Default)]
pub struct ActorLinkWorkspace {
    pub(crate) controllers: EntityHashSet,
    pub(crate) controlled: EntityHashSet,
    pub(crate) attached: EntityHashSet,
    pub(crate) carriers: EntityHashSet,
    pub(crate) processed: EntityHashSet,
}
