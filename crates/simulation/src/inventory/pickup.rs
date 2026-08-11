//! World-side pickups: how an item sitting on the ground becomes inventory.
//!
//! Two collection modes (BotW-accurate): materials/food auto-collect on
//! approach, weapons/large objects need `Interact` — mirrors
//! `mounts::lifecycle::read_interact_requests`'s contextual pattern (nearest
//! candidate in range, consumed once per trigger edge via a per-domain
//! cursor).

use bevy_ecs::prelude::*;
use bevy_transform::components::Transform;

use bof_domain::interaction::{InteractionKind, InteractionRequest};
use bof_domain::movement::{ActorId, Player};

use super::data::{EquipRequestMessage, Inventory, ItemKind, PickupMode, WorldItem};

const AUTO_PICKUP_RANGE: f32 = 1.2;

/// `InventorySet::Collect`: every `Auto` item within range joins the pool
/// this tick, no input required. Still scoped to `Player` for graybox; the
/// interact path is actor-generic now, this one is the remaining holdout.
pub fn auto_collect(
    mut actors: Query<(&ActorId, &Transform, &mut Inventory), With<Player>>,
    items: Query<(Entity, &Transform, &WorldItem)>,
    mut commands: Commands,
) {
    for (item_entity, item_transform, item) in &items {
        if item.mode != PickupMode::Auto {
            continue;
        }
        if item.is_claimed() {
            continue;
        }
        let winner = actors
            .iter_mut()
            .filter_map(|(actor_id, actor_transform, inventory)| {
                let distance_squared = item_transform
                    .translation
                    .distance_squared(actor_transform.translation);
                (distance_squared <= AUTO_PICKUP_RANGE * AUTO_PICKUP_RANGE).then_some((
                    distance_squared,
                    *actor_id,
                    inventory,
                ))
            })
            .min_by(|left, right| {
                left.0
                    .total_cmp(&right.0)
                    .then_with(|| left.1.cmp(&right.1))
            });
        if let Some((_, _, mut inventory)) = winner
            && inventory
                .try_add(item.stack.kind, item.stack.quantity)
                .is_ok()
        {
            commands.entity(item_entity).despawn();
        }
    }
}

/// `InventorySet::Collect`: a weapon requests an equip swap (never enters
/// `Inventory` directly — `equip::apply_equip_requests` decides where the
/// outgoing weapon lands); anything else stacks straight into the pool.
/// Applies the arbiter's decision to a world item. Reads no input: after the
/// interaction arbiter, only one domain can win a given press.
pub fn read_interact_pickups(
    mut interactions: MessageReader<InteractionRequest>,
    mut actors: Query<&mut Inventory>,
    mut items: Query<&mut WorldItem>,
    mut commands: Commands,
    mut equip: MessageWriter<EquipRequestMessage>,
) {
    for interaction in interactions.read() {
        if interaction.kind != InteractionKind::Pickup {
            continue;
        }
        let (Some(item_entity), Ok(mut inventory)) =
            (interaction.target, actors.get_mut(interaction.actor))
        else {
            continue;
        };
        let Ok(mut world_item) = items.get_mut(item_entity) else {
            continue;
        };
        if world_item.is_claimed() {
            continue;
        }
        match world_item.stack.kind {
            ItemKind::Weapon(item) => {
                equip.write(EquipRequestMessage {
                    actor: interaction.actor,
                    item,
                    world_item: Some(item_entity),
                });
            }
            kind @ (ItemKind::Material(_) | ItemKind::Food { .. }) => {
                if inventory.try_add(kind, world_item.stack.quantity).is_ok() && world_item.claim()
                {
                    commands.entity(item_entity).despawn();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::RunSystemOnce;

    use bof_domain::combat::weapon::WeaponProfile;
    use bof_domain::inventory::{
        INVENTORY_SLOTS, ItemStack, MaterialKind, WeaponDurability, WeaponItem,
    };

    fn material_stack() -> ItemStack {
        ItemStack {
            kind: ItemKind::Material(MaterialKind::Wood),
            quantity: 1,
        }
    }

    #[test]
    fn auto_collect_picks_up_in_range_auto_items_and_ignores_interact_items() {
        let mut world = World::new();
        let player = world
            .spawn((
                Player,
                ActorId::PLAYER,
                Transform::default(),
                Inventory::default(),
            ))
            .id();
        let auto_item = world
            .spawn((
                Transform::from_xyz(0.5, 0.0, 0.0),
                WorldItem::new(material_stack(), PickupMode::Auto),
            ))
            .id();
        let far_item = world
            .spawn((
                Transform::from_xyz(10.0, 0.0, 0.0),
                WorldItem::new(material_stack(), PickupMode::Auto),
            ))
            .id();
        let interact_item = world
            .spawn((
                Transform::from_xyz(0.2, 0.0, 0.0),
                WorldItem::new(
                    ItemStack {
                        kind: ItemKind::Weapon(WeaponItem::LOOTABLE_CLUB),
                        quantity: 1,
                    },
                    PickupMode::Interact,
                ),
            ))
            .id();

        world.run_system_once(auto_collect).unwrap();

        assert!(
            world.get_entity(auto_item).is_err(),
            "in-range auto item collected"
        );
        assert!(
            world.get_entity(far_item).is_ok(),
            "out-of-range item stays"
        );
        assert!(
            world.get_entity(interact_item).is_ok(),
            "interact-mode item is never auto-collected"
        );
        assert_eq!(
            world
                .entity(player)
                .get::<Inventory>()
                .unwrap()
                .iter()
                .count(),
            1
        );
    }

    #[test]
    fn one_auto_pickup_is_claimed_once_by_the_nearest_player() {
        let mut world = World::new();
        let farther = world
            .spawn((
                Player,
                ActorId::authored(12),
                Transform::from_xyz(0.8, 0.0, 0.0),
                Inventory::default(),
            ))
            .id();
        let nearer = world
            .spawn((
                Player,
                ActorId::authored(11),
                Transform::from_xyz(0.2, 0.0, 0.0),
                Inventory::default(),
            ))
            .id();
        let item = world
            .spawn((
                Transform::default(),
                WorldItem::new(material_stack(), PickupMode::Auto),
            ))
            .id();

        world.run_system_once(auto_collect).unwrap();

        assert!(world.get_entity(item).is_err());
        assert_eq!(
            world
                .entity(nearer)
                .get::<Inventory>()
                .unwrap()
                .iter()
                .map(|stack| stack.quantity)
                .sum::<u32>(),
            1
        );
        assert_eq!(
            world
                .entity(farther)
                .get::<Inventory>()
                .unwrap()
                .iter()
                .map(|stack| stack.quantity)
                .sum::<u32>(),
            0,
            "deferred despawn must not let a second actor duplicate the pickup"
        );
    }

    /// Input is no longer this system's concern: `interaction` resolves the
    /// press and this only applies the decision. The "one press, one
    /// interaction" and "consumed exactly once" invariants moved with it and
    /// are covered in `interaction::tests`.
    #[test]
    fn an_interaction_request_on_a_weapon_asks_for_an_equip_instead_of_stacking() {
        let mut world = World::new();
        world.init_resource::<Messages<EquipRequestMessage>>();
        world.init_resource::<Messages<InteractionRequest>>();

        let player = world
            .spawn((Player, Transform::default(), Inventory::default()))
            .id();
        let weapon_item = world
            .spawn((
                Transform::from_xyz(0.5, 0.0, 0.0),
                WorldItem::new(
                    ItemStack {
                        kind: ItemKind::Weapon(WeaponItem::LOOTABLE_CLUB),
                        quantity: 1,
                    },
                    PickupMode::Interact,
                ),
            ))
            .id();
        world.write_message(InteractionRequest {
            actor: player,
            target: Some(weapon_item),
            kind: InteractionKind::Pickup,
        });

        world.run_system_once(read_interact_pickups).unwrap();

        assert!(
            world.get_entity(weapon_item).is_ok(),
            "the pickup remains until Inventory commits the swap"
        );
        assert_eq!(
            world
                .entity(player)
                .get::<Inventory>()
                .unwrap()
                .iter()
                .count(),
            0,
            "the weapon must not land directly in the pool"
        );
        let messages = world.resource::<Messages<EquipRequestMessage>>();
        let mut cursor = messages.get_cursor();
        let requests: Vec<_> = cursor.read(messages).collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].actor, player);
        assert_eq!(requests[0].item, WeaponItem::LOOTABLE_CLUB);
        assert_eq!(requests[0].world_item, Some(weapon_item));

        world
            .run_system_once(super::super::equip::apply_equip_requests)
            .unwrap();
        assert!(
            world.get_entity(weapon_item).is_err(),
            "a committed equip consumes the pickup"
        );
    }

    #[test]
    fn rejected_weapon_swap_keeps_both_equipped_weapon_and_world_pickup() {
        let mut world = World::new();
        world.init_resource::<Messages<EquipRequestMessage>>();
        world.init_resource::<Messages<InteractionRequest>>();

        let mut inventory = Inventory::default();
        for _ in 0..INVENTORY_SLOTS {
            inventory
                .try_add(ItemKind::Weapon(WeaponItem::LOOTABLE_CLUB), 1)
                .unwrap();
        }
        let player = world
            .spawn((
                Player,
                Transform::default(),
                inventory,
                WeaponProfile::GRAYBOX_SWORD,
                WeaponDurability::new(WeaponItem::GRAYBOX_SWORD),
            ))
            .id();
        let pickup = world
            .spawn(WorldItem::new(
                ItemStack {
                    kind: ItemKind::Weapon(WeaponItem::LOOTABLE_CLUB),
                    quantity: 1,
                },
                PickupMode::Interact,
            ))
            .id();
        world.write_message(InteractionRequest {
            actor: player,
            target: Some(pickup),
            kind: InteractionKind::Pickup,
        });

        world.run_system_once(read_interact_pickups).unwrap();
        world
            .run_system_once(super::super::equip::apply_equip_requests)
            .unwrap();

        assert!(world.get_entity(pickup).is_ok());
        assert_eq!(
            *world.entity(player).get::<WeaponProfile>().unwrap(),
            WeaponProfile::GRAYBOX_SWORD
        );
        assert_eq!(
            world
                .entity(player)
                .get::<Inventory>()
                .unwrap()
                .iter()
                .count(),
            INVENTORY_SLOTS
        );
    }

    #[test]
    fn one_interact_pickup_cannot_be_claimed_by_two_actors() {
        let mut world = World::new();
        world.init_resource::<Messages<EquipRequestMessage>>();
        world.init_resource::<Messages<InteractionRequest>>();
        let first = world.spawn(Inventory::default()).id();
        let second = world.spawn(Inventory::default()).id();
        let item = world
            .spawn(WorldItem::new(material_stack(), PickupMode::Interact))
            .id();
        for actor in [first, second] {
            world.write_message(InteractionRequest {
                actor,
                target: Some(item),
                kind: InteractionKind::Pickup,
            });
        }

        world.run_system_once(read_interact_pickups).unwrap();

        assert!(world.get_entity(item).is_err());
        let total = [first, second]
            .into_iter()
            .map(|actor| {
                world
                    .entity(actor)
                    .get::<Inventory>()
                    .unwrap()
                    .iter()
                    .map(|stack| stack.quantity)
                    .sum::<u32>()
            })
            .sum::<u32>();
        assert_eq!(total, 1, "a world item has exactly one owner after commit");
    }

    /// A decision aimed at another domain must not be acted on here.
    #[test]
    fn a_mount_interaction_is_ignored_by_pickup() {
        let mut world = World::new();
        world.init_resource::<Messages<EquipRequestMessage>>();
        world.init_resource::<Messages<InteractionRequest>>();

        let player = world
            .spawn((Player, Transform::default(), Inventory::default()))
            .id();
        let item = world
            .spawn((
                Transform::from_xyz(0.5, 0.0, 0.0),
                WorldItem::new(material_stack(), PickupMode::Interact),
            ))
            .id();
        world.write_message(InteractionRequest {
            actor: player,
            target: Some(item),
            kind: InteractionKind::Mount,
        });

        world.run_system_once(read_interact_pickups).unwrap();

        assert!(
            world.get_entity(item).is_ok(),
            "untouched by another domain"
        );
    }
}
