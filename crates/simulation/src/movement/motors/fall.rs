//! Fall motor — airborne fallback with asymmetric gravity and air control.
//!
//! FALL is the default state when no motor proposes anything stronger
//! (DEFAULT priority).

use avian3d::prelude::*;
use bevy_ecs::prelude::*;
use bevy_math::prelude::*;
use bevy_time::prelude::*;

use crate::movement::abilities::AirborneMovement;
use crate::movement::facing::faces_movement;
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

/// Strip the upward push a slide against a too-steep face leaves behind.
///
/// `move_and_slide` projects the swept velocity onto whatever it hits, which is
/// right for a wall and wrong for the face of a hill: walking into a 63° slope
/// at 3.2 m/s came back out of the projection at **+1.26 m/s upward**, so the
/// body bounced up the ramp, re-touched, and bounced again — the `Walk`/`Fall`
/// buzz measured on 2026-08-23.
///
/// Gravity is the only thing allowed to move a falling body upward, and it
/// never does. So: on a face the body cannot stand on, a *rising* projected
/// velocity is an artefact, and the vertical part of it goes. What runs across
/// the face is untouched — that is a real slide along a wall, and removing it
/// would glue the body to every slope it brushed.
pub(crate) fn clamp_against_face(projected: Vec3, ground: &GroundFacts) -> Vec3 {
    if projected.y <= 0.0 || !ground.on_steep_ground() {
        return projected;
    }
    Vec3::new(projected.x, 0.0, projected.z)
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

        if faces_movement(row.facing) {
            apply_locomotion_rotation(
                &mut row.transform,
                row.intents.planar.direction,
                dt,
                profile.rotation_speed,
            );
        }

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

        let projected = body_move_and_slide(
            &mas,
            row.entity,
            row.collider,
            &mut row.transform,
            v,
            time.delta(),
            &mut row.contact,
        );
        row.velocity.0 = clamp_against_face(projected, row.ground);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 62.9° face from the played canyon (2026-08-23).
    fn measured_face() -> Vec3 {
        Vec3::new(-0.74, 0.456, -0.49).normalize()
    }

    fn on_face(normal: Vec3) -> GroundFacts {
        GroundFacts {
            probe_hit: true,
            slope_ok: false,
            probe_normal: normal,
            ..Default::default()
        }
    }

    fn in_the_air() -> GroundFacts {
        GroundFacts::default()
    }

    /// The regression: the logged tick entered `Fall` at `vel.y = +1.26` after
    /// walking into the face at 3.2 m/s. Falling never goes up.
    #[test]
    fn a_rising_projection_off_a_steep_face_loses_its_lift() {
        let thrown_up = Vec3::new(0.86, 1.26, 0.0);
        let out = clamp_against_face(thrown_up, &on_face(measured_face()));
        assert_eq!(out.y, 0.0, "un cuerpo cayendo no sube: {out}");
        assert_eq!(out.x, thrown_up.x, "lo que cruza la cara es del jugador");
        assert_eq!(out.z, thrown_up.z);
    }

    /// A real fall must keep falling: the clamp only ever removes *upward*
    /// motion, never adds or removes downward.
    #[test]
    fn falling_velocity_is_never_touched() {
        let falling = Vec3::new(1.0, -8.0, 0.5);
        for facts in [on_face(measured_face()), in_the_air()] {
            assert_eq!(clamp_against_face(falling, &facts), falling);
        }
    }

    /// Off a face there is nothing to be an artefact of — a jump, a launch, a
    /// wall-jump all rise legitimately.
    #[test]
    fn rising_in_open_air_is_left_alone() {
        let jumping = Vec3::new(0.0, 5.5, 0.0);
        assert_eq!(clamp_against_face(jumping, &in_the_air()), jumping);
    }

    /// Walkable ground is not this motor's business: a body rising off a ramp
    /// it can stand on is `Walk`'s tangential Y, and stealing it here would
    /// fight `snap_to_ground`.
    #[test]
    fn a_walkable_slope_is_not_a_face() {
        let ramp = Vec3::new(-0.34, 0.94, 0.0).normalize();
        let mut facts = on_face(ramp);
        facts.slope_ok = true;
        let rising = Vec3::new(2.35, 0.85, 0.0);
        assert_eq!(clamp_against_face(rising, &facts), rising);
    }

    /// Sweeping the approach instead of trusting the one logged sample.
    #[test]
    fn no_approach_to_a_steep_face_comes_out_rising() {
        let facts = on_face(measured_face());
        for degrees in 0..360 {
            let angle = (degrees as f32).to_radians();
            for speed in [0.5_f32, 3.2, 10.0] {
                let v = Vec3::new(angle.cos() * speed, 1.26, angle.sin() * speed);
                assert!(
                    clamp_against_face(v, &facts).y <= 0.0,
                    "{degrees}° a {speed}"
                );
            }
        }
    }
}
