//! Jump motor — impulse jump with coyote time and input buffering.
//!
//! All five persistent fields (coyote/buffer timers, edge-detection flags)
//! live in a `JumpLocal` per-entity component — was a `Local<JumpLocal>`,
//! promoted so timers/flags don't bleed between actors.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::abilities::JumpMovement;
use crate::movement::facts::{BodyContact, GroundFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity};

/// Above Stairs/Ladder (weight 0) so jumping off them is deterministic, below
/// the specialized climb-state jumps (WallJump 5, EdgeLeap 10, Mantle 10).
/// Ties in arbitration otherwise fall back to system execution order, which
/// Bevy does not guarantee.
const FORCED_WEIGHT: u32 = 1;

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

        if !intents.wants_jump {
            s.needs_release = false;
        }

        // Coyote time: open the window only when walking off a ledge, not after a jump.
        if s.was_on_floor && !on_floor && *current != LocomotionState::Jump {
            s.coyote = movement.coyote_time;
        } else if !on_floor {
            s.coyote = (s.coyote - dt).max(0.0);
        }
        s.was_on_floor = on_floor;

        // Jump buffer: capture the rising edge of wants_jump, hold the intent briefly.
        if intents.jump_pressed || (intents.wants_jump && !s.prev_wants) {
            s.buffer = movement.buffer_time;
        } else if s.buffer > 0.0 {
            s.buffer = (s.buffer - dt).max(0.0);
        }
        s.prev_wants = intents.wants_jump;

        let can_jump = on_floor || s.coyote > 0.0;
        let wants =
            (intents.wants_jump || intents.jump_pressed || s.buffer > 0.0) && !s.needs_release;

        if can_jump && wants {
            s.coyote = 0.0;
            s.buffer = 0.0;
            s.needs_release = true;
            jump_phase.is_player_jump = true;
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Jump,
                Priority::Forced,
                FORCED_WEIGHT,
                "jump",
            ));
        }
    }
}

type TickQuery<'a> = (
    Entity,
    &'a Collider,
    &'a mut Transform,
    &'a mut BodyVelocity,
    &'a Intents,
    &'a mut BodyContact,
    &'a JumpMovement,
    &'a LocomotionState,
);

pub fn tick(
    mut q: Query<TickQuery, (With<Actor>, With<JumpMovement>)>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (entity, collider, mut transform, mut vel, intents, mut contact, movement, state) in &mut q
    {
        if *state != LocomotionState::Jump {
            continue;
        }

        apply_locomotion_rotation(
            &mut transform,
            intents.move_dir,
            dt,
            movement.rotation_speed,
        );

        let mut v = vel.0;
        v.y = movement.impulse;
        vel.0 = body_move_and_slide(
            &mas,
            entity,
            collider,
            &mut transform,
            v,
            time.delta(),
            &mut contact,
        );
    }
}
