//! Edge-leap motor — leap sideways off the edge of a wall while climbing.
//!
//! Timer/leaping flags live in an `EdgeLeapState` component.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::{BodyContact, GroundFacts, LedgeFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::body_move_and_slide;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{BodyVelocity, GRAVITY, Player};

const LEAP_AWAY_IMPULSE: f32 = 8.0;
const VERTICAL_BOOST: f32 = 2.0;
const STAMINA_COST: f32 = 10.0;
const LEAP_DURATION: f32 = 0.3;
const FORCED_WEIGHT: u32 = 10;
const WALL_PUSH_SPEED: f32 = 2.0;

#[derive(Component, Default)]
pub struct EdgeLeapState {
    is_leaping: bool,
    timer: f32,
    needs_release: bool,
}

pub fn propose(
    mut q: Single<
        (
            &Intents,
            &LocomotionState,
            &Stamina,
            &LedgeFacts,
            &mut EdgeLeapState,
            &mut ProposalBuffer,
        ),
        With<Player>,
    >,
) {
    let (intents, current, stamina, ledge, state, buffer) = &mut *q;

    if !intents.wants_jump {
        state.needs_release = false;
    }

    if **current == LocomotionState::Climb && intents.wants_jump && !state.needs_release {
        let at_left_edge = intents.is_climbing_left() && !ledge.has_wall_left;
        let at_right_edge = intents.is_climbing_right() && !ledge.has_wall_right;
        if (at_left_edge || at_right_edge) && !stamina.is_exhausted() {
            state.needs_release = true;
            state.is_leaping = true;
            state.timer = LEAP_DURATION;
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::EdgeLeap,
                Priority::Forced,
                FORCED_WEIGHT,
                "edge_leap",
            ));
            return;
        }
    }

    if **current == LocomotionState::EdgeLeap && state.is_leaping {
        let _ = buffer.push(TransitionProposal::new(
            LocomotionState::EdgeLeap,
            Priority::Forced,
            FORCED_WEIGHT,
            "edge_leap",
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
            &mut BodyContact,
            &mut EdgeLeapState,
            &Intents,
            &mut Stamina,
            &LedgeFacts,
            &GroundFacts,
        ),
        With<Player>,
    >,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    let (
        entity,
        collider,
        mut transform,
        mut vel,
        mut contact,
        mut state,
        intents,
        mut stamina,
        ledge,
        ground,
    ) = player.into_inner();
    let dt = time.delta_secs();

    let mut v = vel.0;

    if state.timer == LEAP_DURATION {
        let mut normal = ledge.climb_normal;
        if normal == Vec3::ZERO {
            normal = if contact.on_wall {
                -contact.wall_normal
            } else {
                transform.rotation * Vec3::Z
            };
        }
        let right_dir = Vec3::Y.cross(normal).normalize_or_zero();
        let jump_dir = if intents.is_climbing_left() {
            -right_dir
        } else if intents.is_climbing_right() {
            right_dir
        } else {
            normal
        };
        v = jump_dir * LEAP_AWAY_IMPULSE + normal * WALL_PUSH_SPEED + Vec3::Y * VERTICAL_BOOST;
        stamina.drain(STAMINA_COST);
    }

    state.timer -= dt;
    v.y -= GRAVITY * dt;

    vel.0 = body_move_and_slide(
        &mas,
        entity,
        collider,
        &mut transform,
        v,
        time.delta(),
        &mut contact,
    );

    if state.timer <= 0.0 || ground.grounded {
        state.is_leaping = false;
    }
}

#[cfg(test)]
mod tests {
    //! Covers the `propose` half: triggers at a wall edge, not mid-wall, and the
    //! needs-release latch. The impulse tick is play-tested.
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    /// Player climbing while pressing left (`is_climbing_left`) and holding jump.
    fn climbing_left() -> Intents {
        Intents {
            wants_jump: true,
            wish_dir: IVec2::new(-1, 0),
            ..default()
        }
    }

    fn setup(intents: Intents, ledge: LedgeFacts, stamina: Stamina) -> (World, Entity) {
        let mut world = World::new();
        let e = world
            .spawn((
                Player,
                intents,
                LocomotionState::Climb,
                stamina,
                ledge,
                EdgeLeapState::default(),
                ProposalBuffer::default(),
            ))
            .id();
        (world, e)
    }

    fn buffer(world: &World, e: Entity) -> Vec<TransitionProposal> {
        world
            .entity(e)
            .get::<ProposalBuffer>()
            .unwrap()
            .iter()
            .copied()
            .collect()
    }

    #[test]
    fn leaps_at_left_edge() {
        // Pressing into open space on the left (no wall there) = at an edge → leap.
        let (mut world, e) = setup(
            climbing_left(),
            LedgeFacts {
                has_wall_left: false,
                ..default()
            },
            Stamina::default(),
        );
        world.run_system_once(propose).expect("propose runs");

        let buf = buffer(&world, e);
        assert_eq!(buf.len(), 1);
        assert_eq!(buf[0].target_state, LocomotionState::EdgeLeap);
        assert_eq!(buf[0].category, Priority::Forced);
        assert_eq!(buf[0].override_weight, FORCED_WEIGHT);

        let st = world.entity(e).get::<EdgeLeapState>().unwrap();
        assert!(st.is_leaping);
        assert_eq!(st.timer, LEAP_DURATION);
        assert!(st.needs_release);
    }

    #[test]
    fn no_leap_when_wall_continues() {
        // A wall to the left means the surface continues — not an edge, so no leap.
        let (mut world, e) = setup(
            climbing_left(),
            LedgeFacts {
                has_wall_left: true,
                ..default()
            },
            Stamina::default(),
        );
        world.run_system_once(propose).expect("propose runs");
        assert!(buffer(&world, e).is_empty());
    }

    #[test]
    fn held_jump_does_not_retrigger() {
        let (mut world, e) = setup(
            climbing_left(),
            LedgeFacts {
                has_wall_left: false,
                ..default()
            },
            Stamina::default(),
        );
        world.run_system_once(propose).expect("propose runs");
        // Next frame: arbiter cleared the buffer, jump is still held.
        world
            .entity_mut(e)
            .get_mut::<ProposalBuffer>()
            .unwrap()
            .clear();
        world.run_system_once(propose).expect("propose runs");
        assert!(
            buffer(&world, e).is_empty(),
            "held jump must not re-trigger EdgeLeap until released",
        );
    }

    #[test]
    fn no_leap_when_exhausted() {
        let mut s = Stamina::default();
        s.drain(1_000.0);
        let (mut world, e) = setup(
            climbing_left(),
            LedgeFacts {
                has_wall_left: false,
                ..default()
            },
            s,
        );
        world.run_system_once(propose).expect("propose runs");
        assert!(buffer(&world, e).is_empty());
    }
}
