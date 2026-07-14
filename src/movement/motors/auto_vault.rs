//! Auto-vault motor — kinematic hop over a waist-high obstacle.
//!
//! Same phase-component pattern as mantle, sharing its
//! `motor_common::KinematicArc`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::{BodyContact, GroundFacts, LedgeFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{KinematicArc, body_move_and_slide};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity};

const WEIGHT: u32 = 20;
const MIN_SPEED: f32 = 0.01;
const MIN_DURATION: f32 = 0.1;
const VAULT_SPEED: f32 = 5.0;
const ARC_HEIGHT: f32 = 0.4;

#[derive(Component, Default)]
pub struct VaultState {
    pub(crate) arc: KinematicArc,
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
        if *current == LocomotionState::AutoVault && state.arc.running {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::AutoVault,
                Priority::Forced,
                WEIGHT,
                "auto_vault",
            ));
            continue;
        }

        if ground.grounded && ledge.is_vaultable && intents.wants_vault {
            let _ = buffer.push(TransitionProposal::new(
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

        if !state.arc.running && !begin_vault(&mut state, transform.translation, ledge) {
            continue;
        }

        transform.translation = state.arc.step(dt, ARC_HEIGHT);
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

fn begin_vault(state: &mut VaultState, pos: Vec3, ledge: &LedgeFacts) -> bool {
    let Some(target) = ledge.vault_target_position else {
        return false;
    };
    let duration = (pos.distance(target) / VAULT_SPEED.max(MIN_SPEED)).max(MIN_DURATION);
    state.arc.begin(pos, target, duration);
    true
}
