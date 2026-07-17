//! Edge-leap motor — leap sideways off the edge of a wall while climbing.
//!
//! Timer/leaping flags live in an `EdgeLeapState` component.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::GRAVITY;
use crate::movement::abilities::WallJumpMovement;
use crate::movement::facts::LedgeFacts;
use crate::movement::intents::{ClimbLateralIntent, Intents};
use crate::movement::motor_common::{body_move_and_slide, launch_normal};
use crate::movement::motors::MotorCore;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;

#[derive(Component, Default)]
pub struct EdgeLeapState {
    is_leaping: bool,
    timer: f32,
    needs_release: bool,
    /// Armed by `propose`, consumed by `tick`'s first active frame (the launch
    /// impulse). See `WallJumpState::launch_pending`.
    launch_pending: bool,
}

type ProposeQuery<'a> = (
    &'a Intents,
    &'a LocomotionState,
    &'a Stamina,
    &'a LedgeFacts,
    &'a WallJumpMovement,
    &'a mut EdgeLeapState,
    &'a mut ProposalBuffer,
);

type ProposeFilter = (
    crate::movement::attachment::LocomotionActorFilter,
    With<WallJumpMovement>,
);

pub fn propose(mut q: Query<ProposeQuery, ProposeFilter>) {
    for (intents, current, stamina, ledge, movement, mut state, mut buffer) in &mut q {
        let jump_requested = intents.jump.held || intents.jump.pressed;
        if !intents.jump.held {
            state.needs_release = false;
        }

        if *current == LocomotionState::Climb && jump_requested && !state.needs_release {
            let at_left_edge =
                intents.climb.lateral == ClimbLateralIntent::Left && !ledge.has_wall_left;
            let at_right_edge =
                intents.climb.lateral == ClimbLateralIntent::Right && !ledge.has_wall_right;
            if (at_left_edge || at_right_edge) && !stamina.is_exhausted() {
                state.needs_release = true;
                state.is_leaping = true;
                state.timer = movement.edge_leap.duration;
                state.launch_pending = true;
                let _ = buffer.push(TransitionProposal::new(
                    LocomotionState::EdgeLeap,
                    Priority::Forced,
                    weight::EDGE_LEAP,
                    "edge_leap",
                ));
                continue;
            }
        }

        if *current == LocomotionState::EdgeLeap && state.is_leaping {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::EdgeLeap,
                Priority::Forced,
                weight::EDGE_LEAP,
                "edge_leap",
            ));
        }
    }
}

type TickQuery<'a> = (
    MotorCore,
    &'a WallJumpMovement,
    &'a LedgeFacts,
    &'a mut EdgeLeapState,
    Option<&'a mut Stamina>,
);

pub fn tick_body(
    mut actors: Query<TickQuery, crate::movement::attachment::LocomotionActorFilter>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    for (mut row, movement, ledge, mut state, mut stamina) in &mut actors {
        if *row.state != LocomotionState::EdgeLeap {
            continue;
        }
        let dt = time.delta_secs();

        let mut v = row.velocity.0;

        if state.launch_pending {
            state.launch_pending = false;
            let profile = movement.edge_leap;
            let normal = launch_normal(ledge.climb_normal, &row.contact, &row.transform);
            let right_dir = Vec3::Y.cross(normal).normalize_or_zero();
            let jump_dir = if row.intents.climb.lateral == ClimbLateralIntent::Left {
                -right_dir
            } else if row.intents.climb.lateral == ClimbLateralIntent::Right {
                right_dir
            } else {
                normal
            };
            v = jump_dir * profile.away_impulse
                + normal * profile.wall_push_speed
                + Vec3::Y * profile.vertical_boost;
            if let Some(stamina) = stamina.as_deref_mut() {
                stamina.drain(profile.stamina_cost);
            }
        }

        state.timer -= dt;
        v.y -= GRAVITY * dt;

        row.velocity.0 = body_move_and_slide(
            &mas,
            row.entity,
            row.collider,
            &mut row.transform,
            v,
            time.delta(),
            &mut row.contact,
        );

        if state.timer <= 0.0 || row.ground.grounded {
            state.is_leaping = false;
        }
    }
}

#[cfg(test)]
mod tests {
    //! Covers the `propose` half: triggers at a wall edge, not mid-wall, and the
    //! needs-release latch. The impulse tick is play-tested.
    use super::*;
    use crate::movement::{Actor, Player};
    use bevy::ecs::system::RunSystemOnce;

    /// Player climbing left while holding jump.
    fn climbing_left() -> Intents {
        Intents {
            jump: crate::movement::intents::JumpIntent {
                held: true,
                ..default()
            },
            climb: crate::movement::intents::ClimbIntent {
                lateral: ClimbLateralIntent::Left,
                ..default()
            },
            ..default()
        }
    }

    fn setup(intents: Intents, ledge: LedgeFacts, stamina: Stamina) -> (World, Entity) {
        let mut world = World::new();
        let e = world
            .spawn((
                Player,
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                WallJumpMovement::PLAYER,
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
            .cloned()
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
        assert_eq!(buf[0].override_weight, weight::EDGE_LEAP);

        let st = world.entity(e).get::<EdgeLeapState>().unwrap();
        assert!(st.is_leaping);
        assert_eq!(st.timer, WallJumpMovement::PLAYER.edge_leap.duration);
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

    #[test]
    fn actor_without_wall_jump_movement_cannot_propose_edge_leap() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                climbing_left(),
                LocomotionState::Climb,
                Stamina::default(),
                LedgeFacts {
                    has_wall_left: false,
                    ..default()
                },
                EdgeLeapState::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

        assert!(
            world
                .entity(entity)
                .get::<ProposalBuffer>()
                .unwrap()
                .iter()
                .next()
                .is_none()
        );
    }
}
