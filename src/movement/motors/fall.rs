//! Fall motor — airborne fallback with asymmetric gravity and air control.
//!
//! FALL is the default state when no motor proposes anything stronger
//! (DEFAULT priority).

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::abilities::AirborneMovement;
use crate::movement::facts::GroundFacts;
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide, move_toward};
use crate::movement::motors::MotorCore;
use crate::movement::motors::jump::JumpPhase;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, GRAVITY};

/// Propose FALL at DEFAULT priority whenever airborne.
type FallProposalFilter = (
    With<Actor>,
    With<AirborneMovement>,
    With<crate::movement::attachment::LocomotionEnabled>,
);

pub fn propose(mut q: Query<(&GroundFacts, &mut ProposalBuffer), FallProposalFilter>) {
    for (ground, mut buffer) in &mut q {
        if !ground.grounded {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Fall,
                Priority::Default,
                weight::FALL,
                "fall",
            ));
        }
    }
}

type TickQuery<'a> = (
    MotorCore,
    &'a AirborneMovement,
    Option<&'a JumpPhase>,
    Option<&'a mut Stamina>,
);

pub fn tick_body(
    mut actors: Query<TickQuery, crate::movement::attachment::LocomotionActorFilter>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    for (mut row, profile, jump_phase, mut stamina) in &mut actors {
        if *row.state != LocomotionState::Fall {
            continue;
        }
        let dt = time.delta_secs();

        apply_locomotion_rotation(
            &mut row.transform,
            row.intents.planar.direction,
            dt,
            profile.rotation_speed,
        );

        let mut v = row.velocity.0;

        // Jump cut: releasing jump on the way up clips upward velocity for a short hop.
        let is_player_jump = jump_phase.is_some_and(|phase| phase.is_player_jump);
        if is_player_jump && !row.intents.jump.held && v.y > profile.jump_cut_velocity {
            v.y = profile.jump_cut_velocity;
        }

        // Asymmetric gravity: snappier rise, heavier fall.
        if v.y < 0.0 {
            v.y -= GRAVITY * profile.fall_gravity_multiplier * dt;
        } else {
            v.y -= GRAVITY * profile.rise_gravity_multiplier * dt;
        }

        let move_dir = Vec3::new(
            row.intents.planar.direction.x,
            0.0,
            row.intents.planar.direction.y,
        )
        .normalize_or_zero();
        if move_dir != Vec3::ZERO {
            v.x = move_toward(
                v.x,
                move_dir.x * profile.max_speed,
                profile.acceleration * dt,
            );
            v.z = move_toward(
                v.z,
                move_dir.z * profile.max_speed,
                profile.acceleration * dt,
            );
        }

        if let Some(stamina) = stamina.as_deref_mut() {
            stamina.recover(profile.stamina_recover_per_sec * profile.stamina_recovery_factor * dt);
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
