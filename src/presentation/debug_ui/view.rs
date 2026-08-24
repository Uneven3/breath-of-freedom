//! Panel construction. Rows are built from the channel/knob enums, so a new
//! debug facility appears here without this file changing.

use bevy::feathers::controls::{ButtonVariant, FeathersButton};
use bevy::feathers::theme::ThemedText;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

use super::style::{
    BORDER, PANEL, TEXT_BRIGHT, TEXT_MUTED, body_font, heading_font, section_title,
};
use super::{
    BenchmarkButton, ChannelButton, ChannelText, DebugTab, DebugUiRoot, DebugUiState,
    FlythroughButton, KnobButton, KnobText, MaterialLabel, MaterialOmittedText, MaterialRow,
    MaterialSummaryText, MaterialSwatch, MaterialTerrainLabel, MaterialTerrainRow,
    MaterialTerrainSwatch, ReadoutText, ScrollPanel, TabButton, TabPane, TerrainViewButton,
    TerrainViewText, TuningButton, TuningText,
};
use crate::debug::MAX_MATERIAL_ROWS;
use crate::debug::channel::{DebugAction, DebugActionRequest, DebugChannel, DebugChannelToggle};
use crate::input::ModalInputFocusRequest;
use crate::perf::BenchSuite;
use crate::perf::sequence::{BenchmarkRequest, VantageMode};
use crate::perf::{FlythroughRequest, PerfKnob, PerfKnobCategory, PerfKnobToggle};
use crate::visuals::terrain_material::{TerrainDebugView, TerrainDebugViewRequest};
use bof_domain::movement::tuning::{TuningField, TuningNudge};

use super::ActionButton;

/// Un observer por *tipo* de botón, no por instancia: `bevy_ui_widgets::Button`
/// no llena `Interaction` (hallazgo del spike de `CloseButton`, ver
/// `AHORA.md`), así que cada `FeathersButton` migrado se engancha acá en vez
/// de en `handle_clicks`. `On<Activate>::entity` es el botón mismo —
/// `apply_scene` no reemplaza el marcador que ya llevaba (`KnobButton`,
/// `ChannelButton`, etc.), lo pisa encima — así que una query por ese
/// marcador alcanza para saber cuál de las N instancias se clickeó.
fn activate_knob(
    activate: On<Activate>,
    knobs: Query<&KnobButton>,
    mut writer: MessageWriter<PerfKnobToggle>,
) {
    if let Ok(knob) = knobs.get(activate.entity) {
        writer.write(PerfKnobToggle(knob.0));
    }
}

fn activate_channel(
    activate: On<Activate>,
    channels: Query<&ChannelButton>,
    mut writer: MessageWriter<DebugChannelToggle>,
) {
    if let Ok(channel) = channels.get(activate.entity) {
        writer.write(DebugChannelToggle(channel.0));
    }
}

fn activate_action(
    activate: On<Activate>,
    actions: Query<&ActionButton>,
    mut writer: MessageWriter<DebugActionRequest>,
) {
    if let Ok(action) = actions.get(activate.entity) {
        writer.write(DebugActionRequest(action.0));
    }
}

fn activate_terrain_view(
    activate: On<Activate>,
    views: Query<&TerrainViewButton>,
    mut writer: MessageWriter<TerrainDebugViewRequest>,
) {
    if let Ok(view) = views.get(activate.entity) {
        writer.write(TerrainDebugViewRequest(view.0));
    }
}

#[allow(clippy::too_many_arguments)]
fn activate_benchmark(
    activate: On<Activate>,
    buttons: Query<&BenchmarkButton>,
    mut writer: MessageWriter<BenchmarkRequest>,
    mut state: ResMut<DebugUiState>,
    root: Single<Entity, With<DebugUiRoot>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
) {
    if let Ok(button) = buttons.get(activate.entity) {
        writer.write(button.0);
        // La corrida mediría el panel junto con la escena; cerrarlo primero.
        super::set_open(&mut state, false, *root, &mut focus);
    }
}

fn activate_flythrough(
    _activate: On<Activate>,
    mut writer: MessageWriter<FlythroughRequest>,
    mut state: ResMut<DebugUiState>,
    root: Single<Entity, With<DebugUiRoot>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
) {
    writer.write(FlythroughRequest);
    super::set_open(&mut state, false, *root, &mut focus);
}

fn activate_tab(activate: On<Activate>, tabs: Query<&TabButton>, mut state: ResMut<DebugUiState>) {
    if let Ok(tab) = tabs.get(activate.entity) {
        state.active_tab = tab.0;
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
                Node {
                    width: Val::Px(720.0),
                    max_width: Val::Percent(96.0),
                    max_height: Val::Percent(94.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(18.0)),
                    row_gap: Val::Px(8.0),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(BORDER),
            ))
            .with_children(|panel| {
                header(panel);
                tab_bar(panel);
                // Sólo esto scrollea: antes era el panel entero, así que
                // cambiar de pestaña también cambiaba cuánto había que bajar
                // para ver el header. Las pestañas viven afuera, fijas.
                panel
                    .spawn((
                        ScrollPanel,
                        // Sin scroll a propósito (2026-08-12): el picking de
                        // Bevy no mantiene el clip de scroll en sincronía con
                        // `ScrollPosition`, y eso rompía el click de las
                        // pestañas de arriba después de scrollear una lista
                        // larga — bug del motor, no de este proyecto (ver
                        // `AHORA.md`). La salida es que nada acá necesite
                        // scrollear nunca: `clip_y` corta si algo se pasa,
                        // visible y sin arriesgar picking.
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(8.0),
                            ..default()
                        },
                    ))
                    .with_children(|scroll| {
                        tab_pane(scroll, DebugTab::Measurement, measurement_section);
                        tab_pane(scroll, DebugTab::Render, render_section);
                        tab_pane(scroll, DebugTab::Grass, grass_section);
                        tab_pane(scroll, DebugTab::Channels, channel_section);
                        tab_pane(scroll, DebugTab::Terrain, terrain_section);
                        tab_pane(scroll, DebugTab::Locomotion, locomotion_section);
                        tab_pane(scroll, DebugTab::Materials, materials_section);
                        tab_pane(scroll, DebugTab::Actions, action_section);
                    });
            });
        });
}

/// Una barra de botones, uno por `DebugTab`. `sync_tab_buttons` pinta cuál
/// está activo y esconde "Pradera" donde no hay pradera que ajustar.
fn tab_bar(panel: &mut ChildSpawnerCommands) {
    panel
        .spawn(Node {
            width: Val::Percent(100.0),
            column_gap: Val::Px(6.0),
            flex_wrap: FlexWrap::Wrap,
            row_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|row| {
            for tab in DebugTab::ALL {
                let label = tab.label();
                row.spawn(TabButton(tab)).apply_scene(bsn! {
                    @FeathersButton
                    on(activate_tab)
                    Children [ (Text({label}) ThemedText TextFont { font_size: FontSize::Px(15.0) }) ]
                });
            }
        });
}

/// Un panel de pestaña: arranca oculto salvo el que abre por default
/// (`DebugTab::default()`); `sync_tab_panes` decide el resto cada frame.
fn tab_pane(
    scroll: &mut ChildSpawnerCommands,
    tab: DebugTab,
    build: impl FnOnce(&mut ChildSpawnerCommands),
) {
    scroll
        .spawn((
            TabPane(tab),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                display: if tab == DebugTab::default() {
                    Display::Flex
                } else {
                    Display::None
                },
                ..default()
            },
        ))
        .with_children(build);
}

/// Cuál pestaña se ve. `Grass` además exige pradera en la escena: si el
/// usuario cambia de escena con esa pestaña activa, cae a `Render` en vez de
/// dejar un panel vacío al que el botón (ya escondido) no puede volver.
pub(super) fn sync_tab_panes(
    mut state: ResMut<DebugUiState>,
    scene: Res<State<crate::scene::AppState>>,
    mut panes: Query<(&mut Node, &TabPane)>,
) {
    let has_meadow = crate::scene::current_scene(&scene).is_some_and(|def| def.contents.meadow);
    if state.active_tab == DebugTab::Grass && !has_meadow {
        state.active_tab = DebugTab::Render;
    }
    let active = state.active_tab;
    for (mut node, pane) in &mut panes {
        let wanted = if pane.0 == active {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != wanted {
            node.display = wanted;
        }
    }
}

/// Qué botón está resaltado, y cuáles existen. Separado de `sync_tab_panes`
/// porque uno lee/escribe el estado activo y el otro sólo lo lee para pintar.
pub(super) fn sync_tab_buttons(
    state: Res<DebugUiState>,
    scene: Res<State<crate::scene::AppState>>,
    mut buttons: Query<(&TabButton, &mut Node, &mut ButtonVariant)>,
) {
    let has_meadow = crate::scene::current_scene(&scene).is_some_and(|def| def.contents.meadow);
    for (tab, mut node, mut variant) in &mut buttons {
        let visible = tab.0 != DebugTab::Grass || has_meadow;
        let wanted_display = if visible {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != wanted_display {
            node.display = wanted_display;
        }
        let wanted_variant = if tab.0 == state.active_tab {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Normal
        };
        if *variant != wanted_variant {
            *variant = wanted_variant;
        }
    }
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
                let label = view.label();
                row.spawn(TerrainViewButton(view)).apply_scene(bsn! {
                    @FeathersButton
                    on(activate_terrain_view)
                    Children [
                        (
                            Text({label})
                            ThemedText
                            TextFont { font_size: FontSize::Px(15.0) }
                            TerrainViewText({view})
                        )
                    ]
                });
            }
        });
}

/// Filas fijas, spawneadas una vez y ocultas por índice (`sync_material_rows`
/// en `mod.rs`) contra el último `MaterialBreakdownSnapshot` — mismo
/// mecanismo que `overlay::TerrainLegendRow`. Sin `bsn!`/`FeathersButton`:
/// nada acá se clickea, es sólo lectura, así que un `Node`/`Text` común
/// alcanza (y no exige `Clone + Default` en los marcadores).
fn materials_section(panel: &mut ChildSpawnerCommands) {
    section_title(
        panel,
        "Materiales",
        "El desglose de \"Material breakdown\" (pestaña Acciones), en colores reales en \
         vez de una tabla en el log. Cada fila es un \"look\" — materiales que un \
         cambio de paleta podría colapsar en uno solo.",
    );
    panel.spawn((
        MaterialSummaryText,
        Text::new("—"),
        body_font(),
        TextColor(TEXT_MUTED),
    ));
    panel
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(8.0),
            row_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|grid| {
            for index in 0..MAX_MATERIAL_ROWS {
                material_row(
                    grid,
                    MaterialRow(index),
                    MaterialSwatch(index),
                    MaterialLabel(index),
                );
            }
            grid.spawn((
                MaterialTerrainRow,
                Node {
                    width: Val::Percent(48.5),
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    display: Display::None,
                    ..default()
                },
            ))
            .with_children(|row| {
                row.spawn((
                    MaterialTerrainSwatch,
                    Node {
                        width: Val::Px(14.0),
                        height: Val::Px(14.0),
                        border_radius: BorderRadius::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ));
                row.spawn((
                    MaterialTerrainLabel,
                    Text::new("—"),
                    body_font(),
                    TextColor(TEXT_BRIGHT),
                ));
            });
        });
    panel.spawn((
        MaterialOmittedText,
        Text::new(""),
        body_font(),
        TextColor(TEXT_MUTED),
    ));
}

fn material_row(
    grid: &mut ChildSpawnerCommands,
    row: MaterialRow,
    swatch: MaterialSwatch,
    label: MaterialLabel,
) {
    grid.spawn((
        row,
        Node {
            width: Val::Percent(48.5),
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            display: Display::None,
            ..default()
        },
    ))
    .with_children(|entry| {
        entry.spawn((
            swatch,
            Node {
                width: Val::Px(14.0),
                height: Val::Px(14.0),
                border_radius: BorderRadius::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ));
        entry.spawn((label, Text::new("—"), body_font(), TextColor(TEXT_BRIGHT)));
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
            // Spike (2026-08-12): único botón migrado a bevy_feathers, para
            // probar el patrón real (`apply_scene` sobre una entidad ya hija
            // de un árbol armado a mano) antes de comprometer el resto del
            // hub. Ver `AHORA.md`. `bevy_ui_widgets::Button` no llena
            // `Interaction` — jugado, el click no cerraba nada hasta que se
            // enganchó acá, con `on(...)`, en vez de `handle_clicks`.
            row.spawn(()).apply_scene(bsn! {
                @FeathersButton
                on(|_activate: On<Activate>,
                   mut state: ResMut<DebugUiState>,
                   root: Single<Entity, With<DebugUiRoot>>,
                   mut focus: MessageWriter<ModalInputFocusRequest>| {
                    super::set_open(&mut state, false, *root, &mut focus);
                })
                Children [ (Text("Cerrar (F1)") ThemedText TextFont { font_size: FontSize::Px(15.0) }) ]
            });
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
                row.spawn(BenchmarkButton(request)).apply_scene(bsn! {
                    @FeathersButton
                    Node {
                        flex_grow: 1.0,
                        justify_content: JustifyContent::Center,
                    }
                    on(activate_benchmark)
                    Children [ (Text({label}) ThemedText TextFont { font_size: FontSize::Px(15.0) }) ]
                });
            }
        });
    section_title(
        panel,
        "Flythrough",
        "Recorre la ruta autoreada (perf::flythrough::ROUTE) y reporta frame/gpu/tris/draws/mats \
         por tramo, con grado de presupuesto móvil. Autorá la ruta volando la freecam (F3) y \
         capturando poses con F4.",
    );
    panel.spawn(FlythroughButton).apply_scene(bsn! {
        @FeathersButton
        Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
        }
        on(activate_flythrough)
        Children [ (Text("Correr flythrough") ThemedText TextFont { font_size: FontSize::Px(15.0) }) ]
    });
    panel.spawn((
        ReadoutText,
        Text::new("—"),
        body_font(),
        TextColor(TEXT_MUTED),
    ));
}

/// Antes una lista plana de `PerfKnob::ALL` bajo un único título "Render":
/// cada lab nuevo (`Pradera`, y el que siga) alargaba esa lista sin decir
/// dónde hace algo. Ahora cada `PerfKnobCategory` es su propia pestaña —
/// `render_section`/`grass_section` — y `tab_pane` decide sola cuál se ve.
/// El picking de `bevy_ui`/`bevy_picking` no mantiene el clip rect de
/// scroll en sincronía con `ScrollPosition` para efectos de hit-test —
/// confirmado jugando: scrollear una pestaña larga deja los botones de
/// arriba (cambiar de tab, fijos, fuera del área que scrollea) sin
/// responder al click aunque se vean iguales. Es del motor (issues
/// públicas de Bevy sobre clip rects y scroll en pantallas de picking), no
/// de este proyecto, y no tiene arreglo chico de nuestro lado. La salida:
/// que ninguna pestaña necesite scrollear nunca — grilla de dos columnas
/// en vez de una lista de una fila por perilla, así entra sin cortar.
fn render_section(panel: &mut ChildSpawnerCommands) {
    section_title(
        panel,
        "Render",
        "Solo presentación. Overdraw respeta el culling del material: 1-2 capas bien, 3-5 medio, 6-9 malo, 10+ crítico si cubre un área grande. La secuencia apaga ambas vistas al medir.",
    );
    knob_grid(panel, knobs_in(PerfKnobCategory::Global));
}

fn grass_section(panel: &mut ChildSpawnerCommands) {
    section_title(
        panel,
        "Pradera",
        "Sólo llega acá si la escena tiene pradera (`sync_tab_buttons` esconde la pestaña si \
         no). `grass-shape`/`grass-card` son el banco de medición: pisan los controles de \
         Grass Lab (F9) mientras no estén en auto/base.",
    );
    knob_grid(panel, knobs_in(PerfKnobCategory::Grass));
}

fn knobs_in(category: PerfKnobCategory) -> impl Iterator<Item = PerfKnob> {
    PerfKnob::ALL
        .into_iter()
        .filter(move |knob| knob.category() == category)
}

/// Dos columnas: la mitad de filas que una lista de una sola columna, para
/// el mismo contenido — ver la nota de `render_section` sobre por qué
/// importa que esto quepa sin scroll.
fn knob_grid(panel: &mut ChildSpawnerCommands, knobs: impl Iterator<Item = PerfKnob>) {
    panel
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(8.0),
            row_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|grid| {
            for knob in knobs {
                knob_row(grid, knob);
            }
        });
}

fn knob_row(panel: &mut ChildSpawnerCommands, knob: PerfKnob) {
    let label = knob.label();
    panel.spawn(KnobButton(knob)).apply_scene(bsn! {
        @FeathersButton
        Node {
            width: Val::Percent(48.5),
            justify_content: JustifyContent::SpaceBetween,
            column_gap: Val::Px(12.0),
        }
        on(activate_knob)
        Children [
            (Text({label}) ThemedText TextFont { font_size: FontSize::Px(15.0) }),
            (
                Text("—")
                ThemedText
                TextFont { font_size: FontSize::Px(15.0) }
                KnobText({knob})
            ),
        ]
    });
}

fn channel_section(panel: &mut ChildSpawnerCommands) {
    section_title(
        panel,
        "Canales",
        "Algunos cuestan frame time — no los dejes prendidos mientras medís.",
    );
    panel
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(8.0),
            row_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|grid| {
            for channel in DebugChannel::ALL {
                let (label, hint) = (channel.label(), channel.hint());
                grid.spawn(ChannelButton(channel)).apply_scene(bsn! {
                    @FeathersButton
                    Node {
                        width: Val::Percent(48.5),
                        justify_content: JustifyContent::SpaceBetween,
                        column_gap: Val::Px(12.0),
                    }
                    on(activate_channel)
                    Children [
                        (
                            Node { flex_direction: FlexDirection::Column }
                            Children [
                                (
                                    Text({label})
                                    ThemedText
                                    TextFont { font_size: FontSize::Px(15.0) }
                                ),
                                (
                                    Text({hint})
                                    ThemedText
                                    TextFont { font_size: FontSize::Px(12.0) }
                                ),
                            ]
                        ),
                        (
                            Text("—")
                            ThemedText
                            TextFont { font_size: FontSize::Px(15.0) }
                            ChannelText({channel})
                        ),
                    ]
                });
            }
        });
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
                let label = action.label();
                row.spawn(ActionButton(action)).apply_scene(bsn! {
                    @FeathersButton
                    on(activate_action)
                    Children [ (Text({label}) ThemedText TextFont { font_size: FontSize::Px(15.0) }) ]
                });
            }
        });
}

/// Las perillas de locomoción: los números que las sesiones jugadas pusieron en
/// duda, movibles sin relanzar.
///
/// Dos botones por fila en vez del click-que-avanza-un-paso de `PerfKnob`:
/// afinar es ir y volver alrededor de un valor, y una perilla que sólo sube
/// obliga a dar la vuelta entera para corregirse medio paso.
fn locomotion_section(panel: &mut ChildSpawnerCommands) {
    section_title(
        panel,
        "Locomoción",
        "Se aplican al toque, sobre el perfil del actor. `BOF_TUNING=nombre=valor` \
         fija los mismos números desde el arranque.",
    );
    panel
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|column| {
            for field in TuningField::ALL {
                tuning_row(column, field);
            }
        });
}

fn tuning_row(panel: &mut ChildSpawnerCommands, field: TuningField) {
    let label = field.label();
    panel
        .spawn(Node {
            width: Val::Percent(100.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text(label.to_string()),
                ThemedText,
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
            ));
            row.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|controls| {
                tuning_button(controls, field, -1, "−");
                controls.spawn((
                    Text("—".to_string()),
                    ThemedText,
                    TextFont {
                        font_size: FontSize::Px(15.0),
                        ..default()
                    },
                    TuningText(field),
                ));
                tuning_button(controls, field, 1, "+");
            });
        });
}

fn tuning_button(panel: &mut ChildSpawnerCommands, field: TuningField, steps: i32, glyph: &str) {
    let glyph = glyph.to_string();
    panel
        .spawn(TuningButton { field, steps })
        .apply_scene(bsn! {
            @FeathersButton
            Node { width: Val::Px(34.0), justify_content: JustifyContent::Center }
            on(activate_tuning)
            Children [
                (Text({glyph}) ThemedText TextFont { font_size: FontSize::Px(15.0) })
            ]
        });
}

fn activate_tuning(
    activate: On<Activate>,
    buttons: Query<&TuningButton>,
    mut writer: MessageWriter<TuningNudge>,
) {
    if let Ok(button) = buttons.get(activate.entity) {
        writer.write(TuningNudge {
            field: button.field,
            steps: button.steps,
        });
    }
}
