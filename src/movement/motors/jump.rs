//! Jump motor — impulse jump with coyote time and input buffering.
//!
//! All five persistent fields (coyote/buffer timers, edge-detection flags)
//! live in a `JumpLocal` per-entity component — was a `Local<JumpLocal>`,
//! promoted so timers/flags don't bleed between actors.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::abilities::JumpMovement;
use crate::movement::facts::GroundFacts;
use crate::movement::intents::Intents;
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide};
use crate::movement::motors::MotorTickItem;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::state::LocomotionState;

/// Persistent jump bookkeeping, per-actor.
///
/// Fields are `pub(crate)`, not private: the multi-actor-migration invariant
/// test (`super::super::actor_isolation_tests`) asserts on them directly to
/// confirm no cross-actor bleed, mirroring the same-module test pattern
/// already used by `MantleState`/`EdgeLeapState` but from outside this file.
#[derive(Component, Default)]
pub struct JumpLocal {
    pub(crate) coyote: f32,
    pub(crate) buffer: f32,
    pub(crate) was_on_floor: bool,
    pub(crate) prev_wants: bool,
    pub(crate) needs_release: bool,
}

/// State indicating whether the current airtime was initiated by a player jump.
#[derive(Component, Default)]
pub struct JumpPhase {
    pub is_player_jump: bool,
}

type ProposeQuery<'a> = (
    &'a GroundFacts,
    &'a Intents,
    &'a JumpMovement,
    &'a LocomotionState,
    &'a mut JumpPhase,
    &'a mut JumpLocal,
    &'a mut ProposalBuffer,
);

pub fn propose(time: Res<Time>, mut q: Query<ProposeQuery, (With<Actor>, With<JumpMovement>)>) {
    let dt = time.delta_secs();
    for (ground, intents, movement, current, mut jump_phase, mut s, mut buffer) in &mut q {
        // Treat stair mode as grounded (stair Y-snap can flicker `grounded` off for a frame).
        let on_floor = ground.grounded || *current == LocomotionState::Stairs;
        let in_jump_arc = *current == LocomotionState::Jump || *current == LocomotionState::Fall;

        if on_floor || !in_jump_arc {
            jump_phase.is_player_jump = false;
        }

        if !intents.jump.held {
            s.needs_release = false;
        }

        // Coyote time: open the window only when walking off a ledge, not after a jump.
        if s.was_on_floor && !on_floor && *current != LocomotionState::Jump {
            s.coyote = movement.coyote_time;
        } else if !on_floor {
            s.coyote = (s.coyote - dt).max(0.0);
        }
        s.was_on_floor = on_floor;

        // Jump buffer: capture the rising edge of JumpIntent::held, hold it briefly.
        if intents.jump.pressed || (intents.jump.held && !s.prev_wants) {
            s.buffer = movement.buffer_time;
        } else if s.buffer > 0.0 {
            s.buffer = (s.buffer - dt).max(0.0);
        }
        s.prev_wants = intents.jump.held;

        let can_jump = on_floor || s.coyote > 0.0;
        let wants =
            (intents.jump.held || intents.jump.pressed || s.buffer > 0.0) && !s.needs_release;

        if can_jump && wants {
            s.coyote = 0.0;
            s.buffer = 0.0;
            s.needs_release = true;
            jump_phase.is_player_jump = true;
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Jump,
                Priority::Forced,
                weight::JUMP,
                "jump",
            ));
        }
    }
}

pub(super) fn tick_body(row: &mut MotorTickItem, mas: &MoveAndSlide, time: &Time) {
    let Some(movement) = row.jump_movement else {
        return;
    };
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
        mas,
        row.entity,
        row.collider,
        &mut row.transform,
        v,
        time.delta(),
        &mut row.contact,
    );
}
