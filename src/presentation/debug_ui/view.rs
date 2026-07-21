//! Panel construction. Rows are built from the channel/knob enums, so a new
//! debug facility appears here without this file changing.

use bevy::prelude::*;

use super::{
    BenchmarkButton, BenchmarkText, ChannelButton, ChannelText, CloseButton, DebugUiRoot,
    KnobButton, KnobText, ReadoutText, ScrollPanel,
};
use crate::debug::channel::{DebugAction, DebugChannel};
use crate::perf::PerfKnob;
use crate::perf::sequence::VantageMode;

use super::ActionButton;

const PANEL: Color = Color::srgba(0.055, 0.065, 0.07, 0.98);
const PANEL_INSET: Color = Color::srgb(0.09, 0.105, 0.11);
const ROW: Color = Color::srgb(0.115, 0.13, 0.135);
const ACCENT: Color = Color::srgb(0.25, 0.82, 0.67);
const ACCENT_DARK: Color = Color::srgb(0.08, 0.34, 0.29);
const BORDER: Color = Color::srgb(0.25, 0.28, 0.29);
const TEXT_BRIGHT: Color = Color::srgb(0.94, 0.96, 0.93);
const TEXT_MUTED: Color = Color::srgb(0.62, 0.67, 0.65);

fn body_font() -> TextFont {
    TextFont {
        font_size: FontSize::Px(15.0),
        ..default()
    }
}

fn heading_font() -> TextFont {
    TextFont {
        font_size: FontSize::Px(18.0),
        ..default()
    }
}

fn row_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)),
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        column_gap: Val::Px(12.0),
        border_radius: BorderRadius::all(Val::Px(4.0)),
        ..default()
    }
}

pub(super) fn spawn_debug_ui(mut commands: Commands) {
    commands
        .spawn((
            DebugUiRoot,
            Name::new("DebugUi"),
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
            GlobalZIndex(120),
        ))
        .with_children(|root| {
            root.spawn((
                ScrollPanel,
                ScrollPosition::default(),
                Node {
                    width: Val::Px(720.0),
                    max_width: Val::Percent(96.0),
                    max_height: Val::Percent(94.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(18.0)),
                    row_gap: Val::Px(8.0),
                    overflow: Overflow::scroll_y(),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(BORDER),
            ))
            .with_children(|panel| {
                header(panel);
                measurement_section(panel);
                knob_section(panel);
                channel_section(panel);
                action_section(panel);
            });
        });
}

fn header(panel: &mut ChildSpawnerCommands) {
    panel
        .spawn(Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((Text::new("Debug"), heading_font(), TextColor(TEXT_BRIGHT)));
            row.spawn((
                CloseButton,
                Button,
                Node {
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(ROW),
            ))
            .with_child((
                Text::new("Cerrar (F1)"),
                body_font(),
                TextColor(TEXT_MUTED),
            ));
        });
}

fn section_title(panel: &mut ChildSpawnerCommands, title: &str, subtitle: &str) {
    panel.spawn((
        Text::new(title),
        heading_font(),
        TextColor(ACCENT),
        Node {
            margin: UiRect::top(Val::Px(6.0)),
            ..default()
        },
    ));
    panel.spawn((Text::new(subtitle), body_font(), TextColor(TEXT_MUTED)));
}

fn measurement_section(panel: &mut ChildSpawnerCommands) {
    section_title(
        panel,
        "Medición",
        "Corre la matriz sola: precalienta, mide 4s por paso con vsync apagado, y repite \
         el baseline para exponer deriva. \"Aquí\" clava la cámara donde estás — usalo en \
         las zonas lentas; el punto canónico sirve para comparar entre sesiones.",
    );
    panel
        .spawn(Node {
            width: Val::Percent(100.0),
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|row| {
            for (mode, label) in [
                (VantageMode::Here, "Correr aquí"),
                (VantageMode::Canonical, "Punto canónico"),
            ] {
                row.spawn((
                    BenchmarkButton(mode),
                    Button,
                    Node {
                        flex_grow: 1.0,
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(10.0)),
                        justify_content: JustifyContent::Center,
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(ACCENT_DARK),
                ))
                .with_child((
                    BenchmarkText,
                    Text::new(label),
                    body_font(),
                    TextColor(TEXT_BRIGHT),
                ));
            }
        });
    panel.spawn((
        ReadoutText,
        Text::new("—"),
        body_font(),
        TextColor(TEXT_MUTED),
    ));
}

fn knob_section(panel: &mut ChildSpawnerCommands) {
    section_title(
        panel,
        "Render",
        "Solo presentación: los colliders y la simulación no cambian.",
    );
    for knob in PerfKnob::ALL {
        panel
            .spawn((KnobButton(knob), Button, row_node(), BackgroundColor(ROW)))
            .with_children(|row| {
                row.spawn((Text::new(knob.label()), body_font(), TextColor(TEXT_BRIGHT)));
                row.spawn((
                    KnobText(knob),
                    Text::new("—"),
                    body_font(),
                    TextColor(ACCENT),
                ));
            });
    }
}

fn channel_section(panel: &mut ChildSpawnerCommands) {
    section_title(
        panel,
        "Canales",
        "Algunos cuestan frame time — no los dejes prendidos mientras medís.",
    );
    for channel in DebugChannel::ALL {
        panel
            .spawn((
                ChannelButton(channel),
                Button,
                row_node(),
                BackgroundColor(ROW),
            ))
            .with_children(|row| {
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                })
                .with_children(|label| {
                    label.spawn((
                        Text::new(channel.label()),
                        body_font(),
                        TextColor(TEXT_BRIGHT),
                    ));
                    label.spawn((
                        Text::new(channel.hint()),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(TEXT_MUTED),
                    ));
                });
                row.spawn((
                    ChannelText(channel),
                    Text::new("—"),
                    body_font(),
                    TextColor(ACCENT),
                ));
            });
    }
}

fn action_section(panel: &mut ChildSpawnerCommands) {
    section_title(panel, "Acciones", "Disparos puntuales.");
    panel
        .spawn(Node {
            width: Val::Percent(100.0),
            column_gap: Val::Px(8.0),
            flex_wrap: FlexWrap::Wrap,
            row_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|row| {
            for action in DebugAction::ALL {
                row.spawn((
                    ActionButton(action),
                    Button,
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(PANEL_INSET),
                ))
                .with_child((
                    Text::new(action.label()),
                    body_font(),
                    TextColor(TEXT_BRIGHT),
                ));
            }
        });
}
