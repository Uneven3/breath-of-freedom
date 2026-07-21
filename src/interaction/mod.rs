//! The single arbiter of contextual interaction.
//!
//! Before this module, `mounts` and `inventory` each consumed
//! `IntentAction::Interact` on their own cursor, with their own range and
//! their own copy of the nearest-candidate search. Nothing was wrong on the
//! graybox layout only because the horse spawned ~8 m from the pickups; the
//! first layout that put a weapon next to a horse would have mounted *and*
//! picked up on one press. Two independent consumers of one key is not a
//! feature you can order your way out of — it needs an owner.
//!
//! So: one cursor consumes the key, one search picks the winner, and one
//! message carries the decision. Domains match on [`InteractionKind`] and act;
//! they no longer read input at all for this. Adding a campfire or a dialogue
//! is a variant plus a reader, not a third competitor.

pub mod data;

use bevy::prelude::*;

pub use data::{
    Interactable, InteractionKind, InteractionOverride, InteractionRequest, nearest_candidate,
};

use crate::input::action::IntentAction;
use crate::input::frame::{ActiveActions, InputConsumeCursor, InputControlledBy};

/// Consumes `Interact` for its actor. One per actor, replacing the per-domain
/// cursors that used to race.
#[derive(Component, Default)]
pub struct InteractionInputCursor(pub InputConsumeCursor);

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum InteractionSet {
    /// Resolves the press. Every domain reader must run after this.
    Arbitrate,
}

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<InteractionRequest>();
        app.add_systems(
            FixedUpdate,
            arbitrate_interactions.in_set(InteractionSet::Arbitrate),
        );
    }
}

type InteractorQuery<'a> = (
    Entity,
    &'a InputControlledBy,
    &'a mut InteractionInputCursor,
    &'a Transform,
    Option<&'a InteractionOverride>,
);

/// Resolves one press into at most one [`InteractionRequest`].
///
/// An [`InteractionOverride`] on the actor wins outright and skips the search:
/// while mounted, `Interact` dismounts even if a weapon is underfoot. Keeping
/// that rule here — rather than as an early return inside `mounts` — is what
/// makes the priority visible to every domain instead of to one.
pub fn arbitrate_interactions(
    actions: Res<ActiveActions>,
    mut actors: Query<InteractorQuery>,
    candidates: Query<(Entity, &Transform, &Interactable)>,
    mut requests: MessageWriter<InteractionRequest>,
) {
    for (actor, source, mut cursor, transform, override_kind) in &mut actors {
        let Some(frame) = actions.frame(source.0) else {
            continue;
        };
        // Consumed unconditionally: the press is spent whether or not anything
        // was in reach, so a miss cannot leak into the next tick.
        if !cursor.0.consume(frame, IntentAction::Interact) {
            continue;
        }

        if let Some(override_kind) = override_kind {
            requests.write(InteractionRequest {
                actor,
                target: None,
                kind: override_kind.kind,
            });
            continue;
        }

        let origin = transform.translation;
        let found = nearest_candidate(
            origin,
            candidates.iter().map(|(entity, transform, interactable)| {
                (entity, transform.translation, *interactable)
            }),
            |interactable| interactable.range,
        );
        if let Some((target, interactable)) = found {
            requests.write(InteractionRequest {
                actor,
                target: Some(target),
                kind: interactable.kind,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    use crate::input::frame::LOCAL_INPUT_SOURCE;

    fn world_with_press() -> World {
        let mut world = World::new();
        let mut actions = ActiveActions::default();
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::Interact);
        world.insert_resource(actions);
        world.init_resource::<Messages<InteractionRequest>>();
        world
    }

    fn emitted(world: &mut World) -> Vec<InteractionRequest> {
        world
            .resource::<Messages<InteractionRequest>>()
            .iter_current_update_messages()
            .copied()
            .collect()
    }

    fn spawn_actor(world: &mut World) -> Entity {
        world
            .spawn((
                InputControlledBy(LOCAL_INPUT_SOURCE),
                InteractionInputCursor::default(),
                Transform::default(),
            ))
            .id()
    }

    /// The bug this module exists for: a horse and a weapon both in reach used
    /// to fire both domains from a single press.
    #[test]
    fn one_press_resolves_to_exactly_one_interaction() {
        let mut world = world_with_press();
        let actor = spawn_actor(&mut world);
        world.spawn((
            Transform::from_xyz(3.0, 0.0, 0.0),
            Interactable {
                kind: InteractionKind::Mount,
                range: 5.0,
            },
        ));
        let apple = world
            .spawn((
                Transform::from_xyz(1.0, 0.0, 0.0),
                Interactable {
                    kind: InteractionKind::Pickup,
                    range: 5.0,
                },
            ))
            .id();

        world.run_system_once(arbitrate_interactions).unwrap();

        let requests = emitted(&mut world);
        assert_eq!(requests.len(), 1, "one press, one interaction");
        assert_eq!(requests[0].actor, actor);
        assert_eq!(requests[0].target, Some(apple), "the nearest one wins");
        assert_eq!(requests[0].kind, InteractionKind::Pickup);
    }

    /// Being mounted must claim the key outright, or dismounting next to a
    /// pickup would grab the item instead.
    #[test]
    fn an_override_beats_everything_in_reach() {
        let mut world = world_with_press();
        let actor = spawn_actor(&mut world);
        world.entity_mut(actor).insert(InteractionOverride {
            kind: InteractionKind::Dismount,
        });
        world.spawn((
            Transform::from_xyz(0.5, 0.0, 0.0),
            Interactable {
                kind: InteractionKind::Pickup,
                range: 5.0,
            },
        ));

        world.run_system_once(arbitrate_interactions).unwrap();

        let requests = emitted(&mut world);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].kind, InteractionKind::Dismount);
        assert_eq!(requests[0].target, None);
    }

    #[test]
    fn a_press_with_nothing_in_reach_emits_nothing() {
        let mut world = world_with_press();
        spawn_actor(&mut world);
        world.spawn((
            Transform::from_xyz(40.0, 0.0, 0.0),
            Interactable {
                kind: InteractionKind::Mount,
                range: 5.0,
            },
        ));

        world.run_system_once(arbitrate_interactions).unwrap();

        assert!(emitted(&mut world).is_empty());
    }

    /// Holding the key must not re-fire: the edge is consumed once.
    #[test]
    fn the_press_is_consumed_once() {
        let mut world = world_with_press();
        spawn_actor(&mut world);
        world.spawn((
            Transform::from_xyz(1.0, 0.0, 0.0),
            Interactable {
                kind: InteractionKind::Pickup,
                range: 5.0,
            },
        ));

        world.run_system_once(arbitrate_interactions).unwrap();
        world.resource_mut::<Messages<InteractionRequest>>().clear();
        world.run_system_once(arbitrate_interactions).unwrap();

        assert!(emitted(&mut world).is_empty(), "no re-fire while held");
    }
}
