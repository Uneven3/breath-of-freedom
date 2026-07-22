use super::*;
use crate::movement::MovementSet;
use crate::movement::control::{ControlMask, ControlRedirect};
use crate::movement::intents::{Intents, JumpIntent, PlanarMoveIntent};
use crate::movement::link::{
    ActorLinkRequestMessage, ActorLinkResultMessage, ActorLinkWorkspace, DetachSafety,
};

#[test]
fn workspace_reserve_accounts_for_existing_entries() {
    let mut set = bevy::ecs::entity::EntityHashSet::with_capacity(4);
    set.insert(Entity::from_raw_u32(1).unwrap());
    set.insert(Entity::from_raw_u32(2).unwrap());

    reserve_for_actor_count(&mut set, 64);

    assert!(set.capacity() >= 64);
}

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
