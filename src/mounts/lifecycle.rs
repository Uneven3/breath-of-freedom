use avian3d::prelude::*;
use bevy::prelude::*;

use crate::combat::context_data::SetMountedCombatMessage;
use crate::health::{DeathMessage, Health, HostileInteractionImmunity};
use crate::movement::abilities::{
    AirborneMovement, GroundMovement, JumpMovement, JumpStaminaCost, SprintMovement, StairsMovement,
};
use crate::movement::attachment::KinematicAttachment;
use crate::movement::body::BodyDimensions;
use crate::movement::bundles::{
    GroundMovementBundle, JumpMovementBundle, KinematicActorBundle, SprintMovementBundle,
    StairsMovementBundle, StaminaBundle,
};
use crate::movement::link::{
    ActorLinkRequestMessage, ActorLinkResultMessage, ActorLinkStatus, DetachSafety,
};
use crate::movement::sensing::GroundSensing;
use crate::movement::{Actor, BodyVelocity, Player};

use super::control::MountControlWriters;
use super::data::{
    Horse, HorseCharge, HorseOwner, MountTransitionRequest, MountedOn, PendingHorseDespawn,
    RiddenBy,
};
use crate::interaction::{Interactable, InteractionKind, InteractionOverride, InteractionRequest};

const HORSE_DIMENSIONS: BodyDimensions = BodyDimensions {
    radius: 0.65,
    standing_capsule_length: 1.8,
    crouched_capsule_length: 1.8,
};
const HORSE_SPAWN: Vec3 = Vec3::new(3.5, 1.55, 0.0);
const MOUNT_RANGE: f32 = 2.5;
const SADDLE_OFFSET: Vec3 = Vec3::new(0.0, 1.65, 0.0);
const DISMOUNT_DISTANCE: f32 = 1.9;
const FLOOR_MIN_UP_DOT: f32 = 0.5;

pub fn spawn_horse_bundle() -> impl Bundle {
    (
        Horse,
        HorseCharge::new(HORSE_SPAWN),
        RiddenBy::default(),
        HorseOwner::default(),
        Name::new("Horse"),
        Health::new(120.0),
        KinematicActorBundle::new(
            Transform::from_translation(HORSE_SPAWN),
            HORSE_DIMENSIONS,
            GroundSensing::PLAYER,
        ),
        GroundMovementBundle::new(GroundMovement::HORSE),
        SprintMovementBundle::new(SprintMovement::HORSE),
        StairsMovementBundle::new(StairsMovement::HORSE),
        StaminaBundle::default(),
        JumpMovementBundle::new(JumpMovement::HORSE),
        JumpStaminaCost(20.0),
        AirborneMovement::HORSE,
    )
}

type AvailableHorseFilter = (With<Horse>, Without<PendingHorseDespawn>);

/// Turns the arbiter's decision into a mount/dismount transition. Reads no
/// input: `interaction` owns the key and already resolved who wins, which is
/// what keeps a horse and a nearby weapon from both firing on one press.
pub fn read_interact_requests(
    mut interactions: MessageReader<InteractionRequest>,
    riders: Query<Option<&MountedOn>>,
    mut requests: MessageWriter<MountTransitionRequest>,
) {
    for interaction in interactions.read() {
        match interaction.kind {
            InteractionKind::Dismount => {
                let Ok(Some(mounted)) = riders.get(interaction.actor) else {
                    continue;
                };
                requests.write(MountTransitionRequest::Dismount {
                    rider: interaction.actor,
                    horse: mounted.0,
                    forced: false,
                });
            }
            InteractionKind::Mount => {
                let Some(horse) = interaction.target else {
                    continue;
                };
                requests.write(MountTransitionRequest::Mount {
                    rider: interaction.actor,
                    horse,
                });
            }
            InteractionKind::Pickup => {}
        }
    }
}

type DespawningInteractableFilter = (With<Horse>, With<PendingHorseDespawn>, With<Interactable>);

/// Availability as a component, so the arbiter never has to know what makes a
/// horse mountable. A ridden or dying horse simply stops being a candidate.
pub fn sync_horse_interactable(
    mut commands: Commands,
    horses: Query<(Entity, &RiddenBy, Has<Interactable>), AvailableHorseFilter>,
    despawning: Query<Entity, DespawningInteractableFilter>,
) {
    for (horse, ridden, marked) in &horses {
        match (ridden.0.is_none(), marked) {
            (true, false) => {
                commands.entity(horse).try_insert(Interactable {
                    kind: InteractionKind::Mount,
                    range: MOUNT_RANGE,
                });
            }
            (false, true) => {
                commands.entity(horse).remove::<Interactable>();
            }
            _ => {}
        }
    }
    for horse in &despawning {
        commands.entity(horse).remove::<Interactable>();
    }
}

/// While mounted, `Interact` means dismount — declared to the arbiter rather
/// than hidden as an early return in this module.
pub fn sync_rider_override(
    mut commands: Commands,
    mounted: Query<Entity, (With<MountedOn>, Without<InteractionOverride>)>,
    dismounted: Query<Entity, (With<InteractionOverride>, Without<MountedOn>)>,
) {
    for rider in &mounted {
        commands.entity(rider).try_insert(InteractionOverride {
            kind: InteractionKind::Dismount,
        });
    }
    for rider in &dismounted {
        commands.entity(rider).remove::<InteractionOverride>();
    }
}

type RiderState<'a> = (
    Option<&'a MountedOn>,
    &'a Transform,
    &'a Collider,
    &'a BodyDimensions,
);
type HorseState<'a> = (
    &'a RiddenBy,
    &'a HorseOwner,
    &'a Transform,
    &'a BodyVelocity,
);

pub fn apply_transitions(
    mut requests: MessageReader<MountTransitionRequest>,
    riders: Query<RiderState, With<Actor>>,
    horses: Query<HorseState, With<Horse>>,
    spatial: SpatialQuery,
    writers: MountControlWriters,
) {
    let mut writers = writers;
    for request in requests.read() {
        match *request {
            MountTransitionRequest::Mount { rider, horse } => {
                let Ok((mounted, _, _, _)) = riders.get(rider) else {
                    continue;
                };
                let Ok((ridden, _, _, _)) = horses.get(horse) else {
                    continue;
                };
                if mounted.is_some() || ridden.0.is_some() {
                    continue;
                }
                writers.attach(rider, horse, Transform::from_translation(SADDLE_OFFSET));
            }
            MountTransitionRequest::Dismount {
                rider,
                horse,
                forced,
            } => {
                let Ok((mounted, rider_transform, collider, body)) = riders.get(rider) else {
                    continue;
                };
                let reciprocal = mounted.is_some_and(|mounted| mounted.0 == horse);
                let Ok((ridden, _, horse_transform, horse_velocity)) = horses.get(horse) else {
                    if forced {
                        writers.detach(
                            rider,
                            horse,
                            *rider_transform,
                            Vec3::ZERO,
                            DetachSafety::NeedsRecovery,
                            true,
                        );
                    }
                    continue;
                };
                if !reciprocal && ridden.0 != Some(rider) {
                    continue;
                }
                let Some(dismount) = find_dismount_pose(
                    &spatial,
                    collider,
                    *body,
                    rider,
                    horse,
                    horse_transform,
                    forced,
                ) else {
                    continue;
                };
                let inherited = Vec3::new(horse_velocity.0.x, 0.0, horse_velocity.0.z);
                writers.detach(
                    rider,
                    horse,
                    dismount.pose,
                    inherited,
                    dismount.safety,
                    forced,
                );
            }
        }
    }
}

pub fn confirm_transitions(
    mut commands: Commands,
    mut results: MessageReader<ActorLinkResultMessage>,
    riders: Query<(Option<&MountedOn>, &Transform), With<Actor>>,
    mut horses: Query<(&mut RiddenBy, &mut HorseOwner), With<Horse>>,
    mut writers: MountControlWriters,
    mut combat: MessageWriter<SetMountedCombatMessage>,
) {
    for result in results.read() {
        match (result.request, result.status) {
            (
                ActorLinkRequestMessage::Attach {
                    controller: rider,
                    controlled: horse,
                    ..
                },
                ActorLinkStatus::Accepted,
            ) => {
                let Ok((mounted, rider_transform)) = riders.get(rider) else {
                    continue;
                };
                let Ok((mut ridden, mut owner)) = horses.get_mut(horse) else {
                    writers.detach(
                        rider,
                        horse,
                        *rider_transform,
                        Vec3::ZERO,
                        DetachSafety::NeedsRecovery,
                        true,
                    );
                    continue;
                };
                if mounted.is_some() || ridden.0.is_some() {
                    writers.detach(
                        rider,
                        horse,
                        *rider_transform,
                        Vec3::ZERO,
                        DetachSafety::NeedsRecovery,
                        true,
                    );
                    continue;
                }
                ridden.0 = Some(rider);
                if owner.0.is_none() {
                    owner.0 = Some(rider);
                }
                if let Some(owner) = owner.0 {
                    commands
                        .entity(horse)
                        .insert(HostileInteractionImmunity(owner));
                }
                commands.entity(rider).insert(MountedOn(horse));
                combat.write(SetMountedCombatMessage {
                    actor: rider,
                    mounted: true,
                });
            }
            (
                ActorLinkRequestMessage::Detach {
                    controller: rider,
                    controlled: horse,
                    ..
                },
                ActorLinkStatus::Accepted,
            ) => {
                if let Ok((mut ridden, _)) = horses.get_mut(horse)
                    && ridden.0 == Some(rider)
                {
                    ridden.0 = None;
                }
                if riders.get(rider).is_ok() {
                    commands.entity(rider).remove::<MountedOn>();
                    combat.write(SetMountedCombatMessage {
                        actor: rider,
                        mounted: false,
                    });
                }
            }
            (_, ActorLinkStatus::Rejected(rejection)) => {
                warn!(?rejection, request = ?result.request, "actor link transition rejected");
            }
            (ActorLinkRequestMessage::Neutralize { .. }, ActorLinkStatus::Accepted) => {}
        }
    }
}

#[derive(Clone, Copy)]
struct DismountPose {
    pose: Transform,
    safety: DetachSafety,
}

fn find_dismount_pose(
    spatial: &SpatialQuery,
    collider: &Collider,
    body: BodyDimensions,
    rider: Entity,
    horse: Entity,
    horse_transform: &Transform,
    forced: bool,
) -> Option<DismountPose> {
    const DIRECTIONS: [Vec3; 8] = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Z,
        Vec3::NEG_Z,
        Vec3::new(0.707, 0.0, 0.707),
        Vec3::new(-0.707, 0.0, 0.707),
        Vec3::new(0.707, 0.0, -0.707),
        Vec3::new(-0.707, 0.0, -0.707),
    ];
    let filter = SpatialQueryFilter::DEFAULT;
    for radius in [DISMOUNT_DISTANCE, 2.8, 4.0] {
        if !forced && radius > DISMOUNT_DISTANCE {
            break;
        }
        for direction in DIRECTIONS {
            let world_direction = horse_transform.rotation * direction;
            let start = horse_transform.translation
                + world_direction * radius
                + Vec3::Y * (body.standing_half_height() + 1.0);
            let Some(hit) = spatial.cast_shape_predicate(
                collider,
                start,
                horse_transform.rotation,
                Dir3::NEG_Y,
                &ShapeCastConfig::from_max_distance(4.0),
                &filter,
                &|entity| entity != rider && entity != horse,
            ) else {
                continue;
            };
            if hit.normal1.y < FLOOR_MIN_UP_DOT {
                continue;
            }
            let position = start - Vec3::Y * hit.distance + Vec3::Y * 0.02;
            if clear_at(
                spatial,
                collider,
                position,
                horse_transform.rotation,
                &filter,
                rider,
                horse,
            ) {
                return Some(DismountPose {
                    pose: Transform::from_translation(position)
                        .with_rotation(horse_transform.rotation),
                    safety: DetachSafety::Validated,
                });
            }
        }
    }
    if forced {
        for height in [3.0, 5.0, 8.0] {
            let position = horse_transform.translation + Vec3::Y * height;
            if clear_at(
                spatial,
                collider,
                position,
                horse_transform.rotation,
                &filter,
                rider,
                horse,
            ) {
                return Some(DismountPose {
                    pose: Transform::from_translation(position)
                        .with_rotation(horse_transform.rotation),
                    safety: DetachSafety::Validated,
                });
            }
        }
        return Some(DismountPose {
            pose: Transform::from_translation(horse_transform.translation + Vec3::Y * 10.0)
                .with_rotation(horse_transform.rotation),
            safety: DetachSafety::NeedsRecovery,
        });
    }
    None
}

fn clear_at(
    spatial: &SpatialQuery,
    collider: &Collider,
    position: Vec3,
    rotation: Quat,
    filter: &SpatialQueryFilter,
    rider: Entity,
    horse: Entity,
) -> bool {
    let mut clear = true;
    spatial.shape_intersections_callback(collider, position, rotation, filter, |entity| {
        if entity == rider || entity == horse {
            true
        } else {
            clear = false;
            false
        }
    });
    clear
}

pub fn reconcile_relationships(
    riders: Query<(Entity, &MountedOn)>,
    mut horses: Query<(Entity, &mut RiddenBy), With<Horse>>,
    existing_riders: Query<Option<&MountedOn>, With<Actor>>,
    mut transitions: MessageWriter<MountTransitionRequest>,
    mut writers: MountControlWriters,
) {
    for (rider, mounted) in &riders {
        match horses.get_mut(mounted.0) {
            Ok((_, ridden)) if ridden.0 == Some(rider) => {}
            _ => {
                transitions.write(MountTransitionRequest::Dismount {
                    rider,
                    horse: mounted.0,
                    forced: true,
                });
            }
        }
    }
    for (horse, mut ridden) in &mut horses {
        let Some(rider) = ridden.0 else {
            continue;
        };
        match existing_riders.get(rider) {
            Err(_) => {
                ridden.0 = None;
                writers.neutralize(horse);
            }
            Ok(mounted) if mounted.is_none_or(|mounted| mounted.0 != horse) => {
                transitions.write(MountTransitionRequest::Dismount {
                    rider,
                    horse,
                    forced: true,
                });
            }
            Ok(_) => {}
        }
    }
}

type AttachedRiders<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static MountedOn>,
        Option<&'static KinematicAttachment>,
    ),
    With<Player>,
>;

pub fn recover_detached_riders(
    riders: AttachedRiders,
    mut transitions: MessageWriter<MountTransitionRequest>,
) {
    for (rider, mounted, attachment) in &riders {
        if let Some(mounted) = mounted
            && attachment.is_none()
        {
            transitions.write(MountTransitionRequest::Dismount {
                rider,
                horse: mounted.0,
                forced: true,
            });
        }
    }
}

pub fn collect_horse_deaths(
    mut commands: Commands,
    mut deaths: MessageReader<DeathMessage>,
    horses: Query<&RiddenBy, With<Horse>>,
    mut transitions: MessageWriter<MountTransitionRequest>,
) {
    for death in deaths.read() {
        let Ok(ridden) = horses.get(death.entity) else {
            continue;
        };
        commands.entity(death.entity).insert(PendingHorseDespawn);
        if let Some(rider) = ridden.0 {
            transitions.write(MountTransitionRequest::Dismount {
                rider,
                horse: death.entity,
                forced: true,
            });
        }
    }
}

type PendingHorses<'w, 's> =
    Query<'w, 's, (Entity, &'static RiddenBy), (With<Horse>, With<PendingHorseDespawn>)>;

pub fn despawn_released_horses(mut commands: Commands, horses: PendingHorses) {
    for (horse, ridden) in &horses {
        if ridden.0.is_none() {
            commands.entity(horse).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use avian3d::collider_tree::ColliderTrees;

    use crate::combat::CombatSet;
    use crate::combat::context;
    use crate::combat::context_data::CombatContext;
    use crate::health::HealthSet;
    use crate::mounts::MountsSet;
    use crate::mounts::data::MountDebugRequest;
    use crate::mounts::debug;
    use crate::movement::MovementSet;
    use crate::movement::attachment::LocomotionEnabled;
    use crate::movement::attachment_systems;
    use crate::movement::control::ControlRedirect;
    use crate::movement::intents::Intents;
    use crate::movement::link::{
        ActorLinkRequestMessage, ActorLinkResultMessage, ActorLinkWorkspace,
    };

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
}
