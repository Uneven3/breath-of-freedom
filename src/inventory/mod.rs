//! Inventory — items, equip/durability, consumables (see
//! `docs/ARCHITECTURE.md`). Same shape as Health: small, focused systems,
//! each request type owned by whoever applies it.

use bevy::prelude::*;

pub mod consume;
pub mod data;
pub mod durability;
pub mod equip;
pub mod pickup;

pub use data::{
    Inventory, InventoryInputCursor, ItemKind, ItemStack, MaterialKind, PickupMode,
    WeaponDurability, WeaponItem,
};
pub use pickup::spawn_world_item;

use data::{EquipRequestMessage, WeaponBrokeMessage};

use crate::combat::CombatSet;
use crate::movement::MovementSet;

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
        app.add_message::<WeaponBrokeMessage>();

        // Same band as `MountsSet::PostMove`: disjoint data, no conflict.
        app.configure_sets(
            FixedUpdate,
            (
                InventorySet::Collect,
                InventorySet::Equip,
                InventorySet::Consume,
            )
                .chain()
                .after(MovementSet::SyncAttachments)
                .before(CombatSet::ApplyContext),
        );
        // The `melee` filter on `HitImpactMessage` makes waiting for
        // Projectiles/Charge unnecessary — a non-melee impact is ignored
        // regardless of when it's read this tick.
        app.configure_sets(
            FixedUpdate,
            InventorySet::Durability.after(CombatSet::EmitConstraints),
        );
        app.configure_sets(
            FixedUpdate,
            InventorySet::Break.after(InventorySet::Durability),
        );

        app.add_systems(
            FixedUpdate,
            (
                pickup::auto_collect,
                pickup::read_interact_pickups,
                equip::read_cycle_weapon_requests,
            )
                .in_set(InventorySet::Collect),
        );
        app.add_systems(
            FixedUpdate,
            equip::apply_equip_requests.in_set(InventorySet::Equip),
        );
        app.add_systems(
            FixedUpdate,
            consume::read_use_item_requests.in_set(InventorySet::Consume),
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

    use bevy::time::TimeUpdateStrategy;

    use super::*;
    use crate::combat::motors::attack::HitImpactMessage;
    use crate::combat::weapon::WeaponProfile;
    use crate::health::HealthPlugin;

    fn real_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            1.0 / 60.0,
        )));
        // `HitImpactMessage` is Combat's contract, and `ActiveActions` is
        // Input's resource; neither plugin is part of this test app, so
        // both are registered by hand here.
        app.add_message::<HitImpactMessage>();
        app.init_resource::<crate::input::frame::ActiveActions>();
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
