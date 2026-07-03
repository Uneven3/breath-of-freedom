//! Mantle motor — kinematic pull-up over a ledge.
//!
//! Its phase machine is shared between `propose` (sticky check) and `tick`
//! (the run), so it must be a **component** (`MantleState`) — `Local` can't be
//! shared across two systems. Movement is a position lerp, not velocity, so
//! `tick` writes `Transform` directly.

use avian3d::prelude::*;
use bevy::prelude::*;
use std::f32::consts::PI;

use crate::movement::facts::{BodyContact, LedgeFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::body_move_and_slide;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::state::LocomotionState;
use crate::movement::{BodyVelocity, Player};

const PRIORITY_WEIGHT: i32 = 10;
const MIN_SPEED: f32 = 0.01;
const MIN_DURATION: f32 = 0.08;
const VERTICAL_SPEED: f32 = 4.0;
const FORWARD_SPEED: f32 = 3.0;
const ARC_HEIGHT: f32 = 0.25;
const TALL_ENOUGH_LIP: f32 = 1.2;

/// Shared phase state for the mantle.
#[derive(Component, Default)]
pub struct MantleState {
    running: bool,
    needs_release: bool,
    elapsed: f32,
    duration: f32,
    start: Vec3,
    target: Vec3,
}

pub fn propose(
    mut q: Single<
        (&Intents, &LocomotionState, &LedgeFacts, &mut MantleState, &mut ProposalBuffer),
        With<Player>,
    >,
) {
    let (intents, current, ledge, state, buffer) = &mut *q;

    if !intents.wants_mantle {
        state.needs_release = false;
    }

    // Sticky: once running, keep MANTLE until tick() finishes.
    if **current == LocomotionState::Mantle && state.running {
        buffer.0.push(TransitionProposal::new(
            LocomotionState::Mantle,
            Priority::Forced,
            PRIORITY_WEIGHT,
            "mantle",
        ));
        return;
    }

    if state.needs_release {
        return;
    }

    let from_climb = **current == LocomotionState::Climb;
    let from_walljump = **current == LocomotionState::WallJump;
    if !(from_climb || from_walljump) {
        return;
    }

    if ledge.is_at_mantle_edge && ledge.lip_height >= TALL_ENOUGH_LIP {
        let requesting = intents.wants_mantle;
        if requesting || (from_walljump && intents.is_climbing_up()) {
            buffer.0.push(TransitionProposal::new(
                LocomotionState::Mantle,
                Priority::Forced,
                PRIORITY_WEIGHT,
                "mantle",
            ));
        }
    }
}

pub fn tick(
    player: Single<
        (
            Entity,
            &Collider,
            &mut Transform,
            &mut BodyVelocity,
            &mut BodyContact,
            &mut MantleState,
            &LedgeFacts,
        ),
        With<Player>,
    >,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    let (entity, collider, mut transform, mut vel, mut contact, mut state, ledge) =
        player.into_inner();
    let dt = time.delta_secs();

    // First active frame: begin the mantle.
    if !state.running && !begin_mantle(&mut state, transform.translation, ledge) {
        // No valid target — hold still for this frame; mantle will drop next frame.
        vel.0 = Vec3::ZERO;
        vel.0 = body_move_and_slide(&mas, entity, collider, &mut transform, Vec3::ZERO, time.delta(), &mut contact);
        return;
    }

    state.elapsed = (state.elapsed + dt).min(state.duration);
    let raw = state.elapsed / state.duration;
    let eased = smoothstep(raw);
    let mut next = state.start.lerp(state.target, eased);
    next.y += (raw * PI).sin() * ARC_HEIGHT;

    transform.translation = next;
    vel.0 = Vec3::ZERO;
    body_move_and_slide(&mas, entity, collider, &mut transform, Vec3::ZERO, time.delta(), &mut contact);

    if raw >= 1.0 {
        transform.translation = state.target;
        state.running = false;
    }
}

fn begin_mantle(state: &mut MantleState, pos: Vec3, ledge: &LedgeFacts) -> bool {
    if ledge.mantle_target_position == Vec3::ZERO {
        return false;
    }
    state.start = pos;
    state.target = ledge.mantle_target_position;
    state.needs_release = true;

    let vertical = (state.target.y - state.start.y).abs();
    let horizontal = Vec2::new(state.start.x, state.start.z)
        .distance(Vec2::new(state.target.x, state.target.z));
    let v_dur = vertical / VERTICAL_SPEED.max(MIN_SPEED);
    let h_dur = horizontal / FORWARD_SPEED.max(MIN_SPEED);
    state.duration = v_dur.max(h_dur).max(MIN_DURATION);
    state.elapsed = 0.0;
    state.running = true;
    true
}

/// `smoothstep(0, 1, x)` = x²(3 − 2x).
fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}
