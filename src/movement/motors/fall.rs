//! Fall motor — airborne fallback with asymmetric gravity and air control.
//!
//! FALL is the default state when no motor proposes anything stronger
//! (DEFAULT priority).

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::{BodyContact, GroundFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide, move_toward};
use crate::movement::motors::jump::JumpPhase;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity, GRAVITY};

const MAX_AIR_SPEED: f32 = 5.0;
const AIR_ACCELERATION: f32 = 5.0;
const RISE_GRAVITY_MULTIPLIER: f32 = 1.3;
const FALL_GRAVITY_MULTIPLIER: f32 = 2.5;
const JUMP_CUT_VELOCITY: f32 = 2.0;
const STAMINA_RECOVER_PER_SEC: f32 = 15.0;
const FALL_STAMINA_RECOVER_FRACTION: f32 = 0.25;

/// Propose FALL at DEFAULT priority whenever airborne.
pub fn propose(mut q: Query<(&GroundFacts, &mut ProposalBuffer), With<Actor>>) {
    for (ground, mut buffer) in &mut q {
        if !ground.grounded {
            buffer.0.push(TransitionProposal::new(
                LocomotionState::Fall,
                Priority::Default,
                0,
                "fall",
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
    &'a mut Stamina,
    &'a mut BodyContact,
    &'a JumpPhase,
    &'a LocomotionState,
);

pub fn tick(mut q: Query<TickQuery, With<Actor>>, mas: MoveAndSlide, time: Res<Time>) {
    let dt = time.delta_secs();
    for (
        entity,
        collider,
        mut transform,
        mut vel,
        intents,
        mut stamina,
        mut contact,
        jump_phase,
        state,
    ) in &mut q
    {
        if *state != LocomotionState::Fall {
            continue;
        }

        apply_locomotion_rotation(&mut transform, intents.move_dir, dt, 15.0);

        let mut v = vel.0;

        // Jump cut: releasing jump on the way up clips upward velocity for a short hop.
        if jump_phase.is_player_jump && !intents.wants_jump && v.y > JUMP_CUT_VELOCITY {
            v.y = JUMP_CUT_VELOCITY;
        }

        // Asymmetric gravity: snappier rise, heavier fall.
        if v.y < 0.0 {
            v.y -= GRAVITY * FALL_GRAVITY_MULTIPLIER * dt;
        } else {
            v.y -= GRAVITY * RISE_GRAVITY_MULTIPLIER * dt;
        }

        let move_dir = Vec3::new(intents.move_dir.x, 0.0, intents.move_dir.y).normalize_or_zero();
        if move_dir != Vec3::ZERO {
            v.x = move_toward(v.x, move_dir.x * MAX_AIR_SPEED, AIR_ACCELERATION * dt);
            v.z = move_toward(v.z, move_dir.z * MAX_AIR_SPEED, AIR_ACCELERATION * dt);
        }

        stamina.recover(STAMINA_RECOVER_PER_SEC * FALL_STAMINA_RECOVER_FRACTION * dt);

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
