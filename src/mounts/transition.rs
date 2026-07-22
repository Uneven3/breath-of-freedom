use avian3d::prelude::*;
use bevy::prelude::*;

use super::control::MountControlWriters;
use super::data::{Horse, HorseOwner, MountTransitionRequest, MountedOn, RiddenBy};
use super::dismount::find_dismount_pose;
use super::lifecycle::SADDLE_OFFSET;
use crate::combat::context_data::SetMountedCombatMessage;
use crate::health::HostileInteractionImmunity;
use crate::movement::body::BodyDimensions;
use crate::movement::link::{
    ActorLinkRequestMessage, ActorLinkResultMessage, ActorLinkStatus, DetachSafety,
};
use crate::movement::{Actor, BodyVelocity};

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
