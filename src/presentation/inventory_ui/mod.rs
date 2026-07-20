//! Read-only inventory presentation. User actions cross into Inventory as
//! validated messages; this module never mutates simulation components.

use bevy::prelude::*;

use crate::input::ModalInputFocusRequest;
use crate::inventory::{
    ConsumeSlotRequestMessage, EquipSlotRequestMessage, Inventory, ItemKind, ItemStack,
    MaterialKind, WeaponDurability,
};
use crate::movement::Player;

mod view;

const SLOT_COUNT: usize = crate::inventory::data::INVENTORY_SLOTS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InventoryCategory {
    All,
    Weapons,
    Food,
    Materials,
}

impl InventoryCategory {
    const ALL: [Self; 4] = [Self::All, Self::Weapons, Self::Food, Self::Materials];

    fn label(self) -> &'static str {
        match self {
            Self::All => "Todos",
            Self::Weapons => "Armas",
            Self::Food => "Comida",
            Self::Materials => "Materiales",
        }
    }

    fn accepts(self, stack: &ItemStack) -> bool {
        matches!(self, Self::All)
            || matches!(
                (self, stack.kind),
                (Self::Weapons, ItemKind::Weapon(_))
                    | (Self::Food, ItemKind::Food { .. })
                    | (Self::Materials, ItemKind::Material(_))
            )
    }

    fn shifted(self, delta: isize) -> Self {
        let current = Self::ALL
            .iter()
            .position(|category| *category == self)
            .unwrap_or(0);
        let next = (current as isize + delta).rem_euclid(Self::ALL.len() as isize) as usize;
        Self::ALL[next]
    }
}

#[derive(Resource)]
struct InventoryUiState {
    open: bool,
    category: InventoryCategory,
    selected_slot: usize,
    action_sent_this_frame: bool,
}

impl Default for InventoryUiState {
    fn default() -> Self {
        Self {
            open: false,
            category: InventoryCategory::All,
            selected_slot: 0,
            action_sent_this_frame: false,
        }
    }
}

#[derive(Component)]
struct InventoryUiRoot;

#[derive(Component)]
struct InventorySlotButton(usize);

#[derive(Component)]
struct InventorySlotText(usize);

#[derive(Component)]
struct CategoryButton(InventoryCategory);

#[derive(Component)]
struct CategoryText(InventoryCategory);

#[derive(Component)]
struct DetailText;

#[derive(Component)]
struct EquippedText;

#[derive(Component)]
struct ActionButton;

#[derive(Component)]
struct ActionText;

#[derive(Component)]
struct CloseButton;

pub struct InventoryUiPlugin;

impl Plugin for InventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InventoryUiState>();
        app.add_systems(Startup, view::spawn_inventory_ui);
        app.add_systems(
            Update,
            (
                reset_action_latch,
                toggle_inventory,
                handle_button_input,
                handle_keyboard_navigation,
                render_inventory,
            )
                .chain(),
        );
    }
}

fn reset_action_latch(mut state: ResMut<InventoryUiState>) {
    state.action_sent_this_frame = false;
}

fn toggle_inventory(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<InventoryUiState>,
    root: Single<Entity, With<InventoryUiRoot>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
) {
    let should_toggle = keys.just_pressed(KeyCode::Tab)
        || keys.just_pressed(KeyCode::KeyI)
        || (state.open && keys.just_pressed(KeyCode::Escape));
    if should_toggle {
        set_open(!state.open, *root, &mut state, &mut focus);
    }
}

fn set_open(
    open: bool,
    owner: Entity,
    state: &mut InventoryUiState,
    focus: &mut MessageWriter<ModalInputFocusRequest>,
) {
    state.open = open;
    focus.write(if open {
        ModalInputFocusRequest::Acquire(owner)
    } else {
        ModalInputFocusRequest::Release(owner)
    });
}

type InventoryActorQuery<'a> = (Entity, &'a Inventory, Option<&'a WeaponDurability>);

#[allow(clippy::too_many_arguments)]
fn handle_button_input(
    mut state: ResMut<InventoryUiState>,
    actor: Query<InventoryActorQuery, With<Player>>,
    root: Single<Entity, With<InventoryUiRoot>>,
    categories: Query<(&Interaction, &CategoryButton), Changed<Interaction>>,
    slots: Query<(&Interaction, &InventorySlotButton), Changed<Interaction>>,
    action: Query<&Interaction, (Changed<Interaction>, With<ActionButton>)>,
    close: Query<&Interaction, (Changed<Interaction>, With<CloseButton>)>,
    mut equip: MessageWriter<EquipSlotRequestMessage>,
    mut consume: MessageWriter<ConsumeSlotRequestMessage>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
) {
    if !state.open {
        return;
    }
    for (interaction, button) in &categories {
        if *interaction == Interaction::Pressed {
            state.category = button.0;
            if let Ok((_, inventory, _)) = actor.single() {
                select_first_visible(&mut state, inventory);
            }
        }
    }
    for (interaction, button) in &slots {
        if *interaction == Interaction::Pressed {
            state.selected_slot = button.0;
        }
    }
    if action
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed)
        && !state.action_sent_this_frame
        && let Ok((entity, inventory, _)) = actor.single()
    {
        state.action_sent_this_frame = emit_action(
            entity,
            inventory,
            state.selected_slot,
            state.category,
            &mut equip,
            &mut consume,
        );
    }
    if close
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed)
    {
        set_open(false, *root, &mut state, &mut focus);
    }
}

fn handle_keyboard_navigation(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<InventoryUiState>,
    actor: Query<(Entity, &Inventory), With<Player>>,
    mut equip: MessageWriter<EquipSlotRequestMessage>,
    mut consume: MessageWriter<ConsumeSlotRequestMessage>,
) {
    if !state.open {
        return;
    }
    let Ok((entity, inventory)) = actor.single() else {
        return;
    };
    let category_delta = i32::from(keys.just_pressed(KeyCode::ArrowRight))
        - i32::from(keys.just_pressed(KeyCode::ArrowLeft));
    if category_delta != 0 {
        state.category = state.category.shifted(category_delta as isize);
        select_first_visible(&mut state, inventory);
    }
    let slot_delta = i32::from(keys.just_pressed(KeyCode::ArrowDown))
        - i32::from(keys.just_pressed(KeyCode::ArrowUp));
    if slot_delta != 0 {
        select_next_visible(&mut state, inventory, slot_delta as isize);
    }
    if keys.just_pressed(KeyCode::Enter) && !state.action_sent_this_frame {
        state.action_sent_this_frame = emit_action(
            entity,
            inventory,
            state.selected_slot,
            state.category,
            &mut equip,
            &mut consume,
        );
    }
}

fn select_first_visible(state: &mut InventoryUiState, inventory: &Inventory) {
    state.selected_slot = (0..SLOT_COUNT)
        .find(|index| slot_visible(inventory, *index, state.category))
        .unwrap_or(SLOT_COUNT);
}

fn select_next_visible(state: &mut InventoryUiState, inventory: &Inventory, delta: isize) {
    if state.selected_slot >= SLOT_COUNT {
        select_first_visible(state, inventory);
        return;
    }
    for step in 1..=SLOT_COUNT {
        let index = (state.selected_slot as isize + delta * step as isize)
            .rem_euclid(SLOT_COUNT as isize) as usize;
        if slot_visible(inventory, index, state.category) {
            state.selected_slot = index;
            return;
        }
    }
}

fn slot_visible(inventory: &Inventory, index: usize, category: InventoryCategory) -> bool {
    match inventory.slot(index) {
        Some(stack) => category.accepts(stack),
        None => category == InventoryCategory::All,
    }
}

fn emit_action(
    actor: Entity,
    inventory: &Inventory,
    slot: usize,
    category: InventoryCategory,
    equip: &mut MessageWriter<EquipSlotRequestMessage>,
    consume: &mut MessageWriter<ConsumeSlotRequestMessage>,
) -> bool {
    if !slot_visible(inventory, slot, category) {
        return false;
    }
    match inventory.slot(slot).map(|stack| stack.kind) {
        Some(ItemKind::Weapon(_)) => {
            equip.write(EquipSlotRequestMessage { actor, slot });
            true
        }
        Some(ItemKind::Food { .. }) => {
            consume.write(ConsumeSlotRequestMessage { actor, slot });
            true
        }
        Some(ItemKind::Material(_)) | None => false,
    }
}

type CategoryButtonQuery<'a> = (&'a CategoryButton, &'a mut BackgroundColor);
type CategoryButtonFilter = (Without<InventorySlotButton>, Without<ActionButton>);
type DetailTextFilter = (
    With<DetailText>,
    Without<EquippedText>,
    Without<ActionText>,
    Without<InventorySlotText>,
);
type EquippedTextFilter = (
    With<EquippedText>,
    Without<DetailText>,
    Without<ActionText>,
    Without<InventorySlotText>,
);
type ActionButtonQuery<'a> = (&'a mut BackgroundColor, &'a mut BorderColor);
type ActionButtonFilter = (
    With<ActionButton>,
    Without<InventorySlotButton>,
    Without<CategoryButton>,
);
type ActionTextFilter = (
    With<ActionText>,
    Without<DetailText>,
    Without<EquippedText>,
    Without<InventorySlotText>,
);

#[allow(clippy::too_many_arguments)]
fn render_inventory(
    state: Res<InventoryUiState>,
    actor: Query<InventoryActorQuery, With<Player>>,
    mut root: Single<&mut Node, (With<InventoryUiRoot>, Without<InventorySlotButton>)>,
    mut slots: Query<(
        &InventorySlotButton,
        &mut Node,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    mut slot_texts: Query<(&InventorySlotText, &mut Text)>,
    mut categories: Query<CategoryButtonQuery, CategoryButtonFilter>,
    mut category_texts: Query<(&CategoryText, &mut TextColor)>,
    mut detail: Single<&mut Text, DetailTextFilter>,
    mut equipped: Single<&mut Text, EquippedTextFilter>,
    mut action: Single<ActionButtonQuery, ActionButtonFilter>,
    mut action_text: Single<&mut Text, ActionTextFilter>,
) {
    root.display = if state.open {
        Display::Flex
    } else {
        Display::None
    };
    if !state.open {
        return;
    }
    let Ok((_, inventory, weapon)) = actor.single() else {
        ***detail = "Inventario no disponible".into();
        ***equipped = "Estado\nSin actor local".into();
        return;
    };

    for (button, mut node, mut background, mut border) in &mut slots {
        node.display = if slot_visible(inventory, button.0, state.category) {
            Display::Flex
        } else {
            Display::None
        };
        let selected = button.0 == state.selected_slot;
        *background = if selected {
            view::SELECTED_SLOT.into()
        } else {
            view::SLOT_BACKGROUND.into()
        };
        *border = BorderColor::all(if selected { view::ACCENT } else { view::BORDER });
    }
    for (label, mut text) in &mut slot_texts {
        **text = slot_label(inventory.slot(label.0), label.0);
    }
    for (button, mut background) in &mut categories {
        *background = if button.0 == state.category {
            view::ACCENT_DARK.into()
        } else {
            view::PANEL_INSET.into()
        };
    }
    for (label, mut color) in &mut category_texts {
        color.0 = if label.0 == state.category {
            view::TEXT_BRIGHT
        } else {
            view::TEXT_MUTED
        };
    }

    ***detail = detail_label(inventory.slot(state.selected_slot));
    ***equipped = equipped_label(weapon);
    let actionable = matches!(
        inventory.slot(state.selected_slot).map(|stack| stack.kind),
        Some(ItemKind::Weapon(_) | ItemKind::Food { .. })
    );
    action.0.0 = if actionable {
        view::ACCENT_DARK
    } else {
        view::DISABLED
    };
    action.1.set_all(if actionable {
        view::ACCENT
    } else {
        view::BORDER
    });
    ***action_text = action_label(inventory.slot(state.selected_slot)).into();
}

fn slot_label(stack: Option<&ItemStack>, index: usize) -> String {
    match stack.map(|stack| (stack.kind, stack.quantity)) {
        Some((ItemKind::Weapon(item), _)) => format!(
            "{:02}  {}\nARMA  {}/{}",
            index + 1,
            item.label,
            item.current_durability,
            item.max_durability
        ),
        Some((ItemKind::Food { label, .. }, quantity)) => {
            format!("{:02}  {label}\nCOMIDA  x{quantity}", index + 1)
        }
        Some((ItemKind::Material(MaterialKind::Wood), quantity)) => {
            format!("{:02}  Madera\nMATERIAL  x{quantity}", index + 1)
        }
        None => format!("{:02}  Vacio", index + 1),
    }
}

fn detail_label(stack: Option<&ItemStack>) -> String {
    match stack.map(|stack| (stack.kind, stack.quantity)) {
        Some((ItemKind::Weapon(item), _)) => format!(
            "{}\n\nEstado        Guardada\nCategoria     Arma\nDurabilidad  {}/{}",
            item.label, item.current_durability, item.max_durability
        ),
        Some((ItemKind::Food { label, heal }, quantity)) => format!(
            "{label}\n\nEstado        Consumible\nCategoria     Comida\nCantidad      x{quantity}\nRecupera      {heal:.0} HP"
        ),
        Some((ItemKind::Material(MaterialKind::Wood), quantity)) => format!(
            "Madera\n\nEstado        Recurso\nCategoria     Material\nCantidad      x{quantity}"
        ),
        None => "Slot vacio\n\nEstado        Disponible".into(),
    }
}

fn equipped_label(weapon: Option<&WeaponDurability>) -> String {
    match weapon {
        Some(weapon) => format!(
            "EQUIPADO\n{}\nDurabilidad {}/{}",
            weapon.label(),
            weapon.current(),
            weapon.max()
        ),
        None => "EQUIPADO\nSin arma".into(),
    }
}

fn action_label(stack: Option<&ItemStack>) -> &'static str {
    match stack.map(|stack| stack.kind) {
        Some(ItemKind::Weapon(_)) => "Equipar",
        Some(ItemKind::Food { .. }) => "Consumir",
        Some(ItemKind::Material(_)) => "Sin accion",
        None => "Slot vacio",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::WeaponItem;

    #[test]
    fn category_filter_preserves_real_slot_identity() {
        let mut inventory = Inventory::default();
        inventory
            .try_add(ItemKind::Material(MaterialKind::Wood), 2)
            .unwrap();
        inventory
            .try_add(ItemKind::Weapon(WeaponItem::LOOTABLE_CLUB), 1)
            .unwrap();

        assert!(!slot_visible(&inventory, 0, InventoryCategory::Weapons));
        assert!(slot_visible(&inventory, 1, InventoryCategory::Weapons));
    }

    #[test]
    fn navigation_skips_slots_outside_the_category() {
        let mut inventory = Inventory::default();
        inventory
            .try_add(ItemKind::Material(MaterialKind::Wood), 2)
            .unwrap();
        inventory
            .try_add(ItemKind::Weapon(WeaponItem::LOOTABLE_CLUB), 1)
            .unwrap();
        let mut state = InventoryUiState {
            category: InventoryCategory::Weapons,
            ..default()
        };

        select_first_visible(&mut state, &inventory);
        assert_eq!(state.selected_slot, 1);
        select_next_visible(&mut state, &inventory, 1);
        assert_eq!(state.selected_slot, 1);
    }

    #[test]
    fn empty_category_clears_selection_instead_of_targeting_hidden_item() {
        let mut inventory = Inventory::default();
        inventory
            .try_add(ItemKind::Material(MaterialKind::Wood), 2)
            .unwrap();
        let mut state = InventoryUiState {
            category: InventoryCategory::Weapons,
            ..default()
        };

        select_first_visible(&mut state, &inventory);

        assert_eq!(state.selected_slot, SLOT_COUNT);
        assert!(!slot_visible(
            &inventory,
            state.selected_slot,
            state.category
        ));
    }
}
