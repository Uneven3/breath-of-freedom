//! Glide motor — slow-fall with air control while holding jump in the air.
//!
//! Glide-press memory and its reset on deactivation both live in a per-entity
//! `GlideLocal` component — was a `Local<GlideLocal>`, promoted so it doesn't
//! bleed between actors.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::GRAVITY;
use crate::movement::abilities::GlideMovement;
use crate::movement::facts::{GroundFacts, LedgeFacts};
use crate::movement::intents::{GlideIntent, Intents};
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide, move_toward};
use crate::movement::motors::MotorCore;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;

#[derive(Component, Default)]
pub struct GlideLocal {
    prev_wants: bool,
    was_glide: bool,
}

type ProposeQuery<'a> = (
    &'a GroundFacts,
    Option<&'a LedgeFacts>,
    &'a Intents,
    &'a LocomotionState,
    &'a mut GlideLocal,
    &'a mut ProposalBuffer,
);

type ProposeFilter = (
    crate::movement::attachment::LocomotionActorFilter,
    With<GlideMovement>,
);

pub fn propose(mut q: Query<ProposeQuery, ProposeFilter>) {
    for (ground, ledge, intents, current, mut s, mut buffer) in &mut q {
        // Clear glide-press memory when leaving GLIDE, so a fresh press right after
        // (e.g. leaving a wall) is not suppressed.
        if s.was_glide && *current != LocomotionState::Glide {
            s.prev_wants = false;
            s.was_glide = false;
        }

        if ground.grounded {
            s.prev_wants = intents.glide == GlideIntent::Requested;
            continue;
        }

        if *current == LocomotionState::Glide {
            s.was_glide = true;
            s.prev_wants = intents.glide == GlideIntent::Requested;
            if intents.glide == GlideIntent::Requested {
                // Downgrade to PLAYER_REQUESTED on a climbable wall so ClimbMotor's
                // heavier PLAYER_REQUESTED out-arbitrates glide (see `weight`).
                let category =
                    if ledge.is_some_and(|ledge| ledge.can_climb) && intents.climb.requested {
                        Priority::PlayerRequested
                    } else {
                        Priority::Forced
                    };
                let _ = buffer.push(TransitionProposal::new(
                    LocomotionState::Glide,
                    category,
                    weight::GLIDE,
                    "glide",
                ));
            }
            continue;
        }

        let requesting = intents.glide == GlideIntent::Requested;
        let fresh_press = requesting && !s.prev_wants;
        s.prev_wants = requesting;
        if *current == LocomotionState::Fall && fresh_press {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Glide,
                Priority::PlayerRequested,
                weight::GLIDE,
                "glide",
            ));
        }
    }
}

type TickQuery<'a> = (MotorCore, &'a GlideMovement, Option<&'a mut Stamina>);

pub fn tick_body(
    mut actors: Query<TickQuery, crate::movement::attachment::LocomotionActorFilter>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    for (mut row, movement, mut stamina) in &mut actors {
        if *row.state != LocomotionState::Glide {
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
        v.y -= GRAVITY * movement.gravity_multiplier * dt;
        v.y = v.y.max(-movement.fall_speed);

        let move_dir = Vec3::new(
            row.intents.planar.direction.x,
            0.0,
            row.intents.planar.direction.y,
        )
        .normalize_or_zero();
        if move_dir != Vec3::ZERO {
            v.x = move_toward(
                v.x,
                move_dir.x * movement.max_speed,
                movement.acceleration * dt,
            );
            v.z = move_toward(
                v.z,
                move_dir.z * movement.max_speed,
                movement.acceleration * dt,
            );
        }

        if let Some(stamina) = stamina.as_deref_mut() {
            stamina
                .recover(movement.stamina_recover_per_sec * movement.stamina_recovery_factor * dt);
        }

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
