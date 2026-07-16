//! Combat Brain — translates resolved input actions into per-actor
//! `CombatIntents`, the same conceptual slot as `movement::brain::read_intents`.
//! AI actors omit `InputControlledBy` and get their `CombatIntents` from
//! `EnemyBrain` (ticket `enemies-combat`).

use bevy::prelude::*;

use super::intent::{AttackIntent, CombatIntents};
use crate::input::InputConsumeCursor;
use crate::input::action::IntentAction;
use crate::input::frame::{ActiveActions, InputControlledBy};
use crate::movement::Actor;

/// Combat's own trigger cursor. A newtype, not the raw `InputConsumeCursor`:
/// Movement already owns one on the same actor, and two consumers sharing one
/// cursor would steal each other's edges.
#[derive(Component, Default)]
pub struct CombatInputCursor(pub InputConsumeCursor);

type BrainQuery<'a> = (
    &'a InputControlledBy,
    &'a mut CombatIntents,
    &'a mut CombatInputCursor,
);

/// `CombatSet::ReadIntents`: resolved actions -> `CombatIntents`.
pub fn read_intents(actions: Res<ActiveActions>, mut q: Query<BrainQuery, With<Actor>>) {
    for (source, mut intents, mut cursor) in &mut q {
        let Some(frame) = actions.frame(source.0) else {
            continue;
        };
        *intents = CombatIntents {
            attack: AttackIntent {
                pressed: cursor.0.consume(frame, IntentAction::Attack),
                held: frame.pressed(IntentAction::Attack),
            },
            wants_aim: frame.pressed(IntentAction::Aim),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::frame::LOCAL_INPUT_SOURCE;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn attack_edge_is_consumed_once_and_held_persists() {
        let mut world = World::new();
        let mut actions = ActiveActions::default();
        actions.set_pressed(LOCAL_INPUT_SOURCE, IntentAction::Attack, true);
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::Attack);
        world.insert_resource(actions);

        let actor = world
            .spawn((
                Actor,
                InputControlledBy(LOCAL_INPUT_SOURCE),
                CombatIntents::default(),
                CombatInputCursor::default(),
            ))
            .id();

        world.run_system_once(read_intents).unwrap();
        let first = *world.entity(actor).get::<CombatIntents>().unwrap();
        assert!(first.attack.pressed && first.attack.held);

        // Same trigger generation next tick: the edge must not re-fire.
        world.run_system_once(read_intents).unwrap();
        let second = *world.entity(actor).get::<CombatIntents>().unwrap();
        assert!(!second.attack.pressed && second.attack.held);
    }
}
