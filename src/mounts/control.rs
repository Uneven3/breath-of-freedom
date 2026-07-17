use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::movement::control::ControlMask;
use crate::movement::link::{ActorLinkRequestMessage, DetachSafety};

#[derive(SystemParam)]
pub struct MountControlWriters<'w> {
    pub link: MessageWriter<'w, ActorLinkRequestMessage>,
}

impl MountControlWriters<'_> {
    pub fn attach(&mut self, rider: Entity, horse: Entity, local_pose: Transform) {
        self.link.write(ActorLinkRequestMessage::Attach {
            controller: rider,
            controlled: horse,
            local_pose,
            mask: ControlMask::MOUNT,
        });
    }

    pub fn detach(
        &mut self,
        rider: Entity,
        horse: Entity,
        world_pose: Transform,
        inherited_velocity: Vec3,
        safety: DetachSafety,
        force: bool,
    ) {
        self.link.write(ActorLinkRequestMessage::Detach {
            controller: rider,
            controlled: horse,
            world_pose,
            inherited_velocity,
            safety,
            force,
        });
    }

    pub fn neutralize(&mut self, actor: Entity) {
        self.link
            .write(ActorLinkRequestMessage::Neutralize { actor });
    }
}
