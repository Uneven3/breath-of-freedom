//! Inventory — items, equip/durability, consumables (see
//! `docs/ARCHITECTURE.md`). Same shape as Health: small, focused systems,
//! each request type owned by whoever applies it.

use bevy_app::{App, FixedUpdate, Plugin};
use bevy_ecs::prelude::*;

pub mod consume;
pub mod data;
pub mod durability;
pub mod equip;
pub mod pickup;

pub use data::{
    ConsumeSlotRequestMessage, EquipSlotRequestMessage, Inventory, InventoryInputCursor, ItemKind,
    ItemStack, MaterialKind, PickupMode, WeaponDurability, WeaponItem,
};
use data::{EquipRequestMessage, WeaponBrokeMessage};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum InventorySet {
    Collect,
    Equip,
    Consume,
    Durability,
    Break,
}

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EquipRequestMessage>();
        app.add_message::<EquipSlotRequestMessage>();
        app.add_message::<ConsumeSlotRequestMessage>();
        app.add_message::<WeaponBrokeMessage>();

        app.configure_sets(
            FixedUpdate,
            (
                InventorySet::Collect,
                InventorySet::Equip,
                InventorySet::Consume,
            )
                .chain(),
        );
        app.configure_sets(
            FixedUpdate,
            InventorySet::Break.after(InventorySet::Durability),
        );

        // Chained: all three mutate the same actor's `Inventory` (and the
        // last two can both write `EquipRequestMessage` off simultaneous
        // key presses) — an unordered tuple would leave "who wins" up to
        // the scheduler instead of a declared, deterministic order.
        app.add_systems(
            FixedUpdate,
            (
                pickup::auto_collect,
                pickup::read_interact_pickups,
                equip::read_cycle_weapon_requests,
            )
                .chain()
                .in_set(InventorySet::Collect),
        );
        app.configure_sets(
            FixedUpdate,
            InventorySet::Collect.after(crate::interaction::InteractionSet::Arbitrate),
        );
        app.add_systems(
            FixedUpdate,
            (equip::read_equip_slot_requests, equip::apply_equip_requests)
                .chain()
                .in_set(InventorySet::Equip),
        );
        app.add_systems(
            FixedUpdate,
            (
                consume::read_consume_slot_requests,
                consume::read_use_item_requests,
            )
                .chain()
                .in_set(InventorySet::Consume),
        );
        app.add_systems(
            FixedUpdate,
            durability::track_weapon_durability.in_set(InventorySet::Durability),
        );
        app.add_systems(
            FixedUpdate,
            durability::unequip_broken_weapons.in_set(InventorySet::Break),
        );
    }
}

#[cfg(test)]
mod plugin_tests {
    use std::time::Duration;

    use bevy_app::TaskPoolPlugin;
    use bevy_math::Vec3;
    use bevy_time::{TimePlugin, TimeUpdateStrategy};

    use super::*;
    use crate::health::HealthPlugin;
    use bof_domain::combat::messages::HitImpactMessage;
    use bof_domain::combat::weapon::WeaponProfile;
    use bof_domain::input::frame::ActiveActions;
    use bof_domain::interaction::InteractionRequest;

    fn real_app() -> App {
        let mut app = App::new();
        app.add_plugins((TaskPoolPlugin::default(), TimePlugin));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            1.0 / 60.0,
        )));
        // `HitImpactMessage` is Combat's contract, and `ActiveActions` is
        // Input's resource; neither plugin is part of this test app, so
        // both are registered by hand here.
        app.add_message::<HitImpactMessage>();
        app.add_message::<InteractionRequest>();
        app.init_resource::<ActiveActions>();
        app.add_plugins((HealthPlugin, InventoryPlugin));
        app.finish();
        app
    }

    /// End-to-end regression for "the component IS the armed boolean"
    /// (`combat::weapon`'s own documented contract): enough melee impacts to
    /// cross zero durability must leave the actor without `WeaponProfile` —
    /// the exact component Combat's attack motor requires to propose a swing.
    #[test]
    fn breaking_the_equipped_weapon_removes_the_armed_component_end_to_end() {
        let mut app = real_app();
        let actor = app
            .world_mut()
            .spawn((
                WeaponProfile::GRAYBOX_SWORD,
                WeaponDurability::new(WeaponItem::GRAYBOX_SWORD),
                Inventory::default(),
            ))
            .id();
        let target = app.world_mut().spawn_empty().id();

        for _ in 0..WeaponItem::GRAYBOX_SWORD.max_durability {
            app.world_mut().write_message(HitImpactMessage {
                target,
                attacker: actor,
                position: Vec3::ZERO,
                damage: 10.0,
                critical: false,
                melee: true,
            });
            app.update();
        }

        let entity = app.world().entity(actor);
        assert!(
            !entity.contains::<WeaponProfile>(),
            "a broken weapon must leave the actor unarmed"
        );
        assert!(!entity.contains::<WeaponDurability>());
        assert!(
            app.world_mut()
                .query_filtered::<Entity, With<WeaponProfile>>()
                .iter(app.world())
                .next()
                .is_none(),
            "no entity should still satisfy Combat's armed query after the break"
        );
    }
}
