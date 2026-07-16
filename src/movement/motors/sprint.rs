//! Sprint motor — faster ground locomotion that drains stamina.
//!
//! Motor-local state (`SprintLock`) is a per-entity component — one instance
//! per actor, so stamina-lock state never bleeds between actors. The tick body
//! is the shared `motor_common::ground_locomotion_step`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::abilities::GroundMovement;
use crate::movement::constraints::LocomotionConstraintFacts;
use crate::movement::facts::{GroundFacts, LedgeFacts, StairsFacts};
use crate::movement::intents::{GaitIntent, Intents};
use crate::movement::motor_common::{GroundLocomotionStep, ground_locomotion_step};
use crate::movement::motors::MotorTickItem;
use crate::movement::motors::sneak::{Crouched, StandClearance};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;

/// Per-actor stamina-lock latch: set when stamina hits zero, cleared once it
/// recovers past `SPRINT_RECHARGE_THRESHOLD`. Was a `Local<bool>`; promoted to
/// a component so it doesn't bleed across actors.
#[derive(Component, Default)]
pub struct SprintLock(pub bool);

const SPRINT_RECHARGE_THRESHOLD: f32 = 20.0;

/// Propose SPRINT at PLAYER_REQUESTED priority while grounded, holding sprint,
/// and not stamina-locked. Abstains on stairs (StairsMotor owns the climb), on
/// a climbable wall the player wants to climb (so ClimbMotor can win), and
/// while forced to stay crouched (standing up would expand into a ceiling —
/// SneakMotor keeps the actor down until the full capsule fits).
type ProposeQuery<'a> = (
    &'a GroundFacts,
    &'a StairsFacts,
    &'a LedgeFacts,
    &'a Intents,
    &'a Stamina,
    &'a Crouched,
    &'a StandClearance,
    Option<&'a LocomotionConstraintFacts>,
    &'a mut SprintLock,
    &'a mut ProposalBuffer,
);

pub fn propose(mut q: Query<ProposeQuery, (With<Actor>, With<GroundMovement>)>) {
    for (
        ground,
        stairs,
        ledge,
        intents,
        stamina,
        crouched,
        clearance,
        constraints,
        mut stamina_locked,
        mut buffer,
    ) in &mut q
    {
        let cur = stamina.current();
        if cur <= 0.0 {
            stamina_locked.0 = true;
        } else if cur >= SPRINT_RECHARGE_THRESHOLD {
            stamina_locked.0 = false;
        }

        // Abstain while committed to a combat action (see
        // `constraints::LocomotionConstraintMessage`).
        if constraints.is_some_and(|c| c.forbid_sprint) {
            continue;
        }
        // Abstain so StairsMotor / ClimbMotor / SneakMotor can take over.
        if stairs.on_stairs {
            continue;
        }
        if ledge.can_climb && intents.climb.requested {
            continue;
        }
        if crouched.0 && !clearance.0 {
            continue;
        }

        if ground.grounded && intents.gait == GaitIntent::Sprint && !stamina_locked.0 {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Sprint,
                Priority::PlayerRequested,
                weight::SPRINT,
                "sprint",
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
        LocomotionState::Sprint,
        mas,
        time,
        &ground_movement.sprint,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn sprint_actor(crouched: bool, clearance: bool) -> impl Bundle {
        (
            Actor,
            GroundMovement::PLAYER,
            GroundFacts {
                grounded: true,
                ..default()
            },
            StairsFacts::default(),
            LedgeFacts::default(),
            Intents {
                gait: crate::movement::intents::GaitIntent::Sprint,
                ..default()
            },
            Stamina::default(),
            Crouched(crouched),
            StandClearance(clearance),
            SprintLock::default(),
            ProposalBuffer::default(),
        )
    }

    fn proposes_sprint(world: &World, entity: Entity) -> bool {
        world
            .entity(entity)
            .get::<ProposalBuffer>()
            .is_some_and(|buffer| buffer.iter().next().is_some())
    }

    #[test]
    fn only_ground_movement_actors_propose_sprint() {
        let mut world = World::new();
        let capable = world.spawn(sprint_actor(false, true)).id();
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
                    gait: crate::movement::intents::GaitIntent::Sprint,
                    ..default()
                },
                Stamina::default(),
                Crouched::default(),
                StandClearance::default(),
                SprintLock::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

        assert!(proposes_sprint(&world, capable));
        assert!(!proposes_sprint(&world, incapable));
    }

    #[test]
    fn sprint_abstains_under_a_forbid_constraint() {
        let mut world = World::new();
        let committed = world
            .spawn((
                sprint_actor(false, true),
                LocomotionConstraintFacts {
                    forbid_sprint: true,
                },
            ))
            .id();
        let free = world
            .spawn((
                sprint_actor(false, true),
                LocomotionConstraintFacts::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

        assert!(!proposes_sprint(&world, committed));
        assert!(proposes_sprint(&world, free));
    }

    #[test]
    fn sprint_yields_while_forced_to_stay_crouched() {
        // Crouched under a ceiling with no room to stand: Sneak's
        // must-remain-crouched proposal must win, or the collider swap on
        // leaving Sneak would expand the capsule into the ceiling.
        let mut world = World::new();
        let blocked = world.spawn(sprint_actor(true, false)).id();
        let free = world.spawn(sprint_actor(true, true)).id();

        world.run_system_once(propose).unwrap();

        assert!(!proposes_sprint(&world, blocked));
        assert!(proposes_sprint(&world, free));
    }
}
