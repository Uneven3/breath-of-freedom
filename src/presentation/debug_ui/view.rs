//! Panel construction. Rows are built from the channel/knob enums, so a new
//! debug facility appears here without this file changing.

use bevy::prelude::*;

use super::style::{
    ACCENT, ACCENT_DARK, BORDER, PANEL, PANEL_INSET, ROW, TEXT_BRIGHT, TEXT_MUTED, body_font,
    heading_font, row_node, section_title,
};
use super::{
    BenchmarkButton, BenchmarkText, ChannelButton, ChannelText, CloseButton, DebugUiRoot,
    FlythroughButton, KnobButton, KnobText, ReadoutText, ScrollPanel, TerrainViewButton,
    TerrainViewText,
};
use crate::debug::channel::{DebugAction, DebugChannel};
use crate::perf::BenchSuite;
use crate::perf::PerfKnob;
use crate::perf::sequence::{BenchmarkRequest, VantageMode};
use crate::visuals::terrain_material::TerrainDebugView;

use super::ActionButton;

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
                terrain_section(panel);
                action_section(panel);
            });
        });
}

fn terrain_section(panel: &mut ChildSpawnerCommands) {
    section_title(
        panel,
        "Terreno semántico",
        "El arte muestra las texturas. Estas vistas muestran los datos que pintaste; \
         la leyenda aparece abajo a la izquierda.",
    );
    panel
        .spawn(Node {
            width: Val::Percent(100.0),
            column_gap: Val::Px(8.0),
            flex_wrap: FlexWrap::Wrap,
            row_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|row| {
            for view in TerrainDebugView::ALL {
                row.spawn((
                    TerrainViewButton(view),
                    Button,
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(PANEL_INSET),
                ))
                .with_child((
                    TerrainViewText(view),
                    Text::new(view.label()),
                    body_font(),
                    TextColor(TEXT_BRIGHT),
                ));
            }
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

fn measurement_section(panel: &mut ChildSpawnerCommands) {
    section_title(
        panel,
        "Medición",
        "Corre una matriz sola: precalienta, mide 4s por paso con vsync apagado, y repite \
         el baseline para exponer deriva. Cada suite se para en su propio mirador y mide en \
         su caja — la del pasto en la pradera, las otras en el Mundo. \"Aquí\" corre la \
         general clavando la cámara donde estás, que es para las zonas lentas que encontrás \
         jugando.",
    );
    panel
        .spawn(Node {
            width: Val::Percent(100.0),
            column_gap: Val::Px(8.0),
            flex_wrap: FlexWrap::Wrap,
            row_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|row| {
            let mut buttons: Vec<(BenchmarkRequest, String)> = BenchSuite::ALL
                .iter()
                .map(|suite| {
                    (
                        BenchmarkRequest::new(*suite, VantageMode::Canonical),
                        suite.label().to_string(),
                    )
                })
                .collect();
            buttons.push((
                BenchmarkRequest::new(BenchSuite::General, VantageMode::Here),
                "aquí".to_string(),
            ));
            for (request, label) in buttons {
                row.spawn((
                    BenchmarkButton(request),
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
                    Text::new(label.clone()),
                    body_font(),
                    TextColor(TEXT_BRIGHT),
                ));
            }
        });
    section_title(
        panel,
        "Flythrough",
        "Recorre la ruta autoreada (perf::flythrough::ROUTE) y reporta frame/gpu/tris/draws/mats \
         por tramo, con grado de presupuesto móvil. Autorá la ruta volando la freecam (F3) y \
         capturando poses con F4.",
    );
    panel
        .spawn((
            FlythroughButton,
            Button,
            Node {
                width: Val::Percent(100.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(10.0)),
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(ACCENT_DARK),
        ))
        .with_child((
            Text::new("Correr flythrough"),
            body_font(),
            TextColor(TEXT_BRIGHT),
        ));
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
        "Solo presentación. Overdraw respeta el culling del material: 1-2 capas bien, 3-5 medio, 6-9 malo, 10+ crítico si cubre un área grande. La secuencia apaga ambas vistas al medir.",
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
