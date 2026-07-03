//! Glide motor — slow-fall with air control while holding jump in the air.
//!
//! Glide-press memory and its reset on deactivation both live in a
//! `Local<GlideLocal>`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::{BodyContact, GroundFacts, LedgeFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide, move_toward};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{BodyVelocity, GRAVITY, Player};

const GLIDE_FALL_SPEED: f32 = 1.5;
const GLIDE_GRAVITY_MULTIPLIER: f32 = 0.25;
const MAX_GLIDE_SPEED: f32 = 6.0;
const GLIDE_ACCELERATION: f32 = 4.0;
const STAMINA_RECOVER_PER_SEC: f32 = 8.0;
const STAMINA_RECOVERY_FACTOR: f32 = 0.25;

#[derive(Default)]
pub struct GlideLocal {
    prev_wants: bool,
    was_glide: bool,
}

pub fn propose(
    mut s: Local<GlideLocal>,
    mut q: Single<
        (
            &GroundFacts,
            &LedgeFacts,
            &Intents,
            &LocomotionState,
            &mut ProposalBuffer,
        ),
        With<Player>,
    >,
) {
    let (ground, ledge, intents, current, buffer) = &mut *q;

    // Clear glide-press memory when leaving GLIDE, so a fresh press right after
    // (e.g. leaving a wall) is not suppressed.
    if s.was_glide && **current != LocomotionState::Glide {
        s.prev_wants = false;
        s.was_glide = false;
    }

    if ground.grounded {
        s.prev_wants = intents.wants_glide;
        return;
    }

    if **current == LocomotionState::Glide {
        s.was_glide = true;
        s.prev_wants = intents.wants_glide;
        if intents.wants_glide {
            // Downgrade to PLAYER_REQUESTED on a climbable wall so ClimbMotor's
            // weight-5 PLAYER_REQUESTED out-arbitrates glide.
            let category = if ledge.can_climb && intents.wants_climb {
                Priority::PlayerRequested
            } else {
                Priority::Forced
            };
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Glide,
                category,
                0,
                "glide",
            ));
        }
        return;
    }

    let fresh_press = intents.wants_glide && !s.prev_wants;
    s.prev_wants = intents.wants_glide;
    if **current == LocomotionState::Fall && fresh_press {
        let _ = buffer.push(TransitionProposal::new(
            LocomotionState::Glide,
            Priority::PlayerRequested,
            0,
            "glide",
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
            &mut Stamina,
            &mut BodyContact,
        ),
        With<Player>,
    >,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    let (entity, collider, mut transform, mut vel, intents, mut stamina, mut contact) =
        player.into_inner();
    let dt = time.delta_secs();

    apply_locomotion_rotation(&mut transform, intents.move_dir, dt, 15.0);

    let mut v = vel.0;
    v.y -= GRAVITY * GLIDE_GRAVITY_MULTIPLIER * dt;
    v.y = v.y.max(-GLIDE_FALL_SPEED);

    let move_dir = Vec3::new(intents.move_dir.x, 0.0, intents.move_dir.y).normalize_or_zero();
    if move_dir != Vec3::ZERO {
        v.x = move_toward(v.x, move_dir.x * MAX_GLIDE_SPEED, GLIDE_ACCELERATION * dt);
        v.z = move_toward(v.z, move_dir.z * MAX_GLIDE_SPEED, GLIDE_ACCELERATION * dt);
    }

    stamina.recover(STAMINA_RECOVER_PER_SEC * STAMINA_RECOVERY_FACTOR * dt);

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
