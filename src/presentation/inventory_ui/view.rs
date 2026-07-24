use bevy::prelude::*;

use super::{
    ActionButton, ActionText, CategoryButton, CategoryText, CloseButton, DetailText, EquippedText,
    InventoryCategory, InventorySlotButton, InventorySlotText, InventoryUiRoot, SLOT_COUNT,
};

pub(super) use crate::presentation::theme::{
    ACCENT, ACCENT_DARK, BORDER, DISABLED, PANEL, PANEL_INSET, ROW_OR_SLOT_BG as SLOT_BACKGROUND,
    SELECTED_SLOT, TEXT_BRIGHT, TEXT_MUTED,
};

fn body_font() -> TextFont {
    crate::presentation::theme::body_font(16.0)
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
