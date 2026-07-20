//! Consumables: eating food asks Health to heal — Health owns that contract
//! (`health::HealRequestMessage`), Inventory only decides which stack to
//! spend.

use bevy::prelude::*;

use crate::health::HealRequestMessage;
use crate::input::action::IntentAction;
use crate::input::frame::{ActiveActions, InputControlledBy};
use crate::movement::Player;

use super::data::{ConsumeSlotRequestMessage, Inventory, InventoryInputCursor};

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
        if !cursor.triggered(&actions, source.0, IntentAction::UseItem) {
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

pub fn read_consume_slot_requests(
    mut requests: MessageReader<ConsumeSlotRequestMessage>,
    mut actors: Query<&mut Inventory>,
    mut heals: MessageWriter<HealRequestMessage>,
) {
    for request in requests.read() {
        let Ok(mut inventory) = actors.get_mut(request.actor) else {
            continue;
        };
        if let Some(amount) = inventory.consume_food_at(request.slot) {
            heals.write(HealRequestMessage {
                target: request.actor,
                amount,
            });
        }
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

    #[test]
    fn slot_request_consumes_selected_food_and_emits_one_heal() {
        let mut world = World::new();
        world.init_resource::<Messages<ConsumeSlotRequestMessage>>();
        world.init_resource::<Messages<HealRequestMessage>>();
        let mut inventory = Inventory::default();
        inventory
            .try_add(ItemKind::Material(crate::inventory::MaterialKind::Wood), 2)
            .unwrap();
        inventory
            .try_add(
                ItemKind::Food {
                    label: "Apple",
                    heal: 25.0,
                },
                2,
            )
            .unwrap();
        let actor = world.spawn(inventory).id();
        world.write_message(ConsumeSlotRequestMessage { actor, slot: 1 });

        world.run_system_once(read_consume_slot_requests).unwrap();

        assert_eq!(
            world
                .entity(actor)
                .get::<Inventory>()
                .unwrap()
                .slot(1)
                .unwrap()
                .quantity,
            1
        );
        let messages = world.resource::<Messages<HealRequestMessage>>();
        let mut cursor = messages.get_cursor();
        let requests: Vec<_> = cursor.read(messages).collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].target, actor);
        assert_eq!(requests[0].amount, 25.0);
    }

    #[test]
    fn invalid_slot_or_non_food_does_not_mutate_or_emit() {
        let mut world = World::new();
        world.init_resource::<Messages<ConsumeSlotRequestMessage>>();
        world.init_resource::<Messages<HealRequestMessage>>();
        let mut inventory = Inventory::default();
        inventory
            .try_add(
                ItemKind::Weapon(crate::inventory::WeaponItem::LOOTABLE_CLUB),
                1,
            )
            .unwrap();
        let actor = world.spawn(inventory).id();
        world.write_message(ConsumeSlotRequestMessage { actor, slot: 0 });
        world.write_message(ConsumeSlotRequestMessage {
            actor,
            slot: super::super::data::INVENTORY_SLOTS,
        });

        world.run_system_once(read_consume_slot_requests).unwrap();

        assert!(matches!(
            world
                .entity(actor)
                .get::<Inventory>()
                .unwrap()
                .slot(0)
                .map(|stack| stack.kind),
            Some(ItemKind::Weapon(_))
        ));
        assert!(world.resource::<Messages<HealRequestMessage>>().is_empty());
    }
}
