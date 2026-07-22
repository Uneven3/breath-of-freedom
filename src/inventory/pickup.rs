//! World-side pickups: how an item sitting on the ground becomes inventory.
//!
//! Two collection modes (BotW-accurate): materials/food auto-collect on
//! approach, weapons/large objects need `Interact` — mirrors
//! `mounts::lifecycle::read_interact_requests`'s contextual pattern (nearest
//! candidate in range, consumed once per trigger edge via a per-domain
//! cursor).

use bevy::prelude::*;

use crate::interaction::{Interactable, InteractionKind, InteractionRequest};
use crate::movement::Player;
use crate::visuals::materials::matte_color;

use super::data::{EquipRequestMessage, Inventory, ItemKind, ItemStack, PickupMode, WorldItem};

const AUTO_PICKUP_RANGE: f32 = 1.2;
const INTERACT_PICKUP_RANGE: f32 = 2.5;

/// No `Collider`/`RigidBody`: a solid pickup would shove the player before
/// the auto-pickup radius ever triggers. Deliberate graybox simplification
/// — a collider lands if the feeling asks for it.
pub fn spawn_world_item(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    name: &str,
    position: Vec3,
    stack: ItemStack,
    mode: PickupMode,
) -> Entity {
    let (dims, color) = match stack.kind {
        ItemKind::Weapon(_) => (Vec3::new(0.15, 0.7, 0.15), Color::srgb(0.6, 0.6, 0.65)),
        ItemKind::Material(_) => (Vec3::new(0.3, 0.3, 0.3), Color::srgb(0.45, 0.3, 0.15)),
        ItemKind::Food { .. } => (Vec3::new(0.25, 0.25, 0.25), Color::srgb(0.8, 0.2, 0.2)),
    };
    let mut item = commands.spawn((
        Name::new(name.to_string()),
        WorldItem { stack, mode },
        Mesh3d(meshes.add(Cuboid::new(dims.x, dims.y, dims.z))),
        MeshMaterial3d(materials.add(matte_color(color))),
        Transform::from_translation(position),
    ));
    // `Auto` items are swept up by proximity and never compete for the key.
    if mode == PickupMode::Interact {
        item.insert(Interactable {
            kind: InteractionKind::Pickup,
            range: INTERACT_PICKUP_RANGE,
        });
    }
    item.id()
}

/// `InventorySet::Collect`: every `Auto` item within range joins the pool
/// this tick, no input required. Still scoped to `Player` for graybox; the
/// interact path is actor-generic now, this one is the remaining holdout.
pub fn auto_collect(
    mut actors: Query<(&Transform, &mut Inventory), With<Player>>,
    items: Query<(Entity, &Transform, &WorldItem)>,
    mut commands: Commands,
) {
    for (actor_transform, mut inventory) in &mut actors {
        for (item_entity, item_transform, item) in &items {
            if item.mode != PickupMode::Auto {
                continue;
            }
            if item_transform
                .translation
                .distance(actor_transform.translation)
                > AUTO_PICKUP_RANGE
            {
                continue;
            }
            if inventory
                .try_add(item.stack.kind, item.stack.quantity)
                .is_ok()
            {
                commands.entity(item_entity).despawn();
            }
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
    items: Query<&WorldItem>,
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
        let Ok(world_item) = items.get(item_entity) else {
            continue;
        };
        match world_item.stack.kind {
            ItemKind::Weapon(item) => {
                equip.write(EquipRequestMessage {
                    actor: interaction.actor,
                    item,
                    world_item: Some(item_entity),
                });
            }
            kind => {
                if inventory.try_add(kind, world_item.stack.quantity).is_ok() {
                    commands.entity(item_entity).despawn();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    use crate::inventory::data::{MaterialKind, WeaponItem};

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
            .spawn((Player, Transform::default(), Inventory::default()))
            .id();
        let auto_item = world
            .spawn((
                Transform::from_xyz(0.5, 0.0, 0.0),
                WorldItem {
                    stack: material_stack(),
                    mode: PickupMode::Auto,
                },
            ))
            .id();
        let far_item = world
            .spawn((
                Transform::from_xyz(10.0, 0.0, 0.0),
                WorldItem {
                    stack: material_stack(),
                    mode: PickupMode::Auto,
                },
            ))
            .id();
        let interact_item = world
            .spawn((
                Transform::from_xyz(0.2, 0.0, 0.0),
                WorldItem {
                    stack: ItemStack {
                        kind: ItemKind::Weapon(WeaponItem::LOOTABLE_CLUB),
                        quantity: 1,
                    },
                    mode: PickupMode::Interact,
                },
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
                WorldItem {
                    stack: ItemStack {
                        kind: ItemKind::Weapon(WeaponItem::LOOTABLE_CLUB),
                        quantity: 1,
                    },
                    mode: PickupMode::Interact,
                },
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
            .run_system_once(crate::inventory::equip::apply_equip_requests)
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
        for _ in 0..crate::inventory::data::INVENTORY_SLOTS {
            inventory
                .try_add(ItemKind::Weapon(WeaponItem::LOOTABLE_CLUB), 1)
                .unwrap();
        }
        let player = world
            .spawn((
                Player,
                Transform::default(),
                inventory,
                crate::combat::weapon::WeaponProfile::GRAYBOX_SWORD,
                crate::inventory::WeaponDurability::new(WeaponItem::GRAYBOX_SWORD),
            ))
            .id();
        let pickup = world
            .spawn(WorldItem {
                stack: ItemStack {
                    kind: ItemKind::Weapon(WeaponItem::LOOTABLE_CLUB),
                    quantity: 1,
                },
                mode: PickupMode::Interact,
            })
            .id();
        world.write_message(InteractionRequest {
            actor: player,
            target: Some(pickup),
            kind: InteractionKind::Pickup,
        });

        world.run_system_once(read_interact_pickups).unwrap();
        world
            .run_system_once(crate::inventory::equip::apply_equip_requests)
            .unwrap();

        assert!(world.get_entity(pickup).is_ok());
        assert_eq!(
            *world
                .entity(player)
                .get::<crate::combat::weapon::WeaponProfile>()
                .unwrap(),
            crate::combat::weapon::WeaponProfile::GRAYBOX_SWORD
        );
        assert_eq!(
            world
                .entity(player)
                .get::<Inventory>()
                .unwrap()
                .iter()
                .count(),
            crate::inventory::data::INVENTORY_SLOTS
        );
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
                WorldItem {
                    stack: material_stack(),
                    mode: PickupMode::Interact,
                },
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
