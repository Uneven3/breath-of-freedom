use bevy::prelude::*;

use super::{
    ActionButton, ActionText, CategoryButton, CategoryText, CloseButton, DetailText, EquippedText,
    InventoryCategory, InventorySlotButton, InventorySlotText, InventoryUiRoot, SLOT_COUNT,
};

pub(super) const PANEL: Color = Color::srgba(0.055, 0.065, 0.07, 0.98);
pub(super) const PANEL_INSET: Color = Color::srgb(0.09, 0.105, 0.11);
pub(super) const SLOT_BACKGROUND: Color = Color::srgb(0.115, 0.13, 0.135);
pub(super) const SELECTED_SLOT: Color = Color::srgb(0.12, 0.21, 0.20);
pub(super) const ACCENT: Color = Color::srgb(0.25, 0.82, 0.67);
pub(super) const ACCENT_DARK: Color = Color::srgb(0.08, 0.34, 0.29);
pub(super) const BORDER: Color = Color::srgb(0.25, 0.28, 0.29);
pub(super) const DISABLED: Color = Color::srgb(0.11, 0.115, 0.12);
pub(super) const TEXT_BRIGHT: Color = Color::srgb(0.94, 0.96, 0.93);
pub(super) const TEXT_MUTED: Color = Color::srgb(0.62, 0.67, 0.65);

fn body_font() -> TextFont {
    TextFont {
        font_size: FontSize::Px(16.0),
        ..default()
    }
}

pub(super) fn spawn_inventory_ui(mut commands: Commands) {
    commands
        .spawn((
            InventoryUiRoot,
            Name::new("InventoryUi"),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                display: Display::None,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.02, 0.02, 0.76)),
            GlobalZIndex(100),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(940.0),
                    max_width: Val::Percent(96.0),
                    height: Val::Percent(94.0),
                    max_height: Val::Percent(94.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(24.0)),
                    row_gap: Val::Px(18.0),
                    overflow: Overflow::scroll_y(),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(BORDER),
            ))
            .with_children(spawn_panel);
        });
}

fn spawn_panel(panel: &mut ChildSpawnerCommands) {
    panel
        .spawn(Node {
            width: Val::Percent(100.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        })
        .with_children(|header| {
            header.spawn((
                Text::new("INVENTARIO"),
                TextFont {
                    font_size: FontSize::Px(28.0),
                    ..default()
                },
                TextColor(TEXT_BRIGHT),
            ));
            header
                .spawn((
                    CloseButton,
                    Button,
                    Node {
                        width: Val::Px(36.0),
                        height: Val::Px(36.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(PANEL_INSET),
                ))
                .with_child((
                    Text::new("X"),
                    TextFont {
                        font_size: FontSize::Px(18.0),
                        ..default()
                    },
                    TextColor(TEXT_MUTED),
                ));
        });

    panel
        .spawn(Node {
            width: Val::Percent(100.0),
            column_gap: Val::Px(8.0),
            row_gap: Val::Px(8.0),
            flex_wrap: FlexWrap::Wrap,
            ..default()
        })
        .with_children(|tabs| {
            for category in InventoryCategory::ALL {
                tabs.spawn((
                    CategoryButton(category),
                    Button,
                    Node {
                        height: Val::Px(38.0),
                        padding: UiRect::horizontal(Val::Px(16.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(PANEL_INSET),
                ))
                .with_child((
                    CategoryText(category),
                    Text::new(category.label()),
                    body_font(),
                    TextColor(TEXT_MUTED),
                ));
            }
        });

    panel
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_grow: 1.0,
            column_gap: Val::Px(20.0),
            row_gap: Val::Px(20.0),
            flex_wrap: FlexWrap::Wrap,
            ..default()
        })
        .with_children(|content| {
            content
                .spawn(Node {
                    min_width: Val::Px(280.0),
                    flex_basis: Val::Px(560.0),
                    flex_grow: 2.0,
                    display: Display::Grid,
                    grid_template_columns: RepeatedGridTrack::flex(2, 1.0),
                    grid_auto_rows: GridTrack::px(104.0),
                    row_gap: Val::Px(10.0),
                    column_gap: Val::Px(10.0),
                    align_content: AlignContent::Start,
                    ..default()
                })
                .with_children(spawn_slots);
            content
                .spawn((
                    Node {
                        min_width: Val::Px(220.0),
                        flex_basis: Val::Px(260.0),
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(18.0)),
                        row_gap: Val::Px(18.0),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(PANEL_INSET),
                    BorderColor::all(BORDER),
                ))
                .with_children(spawn_details);
        });
}

fn spawn_slots(slots: &mut ChildSpawnerCommands) {
    for index in 0..SLOT_COUNT {
        slots
            .spawn((
                InventorySlotButton(index),
                Button,
                Node {
                    min_width: Val::Px(0.0),
                    height: Val::Px(104.0),
                    padding: UiRect::all(Val::Px(14.0)),
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(SLOT_BACKGROUND),
                BorderColor::all(BORDER),
            ))
            .with_child((
                InventorySlotText(index),
                Text::new(""),
                body_font(),
                TextColor(TEXT_BRIGHT),
            ));
    }
}

fn spawn_details(details: &mut ChildSpawnerCommands) {
    details.spawn((
        EquippedText,
        Text::new("EQUIPADO\nSin arma"),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(ACCENT),
    ));
    details.spawn((
        DetailText,
        Text::new("Slot vacio"),
        body_font(),
        TextColor(TEXT_BRIGHT),
        Node {
            flex_grow: 1.0,
            ..default()
        },
    ));
    details
        .spawn((
            ActionButton,
            Button,
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(46.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(DISABLED),
            BorderColor::all(BORDER),
        ))
        .with_child((
            ActionText,
            Text::new("Slot vacio"),
            body_font(),
            TextColor(TEXT_BRIGHT),
        ));
}
