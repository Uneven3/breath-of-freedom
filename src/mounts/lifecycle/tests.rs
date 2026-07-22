use super::*;
use avian3d::collider_tree::ColliderTrees;
use avian3d::prelude::*;

use crate::combat::CombatSet;
use crate::combat::context;
use crate::combat::context_data::{CombatContext, SetMountedCombatMessage};
use crate::health::{DeathMessage, HealthSet, HostileInteractionImmunity};
use crate::mounts::MountsSet;
use crate::mounts::data::MountDebugRequest;
use crate::mounts::debug;
use crate::movement::MovementSet;
use crate::movement::attachment::{KinematicAttachment, LocomotionEnabled};
use crate::movement::attachment_systems;
use crate::movement::control::ControlRedirect;
use crate::movement::intents::Intents;
use crate::movement::link::{ActorLinkRequestMessage, ActorLinkResultMessage, ActorLinkWorkspace};
use crate::movement::{Actor, BodyVelocity, Player};

fn app() -> App {
    let mut app = App::new();
    app.init_resource::<ColliderTrees>();
    app.add_message::<MountDebugRequest>();
    app.add_message::<MountTransitionRequest>();
    app.add_message::<ActorLinkRequestMessage>();
    app.add_message::<ActorLinkResultMessage>();
    app.init_resource::<ActorLinkWorkspace>();
    app.add_message::<SetMountedCombatMessage>();
    app.add_message::<DeathMessage>();
    app.configure_sets(
        FixedUpdate,
        (
            MountsSet::Request,
            MountsSet::Lifecycle,
            MovementSet::ApplyExternal,
            MountsSet::Confirm,
            MovementSet::ReadIntents,
            MovementSet::ControlRedirect,
            MovementSet::SyncAttachments,
            MountsSet::PostMove,
            CombatSet::ApplyContext,
            HealthSet::Apply,
            MountsSet::DeathCleanup,
        )
            .chain(),
    );
    app.add_systems(
        FixedUpdate,
        (debug::process_toggle_requests, reconcile_relationships)
            .chain()
            .in_set(MountsSet::Request),
    );
    app.add_systems(FixedUpdate, apply_transitions.in_set(MountsSet::Lifecycle));
    app.add_systems(
        FixedUpdate,
        (
            attachment_systems::apply_actor_link_requests,
            attachment_systems::recover_orphaned_attachments,
        )
            .chain()
            .in_set(MovementSet::ApplyExternal),
    );
    app.add_systems(
        FixedUpdate,
        (confirm_transitions, recover_detached_riders)
            .chain()
            .in_set(MountsSet::Confirm),
    );
    app.add_systems(
        FixedUpdate,
        attachment_systems::redirect_controls.in_set(MovementSet::ControlRedirect),
    );
    app.add_systems(
        FixedUpdate,
        attachment_systems::sync_attachments.in_set(MovementSet::SyncAttachments),
    );
    app.add_systems(
        FixedUpdate,
        despawn_released_horses.in_set(MountsSet::PostMove),
    );
    app.add_systems(
        FixedUpdate,
        context::apply_mounted_context.in_set(CombatSet::ApplyContext),
    );
    app.add_systems(
        FixedUpdate,
        collect_horse_deaths.in_set(MountsSet::DeathCleanup),
    );
    app
}

fn spawn_rider(app: &mut App) -> Entity {
    app.world_mut()
        .spawn((
            Player,
            Actor,
            LocomotionEnabled,
            Transform::default(),
            BodyVelocity::default(),
            Intents::default(),
            BodyDimensions::PLAYER,
            CombatContext::default(),
            Collider::capsule(
                BodyDimensions::PLAYER.radius,
                BodyDimensions::PLAYER.standing_capsule_length,
            ),
        ))
        .id()
}

fn spawn_horse(app: &mut App) -> Entity {
    app.world_mut()
        .spawn((
            Horse,
            Actor,
            LocomotionEnabled,
            RiddenBy::default(),
            HorseOwner::default(),
            Transform::from_xyz(3.0, 1.0, 0.0),
            BodyVelocity::default(),
            Intents::default(),
        ))
        .id()
}

fn mount(app: &mut App, rider: Entity, horse: Entity) {
    app.update();
    app.world_mut()
        .write_message(MountTransitionRequest::Mount { rider, horse });
    app.world_mut().run_schedule(FixedUpdate);
    assert_eq!(
        app.world().entity(horse).get::<RiddenBy>().unwrap().0,
        Some(rider)
    );
}

fn assert_safely_detached(app: &App, rider: Entity) {
    let rider_entity = app.world().entity(rider);
    assert!(!rider_entity.contains::<MountedOn>());
    assert!(!rider_entity.contains::<KinematicAttachment>());
    assert!(!rider_entity.contains::<ControlRedirect>());
    if rider_entity.contains::<crate::movement::attachment::PendingSafeRecovery>() {
        assert!(rider_entity.contains::<ColliderDisabled>());
        assert!(!rider_entity.contains::<LocomotionEnabled>());
    } else {
        assert!(!rider_entity.contains::<ColliderDisabled>());
        assert!(rider_entity.contains::<LocomotionEnabled>());
    }
}

#[test]
fn app_schedule_enforces_one_rider_and_cleans_an_orphaned_mount() {
    let mut app = app();
    let first = spawn_rider(&mut app);
    let second = spawn_rider(&mut app);
    let horse = spawn_horse(&mut app);
    app.update();
    for rider in [first, second] {
        app.world_mut()
            .write_message(MountTransitionRequest::Mount { rider, horse });
    }

    app.world_mut().run_schedule(FixedUpdate);

    let mounted = [first, second]
        .into_iter()
        .filter(|rider| app.world().entity(*rider).contains::<MountedOn>())
        .collect::<Vec<_>>();
    assert_eq!(mounted.len(), 1);
    let rider = mounted[0];
    assert_eq!(
        app.world().entity(horse).get::<RiddenBy>().unwrap().0,
        Some(rider)
    );
    let rider_entity = app.world().entity(rider);
    assert!(rider_entity.contains::<KinematicAttachment>());
    assert!(rider_entity.contains::<ControlRedirect>());
    assert!(rider_entity.contains::<ColliderDisabled>());
    assert!(!rider_entity.contains::<LocomotionEnabled>());

    app.world_mut().entity_mut(horse).despawn();
    app.world_mut().run_schedule(FixedUpdate);

    let rider_entity = app.world().entity(rider);
    assert!(!rider_entity.contains::<MountedOn>());
    assert!(!rider_entity.contains::<KinematicAttachment>());
    assert!(!rider_entity.contains::<ControlRedirect>());
    assert!(rider_entity.contains::<ColliderDisabled>());
    assert!(rider_entity.contains::<crate::movement::attachment::PendingSafeRecovery>());
    assert!(!rider_entity.contains::<LocomotionEnabled>());
    assert_eq!(rider_entity.get::<BodyVelocity>().unwrap().0, Vec3::ZERO);
    assert_eq!(rider_entity.get::<Intents>().unwrap().planar.strength, 0.0);
}

#[test]
fn one_rider_mounts_only_the_first_horse_in_a_request_batch() {
    let mut app = app();
    let rider = spawn_rider(&mut app);
    let first = spawn_horse(&mut app);
    let second = spawn_horse(&mut app);
    app.update();
    for horse in [first, second] {
        app.world_mut()
            .write_message(MountTransitionRequest::Mount { rider, horse });
    }

    app.world_mut().run_schedule(FixedUpdate);

    assert_eq!(
        app.world().entity(rider).get::<MountedOn>().unwrap().0,
        first
    );
    assert_eq!(
        app.world().entity(first).get::<RiddenBy>().unwrap().0,
        Some(rider)
    );
    assert_eq!(
        app.world().entity(second).get::<RiddenBy>().unwrap().0,
        None
    );
}

#[test]
fn despawned_rider_clears_inverse_relation_and_neutralizes_horse() {
    let mut app = app();
    let rider = spawn_rider(&mut app);
    let horse = spawn_horse(&mut app);
    mount(&mut app, rider, horse);
    app.world_mut().entity_mut(horse).insert(Intents {
        planar: crate::movement::intents::PlanarMoveIntent {
            direction: Vec2::X,
            strength: 1.0,
        },
        wants_sprint: true,
        ..default()
    });

    app.world_mut().entity_mut(rider).despawn();
    app.world_mut().run_schedule(FixedUpdate);

    let horse_entity = app.world().entity(horse);
    assert_eq!(horse_entity.get::<RiddenBy>().unwrap().0, None);
    let intents = horse_entity.get::<Intents>().unwrap();
    assert_eq!(intents.planar.strength, 0.0);
    assert!(!intents.wants_sprint);
}

#[test]
fn voluntary_dismount_without_floor_is_rejected_but_f8_forces_safe_release() {
    let mut app = app();
    let rider = spawn_rider(&mut app);
    let horse = spawn_horse(&mut app);
    app.world_mut()
        .entity_mut(horse)
        .insert(BodyVelocity(Vec3::new(5.0, 2.0, -1.0)));
    mount(&mut app, rider, horse);

    app.world_mut()
        .write_message(MountTransitionRequest::Dismount {
            rider,
            horse,
            forced: false,
        });
    app.world_mut().run_schedule(FixedUpdate);
    assert!(app.world().entity(rider).contains::<MountedOn>());
    assert!(app.world().entity(horse).contains::<Horse>());

    app.world_mut()
        .write_message(MountDebugRequest::ToggleHorse);
    app.world_mut().run_schedule(FixedUpdate);

    assert_safely_detached(&app, rider);
    assert!(app.world().get_entity(horse).is_err());
    assert_eq!(
        app.world().entity(rider).get::<BodyVelocity>().unwrap().0,
        Vec3::new(5.0, 0.0, -1.0)
    );
}

#[test]
fn horse_death_releases_on_the_next_lifecycle_tick_before_despawn() {
    let mut app = app();
    let rider = spawn_rider(&mut app);
    let horse = spawn_horse(&mut app);
    mount(&mut app, rider, horse);

    app.world_mut()
        .write_message(DeathMessage { entity: horse });
    app.world_mut().run_schedule(FixedUpdate);

    assert!(app.world().entity(horse).contains::<PendingHorseDespawn>());
    assert!(app.world().entity(rider).contains::<MountedOn>());

    app.world_mut().run_schedule(FixedUpdate);

    assert_safely_detached(&app, rider);
    assert!(app.world().get_entity(horse).is_err());
}

#[test]
fn a_new_rider_does_not_replace_the_persistent_owner() {
    let mut app = app();
    let owner = spawn_rider(&mut app);
    let guest = spawn_rider(&mut app);
    let horse = spawn_horse(&mut app);
    mount(&mut app, owner, horse);

    app.world_mut()
        .write_message(MountTransitionRequest::Dismount {
            rider: owner,
            horse,
            forced: true,
        });
    app.world_mut().run_schedule(FixedUpdate);
    mount(&mut app, guest, horse);

    let horse_entity = app.world().entity(horse);
    assert_eq!(horse_entity.get::<RiddenBy>().unwrap().0, Some(guest));
    assert_eq!(horse_entity.get::<HorseOwner>().unwrap().0, Some(owner));
    assert_eq!(
        horse_entity.get::<HostileInteractionImmunity>().unwrap().0,
        owner
    );
}
