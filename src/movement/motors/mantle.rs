//! Mantle motor — kinematic pull-up over a ledge.
//!
//! Its phase machine is shared between `propose` (sticky check) and `tick`
//! (the run), so it must be a **component** (`MantleState`) — `Local` can't be
//! shared across two systems. Movement is a position lerp
//! (`motor_common::KinematicArc`, shared with AutoVault), not velocity, so
//! `tick` writes `Transform` directly.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::{BodyContact, LedgeFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{KinematicArc, body_move_and_slide};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity};

const PRIORITY_WEIGHT: u32 = 10;
const MIN_SPEED: f32 = 0.01;
const MIN_DURATION: f32 = 0.08;
const VERTICAL_SPEED: f32 = 4.0;
const FORWARD_SPEED: f32 = 3.0;
const ARC_HEIGHT: f32 = 0.25;
const TALL_ENOUGH_LIP: f32 = 1.2;

/// Shared phase state for the mantle.
#[derive(Component, Default)]
pub struct MantleState {
    pub(crate) arc: KinematicArc,
    needs_release: bool,
}

pub fn propose(
    mut q: Query<
        (
            &Intents,
            &LocomotionState,
            &LedgeFacts,
            &mut MantleState,
            &mut ProposalBuffer,
        ),
        With<Actor>,
    >,
) {
    for (intents, current, ledge, mut state, mut buffer) in &mut q {
        if !intents.wants_mantle {
            state.needs_release = false;
        }

        // Sticky: once running, keep MANTLE until tick() finishes.
        if *current == LocomotionState::Mantle && state.arc.running {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Mantle,
                Priority::Forced,
                PRIORITY_WEIGHT,
                "mantle",
            ));
            continue;
        }

        if state.needs_release {
            continue;
        }

        let from_climb = *current == LocomotionState::Climb;
        let from_walljump = *current == LocomotionState::WallJump;
        if !(from_climb || from_walljump) {
            continue;
        }

        if ledge.is_at_mantle_edge && ledge.lip_height >= TALL_ENOUGH_LIP {
            let requesting = intents.wants_mantle;
            if requesting || (from_walljump && intents.is_climbing_up()) {
                let _ = buffer.push(TransitionProposal::new(
                    LocomotionState::Mantle,
                    Priority::Forced,
                    PRIORITY_WEIGHT,
                    "mantle",
                ));
            }
        }
    }
}

type TickQuery<'a> = (
    Entity,
    &'a Collider,
    &'a mut Transform,
    &'a mut BodyVelocity,
    &'a mut BodyContact,
    &'a mut MantleState,
    &'a LedgeFacts,
    &'a LocomotionState,
);

pub fn tick(mut q: Query<TickQuery, With<Actor>>, mas: MoveAndSlide, time: Res<Time>) {
    let dt = time.delta_secs();
    for (entity, collider, mut transform, mut vel, mut contact, mut state, ledge, loco_state) in
        &mut q
    {
        if *loco_state != LocomotionState::Mantle {
            continue;
        }

        // First active frame: begin the mantle. No valid target — hold still
        // for this frame; mantle will drop next frame.
        if state.arc.running || begin_mantle(&mut state, transform.translation, ledge) {
            transform.translation = state.arc.step(dt, ARC_HEIGHT);
        }

        vel.0 = Vec3::ZERO;
        body_move_and_slide(
            &mas,
            entity,
            collider,
            &mut transform,
            Vec3::ZERO,
            time.delta(),
            &mut contact,
        );
    }
}

fn begin_mantle(state: &mut MantleState, pos: Vec3, ledge: &LedgeFacts) -> bool {
    let Some(target) = ledge.mantle_target_position else {
        return false;
    };
    state.needs_release = true;

    let vertical = (target.y - pos.y).abs();
    let horizontal = Vec2::new(pos.x, pos.z).distance(Vec2::new(target.x, target.z));
    let v_dur = vertical / VERTICAL_SPEED.max(MIN_SPEED);
    let h_dur = horizontal / FORWARD_SPEED.max(MIN_SPEED);
    state
        .arc
        .begin(pos, target, v_dur.max(h_dur).max(MIN_DURATION));
    true
}
