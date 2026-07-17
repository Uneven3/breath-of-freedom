//! Jump motor — impulse jump with coyote time and input buffering.
//!
//! All five persistent fields (coyote/buffer timers, edge-detection flags)
//! live in a `JumpLocal` per-entity component — was a `Local<JumpLocal>`,
//! promoted so timers/flags don't bleed between actors.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::abilities::{JumpMovement, JumpStaminaCost};
use crate::movement::facts::GroundFacts;
use crate::movement::intents::Intents;
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide};
use crate::movement::motors::MotorCore;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;

/// Persistent jump bookkeeping, per-actor.
///
/// Fields are `pub(crate)`, not private: the multi-actor-migration invariant
/// test (`super::super::actor_isolation_tests`) asserts on them directly to
/// confirm no cross-actor bleed, mirroring the same-module test pattern
/// already used by `MantleState`/`EdgeLeapState` but from outside this file.
#[derive(Component, Default, Debug, Clone, Copy, PartialEq)]
pub struct JumpLocal {
    pub(crate) coyote: f32,
    pub(crate) buffer: f32,
    pub(crate) was_on_floor: bool,
    pub(crate) prev_wants: bool,
    pub(crate) needs_release: bool,
}

/// State indicating whether the current airtime was initiated by a player jump.
#[derive(Component, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct JumpPhase {
    pub is_player_jump: bool,
}

type ProposeQuery<'a> = (
    &'a GroundFacts,
    &'a Intents,
    &'a JumpMovement,
    Option<&'a JumpStaminaCost>,
    Option<&'a Stamina>,
    &'a LocomotionState,
    &'a mut JumpPhase,
    &'a mut JumpLocal,
    &'a mut ProposalBuffer,
);

type ProposeFilter = (
    crate::movement::attachment::LocomotionActorFilter,
    With<JumpMovement>,
);

pub fn propose(time: Res<Time>, mut q: Query<ProposeQuery, ProposeFilter>) {
    let dt = time.delta_secs();
    for (ground, intents, movement, cost, stamina, current, mut jump_phase, mut s, mut buffer) in
        &mut q
    {
        let mut next_phase = *jump_phase;
        let mut next = *s;
        // Treat stair mode as grounded (stair Y-snap can flicker `grounded` off for a frame).
        let on_floor = ground.grounded || *current == LocomotionState::Stairs;
        let in_jump_arc = *current == LocomotionState::Jump || *current == LocomotionState::Fall;

        if on_floor || !in_jump_arc {
            next_phase.is_player_jump = false;
        }

        if !intents.jump.held {
            next.needs_release = false;
        }

        // Coyote time: open the window only when walking off a ledge, not after a jump.
        if next.was_on_floor && !on_floor && *current != LocomotionState::Jump {
            next.coyote = movement.coyote_time;
        } else if !on_floor {
            next.coyote = (next.coyote - dt).max(0.0);
        }
        next.was_on_floor = on_floor;

        // Jump buffer: capture the rising edge of JumpIntent::held, hold it briefly.
        if intents.jump.pressed || (intents.jump.held && !next.prev_wants) {
            next.buffer = movement.buffer_time;
        } else if next.buffer > 0.0 {
            next.buffer = (next.buffer - dt).max(0.0);
        }
        next.prev_wants = intents.jump.held;

        let can_jump = on_floor || next.coyote > 0.0;
        let wants =
            (intents.jump.held || intents.jump.pressed || next.buffer > 0.0) && !next.needs_release;

        let can_pay =
            cost.is_none_or(|cost| stamina.is_some_and(|stamina| stamina.current() >= cost.0));
        if can_jump && wants && can_pay {
            if buffer
                .push(TransitionProposal::new(
                    LocomotionState::Jump,
                    Priority::Forced,
                    weight::JUMP,
                    "jump",
                ))
                .is_err()
            {
                continue;
            }
            next.coyote = 0.0;
            next.buffer = 0.0;
            next.needs_release = true;
            next_phase.is_player_jump = true;
        }
        *s = next;
        *jump_phase = next_phase;
    }
}

pub fn pay_accepted_cost(
    mut actors: Query<
        (
            &LocomotionState,
            &JumpStaminaCost,
            &mut crate::movement::stamina::Stamina,
        ),
        Changed<LocomotionState>,
    >,
) {
    for (state, cost, mut stamina) in &mut actors {
        if *state == LocomotionState::Jump {
            stamina.drain(cost.0);
        }
    }
}

type TickQuery<'a> = (MotorCore, &'a JumpMovement);

pub fn tick_body(
    mut actors: Query<TickQuery, crate::movement::attachment::LocomotionActorFilter>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    for (mut row, movement) in &mut actors {
        if *row.state != LocomotionState::Jump {
            continue;
        }
        let dt = time.delta_secs();

        apply_locomotion_rotation(
            &mut row.transform,
            row.intents.planar.direction,
            dt,
            movement.rotation_speed,
        );

        let mut v = row.velocity.0;
        v.y = movement.impulse;
        row.velocity.0 = body_move_and_slide(
            &mas,
            row.entity,
            row.collider,
            &mut row.transform,
            v,
            time.delta(),
            &mut row.contact,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::Actor;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn stamina_is_paid_only_for_an_accepted_jump_state() {
        let mut world = World::new();
        let accepted = world
            .spawn((
                LocomotionState::Jump,
                JumpStaminaCost(20.0),
                Stamina::default(),
            ))
            .id();
        let defeated = world
            .spawn((
                LocomotionState::WallJump,
                JumpStaminaCost(20.0),
                Stamina::default(),
            ))
            .id();
        world.run_system_once(pay_accepted_cost).unwrap();
        assert_eq!(
            world.entity(accepted).get::<Stamina>().unwrap().current(),
            80.0
        );
        assert_eq!(
            world.entity(defeated).get::<Stamina>().unwrap().current(),
            100.0
        );
    }

    #[test]
    fn rejected_overflow_proposal_has_no_stamina_side_effect() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mut proposals = ProposalBuffer::default();
        for index in 0..crate::movement::proposal::MOVEMENT_PROPOSAL_CAPACITY {
            assert!(
                proposals
                    .push(TransitionProposal::new(
                        LocomotionState::WallJump,
                        Priority::Forced,
                        weight::WALL_JUMP,
                        if index == 0 { "full" } else { "overflow_guard" },
                    ))
                    .is_ok()
            );
        }
        let actor = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                Intents {
                    jump: crate::movement::intents::JumpIntent {
                        held: true,
                        pressed: true,
                    },
                    ..default()
                },
                JumpMovement::PLAYER,
                JumpStaminaCost(20.0),
                Stamina::default(),
                LocomotionState::Walk,
                JumpPhase::default(),
                JumpLocal::default(),
                proposals,
            ))
            .id();
        world.run_system_once(propose).unwrap();
        assert_eq!(
            *world.entity(actor).get::<LocomotionState>().unwrap(),
            LocomotionState::Walk
        );
        assert_eq!(
            world.entity(actor).get::<Stamina>().unwrap().current(),
            100.0
        );
        assert_eq!(
            *world.entity(actor).get::<JumpLocal>().unwrap(),
            JumpLocal::default()
        );
        assert_eq!(
            *world.entity(actor).get::<JumpPhase>().unwrap(),
            JumpPhase::default()
        );
    }
}
