use bevy::prelude::*;

use super::control::MountControlWriters;
use super::data::{Horse, MountTransitionRequest, MountedOn, PendingHorseDespawn, RiddenBy};
use crate::health::DeathMessage;
use crate::movement::attachment::KinematicAttachment;
use crate::movement::{Actor, Player};

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
