//! Inventory data: items, the pool, and equip/durability contracts.
//!
//! Pure data (Constitución §6) — mutation only through `Inventory`'s own
//! methods and `WeaponDurability`'s own methods, same pattern as
//! `health::Health`.

use bevy::prelude::*;

use crate::combat::weapon::WeaponProfile;
use crate::input::InputConsumeCursor;
use crate::input::action::IntentAction;
use crate::input::frame::{ActiveActions, InputSource};

pub const INVENTORY_SLOTS: usize = 8;

/// A melee weapon as Inventory sees it: Combat's public tuning
/// (`WeaponProfile`, §5 — depend on what's exposed) plus what Combat has no
/// reason to know (a label for logs/HUD, how many hits it survives).
/// `current_durability` travels with the item across swaps — stashing a
/// half-broken weapon and re-equipping it later must not repair it back to
/// `max_durability` (`equip::apply_equip_requests` round-trips this from
/// `WeaponDurability::current()`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponItem {
    pub profile: WeaponProfile,
    pub label: &'static str,
    pub max_durability: u32,
    pub current_durability: u32,
}

impl WeaponItem {
    pub const GRAYBOX_SWORD: Self = Self {
        profile: WeaponProfile::GRAYBOX_SWORD,
        label: "Sword",
        max_durability: 40,
        current_durability: 40,
    };

    /// Lootable spare weapon. Reuses the bokobo club's tuning (heavy,
    /// telegraphed) so equipping it is felt, not just logged.
    pub const LOOTABLE_CLUB: Self = Self {
        profile: WeaponProfile::BOKOBO_CLUB,
        label: "Bokobo Club",
        max_durability: 15,
        current_durability: 15,
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MaterialKind {
    Wood,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ItemKind {
    Weapon(WeaponItem),
    Material(MaterialKind),
    Food { label: &'static str, heal: f32 },
}

/// `quantity` is always 1 for `Weapon` and never 0 for any kind — invariants
/// the type doesn't encode (splitting `ItemStack` in two isn't worth it for
/// graybox, §15); `Inventory::try_add` is the only constructor and enforces
/// both.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemStack {
    pub kind: ItemKind,
    pub quantity: u32,
}

/// An actor's item pool. Fixed capacity, no heap allocation (§18). Private
/// slots: `try_add`/`consume_first_food`/`take_first_weapon` are the only
/// mutation paths, same idiom as `Health`.
#[derive(Component, Debug, Clone)]
pub struct Inventory {
    slots: [Option<ItemStack>; INVENTORY_SLOTS],
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: [None; INVENTORY_SLOTS],
        }
    }
}

impl Inventory {
    /// Stacks onto an existing slot of the same `kind` (materials/food
    /// only — weapons never stack); otherwise opens the first free slot.
    /// `Err` leaves the inventory unchanged. A zero quantity is rejected
    /// outright (an empty stack is a state to not create, not to hold —
    /// `consume_first_food`'s decrement would underflow on it) and a
    /// weapon's quantity is forced to 1 regardless of what's passed, so
    /// `ItemStack`'s documented "weapons never stack" invariant holds by
    /// construction instead of by caller discipline.
    pub fn try_add(&mut self, kind: ItemKind, quantity: u32) -> Result<(), ()> {
        if quantity == 0 {
            return Err(());
        }
        let quantity = if matches!(kind, ItemKind::Weapon(_)) {
            1
        } else {
            for slot in self.slots.iter_mut().flatten() {
                if slot.kind == kind {
                    let Some(total) = slot.quantity.checked_add(quantity) else {
                        return Err(());
                    };
                    slot.quantity = total;
                    return Ok(());
                }
            }
            quantity
        };
        for slot in &mut self.slots {
            if slot.is_none() {
                *slot = Some(ItemStack { kind, quantity });
                return Ok(());
            }
        }
        Err(())
    }

    /// Consumes one unit from the first `Food` stack, clearing the slot at
    /// zero. Returns that stack's heal amount.
    pub fn consume_first_food(&mut self) -> Option<f32> {
        for slot in &mut self.slots {
            if let Some(stack) = slot
                && let ItemKind::Food { heal, .. } = stack.kind
            {
                stack.quantity -= 1;
                if stack.quantity == 0 {
                    *slot = None;
                }
                return Some(heal);
            }
        }
        None
    }

    pub fn consume_food_at(&mut self, index: usize) -> Option<f32> {
        let stack = self.slots.get_mut(index)?.as_mut()?;
        let ItemKind::Food { heal, .. } = stack.kind else {
            return None;
        };
        stack.quantity -= 1;
        if stack.quantity == 0 {
            self.slots[index] = None;
        }
        Some(heal)
    }

    /// Removes and returns the first `Weapon` stack found (`CycleWeapon`
    /// re-equips whatever this returns).
    pub fn take_first_weapon(&mut self) -> Option<WeaponItem> {
        for slot in &mut self.slots {
            if let Some(stack) = slot
                && let ItemKind::Weapon(item) = stack.kind
            {
                *slot = None;
                return Some(item);
            }
        }
        None
    }

    pub fn take_weapon_at(&mut self, index: usize) -> Option<WeaponItem> {
        let stack = self.slots.get(index)?.as_ref()?;
        let ItemKind::Weapon(item) = stack.kind else {
            return None;
        };
        self.slots[index] = None;
        Some(item)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ItemStack> {
        self.slots.iter().flatten()
    }

    pub fn slot(&self, index: usize) -> Option<&ItemStack> {
        self.slots.get(index).and_then(Option::as_ref)
    }
}

/// The equipped weapon's remaining hits. Wraps the `WeaponItem` itself
/// (rather than re-declaring `label`/`max_durability`/`current_durability`
/// as separate fields) so there is exactly one place that holds a weapon's
/// durability — `equip::apply_equip_requests` reads `item()` straight back
/// out on a swap instead of reconstructing it field-by-field. Lives
/// alongside `WeaponProfile` on the actor (Combat's tuning component);
/// Combat never reads this — Inventory owns the whole equip/durability
/// contract, per `combat::weapon`'s own noted intent.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct WeaponDurability(WeaponItem);

impl WeaponDurability {
    pub fn new(item: WeaponItem) -> Self {
        Self(item)
    }

    /// Subtracts up to `amount`, clamped at zero. Returns what was applied.
    pub fn apply_hit(&mut self, amount: u32) -> u32 {
        let applied = amount.min(self.0.current_durability);
        self.0.current_durability -= applied;
        applied
    }

    pub fn is_broken(&self) -> bool {
        self.0.current_durability == 0
    }

    pub fn current(&self) -> u32 {
        self.0.current_durability
    }

    pub fn max(&self) -> u32 {
        self.0.max_durability
    }

    pub fn label(&self) -> &'static str {
        self.0.label
    }

    /// The weapon as it should re-enter `Inventory` on a swap — remaining
    /// durability included, not reset.
    pub fn item(&self) -> WeaponItem {
        self.0
    }
}

/// Ask Inventory to equip `item` on `actor` — swap atomic with whatever is
/// currently wielded. Written by `pickup::read_interact_pickups` (a weapon
/// found in the world) and `equip::read_cycle_weapon_requests` (a weapon
/// already held).
#[derive(Message, Debug, Clone, Copy)]
pub struct EquipRequestMessage {
    pub actor: Entity,
    pub item: WeaponItem,
    /// World pickup that owns `item`, when applicable. Inventory despawns it
    /// only after the swap commits; a rejected request leaves it recoverable.
    pub world_item: Option<Entity>,
}

/// Presentation intent to equip the weapon currently stored in `slot`.
/// Inventory validates the actor, index, and item kind in `FixedUpdate`.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EquipSlotRequestMessage {
    pub actor: Entity,
    pub slot: usize,
}

/// Presentation intent to consume the food currently stored in `slot`.
/// Inventory validates the actor, index, and item kind in `FixedUpdate`.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsumeSlotRequestMessage {
    pub actor: Entity,
    pub slot: usize,
}

/// `actor`'s equipped weapon reached zero durability. Emitted exactly once
/// per break by `durability::track_weapon_durability`.
#[derive(Message, Debug, Clone, Copy)]
pub struct WeaponBrokeMessage {
    pub actor: Entity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickupMode {
    Auto,
    Interact,
}

/// An item sitting in the world, not yet in anyone's `Inventory`.
#[derive(Component, Clone, Copy)]
pub struct WorldItem {
    pub stack: ItemStack,
    pub mode: PickupMode,
}

/// Inventory's own trigger cursor — a newtype, not the raw
/// `InputConsumeCursor` (same reasoning as `CombatInputCursor`: a second
/// domain sharing Movement's cursor would steal its edges). One cursor
/// multiplexes every inventory action (`Interact`, `UseItem`,
/// `CycleWeapon`) by action index, same idiom Movement uses internally for
/// its own several actions.
#[derive(Component, Default)]
pub struct InventoryInputCursor(pub InputConsumeCursor);

impl InventoryInputCursor {
    /// Resolves `source`'s frame and consumes `action`'s trigger edge in
    /// one call — collapses the `actions.frame(source)` +
    /// `cursor.0.consume(frame, action)` pair that pickup/equip/consume
    /// each repeat once per action they read.
    pub fn triggered(
        &mut self,
        actions: &ActiveActions,
        source: InputSource,
        action: IntentAction,
    ) -> bool {
        actions
            .frame(source)
            .is_some_and(|frame| self.0.consume(frame, action))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materials_and_food_stack_but_weapons_never_do() {
        let mut inventory = Inventory::default();
        assert_eq!(
            inventory.try_add(ItemKind::Material(MaterialKind::Wood), 1),
            Ok(())
        );
        assert_eq!(
            inventory.try_add(ItemKind::Material(MaterialKind::Wood), 2),
            Ok(())
        );
        assert_eq!(
            inventory.try_add(ItemKind::Weapon(WeaponItem::GRAYBOX_SWORD), 1),
            Ok(())
        );
        assert_eq!(
            inventory.try_add(ItemKind::Weapon(WeaponItem::GRAYBOX_SWORD), 1),
            Ok(()),
            "a second identical weapon opens a new slot instead of stacking"
        );
        let stacks: Vec<_> = inventory.iter().copied().collect();
        assert_eq!(stacks.len(), 3, "1 wood stack (qty 3) + 2 separate swords");
        let wood = stacks
            .iter()
            .find(|s| s.kind == ItemKind::Material(MaterialKind::Wood))
            .unwrap();
        assert_eq!(wood.quantity, 3);
    }

    #[test]
    fn try_add_rejects_zero_quantity_and_forces_weapon_quantity_to_one() {
        let mut inventory = Inventory::default();
        assert_eq!(
            inventory.try_add(ItemKind::Material(MaterialKind::Wood), 0),
            Err(()),
            "a zero-quantity stack must never be created — nothing to decrement later"
        );
        assert!(inventory.iter().next().is_none());

        inventory
            .try_add(ItemKind::Weapon(WeaponItem::GRAYBOX_SWORD), 5)
            .unwrap();
        assert_eq!(
            inventory.iter().next().unwrap().quantity,
            1,
            "a weapon stack is always quantity 1, regardless of what's requested"
        );
    }

    #[test]
    fn try_add_does_not_mutate_on_overflow() {
        let mut inventory = Inventory::default();
        for _ in 0..INVENTORY_SLOTS {
            assert_eq!(
                inventory.try_add(ItemKind::Weapon(WeaponItem::GRAYBOX_SWORD), 1),
                Ok(())
            );
        }
        let before: Vec<_> = inventory.iter().copied().collect();
        assert_eq!(
            inventory.try_add(ItemKind::Material(MaterialKind::Wood), 1),
            Err(())
        );
        let after: Vec<_> = inventory.iter().copied().collect();
        assert_eq!(before, after, "a rejected add must leave slots untouched");
    }

    #[test]
    fn try_add_rejects_quantity_overflow_without_mutating_the_stack() {
        let mut inventory = Inventory::default();
        inventory
            .try_add(ItemKind::Material(MaterialKind::Wood), u32::MAX)
            .unwrap();

        assert_eq!(
            inventory.try_add(ItemKind::Material(MaterialKind::Wood), 1),
            Err(())
        );
        assert_eq!(inventory.iter().next().unwrap().quantity, u32::MAX);
    }

    #[test]
    fn consume_first_food_decrements_then_clears_the_slot() {
        let mut inventory = Inventory::default();
        let apple = ItemKind::Food {
            label: "Apple",
            heal: 25.0,
        };
        inventory.try_add(apple, 2).unwrap();

        assert_eq!(inventory.consume_first_food(), Some(25.0));
        assert_eq!(inventory.iter().count(), 1, "one unit left, slot survives");
        assert_eq!(inventory.consume_first_food(), Some(25.0));
        assert_eq!(inventory.iter().count(), 0, "last unit clears the slot");
        assert_eq!(inventory.consume_first_food(), None);
    }

    #[test]
    fn take_first_weapon_removes_it_from_the_pool() {
        let mut inventory = Inventory::default();
        inventory
            .try_add(ItemKind::Material(MaterialKind::Wood), 1)
            .unwrap();
        inventory
            .try_add(ItemKind::Weapon(WeaponItem::LOOTABLE_CLUB), 1)
            .unwrap();

        let taken = inventory.take_first_weapon();
        assert_eq!(taken, Some(WeaponItem::LOOTABLE_CLUB));
        assert!(inventory.take_first_weapon().is_none());
        assert_eq!(inventory.iter().count(), 1, "the wood stack is untouched");
    }

    #[test]
    fn weapon_durability_clamps_at_zero_and_reports_applied_hits() {
        let mut durability = WeaponDurability::new(WeaponItem::LOOTABLE_CLUB);
        assert_eq!(durability.apply_hit(1), 1);
        assert_eq!(durability.current(), 14);
        assert!(!durability.is_broken());
        assert_eq!(durability.apply_hit(100), 14, "overkill clamps at zero");
        assert!(durability.is_broken());
        assert_eq!(durability.apply_hit(1), 0, "already broken applies nothing");
    }
}
