use avian3d::prelude::{Collider, ColliderDisabled, SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

use super::attachment::{KinematicAttachment, LocomotionEnabled, PendingSafeRecovery};
use super::control::ControlRedirect;
use super::intents::Intents;
use super::link::{
    ActorLinkRejection, ActorLinkRequestMessage, ActorLinkResultMessage, ActorLinkStatus,
    ActorLinkWorkspace, DetachSafety,
};
use super::{Actor, BodyVelocity};

type ActorLinks<'a> = (
    Entity,
    Option<&'a KinematicAttachment>,
    Option<&'a ControlRedirect>,
);

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

    // Size and clear the workspace here, same tick as its use — reserve is a
    // no-op once steady, and the current actor count is exact by definition.
    let actor_count = actors.iter().count();
    workspace.controllers.clear();
    workspace.controlled.clear();
    workspace.attached.clear();
    workspace.carriers.clear();
    workspace.processed.clear();
    workspace.controllers.reserve(actor_count);
    workspace.controlled.reserve(actor_count);
    workspace.attached.reserve(actor_count);
    workspace.carriers.reserve(actor_count);
    workspace.processed.reserve(actor_count);
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

pub fn recover_pending_safe_poses(
    mut commands: Commands,
    spatial: SpatialQuery,
    mut actors: Query<(Entity, &Collider, &mut Transform, &mut PendingSafeRecovery), With<Actor>>,
) {
    let filter = SpatialQueryFilter::DEFAULT;
    for (actor, collider, mut transform, mut recovery) in &mut actors {
        let mut recovered = None;
        for _ in 0..4 {
            let candidate = recovery.origin + Vec3::Y * recovery.next_height;
            recovery.next_height += 2.0;
            let mut blocked = false;
            spatial.shape_intersections_callback(
                collider,
                candidate,
                recovery.rotation,
                &filter,
                |entity| {
                    if entity != actor {
                        blocked = true;
                        return false;
                    }
                    true
                },
            );
            if !blocked {
                recovered = Some(candidate);
                break;
            }
        }
        let Some(position) = recovered else {
            continue;
        };
        transform.translation = position;
        transform.rotation = recovery.rotation;
        commands.entity(actor).remove::<PendingSafeRecovery>();
        commands.entity(actor).remove::<ColliderDisabled>();
        commands.entity(actor).insert(LocomotionEnabled);
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

pub fn recover_orphaned_attachments(
    mut commands: Commands,
    attached: Query<
        (
            Entity,
            &KinematicAttachment,
            Option<&ControlRedirect>,
            &Transform,
        ),
        With<Actor>,
    >,
    carriers: Query<(), With<Actor>>,
    mut bodies: Query<(&mut BodyVelocity, &mut Intents), With<Actor>>,
) {
    for (actor, attachment, redirect, transform) in &attached {
        if carriers.get(attachment.carrier).is_ok() {
            continue;
        }
        if let Ok((mut velocity, mut intents)) = bodies.get_mut(actor) {
            velocity.0 = Vec3::ZERO;
            *intents = Intents::default();
        }
        if let Some(redirect) = redirect
            && let Ok((_, mut controlled_intents)) = bodies.get_mut(redirect.controlled)
        {
            *controlled_intents = Intents::default();
        }
        commands
            .entity(actor)
            .remove::<(KinematicAttachment, ControlRedirect)>();
        commands.entity(actor).insert((
            ColliderDisabled,
            PendingSafeRecovery {
                origin: transform.translation,
                rotation: transform.rotation,
                next_height: 0.0,
            },
        ));
        commands.entity(actor).remove::<LocomotionEnabled>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::MovementSet;
    use crate::movement::control::{ControlMask, ControlRedirect};
    use crate::movement::intents::{Intents, JumpIntent, PlanarMoveIntent};
    use crate::movement::link::{
        ActorLinkRequestMessage, ActorLinkResultMessage, ActorLinkWorkspace, DetachSafety,
    };

    #[derive(Component, Default)]
    struct MotorTicks(u32);

    fn tick_enabled_actors(mut actors: Query<&mut MotorTicks, With<LocomotionEnabled>>) {
        for mut ticks in &mut actors {
            ticks.0 += 1;
        }
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_message::<ActorLinkRequestMessage>();
        app.add_message::<ActorLinkResultMessage>();
        app.init_resource::<ActorLinkWorkspace>();
        app.configure_sets(
            FixedUpdate,
            (
                MovementSet::ApplyExternal,
                MovementSet::ReadIntents,
                MovementSet::ControlRedirect,
                MovementSet::SenseWorld,
                MovementSet::GatherProposals,
                MovementSet::Arbitrate,
                MovementSet::TickActiveMotor,
                MovementSet::SyncAttachments,
            )
                .chain(),
        );
        app.add_systems(
            FixedUpdate,
            apply_actor_link_requests.in_set(MovementSet::ApplyExternal),
        );
        app.add_systems(
            FixedUpdate,
            redirect_controls.in_set(MovementSet::ControlRedirect),
        );
        app.add_systems(
            FixedUpdate,
            tick_enabled_actors.in_set(MovementSet::TickActiveMotor),
        );
        app.add_systems(
            FixedUpdate,
            sync_attachments.in_set(MovementSet::SyncAttachments),
        );
        app
    }

    fn prepare(app: &mut App) {
        app.update();
    }

    fn spawn_actor(app: &mut App, transform: Transform, intents: Intents) -> Entity {
        app.world_mut()
            .spawn((
                Actor,
                LocomotionEnabled,
                transform,
                BodyVelocity::default(),
                intents,
                MotorTicks::default(),
            ))
            .id()
    }

    #[test]
    fn app_schedule_attaches_redirects_and_releases_without_rider_motor_tick() {
        let mut app = app();
        let rider_intents = Intents {
            planar: PlanarMoveIntent {
                direction: Vec2::X,
                strength: 0.75,
            },
            wants_sprint: true,
            wants_sneak: true,
            jump: JumpIntent {
                held: true,
                pressed: true,
            },
            ..default()
        };
        let rider = spawn_actor(&mut app, Transform::default(), rider_intents);
        let carrier = spawn_actor(
            &mut app,
            Transform::from_xyz(4.0, 2.0, -3.0),
            Intents::default(),
        );
        prepare(&mut app);
        app.world_mut()
            .write_message(ActorLinkRequestMessage::Attach {
                controller: rider,
                controlled: carrier,
                local_pose: Transform::from_xyz(0.0, 1.5, 0.0),
                mask: ControlMask::MOUNT,
            });

        app.world_mut().run_schedule(FixedUpdate);

        let rider_entity = app.world().entity(rider);
        assert!(rider_entity.contains::<KinematicAttachment>());
        assert!(rider_entity.contains::<ColliderDisabled>());
        assert!(!rider_entity.contains::<LocomotionEnabled>());
        assert_eq!(rider_entity.get::<MotorTicks>().unwrap().0, 0);
        assert_eq!(
            rider_entity.get::<Transform>().unwrap().translation,
            Vec3::new(4.0, 3.5, -3.0)
        );
        assert_eq!(rider_entity.get::<Intents>().unwrap().planar.strength, 0.0);
        let carrier_entity = app.world().entity(carrier);
        assert_eq!(carrier_entity.get::<MotorTicks>().unwrap().0, 1);
        let carrier_intents = carrier_entity.get::<Intents>().unwrap();
        assert_eq!(carrier_intents.planar.direction, Vec2::X);
        assert!(carrier_intents.wants_sprint);
        assert!(!carrier_intents.wants_sneak);
        assert!(carrier_intents.jump.pressed);

        app.world_mut()
            .write_message(ActorLinkRequestMessage::Detach {
                controller: rider,
                controlled: carrier,
                world_pose: Transform::from_xyz(6.0, 1.0, -3.0),
                inherited_velocity: Vec3::new(2.0, 0.0, 1.0),
                safety: DetachSafety::Validated,
                force: false,
            });
        app.world_mut().run_schedule(FixedUpdate);

        let rider_entity = app.world().entity(rider);
        assert!(!rider_entity.contains::<KinematicAttachment>());
        assert!(!rider_entity.contains::<ColliderDisabled>());
        assert!(rider_entity.contains::<LocomotionEnabled>());
        assert!(!rider_entity.contains::<ControlRedirect>());
        assert_eq!(rider_entity.get::<MotorTicks>().unwrap().0, 1);
        assert_eq!(
            rider_entity.get::<BodyVelocity>().unwrap().0,
            Vec3::new(2.0, 0.0, 1.0)
        );
        assert_eq!(
            app.world()
                .entity(carrier)
                .get::<Intents>()
                .unwrap()
                .planar
                .strength,
            0.0
        );
    }

    #[test]
    fn one_target_accepts_only_the_first_controller_in_a_batch() {
        let mut app = app();
        let first = spawn_actor(&mut app, Transform::default(), Intents::default());
        let second = spawn_actor(&mut app, Transform::default(), Intents::default());
        let target = spawn_actor(&mut app, Transform::default(), Intents::default());
        prepare(&mut app);
        for controller in [first, second] {
            app.world_mut()
                .write_message(ActorLinkRequestMessage::Attach {
                    controller,
                    controlled: target,
                    local_pose: Transform::IDENTITY,
                    mask: ControlMask::MOUNT,
                });
        }

        app.world_mut().run_schedule(FixedUpdate);

        let redirects = [first, second]
            .into_iter()
            .filter(|entity| app.world().entity(*entity).contains::<ControlRedirect>())
            .count();
        assert_eq!(redirects, 1);
    }

    #[test]
    fn one_controller_accepts_only_the_first_target_and_never_partially_links() {
        let mut app = app();
        let controller = spawn_actor(&mut app, Transform::default(), Intents::default());
        let first = spawn_actor(&mut app, Transform::default(), Intents::default());
        let second = spawn_actor(&mut app, Transform::default(), Intents::default());
        prepare(&mut app);
        for controlled in [first, second] {
            app.world_mut()
                .write_message(ActorLinkRequestMessage::Attach {
                    controller,
                    controlled,
                    local_pose: Transform::IDENTITY,
                    mask: ControlMask::MOUNT,
                });
        }

        app.world_mut().run_schedule(FixedUpdate);

        let entity = app.world().entity(controller);
        assert_eq!(entity.get::<ControlRedirect>().unwrap().controlled, first);
        assert_eq!(entity.get::<KinematicAttachment>().unwrap().carrier, first);
        assert_ne!(entity.get::<ControlRedirect>().unwrap().controlled, second);
    }

    #[test]
    fn missing_target_and_chain_requests_leave_no_partial_link() {
        let mut app = app();
        let first = spawn_actor(&mut app, Transform::default(), Intents::default());
        let second = spawn_actor(&mut app, Transform::default(), Intents::default());
        let third = spawn_actor(&mut app, Transform::default(), Intents::default());
        let missing = app.world_mut().spawn_empty().id();
        app.world_mut().entity_mut(missing).despawn();
        prepare(&mut app);
        app.world_mut()
            .write_message(ActorLinkRequestMessage::Attach {
                controller: third,
                controlled: missing,
                local_pose: Transform::IDENTITY,
                mask: ControlMask::MOUNT,
            });
        app.world_mut()
            .write_message(ActorLinkRequestMessage::Attach {
                controller: first,
                controlled: second,
                local_pose: Transform::IDENTITY,
                mask: ControlMask::MOUNT,
            });
        app.world_mut()
            .write_message(ActorLinkRequestMessage::Attach {
                controller: second,
                controlled: third,
                local_pose: Transform::IDENTITY,
                mask: ControlMask::MOUNT,
            });

        app.world_mut().run_schedule(FixedUpdate);

        assert!(!app.world().entity(third).contains::<KinematicAttachment>());
        assert!(!app.world().entity(third).contains::<ControlRedirect>());
        assert!(app.world().entity(first).contains::<KinematicAttachment>());
        assert!(!app.world().entity(second).contains::<KinematicAttachment>());
    }

    #[test]
    fn fixed_link_path_keeps_workspace_capacities_once_steady() {
        let mut app = app();
        let controller = spawn_actor(&mut app, Transform::default(), Intents::default());
        let controlled = spawn_actor(&mut app, Transform::default(), Intents::default());

        // First request sizes the workspace inside the fixed path itself.
        app.world_mut()
            .write_message(ActorLinkRequestMessage::Attach {
                controller,
                controlled,
                local_pose: Transform::IDENTITY,
                mask: ControlMask::MOUNT,
            });
        app.world_mut().run_schedule(FixedUpdate);
        let before = {
            let workspace = app.world().resource::<ActorLinkWorkspace>();
            (
                workspace.controllers.capacity(),
                workspace.controlled.capacity(),
                workspace.attached.capacity(),
                workspace.carriers.capacity(),
                workspace.processed.capacity(),
            )
        };

        // Steady state: further requests must not reallocate.
        app.world_mut()
            .write_message(ActorLinkRequestMessage::Detach {
                controller,
                controlled,
                world_pose: Transform::from_xyz(2.0, 1.0, 0.0),
                inherited_velocity: Vec3::X,
                safety: DetachSafety::Validated,
                force: false,
            });
        app.world_mut().run_schedule(FixedUpdate);

        let workspace = app.world().resource::<ActorLinkWorkspace>();
        assert_eq!(
            before,
            (
                workspace.controllers.capacity(),
                workspace.controlled.capacity(),
                workspace.attached.capacity(),
                workspace.carriers.capacity(),
                workspace.processed.capacity(),
            )
        );
    }

    /// Regression guard for the removed cross-schedule "prepare" pattern:
    /// attach, detach and neutralize must all apply on the very tick their
    /// request arrives, with no preparation run in between.
    #[test]
    fn link_requests_apply_the_same_tick_they_arrive() {
        let mut app = app();
        let controller = spawn_actor(&mut app, Transform::default(), Intents::default());
        let controlled = spawn_actor(&mut app, Transform::default(), Intents::default());

        app.world_mut()
            .write_message(ActorLinkRequestMessage::Attach {
                controller,
                controlled,
                local_pose: Transform::IDENTITY,
                mask: ControlMask::MOUNT,
            });
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(
            app.world()
                .entity(controller)
                .get::<ControlRedirect>()
                .unwrap()
                .controlled,
            controlled
        );

        app.world_mut()
            .write_message(ActorLinkRequestMessage::Detach {
                controller,
                controlled,
                world_pose: Transform::from_xyz(2.0, 1.0, 0.0),
                inherited_velocity: Vec3::X,
                safety: DetachSafety::Validated,
                force: false,
            });
        app.world_mut().run_schedule(FixedUpdate);
        assert!(!app.world().entity(controller).contains::<ControlRedirect>());

        app.world_mut().entity_mut(controlled).insert(Intents {
            wants_sprint: true,
            ..default()
        });
        app.world_mut()
            .write_message(ActorLinkRequestMessage::Neutralize { actor: controlled });
        app.world_mut().run_schedule(FixedUpdate);
        assert!(
            !app.world()
                .entity(controlled)
                .get::<Intents>()
                .unwrap()
                .wants_sprint
        );
    }
}
