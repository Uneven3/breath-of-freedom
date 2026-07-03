//! Auto-vault motor — kinematic hop over a waist-high obstacle.
//!
//! Same phase-component pattern as mantle.

use avian3d::prelude::*;
use bevy::prelude::*;
use std::f32::consts::PI;

use crate::movement::facts::{BodyContact, GroundFacts, LedgeFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::body_move_and_slide;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity};

const WEIGHT: i32 = 20;
const MIN_SPEED: f32 = 0.01;
const MIN_DURATION: f32 = 0.1;
const VAULT_SPEED: f32 = 5.0;
const ARC_HEIGHT: f32 = 0.4;

#[derive(Component, Default)]
pub struct VaultState {
    running: bool,
    elapsed: f32,
    duration: f32,
    start: Vec3,
    target: Vec3,
}

type ProposeQuery<'a> = (
    &'a GroundFacts,
    &'a LedgeFacts,
    &'a Intents,
    &'a LocomotionState,
    &'a VaultState,
    &'a mut ProposalBuffer,
);

pub fn propose(mut q: Query<ProposeQuery, With<Actor>>) {
    for (ground, ledge, intents, current, state, mut buffer) in &mut q {
        if *current == LocomotionState::AutoVault && state.running {
            buffer.0.push(TransitionProposal::new(
                LocomotionState::AutoVault,
                Priority::Forced,
                WEIGHT,
                "auto_vault",
            ));
            continue;
        }

        if ground.grounded && ledge.is_vaultable && intents.wants_vault {
            buffer.0.push(TransitionProposal::new(
                LocomotionState::AutoVault,
                Priority::PlayerRequested,
                WEIGHT,
                "auto_vault",
            ));
        }
    }
}

type TickQuery<'a> = (
    Entity,
    &'a Collider,
    &'a mut Transform,
    &'a mut BodyVelocity,
    &'a mut BodyContact,
    &'a mut VaultState,
    &'a LedgeFacts,
    &'a LocomotionState,
);

pub fn tick(mut q: Query<TickQuery, With<Actor>>, mas: MoveAndSlide, time: Res<Time>) {
    let dt = time.delta_secs();
    for (entity, collider, mut transform, mut vel, mut contact, mut state, ledge, loco_state) in
        &mut q
    {
        if *loco_state != LocomotionState::AutoVault {
            continue;
        }

        if !state.running && !begin_vault(&mut state, transform.translation, ledge) {
            state.running = false;
            continue;
        }

        state.elapsed = (state.elapsed + dt).min(state.duration);
        let raw = state.elapsed / state.duration;
        let eased = raw.clamp(0.0, 1.0);
        let eased = eased * eased * (3.0 - 2.0 * eased);
        let mut next = state.start.lerp(state.target, eased);
        next.y += (raw * PI).sin() * ARC_HEIGHT;

        transform.translation = next;
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

        if raw >= 1.0 {
            transform.translation = state.target;
            state.running = false;
        }
    }
}

fn begin_vault(state: &mut VaultState, pos: Vec3, ledge: &LedgeFacts) -> bool {
    if ledge.vault_target_position == Vec3::ZERO {
        return false;
    }
    state.start = pos;
    state.target = ledge.vault_target_position;
    let distance = state.start.distance(state.target);
    state.duration = (distance / VAULT_SPEED.max(MIN_SPEED)).max(MIN_DURATION);
    state.elapsed = 0.0;
    state.running = true;
    true
}
