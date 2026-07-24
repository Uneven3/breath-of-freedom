//! Body facing: the single owner of *decoupled* facing.
//!
//! Free locomotion faces where it moves — the motors own that, because the turn
//! rate is coupled to the velocity profile (`apply_locomotion_rotation`). When
//! facing is instead pinned to the camera look (aim) or a locked target
//! (Zelda-style lock-on), that is one concept — [`FacingSource`] — resolved
//! here in one place, after the active motor has moved the body. Climb/ladder
//! force facing toward their wall inside the motor, so they are skipped.

use bevy::prelude::*;

use crate::input::frame::ControlOrientation;
use crate::movement::state::LocomotionState;

/// What governs an actor's body yaw. `Free` leaves the move-relative turn to the
/// motors; `Look` and `LockOn` decouple facing from movement so the actor can
/// strafe and back-pedal — the shared basis for aim and lock-on.
// Look/LockOn are wired by aim and lock-on (roadmap 3b); Free is the only
// variant set today.
#[allow(dead_code)]
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FacingSource {
    #[default]
    Free,
    /// Face the camera look direction (`ControlOrientation.yaw`). Used by aim.
    Look,
    /// Face a locked target entity. Used by lock-on.
    LockOn(Entity),
}

/// Yaw that points the body's forward (`-Z`) along the planar `dir`.
fn planar_yaw(dir: Vec2) -> Option<f32> {
    (dir.length_squared() > 1e-4).then(|| (-dir.x).atan2(-dir.y))
}

/// Runs after `TickActiveMotor`: the sole writer of body yaw when facing is
/// decoupled (`Look`/`LockOn`). It sets the yaw straight to the target, fully
/// overriding the toward-movement rotation the motor applied this step (which is
/// never rendered — the frame draws this post-motor transform), so the body
/// faces the target cleanly instead of settling between the two. `Free` and the
/// wall-forced climb/ladder states are left to the motor, so with the default
/// `FacingSource::Free` this is a no-op.
pub fn resolve_facing(
    // Lock-on targets never carry `FacingSource` themselves, keeping this query
    // disjoint from the mutable one below.
    targets: Query<&Transform, Without<FacingSource>>,
    mut actors: Query<(
        &mut Transform,
        &FacingSource,
        &LocomotionState,
        &ControlOrientation,
    )>,
) {
    for (mut transform, facing, state, orientation) in &mut actors {
        if matches!(state, LocomotionState::Climb | LocomotionState::Ladder) {
            continue;
        }
        let target_yaw = match facing {
            FacingSource::Free => continue,
            FacingSource::Look => orientation.yaw,
            FacingSource::LockOn(target) => {
                let Ok(target_tf) = targets.get(*target) else {
                    continue;
                };
                let to = target_tf.translation - transform.translation;
                let Some(yaw) = planar_yaw(Vec2::new(to.x, to.z)) else {
                    continue;
                };
                yaw
            }
        };
        transform.rotation = Quat::from_rotation_y(target_yaw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn yaw_of(world: &mut World, entity: Entity) -> f32 {
        world
            .entity(entity)
            .get::<Transform>()
            .unwrap()
            .rotation
            .to_euler(EulerRot::YXZ)
            .0
    }

    #[test]
    fn free_and_wall_states_leave_rotation_untouched() {
        let mut world = World::new();
        for state in [
            LocomotionState::Walk,
            LocomotionState::Climb,
            LocomotionState::Ladder,
        ] {
            let facing = if state == LocomotionState::Walk {
                FacingSource::Free
            } else {
                FacingSource::Look
            };
            let actor = world
                .spawn((
                    Transform::from_rotation(Quat::IDENTITY),
                    facing,
                    state,
                    ControlOrientation {
                        yaw: 1.0,
                        pitch: 0.0,
                    },
                ))
                .id();
            world.run_system_once(resolve_facing).unwrap();
            assert!(yaw_of(&mut world, actor).abs() < 1e-4, "{state:?} moved");
        }
    }

    #[test]
    fn lock_on_turns_the_body_toward_the_target() {
        let mut world = World::new();
        let target = world.spawn(Transform::from_xyz(0.0, 0.0, -10.0)).id();
        let actor = world
            .spawn((
                Transform::from_xyz(0.0, 0.0, 0.0),
                FacingSource::LockOn(target),
                LocomotionState::Walk,
                ControlOrientation::default(),
            ))
            .id();

        // The target sits straight ahead (-Z), so the body should already face
        // it: yaw stays ~0 and the forward axis points at the target.
        world.run_system_once(resolve_facing).unwrap();
        let forward = world.entity(actor).get::<Transform>().unwrap().forward();
        assert!(forward.z < -0.99, "not facing the target: {forward:?}");
    }
}
