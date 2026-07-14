//! Fall motor — airborne fallback with asymmetric gravity and air control.
//!
//! FALL is the default state when no motor proposes anything stronger
//! (DEFAULT priority).

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::abilities::AirborneMovement;
use crate::movement::facts::{BodyContact, GroundFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide, move_toward};
use crate::movement::motors::jump::JumpPhase;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity, GRAVITY};

/// Propose FALL at DEFAULT priority whenever airborne.
type FallProposalFilter = (With<Actor>, With<AirborneMovement>);

pub fn propose(mut q: Query<(&GroundFacts, &mut ProposalBuffer), FallProposalFilter>) {
    for (ground, mut buffer) in &mut q {
        if !ground.grounded {
            let _ = buffer.push(TransitionProposal::new(
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
    &'a AirborneMovement,
);

pub fn tick(
    mut q: Query<TickQuery, (With<Actor>, With<AirborneMovement>)>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
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
        profile,
    ) in &mut q
    {
        if *state != LocomotionState::Fall {
            continue;
        }

        apply_locomotion_rotation(&mut transform, intents.move_dir, dt, profile.rotation_speed);

        let mut v = vel.0;

        // Jump cut: releasing jump on the way up clips upward velocity for a short hop.
        if jump_phase.is_player_jump && !intents.wants_jump && v.y > profile.jump_cut_velocity {
            v.y = profile.jump_cut_velocity;
        }

        // Asymmetric gravity: snappier rise, heavier fall.
        if v.y < 0.0 {
            v.y -= GRAVITY * profile.fall_gravity_multiplier * dt;
        } else {
            v.y -= GRAVITY * profile.rise_gravity_multiplier * dt;
        }

        let move_dir = Vec3::new(intents.move_dir.x, 0.0, intents.move_dir.y).normalize_or_zero();
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

        stamina.recover(profile.stamina_recover_per_sec * profile.stamina_recovery_factor * dt);

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
