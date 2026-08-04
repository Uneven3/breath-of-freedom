use bevy_ecs::prelude::*;

use super::data::{
    Horse, HorseSpawnRequest, MountTransitionRequest, PendingHorseDespawn, RiddenBy,
};
use super::lifecycle::spawn_horse_bundle;

type ToggleHorseQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static RiddenBy), (With<Horse>, Without<PendingHorseDespawn>)>;

pub fn process_spawn_requests(
    mut commands: Commands,
    mut requests: MessageReader<HorseSpawnRequest>,
    horses: ToggleHorseQuery,
    mut transitions: MessageWriter<MountTransitionRequest>,
) {
    let exists = !horses.is_empty();
    let mut wanted = exists;
    let mut received = false;
    for request in requests.read().copied() {
        received = true;
        match request {
            HorseSpawnRequest::Ensure => wanted = true,
            HorseSpawnRequest::Toggle => wanted = !wanted,
        }
    }
    if !received || wanted == exists {
        return;
    }

    if wanted {
        commands.spawn(spawn_horse_bundle());
        return;
    }

    for (horse, ridden) in &horses {
        commands.entity(horse).insert(PendingHorseDespawn);
        if let Some(rider) = ridden.0 {
            transitions.write(MountTransitionRequest::Dismount {
                rider,
                horse,
                forced: true,
            });
        }
    }
}
