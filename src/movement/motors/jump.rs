//! Jump motor — impulse jump with coyote time and input buffering.
//!
//! All five persistent fields (coyote/buffer timers, edge-detection flags)
//! live in a `Local<JumpLocal>` on the propose system.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::{BodyContact, GroundFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::state::LocomotionState;
use crate::movement::{BodyVelocity, Player};

const JUMP_IMPULSE: f32 = 5.5;
const COYOTE_TIME: f32 = 0.12;
const JUMP_BUFFER_TIME: f32 = 0.12;

/// Persistent jump bookkeeping, scoped to the propose system.
#[derive(Default)]
pub struct JumpLocal {
    coyote: f32,
    buffer: f32,
    was_on_floor: bool,
    prev_wants: bool,
    needs_release: bool,
}

/// State indicating whether the current airtime was initiated by a player jump.
#[derive(Component, Default)]
pub struct JumpPhase {
    pub is_player_jump: bool,
}

pub fn propose(
    mut s: Local<JumpLocal>,
    time: Res<Time>,
    mut q: Single<
        (
            &GroundFacts,
            &Intents,
            &LocomotionState,
            &mut JumpPhase,
            &mut ProposalBuffer,
        ),
        With<Player>,
    >,
) {
    let (ground, intents, current, jump_phase, buffer) = &mut *q;
    let dt = time.delta_secs();
    // Treat stair mode as grounded (stair Y-snap can flicker `grounded` off for a frame).
    let on_floor = ground.grounded || **current == LocomotionState::Stairs;
    let in_jump_arc = **current == LocomotionState::Jump || **current == LocomotionState::Fall;

    if on_floor || !in_jump_arc {
        jump_phase.is_player_jump = false;
    }

    if !intents.wants_jump {
        s.needs_release = false;
    }

    // Coyote time: open the window only when walking off a ledge, not after a jump.
    if s.was_on_floor && !on_floor && **current != LocomotionState::Jump {
        s.coyote = COYOTE_TIME;
    } else if !on_floor {
        s.coyote = (s.coyote - dt).max(0.0);
    }
    s.was_on_floor = on_floor;

    // Jump buffer: capture the rising edge of wants_jump, hold the intent briefly.
    if intents.wants_jump && !s.prev_wants {
        s.buffer = JUMP_BUFFER_TIME;
    } else if s.buffer > 0.0 {
        s.buffer = (s.buffer - dt).max(0.0);
    }
    s.prev_wants = intents.wants_jump;

    let can_jump = on_floor || s.coyote > 0.0;
    let wants = (intents.wants_jump || s.buffer > 0.0) && !s.needs_release;

    if can_jump && wants {
        s.coyote = 0.0;
        s.buffer = 0.0;
        s.needs_release = true;
        jump_phase.is_player_jump = true;
        buffer.0.push(TransitionProposal::new(
            LocomotionState::Jump,
            Priority::Forced,
            0,
            "jump",
        ));
    }
}

pub fn tick(
    player: Single<
        (
            Entity,
            &Collider,
            &mut Transform,
            &mut BodyVelocity,
            &Intents,
            &mut BodyContact,
        ),
        With<Player>,
    >,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    let (entity, collider, mut transform, mut vel, intents, mut contact) = player.into_inner();
    let dt = time.delta_secs();

    apply_locomotion_rotation(&mut transform, intents.move_dir, dt, 15.0);

    let mut v = vel.0;
    v.y = JUMP_IMPULSE;
    vel.0 = body_move_and_slide(&mas, entity, collider, &mut transform, v, time.delta(), &mut contact);
}
