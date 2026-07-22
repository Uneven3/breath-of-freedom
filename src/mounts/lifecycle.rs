use bevy::prelude::*;

use crate::health::Health;
use crate::movement::abilities::{
    AirborneMovement, GroundMovement, JumpMovement, JumpStaminaCost, SprintMovement, StairsMovement,
};
use crate::movement::body::BodyDimensions;
use crate::movement::bundles::{
    GroundMovementBundle, JumpMovementBundle, KinematicActorBundle, SprintMovementBundle,
    StairsMovementBundle, StaminaBundle,
};
use crate::movement::sensing::GroundSensing;

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
pub(super) const SADDLE_OFFSET: Vec3 = Vec3::new(0.0, 1.65, 0.0);
pub(super) const DISMOUNT_DISTANCE: f32 = 1.9;
pub(super) const FLOOR_MIN_UP_DOT: f32 = 0.5;

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

pub use super::recovery::{
    collect_horse_deaths, despawn_released_horses, reconcile_relationships, recover_detached_riders,
};
pub use super::transition::{apply_transitions, confirm_transitions};

#[cfg(test)]
mod tests;
