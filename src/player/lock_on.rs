//! Zelda-style lock-on: toggle the body to face the enemy nearest the
//! crosshair, so movement decouples from facing (strafe / back-pedal).
//!
//! This only *chooses the target* and writes [`FacingSource::LockOn`]; the
//! actual turning is done by `movement::facing::resolve_facing`, the single
//! owner of decoupled facing. The camera (roadmap 3c) will read the same
//! `FacingSource` to frame the target.

use bevy::prelude::*;

use crate::enemies::Enemy;
use crate::input::InputConsumeCursor;
use crate::input::action::IntentAction;
use crate::input::frame::{ActiveActions, ControlOrientation, InputControlledBy};
use crate::movement::Player;
use crate::movement::facing::FacingSource;

/// Max distance to acquire a lock.
const ACQUIRE_RANGE: f32 = 30.0;
/// A held lock only breaks past this (hysteresis, so it does not flicker at the
/// acquire edge).
const BREAK_RANGE: f32 = 40.0;
/// Enemy must sit within this cosine of the look direction to be lockable
/// (~60° half-angle). Among those, the most centered wins.
const CONE_MIN_DOT: f32 = 0.5;

/// Lock-on's own trigger cursor — a newtype so it never steals Movement's or
/// Combat's edges off the shared `ActiveActions` (see `CombatInputCursor`).
#[derive(Component, Default)]
pub struct LockOnInputCursor(pub InputConsumeCursor);

/// The look ray implied by the control orientation (yaw then pitch, down `-Z`),
/// the direction the crosshair points.
fn look_direction(orientation: &ControlOrientation) -> Vec3 {
    (Quat::from_rotation_y(orientation.yaw) * Quat::from_rotation_x(orientation.pitch))
        * Vec3::NEG_Z
}

/// The most crosshair-centered enemy within range and the acquire cone.
fn acquire(
    origin: Vec3,
    orientation: &ControlOrientation,
    enemies: &Query<(Entity, &Transform), With<Enemy>>,
) -> Option<Entity> {
    let look = look_direction(orientation);
    let mut best: Option<Entity> = None;
    let mut best_alignment = CONE_MIN_DOT;
    for (entity, transform) in enemies {
        let to = transform.translation - origin;
        if to.length_squared() > ACQUIRE_RANGE * ACQUIRE_RANGE {
            continue;
        }
        let alignment = to.normalize_or_zero().dot(look);
        if alignment > best_alignment {
            best_alignment = alignment;
            best = Some(entity);
        }
    }
    best
}

/// Runs before `resolve_facing`: consumes the lock-on toggle, acquires/drops a
/// target, and keeps `FacingSource` in sync. Held locks break when the target
/// despawns or leaves `BREAK_RANGE`.
pub fn update_lock_on(
    actions: Res<ActiveActions>,
    mut player: Query<
        (
            &InputControlledBy,
            &Transform,
            &ControlOrientation,
            &mut FacingSource,
            &mut LockOnInputCursor,
        ),
        With<Player>,
    >,
    enemies: Query<(Entity, &Transform), With<Enemy>>,
) {
    let Ok((source, transform, orientation, mut facing, mut cursor)) = player.single_mut() else {
        return;
    };
    let Some(frame) = actions.frame(source.0) else {
        return;
    };
    let toggled = cursor.0.consume(frame, IntentAction::LockOn);

    // Break a lock whose target vanished or wandered past the hysteresis range.
    if let FacingSource::LockOn(target) = *facing {
        let held = enemies.get(target).is_ok_and(|(_, target_tf)| {
            transform
                .translation
                .distance_squared(target_tf.translation)
                <= BREAK_RANGE * BREAK_RANGE
        });
        if !held {
            *facing = FacingSource::Free;
        }
    }

    if !toggled {
        return;
    }
    match *facing {
        FacingSource::LockOn(_) => *facing = FacingSource::Free,
        _ => {
            if let Some(target) = acquire(transform.translation, orientation, &enemies) {
                *facing = FacingSource::LockOn(target);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::frame::LOCAL_INPUT_SOURCE;
    use bevy::ecs::system::RunSystemOnce;

    fn spawn_player(world: &mut World) -> Entity {
        world
            .spawn((
                Player,
                Transform::from_xyz(0.0, 0.0, 0.0),
                InputControlledBy(LOCAL_INPUT_SOURCE),
                ControlOrientation::default(), // looks down -Z
                FacingSource::Free,
                LockOnInputCursor::default(),
            ))
            .id()
    }

    fn toggle(world: &mut World) {
        let mut actions = world.resource_mut::<ActiveActions>();
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::LockOn);
    }

    fn facing(world: &mut World, player: Entity) -> FacingSource {
        *world.entity(player).get::<FacingSource>().unwrap()
    }

    #[test]
    fn toggle_locks_the_most_centered_enemy_and_toggles_off() {
        let mut world = World::new();
        world.insert_resource(ActiveActions::default());
        let player = spawn_player(&mut world);
        // Centered enemy straight ahead (-Z) and an off-axis one to the side.
        let centered = world
            .spawn((Enemy, Transform::from_xyz(0.0, 0.0, -8.0)))
            .id();
        world.spawn((Enemy, Transform::from_xyz(9.0, 0.0, -2.0)));

        toggle(&mut world);
        world.run_system_once(update_lock_on).unwrap();
        assert_eq!(facing(&mut world, player), FacingSource::LockOn(centered));

        toggle(&mut world);
        world.run_system_once(update_lock_on).unwrap();
        assert_eq!(facing(&mut world, player), FacingSource::Free);
    }

    #[test]
    fn enemies_behind_or_out_of_range_are_not_acquired() {
        let mut world = World::new();
        world.insert_resource(ActiveActions::default());
        let player = spawn_player(&mut world);
        world.spawn((Enemy, Transform::from_xyz(0.0, 0.0, 8.0))); // behind (+Z)
        world.spawn((Enemy, Transform::from_xyz(0.0, 0.0, -100.0))); // too far

        toggle(&mut world);
        world.run_system_once(update_lock_on).unwrap();
        assert_eq!(facing(&mut world, player), FacingSource::Free);
    }

    #[test]
    fn a_target_leaving_range_drops_the_lock_without_a_toggle() {
        let mut world = World::new();
        world.insert_resource(ActiveActions::default());
        let player = spawn_player(&mut world);
        let target = world
            .spawn((Enemy, Transform::from_xyz(0.0, 0.0, -8.0)))
            .id();

        toggle(&mut world);
        world.run_system_once(update_lock_on).unwrap();
        assert_eq!(facing(&mut world, player), FacingSource::LockOn(target));

        // Target flees past the break range; no toggle this frame.
        world
            .entity_mut(target)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .z = -100.0;
        world.run_system_once(update_lock_on).unwrap();
        assert_eq!(facing(&mut world, player), FacingSource::Free);
    }
}
