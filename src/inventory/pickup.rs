//! World-side pickups: how an item sitting on the ground becomes inventory.
//!
//! Two collection modes (BotW-accurate): materials/food auto-collect on
//! approach, weapons/large objects need `Interact` — mirrors
//! `mounts::lifecycle::read_interact_requests`'s contextual pattern (nearest
//! candidate in range, consumed once per trigger edge via a per-domain
//! cursor).

use bevy::prelude::*;

use crate::input::action::IntentAction;
use crate::input::frame::{ActiveActions, InputControlledBy};
use crate::movement::Player;
use crate::visuals::ToonMaterial;
use crate::visuals::toon::toon_color;

use super::data::{
    EquipRequestMessage, Inventory, InventoryInputCursor, ItemKind, ItemStack, PickupMode,
    WorldItem,
};

const AUTO_PICKUP_RANGE: f32 = 1.2;
const INTERACT_PICKUP_RANGE: f32 = 2.5;

/// No `Collider`/`RigidBody`: a solid pickup would shove the player before
/// the auto-pickup radius ever triggers. Deliberate graybox simplification
/// — a collider lands if the feeling asks for it.
pub fn spawn_world_item(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ToonMaterial>,
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
    commands
        .spawn((
            Name::new(name.to_string()),
            WorldItem { stack, mode },
            Mesh3d(meshes.add(Cuboid::new(dims.x, dims.y, dims.z))),
            MeshMaterial3d(materials.add(toon_color(color))),
            Transform::from_translation(position),
        ))
        .id()
}

/// `InventorySet::Collect`: every `Auto` item within range joins the pool
/// this tick, no input required. Scoped to `Player` for graybox — the same
/// scoping `mounts::lifecycle::read_interact_requests` uses today.
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

type PickupActorQuery<'a> = (
    Entity,
    &'a InputControlledBy,
    &'a mut InventoryInputCursor,
    &'a Transform,
    &'a mut Inventory,
);

/// `InventorySet::Collect`: `Interact` on the nearest `Interact`-mode item
/// in range. A weapon requests an equip swap (never enters `Inventory`
/// directly — `equip::apply_equip_requests` decides where the outgoing
/// weapon lands); anything else stacks straight into the pool.
pub fn read_interact_pickups(
    actions: Res<ActiveActions>,
    mut actors: Query<PickupActorQuery, With<Player>>,
    items: Query<(Entity, &Transform, &WorldItem)>,
    mut commands: Commands,
    mut equip: MessageWriter<EquipRequestMessage>,
) {
    for (actor, source, mut cursor, actor_transform, mut inventory) in &mut actors {
        if !cursor.triggered(&actions, source.0, IntentAction::Interact) {
            continue;
        }
        let origin = actor_transform.translation;
        let candidate = items
            .iter()
            .filter(|(_, transform, item)| {
                item.mode == PickupMode::Interact
                    && transform.translation.distance(origin) <= INTERACT_PICKUP_RANGE
            })
            .min_by(|(_, left, _), (_, right, _)| {
                left.translation
                    .distance_squared(origin)
                    .total_cmp(&right.translation.distance_squared(origin))
            });
        let Some((item_entity, _, world_item)) = candidate else {
            continue;
        };
        match world_item.stack.kind {
            ItemKind::Weapon(item) => {
                equip.write(EquipRequestMessage { actor, item });
                commands.entity(item_entity).despawn();
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

    use crate::input::frame::{ActiveActions, LOCAL_INPUT_SOURCE};
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

    #[test]
    fn interact_pickup_on_weapon_requests_equip_instead_of_stacking() {
        let mut world = World::new();
        world.init_resource::<Messages<EquipRequestMessage>>();
        let mut actions = ActiveActions::default();
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::Interact);
        world.insert_resource(actions);

        let player = world
            .spawn((
                Player,
                InputControlledBy(LOCAL_INPUT_SOURCE),
                InventoryInputCursor::default(),
                Transform::default(),
                Inventory::default(),
            ))
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

        world.run_system_once(read_interact_pickups).unwrap();

        assert!(world.get_entity(weapon_item).is_err(), "picked up");
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
    }

    #[test]
    fn interact_pickup_consumes_the_trigger_exactly_once() {
        let mut world = World::new();
        world.init_resource::<Messages<EquipRequestMessage>>();
        let mut actions = ActiveActions::default();
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::Interact);
        world.insert_resource(actions);

        let player = world
            .spawn((
                Player,
                InputControlledBy(LOCAL_INPUT_SOURCE),
                InventoryInputCursor::default(),
                Transform::default(),
                Inventory::default(),
            ))
            .id();
        world.spawn((
            Transform::from_xyz(0.5, 0.0, 0.0),
            WorldItem {
                stack: material_stack(),
                mode: PickupMode::Interact,
            },
        ));

        world.run_system_once(read_interact_pickups).unwrap();
        world.spawn((
            Transform::from_xyz(0.5, 0.0, 0.0),
            WorldItem {
                stack: material_stack(),
                mode: PickupMode::Interact,
            },
        ));
        world.run_system_once(read_interact_pickups).unwrap();

        assert_eq!(
            world
                .entity(player)
                .get::<Inventory>()
                .unwrap()
                .iter()
                .count(),
            1,
            "second run sees no new trigger generation, so no second pickup"
        );
    }
}
