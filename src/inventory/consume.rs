//! Consumables: eating food asks Health to heal — Health owns that contract
//! (`health::HealRequestMessage`), Inventory only decides which stack to
//! spend.

use bevy::prelude::*;

use crate::health::HealRequestMessage;
use crate::input::action::IntentAction;
use crate::input::frame::{ActiveActions, InputControlledBy};
use crate::movement::Player;

use super::data::{Inventory, InventoryInputCursor};

type ConsumeActorQuery<'a> = (
    Entity,
    &'a InputControlledBy,
    &'a mut InventoryInputCursor,
    &'a mut Inventory,
);

pub fn read_use_item_requests(
    actions: Res<ActiveActions>,
    mut actors: Query<ConsumeActorQuery, With<Player>>,
    mut heals: MessageWriter<HealRequestMessage>,
) {
    for (actor, source, mut cursor, mut inventory) in &mut actors {
        let Some(frame) = actions.frame(source.0) else {
            continue;
        };
        if !cursor.0.consume(frame, IntentAction::UseItem) {
            continue;
        }
        let Some(heal) = inventory.consume_first_food() else {
            continue;
        };
        heals.write(HealRequestMessage {
            target: actor,
            amount: heal,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    use crate::input::frame::LOCAL_INPUT_SOURCE;
    use crate::inventory::data::ItemKind;

    #[test]
    fn use_item_consumes_food_and_requests_a_heal() {
        let mut world = World::new();
        world.init_resource::<Messages<HealRequestMessage>>();
        let mut actions = ActiveActions::default();
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::UseItem);
        world.insert_resource(actions);

        let mut inventory = Inventory::default();
        inventory
            .try_add(
                ItemKind::Food {
                    label: "Apple",
                    heal: 25.0,
                },
                1,
            )
            .unwrap();
        let actor = world
            .spawn((
                Player,
                InputControlledBy(LOCAL_INPUT_SOURCE),
                InventoryInputCursor::default(),
                inventory,
            ))
            .id();

        world.run_system_once(read_use_item_requests).unwrap();

        let messages = world.resource::<Messages<HealRequestMessage>>();
        let mut cursor = messages.get_cursor();
        let requests: Vec<_> = cursor.read(messages).collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].target, actor);
        assert_eq!(requests[0].amount, 25.0);
        assert_eq!(
            world
                .entity(actor)
                .get::<Inventory>()
                .unwrap()
                .iter()
                .count(),
            0
        );
    }

    #[test]
    fn use_item_with_no_food_writes_nothing() {
        let mut world = World::new();
        world.init_resource::<Messages<HealRequestMessage>>();
        let mut actions = ActiveActions::default();
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::UseItem);
        world.insert_resource(actions);

        world.spawn((
            Player,
            InputControlledBy(LOCAL_INPUT_SOURCE),
            InventoryInputCursor::default(),
            Inventory::default(),
        ));

        world.run_system_once(read_use_item_requests).unwrap();

        assert!(world.resource::<Messages<HealRequestMessage>>().is_empty());
    }
}
