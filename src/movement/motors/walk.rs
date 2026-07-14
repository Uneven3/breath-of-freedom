//! Walk motor — flat-ground locomotion.
//!
//! Each motor is two systems: `propose` (runs every frame, in
//! `GatherProposals`) and `tick` (runs only when Walk is the active state, in
//! `TickActiveMotor`). The tick body is the shared
//! `motor_common::ground_locomotion_step`; its tuning comes from the actor's
//! `GroundMovement` component. See `docs/architecture/movement.md`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::abilities::GroundMovement;
use crate::movement::facts::GroundFacts;
use crate::movement::motor_common::{
    GroundLocomotionStep, GroundTickQuery, ground_locomotion_step,
};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::state::LocomotionState;

type WalkProposalQuery<'a> = (&'a GroundFacts, &'a mut ProposalBuffer);
type WalkProposalFilter = (With<Actor>, With<GroundMovement>);

/// Propose WALK at PLAYER_REQUESTED priority whenever grounded.
pub fn propose(mut q: Query<WalkProposalQuery, WalkProposalFilter>) {
    for (ground, mut buffer) in &mut q {
        if ground.grounded {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Walk,
                Priority::PlayerRequested,
                0,
                "walk",
            ));
        }
    }
}

pub fn tick(
    mut q: Query<(&GroundMovement, GroundTickQuery), With<Actor>>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    for (ground_movement, row) in &mut q {
        let (
            entity,
            collider,
            mut transform,
            mut velocity,
            intents,
            mut stamina,
            mut contact,
            ground,
            state,
        ) = row;
        ground_locomotion_step(
            GroundLocomotionStep {
                entity,
                collider,
                transform: &mut transform,
                velocity: &mut velocity,
                intents,
                stamina: &mut stamina,
                contact: &mut contact,
                ground,
                state: *state,
            },
            LocomotionState::Walk,
            &mas,
            &time,
            &ground_movement.walk,
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
