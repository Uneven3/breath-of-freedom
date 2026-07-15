//! Fall motor — airborne fallback with asymmetric gravity and air control.
//!
//! FALL is the default state when no motor proposes anything stronger
//! (DEFAULT priority).

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::abilities::AirborneMovement;
use crate::movement::facts::GroundFacts;
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide, move_toward};
use crate::movement::motors::MotorTickItem;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, GRAVITY};

/// Propose FALL at DEFAULT priority whenever airborne.
type FallProposalFilter = (With<Actor>, With<AirborneMovement>);

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

pub(super) fn tick_body(row: &mut MotorTickItem, mas: &MoveAndSlide, time: &Time) {
    let Some(profile) = row.airborne_movement else {
        return;
    };
    let dt = time.delta_secs();

    apply_locomotion_rotation(
        &mut row.transform,
        row.intents.planar.direction,
        dt,
        profile.rotation_speed,
    );

    let mut v = row.velocity.0;

    // Jump cut: releasing jump on the way up clips upward velocity for a short hop.
    let is_player_jump = row.jump_phase.is_some_and(|phase| phase.is_player_jump);
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

    row.stamina
        .recover(profile.stamina_recover_per_sec * profile.stamina_recovery_factor * dt);

    row.velocity.0 = body_move_and_slide(
        mas,
        row.entity,
        row.collider,
        &mut row.transform,
        v,
        time.delta(),
        &mut row.contact,
    );
}
