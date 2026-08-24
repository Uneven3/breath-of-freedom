//! Slide motor — **off by design**: no actor is given `SlideMovement`.
//!
//! Turning it off is the switch, not a flag: `propose` filters on
//! `With<SlideMovement>`, so an actor without the profile never enters the
//! state and the motor costs one empty query.
//!
//! **Why it is off** (user, 2026-08-23): it is not how the reference games
//! work. There, a surface you cannot walk on hands you to a fall — unless you
//! asked to climb it *and* you are facing it. A third state that takes the body
//! automatically reads as the game grabbing the player. Those three cases are
//! `Walk`, `Climb` and `Fall`, and they already exist.
//!
//! **Why it is kept.** What it was written for is measured and still true: in
//! the played session of 2026-08-23, **688 of 869 `Fall` ticks had the probe
//! touching a surface** — the body was resting on a face while free-fall
//! gravity and air control ran on it, and `move_and_slide` projected that
//! velocity into the face and threw the body *up* the ramp at 1.26 m/s. Handing
//! the case back to `Fall` brings that back, and `fall::clamp_against_face` is
//! what answers it now. If a wet-rock or out-of-stamina slip is ever authored,
//! this is where it goes: give that actor the profile.
//!
//! The numbers below stay tunable through `SlideMovement` and `BOF_TUNING`.

use avian3d::prelude::*;
use bevy_ecs::prelude::*;
use bevy_math::prelude::*;
use bevy_time::prelude::*;

use crate::movement::abilities::SlideMovement;
use crate::movement::facts::GroundFacts;
use crate::movement::motor_common::body_move_and_slide;
use crate::movement::motors::MotorCore;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, GRAVITY};

/// Redirect a velocity onto the face the body is sliding on, and bleed it.
///
/// Three things happen here, and each one is a rule the player should feel:
///
/// 1. What pointed **into** the face is spent on the impact. The naive
///    projection `v - (v·n)n` is the bug this motor exists to fix — it turns a
///    run into the slope into *height*, which is how `Fall` was throwing the
///    body up the ramp at 1.26 m/s.
/// 2. What pointed **up** the face is dropped. You do not walk up a wall.
/// 3. What ran **across** the face decays, so you cannot stroll sideways along
///    something you are not allowed to stand on.
///
/// The contour is exactly horizontal (it is perpendicular to the line of
/// steepest descent *inside* the face), so the result's vertical motion is
/// `downhill.y * descending`, which can never be positive.
fn slide_along_face(v: Vec3, normal: Vec3, downhill: Vec3, friction: f32, dt: f32) -> Vec3 {
    let tangential = v - normal * v.dot(normal);
    // Split before clamping, not after: the contour has to be taken out using
    // the *signed* descent, or an uphill tangential (a body that just walked
    // into the face) survives inside the contour and puts its height back.
    let descending = tangential.dot(downhill);
    let across = tangential - downhill * descending;
    let braked = across * (1.0 - friction * dt).clamp(0.0, 1.0);
    braked + downhill * descending.max(0.0)
}

type SlideProposalQuery<'a> = (&'a GroundFacts, &'a mut ProposalBuffer);
type SlideProposalFilter = (
    With<Actor>,
    With<SlideMovement>,
    With<crate::movement::attachment::LocomotionEnabled>,
);

/// Propose SLIDE at DEFAULT priority whenever the probe is on a face too steep
/// to walk. Outranks FALL (`weight::SLIDE`), yields to WALK the moment the
/// surface passes the slope filter, and yields to CLIMB automatically because
/// an explicit climb is `PlayerRequested`, a higher category.
pub fn propose(mut q: Query<SlideProposalQuery, SlideProposalFilter>) {
    for (ground, mut buffer) in &mut q {
        if ground.on_steep_ground() {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Slide,
                Priority::Default,
                weight::SLIDE,
                "slide",
            ));
        }
    }
}

type TickQuery<'a> = (MotorCore, &'a SlideMovement);

pub fn tick_body(
    mut actors: Query<TickQuery, crate::movement::attachment::LocomotionActorFilter>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    for (mut row, slide) in &mut actors {
        if *row.state != LocomotionState::Slide {
            continue;
        }
        let dt = time.delta_secs();
        let normal = row.ground.probe_normal;
        let downhill = row.ground.downhill();

        let mut v = slide_along_face(row.velocity.0, normal, downhill, slide.contour_friction, dt);

        // Gravity, but along the surface instead of through it, and only a
        // fraction of it: the body is supported, so it seeps down the face
        // instead of dropping.
        v += downhill * GRAVITY * slide.gravity_factor * dt;

        // No steering. The player asked for this face to read as a wall — "que
        // la pared me frene de forma natural, que sea evidente que necesito
        // escalar por ahí" — and a wall you can still drive along is not one.
        // Input is not ignored, it is simply not a way through: pressing climb
        // hands the body to `Climb`, which is the way through.

        if v.length() > slide.max_speed {
            v = v.normalize() * slide.max_speed;
        }

        // The body keeps facing wherever the player pointed it. Turning it
        // downhill made brushing a steep face yank the character away from the
        // direction of travel, which reads as the state grabbing the player
        // rather than as a wall stopping them.
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::RunSystemOnce;

    /// The 63° face despejada from the played session's log (2026-08-23): the
    /// body walked into it at 3.21 m/s and `Fall` threw it upward at 1.26 m/s.
    pub(super) fn measured_face() -> Vec3 {
        Vec3::new(-0.74, 0.456, -0.49).normalize()
    }

    pub(super) fn steep_ground(normal: Vec3) -> GroundFacts {
        GroundFacts {
            grounded: false,
            probe_hit: true,
            slope_ok: false,
            probe_normal: normal,
            ..Default::default()
        }
    }

    #[test]
    fn a_touched_steep_face_proposes_slide_and_flat_ground_does_not() {
        let mut world = World::new();
        let sliding = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                SlideMovement::PLAYER,
                steep_ground(measured_face()),
                ProposalBuffer::default(),
            ))
            .id();
        let airborne = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                SlideMovement::PLAYER,
                GroundFacts::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

        assert!(
            world
                .entity(sliding)
                .get::<ProposalBuffer>()
                .is_some_and(|b| b.iter().next().is_some()),
            "a body on a 63° face must be owned by Slide, not left to Fall"
        );
        assert!(
            world
                .entity(airborne)
                .get::<ProposalBuffer>()
                .is_some_and(|b| b.iter().next().is_none()),
            "with no surface under the probe this is a real fall"
        );
    }

    #[test]
    fn downhill_follows_the_face_and_never_points_up() {
        let facts = steep_ground(measured_face());
        let downhill = facts.downhill();
        assert!(downhill.y < 0.0, "sliding must descend, got {downhill}");
        assert!(
            downhill.dot(measured_face()).abs() < 1e-5,
            "the slide direction must lie in the face, not through it"
        );
        assert!((downhill.length() - 1.0).abs() < 1e-5);
    }

    /// The regression this motor exists for: the logged tick entered `Fall`
    /// with `vel.y = +1.26` — the body was thrown *up* the ramp it had just
    /// walked into, using the very velocity it had walked in with. A slide may
    /// never do that, at any speed or angle of approach.
    #[test]
    fn walking_into_the_measured_face_never_launches_the_body_upward() {
        let normal = measured_face();
        let downhill = steep_ground(normal).downhill();
        let walked_in_at = Vec3::new(2.91, 0.0, 1.36);

        let out = slide_along_face(
            walked_in_at,
            normal,
            downhill,
            SlideMovement::PLAYER.contour_friction,
            1.0 / 60.0,
        );

        assert!(out.y <= 0.0, "a slide must not gain height: {out}");
        // The naive projection is what the old path did; pin the difference so
        // nobody "simplifies" this back into the bug.
        let naive = walked_in_at - normal * walked_in_at.dot(normal);
        assert!(
            naive.y > 1.0,
            "the projection this replaces should still climb, or the test proves nothing: {naive}"
        );
    }

    /// Sweep the approach instead of trusting the one logged sample: whatever
    /// direction and speed the body arrives at, it may not come out rising.
    #[test]
    fn no_approach_to_a_steep_face_comes_out_rising() {
        let normal = measured_face();
        let downhill = steep_ground(normal).downhill();
        for degrees in 0..360 {
            let angle = (degrees as f32).to_radians();
            for speed in [0.5_f32, 3.2, 10.0] {
                let v = Vec3::new(angle.cos() * speed, 0.0, angle.sin() * speed);
                let out = slide_along_face(
                    v,
                    normal,
                    downhill,
                    SlideMovement::PLAYER.contour_friction,
                    1.0 / 60.0,
                );
                assert!(
                    out.y <= 1e-6,
                    "approach {degrees}° at {speed} m/s came out rising: {out}"
                );
            }
        }
    }

    /// Moving across the face is not a loophole. It used to be: the contour
    /// component was preserved exactly, so a player could walk sideways along a
    /// 63° face at full speed — not uphill, but reading just like walking where
    /// walking is supposed to be impossible. The wall has to stop you.
    #[test]
    fn moving_across_the_face_is_braked_and_dies_out() {
        let normal = measured_face();
        let downhill = steep_ground(normal).downhill();
        let contour = downhill.cross(normal).normalize();
        let dt = 1.0 / 60.0;

        let one_tick = slide_along_face(
            contour * 4.0,
            normal,
            downhill,
            SlideMovement::PLAYER.contour_friction,
            dt,
        );
        assert!(
            one_tick.length() < 4.0,
            "crossing the face must lose speed, got {one_tick}"
        );
        assert!(one_tick.y.abs() < 1e-5, "the contour is still level");

        let mut v = contour * 4.0;
        for _ in 0..120 {
            v = slide_along_face(
                v,
                normal,
                downhill,
                SlideMovement::PLAYER.contour_friction,
                dt,
            );
        }
        assert!(
            v.length() < 0.1,
            "two seconds of pushing across should go nowhere, got {v}"
        );
    }

    #[test]
    fn a_long_slide_settles_at_the_terminal_speed() {
        let facts = steep_ground(measured_face());
        let mut v = Vec3::ZERO;
        for _ in 0..600 {
            v += facts.downhill() * GRAVITY * SlideMovement::PLAYER.gravity_factor * (1.0 / 60.0);
            if v.length() > SlideMovement::PLAYER.max_speed {
                v = v.normalize() * SlideMovement::PLAYER.max_speed;
            }
        }
        assert!((v.length() - SlideMovement::PLAYER.max_speed).abs() < 1e-4);
    }
}

#[cfg(test)]
mod off_by_design_tests {
    use super::tests::{measured_face, steep_ground};
    use super::*;
    use crate::movement::sensing::LedgeSensing;
    use bevy_ecs::system::RunSystemOnce;

    /// The switch, pinned: an actor without `SlideMovement` never enters the
    /// state, however steep the face under it. Handing that case to `Fall` is
    /// the reference-game rule — you cannot walk it, so you fall, unless you
    /// asked to climb and are facing it.
    #[test]
    fn an_actor_without_the_profile_never_proposes_slide() {
        let mut world = World::new();
        let actor = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                LedgeSensing::PLAYER,
                steep_ground(measured_face()),
                ProposalBuffer::default(),
            ))
            .id();

        let _ = world.run_system_once(propose);

        assert!(
            world
                .entity(actor)
                .get::<ProposalBuffer>()
                .is_some_and(|b| b.iter().next().is_none()),
            "sin la capacidad declarada, la cara empinada es asunto de Fall"
        );
    }

    /// And the player is exactly such an actor: nothing in `spawn_player` grants
    /// the profile. A future slip mechanic turns this red on purpose.
    #[test]
    fn the_player_is_not_given_the_slide_profile() {
        let source = include_str!("../../player/mod.rs");
        assert!(
            !source.contains("SlideMovement"),
            "el jugador recibió SlideMovement: si es a propósito, actualizá el \
             doc del módulo, que dice que está apagado"
        );
    }
}
