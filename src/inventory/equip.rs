//! Equip: the swap contract `combat::weapon` already anticipates — "equip
//! inserts/retires `WeaponProfile`; the component IS the armed boolean."

use bevy::prelude::*;

use crate::input::action::IntentAction;
use crate::input::frame::{ActiveActions, InputControlledBy};
use crate::movement::Player;

use super::data::{
    EquipRequestMessage, Inventory, InventoryInputCursor, ItemKind, WeaponDurability,
};

type CurrentWeapon<'a> = (Option<&'a WeaponDurability>, &'a mut Inventory);

/// The only writer of `WeaponProfile`/`WeaponDurability` on an equipped
/// actor. Swap is atomic: the outgoing weapon (if any) returns to
/// `Inventory` with its remaining durability intact before the incoming one
/// is inserted. Reads `WeaponDurability` only, not `WeaponProfile` — the two
/// are always inserted/removed together (here, in `player.rs`, and in
/// `durability::unequip_broken_weapons`), so `WeaponDurability::item()`
/// already carries the profile that would otherwise need a second query.
pub fn apply_equip_requests(
    mut requests: MessageReader<EquipRequestMessage>,
    mut actors: Query<CurrentWeapon>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok((current_durability, mut inventory)) = actors.get_mut(request.actor) else {
            continue;
        };
        if let Some(durability) = current_durability {
            let outgoing = durability.item();
            if inventory.try_add(ItemKind::Weapon(outgoing), 1).is_err() {
                warn!(
                    "[inventory] no room to keep '{}', it was lost in the swap",
                    outgoing.label
                );
            }
        }
        commands
            .entity(request.actor)
            .insert(request.item.profile)
            .insert(WeaponDurability::new(request.item));
        info!("[inventory] equipped {}", request.item.label);
    }
}

type CycleActorQuery<'a> = (
    Entity,
    &'a InputControlledBy,
    &'a mut InventoryInputCursor,
    &'a mut Inventory,
);

/// `IntentAction::CycleWeapon`: re-equip the first weapon held in
/// `Inventory`. Without this, breaking the last weapon on hand with no
/// world pickup nearby leaves the player permanently unarmed — the
/// checkpoint this exists for.
pub fn read_cycle_weapon_requests(
    actions: Res<ActiveActions>,
    mut actors: Query<CycleActorQuery, With<Player>>,
    mut equip: MessageWriter<EquipRequestMessage>,
) {
    for (actor, source, mut cursor, mut inventory) in &mut actors {
        if !cursor.triggered(&actions, source.0, IntentAction::CycleWeapon) {
            continue;
        }
        if let Some(item) = inventory.take_first_weapon() {
            equip.write(EquipRequestMessage { actor, item });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    use crate::combat::weapon::WeaponProfile;
    use crate::input::frame::LOCAL_INPUT_SOURCE;
    use crate::inventory::data::WeaponItem;

    #[test]
    fn equipping_an_unarmed_actor_inserts_profile_and_durability() {
        let mut world = World::new();
        world.init_resource::<Messages<EquipRequestMessage>>();
        let actor = world.spawn(Inventory::default()).id();
        world.write_message(EquipRequestMessage {
            actor,
            item: WeaponItem::GRAYBOX_SWORD,
        });

        world.run_system_once(apply_equip_requests).unwrap();

        let entity = world.entity(actor);
        assert_eq!(
            *entity.get::<WeaponProfile>().unwrap(),
            WeaponProfile::GRAYBOX_SWORD
        );
        assert_eq!(entity.get::<WeaponDurability>().unwrap().current(), 40);
        assert_eq!(entity.get::<Inventory>().unwrap().iter().count(), 0);
    }

    #[test]
    fn swapping_returns_the_damaged_weapon_with_remaining_durability_intact() {
        let mut world = World::new();
        world.init_resource::<Messages<EquipRequestMessage>>();
        let mut worn_club = WeaponDurability::new(WeaponItem::LOOTABLE_CLUB);
        worn_club.apply_hit(10); // 15 -> 5 remaining
        let actor = world
            .spawn((WeaponProfile::BOKOBO_CLUB, worn_club, Inventory::default()))
            .id();

        world.write_message(EquipRequestMessage {
            actor,
            item: WeaponItem::GRAYBOX_SWORD,
        });
        world.run_system_once(apply_equip_requests).unwrap();

        let entity = world.entity(actor);
        assert_eq!(
            *entity.get::<WeaponProfile>().unwrap(),
            WeaponProfile::GRAYBOX_SWORD
        );
        let stacks: Vec<_> = entity.get::<Inventory>().unwrap().iter().copied().collect();
        assert_eq!(stacks.len(), 1);
        let ItemKind::Weapon(stashed) = stacks[0].kind else {
            panic!("expected a weapon stack");
        };
        assert_eq!(
            stashed.current_durability, 5,
            "the stashed club must remember it was worn down, not reset to max"
        );
    }

    #[test]
    fn swap_with_a_full_inventory_still_equips_and_loses_the_old_weapon_without_panic() {
        let mut world = World::new();
        world.init_resource::<Messages<EquipRequestMessage>>();
        let mut full = Inventory::default();
        for _ in 0..super::super::data::INVENTORY_SLOTS {
            full.try_add(ItemKind::Weapon(WeaponItem::LOOTABLE_CLUB), 1)
                .unwrap();
        }
        let actor = world
            .spawn((
                WeaponProfile::GRAYBOX_SWORD,
                WeaponDurability::new(WeaponItem::GRAYBOX_SWORD),
                full,
            ))
            .id();

        world.write_message(EquipRequestMessage {
            actor,
            item: WeaponItem::LOOTABLE_CLUB,
        });
        world.run_system_once(apply_equip_requests).unwrap();

        let entity = world.entity(actor);
        assert_eq!(
            *entity.get::<WeaponProfile>().unwrap(),
            WeaponProfile::BOKOBO_CLUB
        );
        assert_eq!(
            entity.get::<Inventory>().unwrap().iter().count(),
            super::super::data::INVENTORY_SLOTS,
            "still full — the outgoing sword had nowhere to go"
        );
    }

    #[test]
    fn cycle_weapon_re_equips_the_first_stashed_weapon() {
        let mut world = World::new();
        world.init_resource::<Messages<EquipRequestMessage>>();
        let mut actions = ActiveActions::default();
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::CycleWeapon);
        world.insert_resource(actions);

        let mut inventory = Inventory::default();
        inventory
            .try_add(ItemKind::Weapon(WeaponItem::LOOTABLE_CLUB), 1)
            .unwrap();
        let actor = world
            .spawn((
                Player,
                InputControlledBy(LOCAL_INPUT_SOURCE),
                InventoryInputCursor::default(),
                inventory,
            ))
            .id();

        world.run_system_once(read_cycle_weapon_requests).unwrap();

        let messages = world.resource::<Messages<EquipRequestMessage>>();
        let mut cursor = messages.get_cursor();
        let requests: Vec<_> = cursor.read(messages).collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].actor, actor);
        assert_eq!(requests[0].item, WeaponItem::LOOTABLE_CLUB);
        assert_eq!(
            world
                .entity(actor)
                .get::<Inventory>()
                .unwrap()
                .iter()
                .count(),
            0,
            "taken out of the pool the moment the request is written"
        );
    }
}
