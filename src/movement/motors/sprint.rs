//! Sprint motor — faster ground locomotion that drains stamina.
//!
//! Motor-local state (`SprintLock`) is a per-entity component — one instance
//! per actor, so stamina-lock state never bleeds between actors. The tick body
//! is the shared `motor_common::ground_locomotion_step`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::abilities::SprintMovement;
use crate::movement::constraints::LocomotionConstraintFacts;
use crate::movement::facts::{GroundFacts, LedgeFacts, StairsFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{GroundDriveStep, ground_drive_step};
use crate::movement::motors::MotorCore;
use crate::movement::motors::sneak::{Crouched, StandClearance};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;

/// Per-actor stamina-lock latch: set when stamina hits zero, cleared once it
/// recovers past `SPRINT_RECHARGE_THRESHOLD`. Was a `Local<bool>`; promoted to
/// a component so it doesn't bleed across actors.
#[derive(Component, Default)]
pub struct SprintLock(pub bool);

/// Propose SPRINT at PLAYER_REQUESTED priority while grounded, holding sprint,
/// and not stamina-locked. Abstains on stairs (StairsMotor owns the climb), on
/// a climbable wall the player wants to climb (so ClimbMotor can win), and
/// while forced to stay crouched (standing up would expand into a ceiling —
/// SneakMotor keeps the actor down until the full capsule fits).
type ProposeQuery<'a> = (
    &'a SprintMovement,
    &'a GroundFacts,
    Option<&'a StairsFacts>,
    Option<&'a LedgeFacts>,
    &'a Intents,
    &'a Stamina,
    Option<&'a Crouched>,
    Option<&'a StandClearance>,
    Option<&'a LocomotionConstraintFacts>,
    &'a mut SprintLock,
    &'a mut ProposalBuffer,
);

pub fn propose(
    mut q: Query<
        ProposeQuery,
        (
            With<Actor>,
            With<crate::movement::attachment::LocomotionEnabled>,
        ),
    >,
) {
    for (
        profile,
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
        } else if cur >= profile.recharge_threshold {
            stamina_locked.0 = false;
        }

        // Abstain while committed to a combat action (see
        // `constraints::LocomotionConstraintMessage`).
        if constraints.is_some_and(|c| c.forbid_sprint) {
            continue;
        }
        // Abstain so StairsMotor / ClimbMotor / SneakMotor can take over.
        if stairs.is_some_and(|stairs| stairs.on_stairs) {
            continue;
        }
        if ledge.is_some_and(|ledge| ledge.can_climb) && intents.climb.requested {
            continue;
        }
        if crouched.is_some_and(|crouched| crouched.0)
            && clearance.is_some_and(|clearance| !clearance.0)
        {
            continue;
        }

        if ground.grounded && intents.wants_sprint && !stamina_locked.0 {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Sprint,
                Priority::PlayerRequested,
                weight::SPRINT,
                "sprint",
            ));
        }
    }
}

type TickQuery<'a> = (MotorCore, &'a SprintMovement, Option<&'a mut Stamina>);

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
            LocomotionState::Sprint,
            &mas,
            &time,
            &movement.drive,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::abilities::SprintMovement;
    use bevy::ecs::system::RunSystemOnce;

    fn sprint_actor(crouched: bool, clearance: bool) -> impl Bundle {
        (
            Actor,
            crate::movement::attachment::LocomotionEnabled,
            SprintMovement::PLAYER,
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
                crate::movement::attachment::LocomotionEnabled,
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
