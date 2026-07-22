use avian3d::prelude::ColliderDisabled;
use bevy::prelude::*;

use super::attachment::{KinematicAttachment, LocomotionEnabled, PendingSafeRecovery};
use super::control::ControlRedirect;
use super::intents::Intents;
use super::link::{
    ActorLinkRejection, ActorLinkRequestMessage, ActorLinkResultMessage, ActorLinkStatus,
    ActorLinkWorkspace, DetachSafety,
};
use super::{Actor, BodyVelocity};

pub use super::attachment_recovery::{recover_orphaned_attachments, recover_pending_safe_poses};

type ActorLinks<'a> = (
    Entity,
    Option<&'a KinematicAttachment>,
    Option<&'a ControlRedirect>,
);

/// Grows scratch storage before the fixed-step hot path. `clear` in
/// `apply_actor_link_requests` retains this capacity, so link processing does
/// not call the allocator even as the actor population grows.
pub fn prepare_actor_link_workspace(
    actors: Query<Entity, With<Actor>>,
    mut workspace: ResMut<ActorLinkWorkspace>,
) {
    let actor_count = actors.iter().count();
    reserve_for_actor_count(&mut workspace.controllers, actor_count);
    reserve_for_actor_count(&mut workspace.controlled, actor_count);
    reserve_for_actor_count(&mut workspace.attached, actor_count);
    reserve_for_actor_count(&mut workspace.carriers, actor_count);
    reserve_for_actor_count(&mut workspace.processed, actor_count);
}

fn reserve_for_actor_count(set: &mut bevy::ecs::entity::EntityHashSet, actor_count: usize) {
    // `HashSet::reserve` guarantees capacity for `len + additional`, not for
    // `capacity + additional`. The workspace still contains the previous
    // tick here, so subtracting capacity could under-reserve before Fixed.
    let additional = actor_count.saturating_sub(set.len());
    set.reserve(additional);
}

pub fn apply_actor_link_requests(
    mut commands: Commands,
    mut requests: MessageReader<ActorLinkRequestMessage>,
    mut results: MessageWriter<ActorLinkResultMessage>,
    mut workspace: ResMut<ActorLinkWorkspace>,
    actors: Query<ActorLinks, With<Actor>>,
    mut bodies: Query<(&mut Transform, &mut BodyVelocity, &mut Intents), With<Actor>>,
) {
    if requests.is_empty() {
        return;
    }

    workspace.controllers.clear();
    workspace.controlled.clear();
    workspace.attached.clear();
    workspace.carriers.clear();
    workspace.processed.clear();
    for (entity, attachment, redirect) in &actors {
        if let Some(attachment) = attachment {
            workspace.attached.insert(entity);
            workspace.carriers.insert(attachment.carrier);
        }
        if let Some(redirect) = redirect {
            workspace.controllers.insert(entity);
            workspace.controlled.insert(redirect.controlled);
        }
    }

    for request in requests.read().copied() {
        let status = match request {
            ActorLinkRequestMessage::Attach {
                controller,
                controlled,
                local_pose,
                mask,
            } => {
                let rejection = if workspace.processed.contains(&controller) {
                    Some(ActorLinkRejection::ControllerBusy)
                } else if controller == controlled {
                    Some(ActorLinkRejection::SelfLink)
                } else if actors.get(controller).is_err() {
                    Some(ActorLinkRejection::MissingController)
                } else if actors.get(controlled).is_err() {
                    Some(ActorLinkRejection::MissingControlled)
                } else if workspace.controllers.contains(&controller)
                    || workspace.attached.contains(&controller)
                    || workspace.controlled.contains(&controller)
                {
                    Some(ActorLinkRejection::ControllerBusy)
                } else if workspace.controlled.contains(&controlled) {
                    Some(ActorLinkRejection::ControlledBusy)
                } else if workspace.attached.contains(&controlled)
                    || workspace.carriers.contains(&controller)
                    || workspace.controllers.contains(&controlled)
                {
                    Some(ActorLinkRejection::ChainOrCycle)
                } else {
                    None
                };
                if let Some(rejection) = rejection {
                    ActorLinkStatus::Rejected(rejection)
                } else {
                    workspace.controllers.insert(controller);
                    workspace.controlled.insert(controlled);
                    workspace.attached.insert(controller);
                    workspace.carriers.insert(controlled);
                    workspace.processed.insert(controller);
                    commands.entity(controller).insert((
                        KinematicAttachment {
                            carrier: controlled,
                            local_pose,
                        },
                        ControlRedirect { controlled, mask },
                        ColliderDisabled,
                    ));
                    commands.entity(controller).remove::<LocomotionEnabled>();
                    ActorLinkStatus::Accepted
                }
            }
            ActorLinkRequestMessage::Detach {
                controller,
                controlled,
                world_pose,
                inherited_velocity,
                safety,
                force,
            } => {
                if workspace.processed.contains(&controller) {
                    results.write(ActorLinkResultMessage {
                        request,
                        status: ActorLinkStatus::Rejected(ActorLinkRejection::ControllerBusy),
                    });
                    continue;
                }
                let Ok((_, attachment, redirect)) = actors.get(controller) else {
                    results.write(ActorLinkResultMessage {
                        request,
                        status: ActorLinkStatus::Rejected(ActorLinkRejection::MissingController),
                    });
                    continue;
                };
                let consistent = attachment.is_some_and(|value| value.carrier == controlled)
                    && redirect.is_some_and(|value| value.controlled == controlled);
                if !consistent && !force {
                    ActorLinkStatus::Rejected(ActorLinkRejection::InconsistentLink)
                } else if bodies.get(controller).is_ok() {
                    if let Ok((mut transform, mut velocity, mut intents)) =
                        bodies.get_mut(controller)
                    {
                        *transform = world_pose;
                        velocity.0 = inherited_velocity;
                        *intents = Intents::default();
                    }
                    if let Ok((_, _, mut controlled_intents)) = bodies.get_mut(controlled) {
                        *controlled_intents = Intents::default();
                    }
                    commands
                        .entity(controller)
                        .remove::<(KinematicAttachment, ControlRedirect)>();
                    match safety {
                        DetachSafety::Validated => {
                            commands
                                .entity(controller)
                                .remove::<(ColliderDisabled, PendingSafeRecovery)>();
                            commands.entity(controller).insert(LocomotionEnabled);
                        }
                        DetachSafety::NeedsRecovery => {
                            commands.entity(controller).insert((
                                ColliderDisabled,
                                PendingSafeRecovery {
                                    origin: world_pose.translation,
                                    rotation: world_pose.rotation,
                                    next_height: 0.0,
                                },
                            ));
                            commands.entity(controller).remove::<LocomotionEnabled>();
                        }
                    }
                    workspace.controllers.remove(&controller);
                    workspace.controlled.remove(&controlled);
                    workspace.attached.remove(&controller);
                    workspace.carriers.remove(&controlled);
                    workspace.processed.insert(controller);
                    ActorLinkStatus::Accepted
                } else {
                    ActorLinkStatus::Rejected(ActorLinkRejection::MissingController)
                }
            }
            ActorLinkRequestMessage::Neutralize { actor } => {
                if let Ok((_, _, mut intents)) = bodies.get_mut(actor) {
                    *intents = Intents::default();
                    ActorLinkStatus::Accepted
                } else {
                    ActorLinkStatus::Rejected(ActorLinkRejection::MissingController)
                }
            }
        };
        results.write(ActorLinkResultMessage { request, status });
    }
}

pub fn redirect_controls(
    mut commands: Commands,
    controllers: Query<(Entity, &ControlRedirect), With<Actor>>,
    mut intents: Query<&mut Intents, With<Actor>>,
) {
    for (controller_entity, redirect) in &controllers {
        let Ok([mut controller, mut controlled]) =
            intents.get_many_mut([controller_entity, redirect.controlled])
        else {
            if let Ok(mut controller) = intents.get_mut(controller_entity) {
                *controller = Intents::default();
            }
            commands
                .entity(controller_entity)
                .remove::<ControlRedirect>();
            continue;
        };
        let source = controller.clone();
        *controlled = Intents {
            planar: if redirect.mask.planar {
                source.planar
            } else {
                default()
            },
            wants_sprint: redirect.mask.sprint && source.wants_sprint,
            wants_sneak: false,
            jump: if redirect.mask.jump {
                source.jump
            } else {
                default()
            },
            ..default()
        };
        *controller = Intents::default();
    }
}

type AttachedBody<'a> = (
    &'a KinematicAttachment,
    &'a mut Transform,
    &'a mut BodyVelocity,
);
type CarrierBody<'a> = (&'a Transform, &'a BodyVelocity);

pub fn sync_attachments(
    mut attached: Query<AttachedBody, With<Actor>>,
    carriers: Query<CarrierBody, (With<Actor>, Without<KinematicAttachment>)>,
) {
    for (attachment, mut transform, mut velocity) in &mut attached {
        let Ok((carrier_transform, carrier_velocity)) = carriers.get(attachment.carrier) else {
            continue;
        };
        *transform = carrier_transform.mul_transform(attachment.local_pose);
        velocity.0 = carrier_velocity.0;
    }
}

#[cfg(test)]
mod tests;
