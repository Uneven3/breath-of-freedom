//! Sprint motor — faster ground locomotion that drains stamina.
//!
//! Motor-local state (`SprintLock`) is a per-entity component — one instance
//! per actor, so stamina-lock state never bleeds between actors. The tick body
//! is the shared `motor_common::ground_locomotion_step`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::abilities::GroundMovement;
use crate::movement::facts::{GroundFacts, LedgeFacts, StairsFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{
    GroundLocomotionStep, GroundTickQuery, ground_locomotion_step,
};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;

/// Per-actor stamina-lock latch: set when stamina hits zero, cleared once it
/// recovers past `SPRINT_RECHARGE_THRESHOLD`. Was a `Local<bool>`; promoted to
/// a component so it doesn't bleed across actors.
#[derive(Component, Default)]
pub struct SprintLock(pub bool);

const SPRINT_RECHARGE_THRESHOLD: f32 = 20.0;

/// Propose SPRINT at OPPORTUNISTIC priority while grounded, holding sprint, and not
/// stamina-locked. Abstains on stairs (StairsMotor owns the climb) and on a climbable
/// wall the player wants to climb (so ClimbMotor can win).
type ProposeQuery<'a> = (
    &'a GroundFacts,
    &'a StairsFacts,
    &'a LedgeFacts,
    &'a Intents,
    &'a Stamina,
    &'a mut SprintLock,
    &'a mut ProposalBuffer,
);

pub fn propose(mut q: Query<ProposeQuery, (With<Actor>, With<GroundMovement>)>) {
    for (ground, stairs, ledge, intents, stamina, mut stamina_locked, mut buffer) in &mut q {
        let cur = stamina.current();
        if cur <= 0.0 {
            stamina_locked.0 = true;
        } else if cur >= SPRINT_RECHARGE_THRESHOLD {
            stamina_locked.0 = false;
        }

        // Abstain so StairsMotor / ClimbMotor can take over.
        if stairs.on_stairs {
            continue;
        }
        if ledge.can_climb && intents.wants_climb {
            continue;
        }

        if ground.grounded && intents.wants_sprint && !stamina_locked.0 {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Sprint,
                Priority::Opportunistic,
                0,
                "sprint",
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
            LocomotionState::Sprint,
            &mas,
            &time,
            &ground_movement.sprint,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn only_ground_movement_actors_propose_sprint() {
        let mut world = World::new();
        let capable = world
            .spawn((
                Actor,
                GroundMovement::PLAYER,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                StairsFacts::default(),
                LedgeFacts::default(),
                Intents {
                    wants_sprint: true,
                    ..default()
                },
                Stamina::default(),
                SprintLock::default(),
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
                StairsFacts::default(),
                LedgeFacts::default(),
                Intents {
                    wants_sprint: true,
                    ..default()
                },
                Stamina::default(),
                SprintLock::default(),
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
