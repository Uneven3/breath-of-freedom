//! Compatibility path plus the world-item presentation adapter.

pub mod data;
mod pickup;

pub use data::{
    ConsumeSlotRequestMessage, EquipSlotRequestMessage, Inventory, ItemKind, ItemStack,
    MaterialKind, PickupMode, WeaponDurability, WeaponItem,
};
pub use pickup::spawn_world_item;
