//! Movement Brain — translates resolved input actions into per-actor intents.

use bevy::prelude::*;

use super::Actor;
use super::facing::FacingSource;
use super::intents::{
    ClimbLateralIntent, ClimbVerticalIntent, GlideIntent, Intents, JumpIntent, LadderIntent,
    TraversalActionIntent,
};
use super::state::LocomotionState;
use crate::input::InputConsumeCursor;
use crate::input::action::IntentAction;
use crate::input::frame::{ActiveActions, ControlOrientation, InputControlledBy};

const WISH_DIR_THRESHOLD: f32 = 0.5;

/// Movement-owned interpretation of the climb toggle for one actor.
#[derive(Component, Default)]
pub struct ClimbInputState(pub bool);

type BrainQuery<'a> = (
    &'a InputControlledBy,
    &'a ControlOrientation,
    &'a mut Intents,
    &'a mut ClimbInputState,
    &'a mut InputConsumeCursor,
    &'a FacingSource,
    &'a Transform,
);

/// `MovementSet::ReadIntents`: resolved actions -> `Intents` for each
/// input-controlled actor. AI actors omit `InputControlledBy` and own their
/// intents through their own Brain.
pub fn read_intents(
    actions: Res<ActiveActions>,
    transforms: Query<&Transform>,
    mut q: Query<BrainQuery, With<Actor>>,
) {
    for (source, orientation, mut intents, mut climb, mut cursor, facing, transform) in &mut q {
        let Some(frame) = actions.frame(source.0) else {
            continue;
        };

        let mut input = Vec2::ZERO;
        if frame.pressed(IntentAction::MoveRight) {
            input.x += 1.0;
        }
        if frame.pressed(IntentAction::MoveLeft) {
            input.x -= 1.0;
        }
        if frame.pressed(IntentAction::MoveBack) {
            input.y += 1.0;
        }
        if frame.pressed(IntentAction::MoveForward) {
            input.y -= 1.0;
        }
        if input.length_squared() > 1.0 {
            input = input.normalize();
        }

        *intents = Intents::default();
        intents.planar.strength = input.length();
        intents.climb.lateral = if input.x > WISH_DIR_THRESHOLD {
            ClimbLateralIntent::Right
        } else if input.x < -WISH_DIR_THRESHOLD {
            ClimbLateralIntent::Left
        } else {
            ClimbLateralIntent::Neutral
        };
        intents.climb.vertical = if input.y < -WISH_DIR_THRESHOLD {
            ClimbVerticalIntent::Up
        } else if input.y > WISH_DIR_THRESHOLD {
            ClimbVerticalIntent::Down
        } else {
            ClimbVerticalIntent::Neutral
        };

        if input != Vec2::ZERO {
            // FacingSource decides both the frame the stick is read in and
            // whether the move is a strafe — the explicit "I'm moving laterally
            // because I'm locked on" the emergent camera-relative path hid.
            let (world, local) = match facing {
                FacingSource::Free => {
                    // Face-where-you-move: camera-relative translation, and
                    // always forward relative to facing (the body turns to it).
                    let yaw = Quat::from_rotation_y(orientation.yaw);
                    let world =
                        (yaw * Vec3::X * input.x - yaw * Vec3::NEG_Z * input.y).normalize_or_zero();
                    (world, Vec2::new(0.0, -input.length()))
                }
                FacingSource::Look | FacingSource::LockOn(_) => {
                    // Facing decoupled: the stick IS the facing-relative intent
                    // (x strafe, -y toward the faced target), translated in that
                    // frame — circle-strafe around the target. The frame is the
                    // *intended* facing (look yaw or direction to the target),
                    // NOT the body's current yaw: the body is mid-rotation toward
                    // it, and feeding that in-progress yaw back into the movement
                    // direction spiralled the player off the map.
                    let basis_yaw = match facing {
                        FacingSource::LockOn(target) => transforms
                            .get(*target)
                            .map(|target_tf| {
                                let to = target_tf.translation - transform.translation;
                                (-to.x).atan2(-to.z)
                            })
                            .unwrap_or(orientation.yaw),
                        _ => orientation.yaw,
                    };
                    let yaw = Quat::from_rotation_y(basis_yaw);
                    let world =
                        (yaw * Vec3::X * input.x - yaw * Vec3::NEG_Z * input.y).normalize_or_zero();
                    (world, input)
                }
            };
            intents.planar.direction = Vec2::new(world.x, world.z);
            intents.planar.local = local;
        }

        if cursor.consume(frame, IntentAction::ClimbToggle) {
            climb.0 = !climb.0;
        }
        intents.jump = JumpIntent {
            held: frame.pressed(IntentAction::Jump),
            pressed: cursor.consume(frame, IntentAction::Jump),
        };
        intents.glide = if frame.pressed(IntentAction::Glide) {
            GlideIntent::Requested
        } else {
            GlideIntent::Inactive
        };
        intents.wants_sneak = frame.pressed(IntentAction::Sneak);
        intents.wants_sprint = frame.pressed(IntentAction::Sprint);
        intents.climb.requested = climb.0;
        intents.ladder = match intents.climb.vertical {
            ClimbVerticalIntent::Up => LadderIntent::Up,
            ClimbVerticalIntent::Down => LadderIntent::Down,
            ClimbVerticalIntent::Neutral => LadderIntent::Hold,
        };
        intents.traversal = if frame.pressed(IntentAction::Mantle) {
            TraversalActionIntent::Mantle
        } else if frame.pressed(IntentAction::Vault) {
            TraversalActionIntent::Vault
        } else {
            TraversalActionIntent::None
        };
    }
}

type ActorTransition = (With<Actor>, Changed<LocomotionState>);

/// Clear climb intent after transitions that explicitly release a climb latch.
pub fn reset_climb_toggle(
    mut q: Query<(&LocomotionState, &Intents, &mut ClimbInputState), ActorTransition>,
) {
    for (state, intents, mut climb) in &mut q {
        match *state {
            LocomotionState::Mantle | LocomotionState::AutoVault | LocomotionState::EdgeLeap => {
                climb.0 = false;
            }
            LocomotionState::WallJump => {
                let sticking = intents.climb.vertical == ClimbVerticalIntent::Up
                    || intents.climb.lateral != ClimbLateralIntent::Neutral;
                if !sticking {
                    climb.0 = false;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::frame::{InputSource, LOCAL_INPUT_SOURCE, MAX_INPUT_SOURCES};
    use crate::movement::intents::ClimbIntent;
    use bevy::ecs::system::RunSystemOnce;

    fn latch_after(state: LocomotionState, intents: Intents) -> bool {
        let mut world = World::new();
        let entity = world
            .spawn((Actor, state, intents, ClimbInputState(true)))
            .id();
        world.run_system_once(reset_climb_toggle).unwrap();
        world.entity(entity).get::<ClimbInputState>().unwrap().0
    }

    #[test]
    fn releases_latch_after_mantle_vault_or_edge_leap() {
        assert!(!latch_after(LocomotionState::Mantle, Intents::default()));
        assert!(!latch_after(LocomotionState::AutoVault, Intents::default()));
        assert!(!latch_after(LocomotionState::EdgeLeap, Intents::default()));
    }

    #[test]
    fn lateral_wall_jump_keeps_latch() {
        assert!(latch_after(
            LocomotionState::WallJump,
            Intents {
                climb: ClimbIntent {
                    lateral: ClimbLateralIntent::Left,
                    ..default()
                },
                ..default()
            },
        ));
    }

    #[test]
    fn away_or_downward_wall_jump_releases_latch() {
        assert!(!latch_after(LocomotionState::WallJump, Intents::default()));
        assert!(!latch_after(
            LocomotionState::WallJump,
            Intents {
                climb: ClimbIntent {
                    vertical: ClimbVerticalIntent::Down,
                    ..default()
                },
                ..default()
            },
        ));
    }

    #[test]
    fn ordinary_jump_keeps_latch() {
        assert!(latch_after(LocomotionState::Jump, Intents::default()));
    }

    #[test]
    fn local_actions_follow_actor_orientation_without_overwriting_ai_intents() {
        let mut world = World::new();
        let mut actions = ActiveActions::default();
        actions.set_pressed(LOCAL_INPUT_SOURCE, IntentAction::MoveForward, true);
        world.insert_resource(actions);

        let player = world
            .spawn((
                Actor,
                InputControlledBy(LOCAL_INPUT_SOURCE),
                ControlOrientation {
                    yaw: std::f32::consts::FRAC_PI_2,
                    ..default()
                },
                Intents::default(),
                ClimbInputState::default(),
                InputConsumeCursor::default(),
                FacingSource::default(),
                Transform::default(),
            ))
            .id();
        let ai_intents = Intents {
            wants_sprint: true,
            planar: crate::movement::intents::PlanarMoveIntent {
                direction: Vec2::X,
                strength: 1.0,
                local: Vec2::ZERO,
            },
            ..default()
        };
        let ai = world.spawn((Actor, ai_intents)).id();

        world.run_system_once(read_intents).unwrap();

        let player_intents = world.entity(player).get::<Intents>().unwrap();
        assert!(player_intents.planar.direction.x < -0.99);
        assert!(player_intents.planar.direction.y.abs() < 1e-5);
        let preserved = world.entity(ai).get::<Intents>().unwrap();
        assert!(preserved.wants_sprint);
        assert_eq!(preserved.planar.direction, Vec2::X);
    }

    #[test]
    fn remote_source_actions_are_independent_from_local_source() {
        let remote = InputSource((MAX_INPUT_SOURCES - 1) as u8);
        let mut world = World::new();
        let mut actions = ActiveActions::default();
        actions.set_pressed(remote, IntentAction::MoveRight, true);
        world.insert_resource(actions);

        let remote_actor = world
            .spawn((
                Actor,
                InputControlledBy(remote),
                ControlOrientation::default(),
                Intents::default(),
                ClimbInputState::default(),
                InputConsumeCursor::default(),
                FacingSource::default(),
                Transform::default(),
            ))
            .id();
        world.run_system_once(read_intents).unwrap();

        let intents = world.entity(remote_actor).get::<Intents>().unwrap();
        assert_eq!(intents.climb.lateral, ClimbLateralIntent::Right);
    }

    #[test]
    fn actions_translate_to_named_locomotion_intents() {
        let mut world = World::new();
        let mut actions = ActiveActions::default();
        for action in [
            IntentAction::MoveForward,
            IntentAction::Jump,
            IntentAction::Sneak,
            IntentAction::Mantle,
            IntentAction::Glide,
        ] {
            actions.set_pressed(LOCAL_INPUT_SOURCE, action, true);
        }
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::Jump);
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::ClimbToggle);
        world.insert_resource(actions);

        let actor = world
            .spawn((
                Actor,
                InputControlledBy(LOCAL_INPUT_SOURCE),
                ControlOrientation::default(),
                Intents::default(),
                ClimbInputState::default(),
                InputConsumeCursor::default(),
                FacingSource::default(),
                Transform::default(),
            ))
            .id();
        world.run_system_once(read_intents).unwrap();

        let intents = world.entity(actor).get::<Intents>().unwrap();
        assert_eq!(intents.planar.direction, Vec2::NEG_Y);
        assert_eq!(intents.climb.vertical, ClimbVerticalIntent::Up);
        assert_eq!(intents.ladder, LadderIntent::Up);
        assert!(intents.wants_sneak);
        assert!(intents.jump.held && intents.jump.pressed);
        assert!(intents.climb.requested);
        assert_eq!(intents.traversal, TraversalActionIntent::Mantle);
        assert_eq!(intents.glide, GlideIntent::Requested);
    }

    #[test]
    fn decoupled_facing_reads_lateral_input_as_an_explicit_strafe() {
        use crate::movement::intents::StrafeDir;

        let mut world = World::new();
        let mut actions = ActiveActions::default();
        actions.set_pressed(LOCAL_INPUT_SOURCE, IntentAction::MoveLeft, true);
        world.insert_resource(actions);

        let actor = world
            .spawn((
                Actor,
                InputControlledBy(LOCAL_INPUT_SOURCE),
                ControlOrientation::default(),
                Intents::default(),
                ClimbInputState::default(),
                InputConsumeCursor::default(),
                // Facing decoupled from movement (aim/lock-on).
                FacingSource::Look,
                Transform::from_rotation(Quat::from_rotation_y(0.0)), // faces -Z
            ))
            .id();
        world.run_system_once(read_intents).unwrap();

        let intents = world.entity(actor).get::<Intents>().unwrap();
        // The stick is understood in the facing frame: "left" is an explicit
        // strafe, not an emergent camera-relative move.
        assert_eq!(intents.planar.strafe_dir(), StrafeDir::Left);
        assert!(intents.planar.local.x < 0.0);
        // Translation is left relative to the body's facing (-Z forward → -X).
        assert!(intents.planar.direction.x < 0.0);
    }
}
