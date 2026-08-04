//! Compatibility path plus the world-item presentation adapter.

pub mod data;
mod pickup;

pub use bof_simulation::inventory::{InventoryPlugin, InventorySet};
pub use data::{
    ConsumeSlotRequestMessage, EquipSlotRequestMessage, Inventory, InventoryInputCursor, ItemKind,
    ItemStack, MaterialKind, PickupMode, WeaponDurability, WeaponItem,
};
pub use pickup::spawn_world_item;
