//! Zelda-style lock-on: toggle the body to face the enemy nearest the
//! crosshair, so movement decouples from facing (strafe / back-pedal).
//!
//! This only *chooses the target* and writes [`FacingSource::LockOn`]; the
//! actual turning is done by `movement::facing::resolve_facing`, the single
//! owner of decoupled facing. The camera (roadmap 3c) will read the same
//! `FacingSource` to frame the target.

use bevy_ecs::prelude::*;
use bevy_math::prelude::*;
use bevy_transform::prelude::*;

use crate::enemies::Enemy;
use crate::movement::facing::FacingSource;
use crate::movement::{ActorId, Player};
use bof_domain::input::InputConsumeCursor;
use bof_domain::input::action::IntentAction;
use bof_domain::input::frame::{ActiveActions, ControlOrientation, InputControlledBy};

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

/// The most crosshair-centered enemy within range and the acquire cone. Ties
/// prefer distance, world position and authored identity in that order.
fn acquire(
    origin: Vec3,
    orientation: &ControlOrientation,
    enemies: &Query<(Entity, &ActorId, &Transform), With<Enemy>>,
) -> Option<Entity> {
    let look = look_direction(orientation);
    enemies
        .iter()
        .filter_map(|(entity, actor_id, transform)| {
            let position = transform.translation;
            let to = position - origin;
            let distance_squared = to.length_squared();
            if distance_squared > ACQUIRE_RANGE * ACQUIRE_RANGE {
                return None;
            }
            let alignment = to.normalize_or_zero().dot(look);
            (alignment > CONE_MIN_DOT).then_some((
                entity,
                *actor_id,
                position,
                alignment,
                distance_squared,
            ))
        })
        .max_by(|left, right| {
            left.3
                .total_cmp(&right.3)
                .then_with(|| right.4.total_cmp(&left.4))
                .then_with(|| right.2.x.total_cmp(&left.2.x))
                .then_with(|| right.2.y.total_cmp(&left.2.y))
                .then_with(|| right.2.z.total_cmp(&left.2.z))
                .then_with(|| right.1.cmp(&left.1))
        })
        .map(|(entity, ..)| entity)
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
    enemies: Query<(Entity, &ActorId, &Transform), With<Enemy>>,
) {
    for (source, transform, orientation, mut facing, mut cursor) in &mut player {
        let Some(frame) = actions.frame(source.0) else {
            continue;
        };
        let toggled = cursor.0.consume(frame, IntentAction::LockOn);

        // Break a lock whose target vanished or wandered past the hysteresis range.
        if let FacingSource::LockOn(target) = *facing {
            let held = enemies.get(target).is_ok_and(|(_, _, target_tf)| {
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
            continue;
        }
        match *facing {
            FacingSource::LockOn(_) => *facing = FacingSource::Free,
            FacingSource::Free | FacingSource::Look => {
                if let Some(target) = acquire(transform.translation, orientation, &enemies) {
                    *facing = FacingSource::LockOn(target);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::RunSystemOnce;
    use bof_domain::input::frame::{InputSource, LOCAL_INPUT_SOURCE};

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
            .spawn((
                Enemy,
                ActorId::authored(10),
                Transform::from_xyz(0.0, 0.0, -8.0),
            ))
            .id();
        world.spawn((
            Enemy,
            ActorId::authored(11),
            Transform::from_xyz(9.0, 0.0, -2.0),
        ));

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
        world.spawn((
            Enemy,
            ActorId::authored(10),
            Transform::from_xyz(0.0, 0.0, 8.0),
        )); // behind (+Z)
        world.spawn((
            Enemy,
            ActorId::authored(11),
            Transform::from_xyz(0.0, 0.0, -100.0),
        )); // too far

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
            .spawn((
                Enemy,
                ActorId::authored(10),
                Transform::from_xyz(0.0, 0.0, -8.0),
            ))
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

    #[test]
    fn one_players_lock_input_is_not_disabled_by_another_player() {
        let mut world = World::new();
        world.insert_resource(ActiveActions::default());
        let local = spawn_player(&mut world);
        let remote = world
            .spawn((
                Player,
                Transform::from_xyz(20.0, 0.0, 0.0),
                InputControlledBy(InputSource(1)),
                ControlOrientation::default(),
                FacingSource::Free,
                LockOnInputCursor::default(),
            ))
            .id();
        let target = world
            .spawn((
                Enemy,
                ActorId::authored(10),
                Transform::from_xyz(0.0, 0.0, -8.0),
            ))
            .id();

        toggle(&mut world);
        world.run_system_once(update_lock_on).unwrap();

        assert_eq!(facing(&mut world, local), FacingSource::LockOn(target));
        assert_eq!(facing(&mut world, remote), FacingSource::Free);
    }
}
