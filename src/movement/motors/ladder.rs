//! Ladder motor — climb a ladder, snapping to its anchor rail.
//!
//! Stateless: reads `LadderFacts` each frame.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::abilities::LadderMovement;
use crate::movement::body::BodyDimensions;
use crate::movement::facts::LadderFacts;
use crate::movement::intents::{Intents, LadderIntent};
use crate::movement::motor_common::body_move_and_slide;
use crate::movement::motors::MotorCore;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::state::LocomotionState;

const BOTTOM_EXIT_CLEARANCE: f32 = 0.1;
const TOP_HOLD_CLEARANCE: f32 = 0.25;

type ProposeQuery<'a> = (
    &'a LadderFacts,
    &'a Intents,
    &'a LocomotionState,
    &'a Transform,
    &'a BodyDimensions,
    &'a mut ProposalBuffer,
);

type ProposeFilter = (
    crate::movement::attachment::LocomotionActorFilter,
    With<LadderMovement>,
);

pub fn propose(mut q: Query<ProposeQuery, ProposeFilter>) {
    for (ladder, intents, current, transform, body, mut buffer) in &mut q {
        if !ladder.on_ladder {
            continue;
        }
        let descending_to_ground = intents.ladder == LadderIntent::Down
            && transform.translation.y
                <= ladder.bottom_y + body.standing_half_height() + BOTTOM_EXIT_CLEARANCE;
        if intents.jump.held || intents.jump.pressed || descending_to_ground {
            continue;
        }
        if *current == LocomotionState::Ladder || intents.climb.requested {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Ladder,
                Priority::Forced,
                weight::LADDER,
                "ladder",
            ));
        }
    }
}

type TickQuery<'a> = (MotorCore, &'a LadderMovement, &'a LadderFacts);

pub fn tick_body(
    mut actors: Query<TickQuery, crate::movement::attachment::LocomotionActorFilter>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    for (mut row, movement, ladder) in &mut actors {
        if *row.state != LocomotionState::Ladder {
            continue;
        }
        let dt = time.delta_secs();

        row.transform.translation.x = ladder.body_anchor_xz.x;
        row.transform.translation.z = ladder.body_anchor_xz.y;
        face_ladder(&mut row.transform, ladder.outward_normal);

        // A ladder is an attachment constraint: it accepts only vertical input.
        let min_y = ladder.bottom_y + row.body.standing_half_height();
        let max_y = ladder.top_y - TOP_HOLD_CLEARANCE;
        let vertical_speed = match row.intents.ladder {
            LadderIntent::Up => movement.speed,
            LadderIntent::Down => -movement.speed,
            LadderIntent::Hold => 0.0,
        };
        let target_y =
            (row.transform.translation.y + vertical_speed * dt).clamp(min_y, max_y.max(min_y));
        let v = Vec3::Y * ((target_y - row.transform.translation.y) / dt.max(f32::EPSILON));

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

fn face_ladder(transform: &mut Transform, outward_normal: Vec3) {
    let mut face = -outward_normal;
    face.y = 0.0;
    if face.length_squared() > f32::EPSILON {
        let face = face.normalize();
        transform.rotation = Quat::from_rotation_y((-face.x).atan2(-face.z));
    }
}

#[cfg(test)]
mod tests {
    //! Covers the latch/release rules of `propose`; the rail-snap tick is
    //! play-tested.
    use super::*;
    use crate::movement::Actor;
    use bevy::ecs::system::RunSystemOnce;

    fn proposals(intents: Intents, state: LocomotionState) -> Vec<TransitionProposal> {
        let mut world = World::new();
        let e = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                LadderMovement::PLAYER,
                BodyDimensions::PLAYER,
                LadderFacts {
                    on_ladder: true,
                    outward_normal: Vec3::Z,
                    ..default()
                },
                intents,
                state,
                Transform::from_xyz(0.0, 1.0, 0.0),
                ProposalBuffer::default(),
            ))
            .id();
        world.run_system_once(propose).expect("propose runs");
        world
            .entity(e)
            .get::<ProposalBuffer>()
            .unwrap()
            .iter()
            .cloned()
            .collect()
    }

    #[test]
    fn latched_ladder_keeps_proposing() {
        let out = proposals(Intents::default(), LocomotionState::Ladder);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].target_state, LocomotionState::Ladder);
    }

    #[test]
    fn jump_releases_the_latch() {
        let out = proposals(
            Intents {
                jump: crate::movement::intents::JumpIntent {
                    held: true,
                    ..default()
                },
                ..default()
            },
            LocomotionState::Ladder,
        );
        assert!(out.is_empty(), "jump must release the ladder latch");
    }

    #[test]
    fn held_jump_blocks_re_entry() {
        // Falling next to the ladder while still holding Space and a move key
        // must not re-latch (that would undo the jump release instantly).
        let out = proposals(
            Intents {
                jump: crate::movement::intents::JumpIntent {
                    held: true,
                    ..default()
                },
                planar: crate::movement::intents::PlanarMoveIntent {
                    direction: Vec2::new(0.0, 1.0),
                    strength: 1.0,
                    local: Vec2::ZERO,
                },
                ..default()
            },
            LocomotionState::Fall,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn lateral_input_does_not_attach() {
        let out = proposals(
            Intents {
                planar: crate::movement::intents::PlanarMoveIntent {
                    direction: Vec2::X,
                    strength: 1.0,
                    local: Vec2::ZERO,
                },
                ..default()
            },
            LocomotionState::Walk,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn climb_toggle_attaches() {
        let out = proposals(
            Intents {
                climb: crate::movement::intents::ClimbIntent {
                    requested: true,
                    ..default()
                },
                ..default()
            },
            LocomotionState::Walk,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].target_state, LocomotionState::Ladder);
    }

    #[test]
    fn actor_without_ladder_movement_cannot_propose_ladder() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                BodyDimensions::PLAYER,
                LadderFacts {
                    on_ladder: true,
                    ..default()
                },
                Intents {
                    climb: crate::movement::intents::ClimbIntent {
                        requested: true,
                        ..default()
                    },
                    ..default()
                },
                LocomotionState::Walk,
                Transform::default(),
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

    #[test]
    fn ladder_movement_requires_body_dimensions() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                LadderMovement::PLAYER,
                LadderFacts {
                    on_ladder: true,
                    outward_normal: Vec3::Z,
                    ..default()
                },
                Intents {
                    climb: crate::movement::intents::ClimbIntent {
                        requested: true,
                        ..default()
                    },
                    ..default()
                },
                LocomotionState::Walk,
                Transform::default(),
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
                .is_none(),
            "Ladder must not run without the body profile it uses for its limits"
        );
    }
}
