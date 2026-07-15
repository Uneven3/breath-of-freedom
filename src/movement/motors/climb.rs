//! Climb motor — wall climbing with lateral movement, wall-stick, and a soft ceiling.
//!
//! Stateless: both `propose` and `tick` read `LedgeFacts` / `GroundFacts`
//! afresh each frame.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::abilities::ClimbMovement;
use crate::movement::facts::{GroundFacts, LedgeFacts};
use crate::movement::intents::{ClimbLateralIntent, ClimbVerticalIntent, Intents};
use crate::movement::motor_common::{body_move_and_slide, clip_below_ledge_lip};
use crate::movement::motors::MotorTickItem;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;

const MIN_DIR_SQ: f32 = 0.001;

type ProposeQuery<'a> = (
    &'a GroundFacts,
    &'a LedgeFacts,
    &'a Stamina,
    &'a Intents,
    &'a LocomotionState,
    &'a mut ProposalBuffer,
);

pub fn propose(mut q: Query<ProposeQuery, (With<Actor>, With<ClimbMovement>)>) {
    for (ground, ledge, stamina, intents, current, mut buffer) in &mut q {
        if stamina.is_exhausted() || !intents.climb.requested {
            continue;
        }
        let climbing = *current == LocomotionState::Climb;
        // Sticky-on-floor at curved apexes where is_on_floor flickers (sphere/cylinder top).
        let near_apex =
            ground.grounded && !ledge.has_head_hit && ledge.mantle_ledge_point.is_some();

        if climbing && (near_apex || ledge.can_continue_climb) {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Climb,
                Priority::Continuation,
                weight::CLIMB,
                "climb",
            ));
        } else if !climbing && ledge.can_climb {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Climb,
                Priority::PlayerRequested,
                weight::CLIMB,
                "climb",
            ));
        }
    }
}

pub(super) fn tick_body(row: &mut MotorTickItem, mas: &MoveAndSlide, time: &Time) {
    let Some(movement) = row.climb_movement else {
        return;
    };
    let ledge = row.ledge;
    let Some(climb_normal) = ledge.climb_normal else {
        return;
    };
    let dt = time.delta_secs();

    let near_apex =
        row.ground.grounded && !ledge.has_head_hit && ledge.mantle_ledge_point.is_some();
    let touching_wall = ledge.can_continue_climb;

    // Face the wall (horizontal only; curved surfaces would otherwise tilt the body).
    if !near_apex {
        let face_dir = if touching_wall {
            Some(-climb_normal)
        } else {
            ledge.wall_point.map(|wp| wp - row.transform.translation)
        };
        if let Some(mut face_dir) = face_dir {
            face_dir.y = 0.0;
            if face_dir.length_squared() > MIN_DIR_SQ {
                let fd = face_dir.normalize();
                row.transform.rotation = Quat::from_rotation_y((-fd.x).atan2(-fd.z));
            }
        }
    }

    let mut v = row.velocity.0;
    v.y = match row.intents.climb.vertical {
        ClimbVerticalIntent::Up => movement.speed,
        ClimbVerticalIntent::Down => -movement.speed,
        ClimbVerticalIntent::Neutral => 0.0,
    };

    // Lateral movement, gated by side-wall presence.
    let lateral_input = match row.intents.climb.lateral {
        ClimbLateralIntent::Left if ledge.has_wall_left => -1.0,
        ClimbLateralIntent::Right if ledge.has_wall_right => 1.0,
        _ => 0.0,
    };
    let right_dir = Vec3::Y.cross(climb_normal).normalize_or_zero();
    let lateral_vel = right_dir * lateral_input * movement.speed;

    // Wall-stick: pull toward the actual contact point while approaching.
    // Only when the sensor still sees the wall — with no `wall_point` there
    // is nothing to stick to (the old `Vec3::ZERO` sentinel silently pulled
    // toward the world origin here).
    let mut wall_stick = Vec3::ZERO;
    if !near_apex
        && !touching_wall
        && let Some(wall_point) = ledge.wall_point
    {
        let mut to_wall = wall_point - row.transform.translation;
        to_wall.y = 0.0;
        if to_wall.length_squared() > MIN_DIR_SQ {
            wall_stick = to_wall.normalize() * movement.wall_approach_speed;
        }
    }
    v.x = lateral_vel.x + wall_stick.x;
    v.z = lateral_vel.z + wall_stick.z;

    // Soft ceiling: cap climb just below the lip so the player can't climb
    // over — forces a Mantle.
    clip_below_ledge_lip(&mut row.transform, &mut v, ledge.lip_height, *row.body, dt);

    row.velocity.0 = body_move_and_slide(
        mas,
        row.entity,
        row.collider,
        &mut row.transform,
        v,
        time.delta(),
        &mut row.contact,
    );

    row.stamina.drain(movement.stamina_cost_per_sec * dt);
}

#[cfg(test)]
mod tests {
    //! Covers the `propose` half: the "climbing the air" regression and
    //! apex/wall-continuation rules. The tick assertions need a physics world
    //! and are covered by play-testing.
    use super::*;
    use crate::movement::Player;
    use bevy::ecs::system::RunSystemOnce;

    /// Spawn a lone player, run `climb::propose`, return its resulting proposals.
    fn propose_with(
        ground: GroundFacts,
        ledge: LedgeFacts,
        stamina: Stamina,
        intents: Intents,
        state: LocomotionState,
    ) -> Vec<TransitionProposal> {
        let mut world = World::new();
        let e = world
            .spawn((
                Player,
                Actor,
                ClimbMovement::PLAYER,
                ground,
                ledge,
                stamina,
                intents,
                state,
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

    fn exhausted() -> Stamina {
        let mut s = Stamina::default();
        s.drain(1_000.0);
        s
    }

    #[test]
    fn does_not_climb_the_air() {
        // Flat ground, climb toggled on, but no climbable ledge under the sensors.
        let out = propose_with(
            GroundFacts {
                grounded: true,
                ..default()
            },
            LedgeFacts::default(), // can_climb = false
            Stamina::default(),
            Intents {
                climb: crate::movement::intents::ClimbIntent {
                    requested: true,
                    ..default()
                },
                ..default()
            },
            LocomotionState::Fall,
        );
        assert!(
            out.is_empty(),
            "must not propose Climb on flat ground with no ledge"
        );
    }

    #[test]
    fn proposes_climb_when_can_climb() {
        let out = propose_with(
            GroundFacts::default(),
            LedgeFacts {
                can_climb: true,
                ..default()
            },
            Stamina::default(),
            Intents {
                climb: crate::movement::intents::ClimbIntent {
                    requested: true,
                    ..default()
                },
                ..default()
            },
            LocomotionState::Fall,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].target_state, LocomotionState::Climb);
        assert_eq!(out[0].category, Priority::PlayerRequested);
        assert_eq!(out[0].override_weight, 5);
    }

    #[test]
    fn stays_climbing_while_wall_continues() {
        let out = propose_with(
            GroundFacts::default(),
            LedgeFacts {
                can_continue_climb: true,
                ..default()
            },
            Stamina::default(),
            Intents {
                climb: crate::movement::intents::ClimbIntent {
                    requested: true,
                    ..default()
                },
                ..default()
            },
            LocomotionState::Climb,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].target_state, LocomotionState::Climb);
        assert_eq!(out[0].category, Priority::Continuation);
        assert_eq!(out[0].override_weight, 5);
    }

    #[test]
    fn stays_climbing_at_curved_apex() {
        // Grounded on a sphere/cylinder top where is_on_floor flickers: a found ledge
        // point with no head hit must keep Climb alive (Continuation), not drop to Walk.
        let out = propose_with(
            GroundFacts {
                grounded: true,
                ..default()
            },
            LedgeFacts {
                has_head_hit: false,
                mantle_ledge_point: Some(Vec3::new(0.0, 1.0, 0.0)),
                ..default()
            },
            Stamina::default(),
            Intents {
                climb: crate::movement::intents::ClimbIntent {
                    requested: true,
                    ..default()
                },
                ..default()
            },
            LocomotionState::Climb,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].category, Priority::Continuation);
    }

    #[test]
    fn no_climb_when_exhausted() {
        let out = propose_with(
            GroundFacts::default(),
            LedgeFacts {
                can_climb: true,
                ..default()
            },
            exhausted(),
            Intents {
                climb: crate::movement::intents::ClimbIntent {
                    requested: true,
                    ..default()
                },
                ..default()
            },
            LocomotionState::Fall,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn no_climb_without_intent() {
        let out = propose_with(
            GroundFacts::default(),
            LedgeFacts {
                can_climb: true,
                ..default()
            },
            Stamina::default(),
            Intents::default(), // climb.requested = false
            LocomotionState::Fall,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn actor_without_climb_movement_cannot_propose_climb() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Actor,
                GroundFacts::default(),
                LedgeFacts {
                    can_climb: true,
                    ..default()
                },
                Stamina::default(),
                Intents {
                    climb: crate::movement::intents::ClimbIntent {
                        requested: true,
                        ..default()
                    },
                    ..default()
                },
                LocomotionState::Fall,
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
