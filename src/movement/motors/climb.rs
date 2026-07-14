//! Climb motor — wall climbing with lateral movement, wall-stick, and a soft ceiling.
//!
//! Stateless: both `propose` and `tick` read `LedgeFacts` / `GroundFacts`
//! afresh each frame.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::{BodyContact, GroundFacts, LedgeFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{body_move_and_slide, clip_below_ledge_lip};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity};

const CLIMB_SPEED: f32 = 2.5;
const STAMINA_COST_PER_SEC: f32 = 5.0;
const WALL_APPROACH_SPEED: f32 = 0.5;
const MIN_DIR_SQ: f32 = 0.001;

type ProposeQuery<'a> = (
    &'a GroundFacts,
    &'a LedgeFacts,
    &'a Stamina,
    &'a Intents,
    &'a LocomotionState,
    &'a mut ProposalBuffer,
);

pub fn propose(mut q: Query<ProposeQuery, With<Actor>>) {
    for (ground, ledge, stamina, intents, current, mut buffer) in &mut q {
        if stamina.is_exhausted() || !intents.wants_climb {
            continue;
        }
        let climbing = *current == LocomotionState::Climb;
        // Sticky-on-floor at curved apexes where is_on_floor flickers (sphere/cylinder top).
        let near_apex =
            ground.grounded && !ledge.has_head_hit && ledge.mantle_ledge_point.is_some();

        if climbing && (near_apex || ledge.can_continue_climb) {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Climb,
                Priority::Opportunistic,
                5,
                "climb",
            ));
        } else if !climbing && ledge.can_climb {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Climb,
                Priority::PlayerRequested,
                5,
                "climb",
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
    &'a LedgeFacts,
    &'a GroundFacts,
    &'a LocomotionState,
);

pub fn tick(mut q: Query<TickQuery, With<Actor>>, mas: MoveAndSlide, time: Res<Time>) {
    let dt = time.delta_secs();
    for (
        entity,
        collider,
        mut transform,
        mut vel,
        intents,
        mut stamina,
        mut contact,
        ledge,
        ground,
        state,
    ) in &mut q
    {
        if *state != LocomotionState::Climb {
            continue;
        }

        let Some(climb_normal) = ledge.climb_normal else {
            continue;
        };

        let near_apex =
            ground.grounded && !ledge.has_head_hit && ledge.mantle_ledge_point.is_some();
        let touching_wall = ledge.can_continue_climb;

        // Face the wall (horizontal only; curved surfaces would otherwise tilt the body).
        if !near_apex {
            let face_dir = if touching_wall {
                Some(-climb_normal)
            } else {
                ledge.wall_point.map(|wp| wp - transform.translation)
            };
            if let Some(mut face_dir) = face_dir {
                face_dir.y = 0.0;
                if face_dir.length_squared() > MIN_DIR_SQ {
                    let fd = face_dir.normalize();
                    transform.rotation = Quat::from_rotation_y((-fd.x).atan2(-fd.z));
                }
            }
        }

        let mut v = vel.0;
        v.y = -intents.raw_input.y * CLIMB_SPEED;

        // Lateral movement, gated by side-wall presence.
        let mut lateral_input = intents.raw_input.x;
        if (intents.is_climbing_right() && !ledge.has_wall_right)
            || (intents.is_climbing_left() && !ledge.has_wall_left)
        {
            lateral_input = 0.0;
        }
        let right_dir = Vec3::Y.cross(climb_normal).normalize_or_zero();
        let lateral_vel = right_dir * lateral_input * CLIMB_SPEED;

        // Wall-stick: pull toward the actual contact point while approaching.
        // Only when the sensor still sees the wall — with no `wall_point` there
        // is nothing to stick to (the old `Vec3::ZERO` sentinel silently pulled
        // toward the world origin here).
        let mut wall_stick = Vec3::ZERO;
        if !near_apex
            && !touching_wall
            && let Some(wall_point) = ledge.wall_point
        {
            let mut to_wall = wall_point - transform.translation;
            to_wall.y = 0.0;
            if to_wall.length_squared() > MIN_DIR_SQ {
                wall_stick = to_wall.normalize() * WALL_APPROACH_SPEED;
            }
        }
        v.x = lateral_vel.x + wall_stick.x;
        v.z = lateral_vel.z + wall_stick.z;

        // Soft ceiling: cap climb just below the lip so the player can't climb
        // over — forces a Mantle.
        clip_below_ledge_lip(&mut transform, &mut v, ledge.lip_height, dt);

        vel.0 = body_move_and_slide(
            &mas,
            entity,
            collider,
            &mut transform,
            v,
            time.delta(),
            &mut contact,
        );

        stamina.drain(STAMINA_COST_PER_SEC * dt);
    }
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
                wants_climb: true,
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
                wants_climb: true,
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
                wants_climb: true,
                ..default()
            },
            LocomotionState::Climb,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].target_state, LocomotionState::Climb);
        assert_eq!(out[0].category, Priority::Opportunistic);
        assert_eq!(out[0].override_weight, 5);
    }

    #[test]
    fn stays_climbing_at_curved_apex() {
        // Grounded on a sphere/cylinder top where is_on_floor flickers: a found ledge
        // point with no head hit must keep Climb alive (Opportunistic), not drop to Walk.
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
                wants_climb: true,
                ..default()
            },
            LocomotionState::Climb,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].category, Priority::Opportunistic);
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
                wants_climb: true,
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
            Intents::default(), // wants_climb = false
            LocomotionState::Fall,
        );
        assert!(out.is_empty());
    }
}
