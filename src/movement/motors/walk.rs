//! Walk motor — flat-ground locomotion.
//!
//! Each motor is a `propose` system (runs every frame, in `GatherProposals`)
//! plus a `tick_body` (runs only when Walk is the active state, called by the
//! `motors::tick_active_motor` dispatcher). The tick body is the shared
//! `motor_common::ground_locomotion_step`; its tuning comes from the actor's
//! `GroundMovement` component. See `docs/architecture/movement.md`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::abilities::GroundMovement;
use crate::movement::facts::GroundFacts;
use crate::movement::motor_common::{GroundLocomotionStep, ground_locomotion_step};
use crate::movement::motors::MotorTickItem;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::state::LocomotionState;

type WalkProposalQuery<'a> = (&'a GroundFacts, &'a mut ProposalBuffer);
type WalkProposalFilter = (With<Actor>, With<GroundMovement>);

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

pub(super) fn tick_body(row: &mut MotorTickItem, mas: &MoveAndSlide, time: &Time) {
    let Some(ground_movement) = row.ground_movement else {
        return;
    };
    ground_locomotion_step(
        GroundLocomotionStep {
            entity: row.entity,
            collider: row.collider,
            transform: &mut row.transform,
            velocity: &mut row.velocity,
            intents: row.intents,
            stamina: &mut row.stamina,
            contact: &mut row.contact,
            ground: row.ground,
            state: *row.state,
        },
        LocomotionState::Walk,
        mas,
        time,
        &ground_movement.walk,
    );
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
