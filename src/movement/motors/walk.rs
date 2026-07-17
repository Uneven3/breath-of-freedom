//! Walk motor — flat-ground locomotion.
//!
//! Each motor is a `propose` system (runs every frame, in `GatherProposals`)
//! plus a `tick_body` (runs only when Walk is the active state, called by the
//! capability-specific tick system). The tick body is the shared
//! `motor_common::ground_locomotion_step`; its tuning comes from the actor's
//! `GroundMovement` component. See `docs/ARCHITECTURE.md`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::abilities::GroundMovement;
use crate::movement::facts::GroundFacts;
use crate::movement::motor_common::{GroundDriveStep, ground_drive_step};
use crate::movement::motors::MotorCore;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;

type WalkProposalQuery<'a> = (&'a GroundFacts, &'a mut ProposalBuffer);
type WalkProposalFilter = (
    With<Actor>,
    With<GroundMovement>,
    With<crate::movement::attachment::LocomotionEnabled>,
);

/// Propose WALK at DEFAULT priority whenever grounded — the standing-still
/// fallback (there is no Idle state), not a player request.
pub fn propose(mut q: Query<WalkProposalQuery, WalkProposalFilter>) {
    for (ground, mut buffer) in &mut q {
        if ground.grounded {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Walk,
                Priority::Default,
                weight::WALK,
                "walk",
            ));
        }
    }
}

type TickQuery<'a> = (MotorCore, &'a GroundMovement, Option<&'a mut Stamina>);

pub fn tick_body(
    mut actors: Query<TickQuery, crate::movement::attachment::LocomotionActorFilter>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    for (mut row, movement, mut stamina) in &mut actors {
        ground_drive_step(
            GroundDriveStep {
                entity: row.entity,
                collider: row.collider,
                transform: &mut row.transform,
                velocity: &mut row.velocity,
                intents: row.intents,
                stamina: stamina.as_deref_mut(),
                contact: &mut row.contact,
                ground: row.ground,
                state: *row.state,
            },
            LocomotionState::Walk,
            &mas,
            &time,
            &movement.drive,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn only_ground_movement_actors_propose_walk() {
        let mut world = World::new();
        let capable = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                GroundMovement::PLAYER,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                ProposalBuffer::default(),
            ))
            .id();
        let incapable = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

        assert!(
            world
                .entity(capable)
                .get::<ProposalBuffer>()
                .is_some_and(|buffer| buffer.iter().next().is_some())
        );
        assert!(
            world
                .entity(incapable)
                .get::<ProposalBuffer>()
                .is_some_and(|buffer| buffer.iter().next().is_none())
        );
    }
}
