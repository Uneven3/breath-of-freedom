use bevy::prelude::*;

use super::data::{
    Horse, MountDebugRequest, MountTransitionRequest, PendingHorseDespawn, RiddenBy,
};
use super::lifecycle::spawn_horse_bundle;

type ToggleHorseQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static RiddenBy), (With<Horse>, Without<PendingHorseDespawn>)>;

pub fn process_toggle_requests(
    mut commands: Commands,
    mut requests: MessageReader<MountDebugRequest>,
    horses: ToggleHorseQuery,
    mut transitions: MessageWriter<MountTransitionRequest>,
) {
    for request in requests.read() {
        match request {
            MountDebugRequest::ToggleHorse if horses.is_empty() => {
                commands.spawn(spawn_horse_bundle());
            }
            MountDebugRequest::ToggleHorse => {
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
        }
    }
}
