//! The debug panels: **F1** opens the tools hub, **F2** the readout menu.
//! Everything else is a click.
//!
//! Read-only over gameplay and over `debug`/`perf` state — each renders what it
//! finds and turns clicks into messages the owning modules validate and apply
//! (§20). Together they replace twelve function keys that had no hierarchy, no
//! discoverability, and no room left to grow. The hub holds channels, render
//! knobs, one-shot actions and the benchmark; the readout menu (`hud_menu`)
//! picks which real-time groups the on-screen overlay draws.
//!
//! The panel deliberately closes itself before a benchmark run starts: a modal
//! overlay is extra UI draw work and holds the pointer, and neither belongs in
//! the frame times the run is about to record.

use bevy::prelude::*;

use crate::debug::MaterialBreakdownSnapshot;
use crate::debug::channel::{DebugAction, DebugChannel, DebugConfigView};
use crate::input::ModalInputFocusRequest;
use crate::perf::{Benchmark, Flythrough, PerfKnob, PerfToggles};
use crate::visuals::terrain_material::{TerrainDebugState, TerrainDebugView};

mod hud_menu;
mod overlay;
mod style;
mod view;

#[derive(Resource, Default)]
struct DebugUiState {
    open: bool,
    active_tab: DebugTab,
}

/// Qué contenido muestra el cuerpo del hub. Todo vivía en un solo scroll: un
/// laboratorio nuevo alargaba el scroll de **todos**, no sólo el suyo — la
/// queja concreta fue tener que bajar por Medición/Render/Canales/Terreno
/// enteros para llegar a Pradera. Una pestaña por bloque, y sólo la activa
/// ocupa el panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum DebugTab {
    #[default]
    Measurement,
    Render,
    Grass,
    Channels,
    Terrain,
    Materials,
    Actions,
}

impl DebugTab {
    const ALL: [DebugTab; 7] = [
        DebugTab::Measurement,
        DebugTab::Render,
        DebugTab::Grass,
        DebugTab::Channels,
        DebugTab::Terrain,
        DebugTab::Materials,
        DebugTab::Actions,
    ];

    const fn label(self) -> &'static str {
        match self {
            DebugTab::Measurement => "Medición",
            DebugTab::Render => "Render",
            DebugTab::Grass => "Pradera",
            DebugTab::Channels => "Canales",
            DebugTab::Terrain => "Terreno",
            DebugTab::Materials => "Materiales",
            DebugTab::Actions => "Acciones",
        }
    }
}

#[derive(Component)]
struct TabButton(DebugTab);

/// El contenido de una pestaña. Sólo uno está `Display::Flex` a la vez
/// (`view::sync_tab_panes`); el resto existe pero no se dibuja ni se mide.
#[derive(Component)]
struct TabPane(DebugTab);

#[derive(Component)]
struct DebugUiRoot;

/// The panel body. No `overflow` at all — not `scroll_y`, not even `clip_y`
/// (2026-08-12): Bevy's picking doesn't keep the scroll clip in sync with
/// `ScrollPosition` for hit-testing, which broke the fixed tab buttons above
/// it after scrolling a long tab — an engine issue, not fixable from here.
/// `clip_y` was tried as a "fail visibly instead of breaking clicks" net, but
/// it clipped valid content of its own (a layout-timing bug in how Bevy
/// computes clipped height), so it was removed too — see `AHORA.md`. Every
/// tab pane has to genuinely fit without any clip to catch it if it doesn't.
/// Kept as a marker for layout identification only.
#[derive(Component)]
struct ScrollPanel;

#[derive(Component)]
struct BenchmarkButton(crate::perf::sequence::BenchmarkRequest);

#[derive(Component)]
struct FlythroughButton;

#[derive(Component)]
struct ReadoutText;

#[derive(Component)]
struct KnobButton(PerfKnob);

// `Clone, Default`: estos viven dentro de un `bsn!` (etiqueta de un botón de
// Feathers, `presentation::debug_ui::view`), que los exige aunque el valor
// real llegue interpolado — ver la nota en `PerfKnob`/`DebugChannel` sobre
// por qué el default nunca sobrevive al spawn.
#[derive(Component, Clone, Default)]
struct KnobText(PerfKnob);

#[derive(Component)]
struct ChannelButton(DebugChannel);

#[derive(Component, Clone, Default)]
struct ChannelText(DebugChannel);

#[derive(Component)]
struct ActionButton(DebugAction);

#[derive(Component)]
struct TerrainViewButton(TerrainDebugView);

#[derive(Component, Clone, Default)]
struct TerrainViewText(TerrainDebugView);

/// Pestaña "Materiales" — filas fijas, sincronizadas por índice contra
/// `MaterialBreakdownSnapshot` (mismo patrón que `overlay::TerrainLegendRow`/
/// `TerrainLegendSwatch`/`TerrainLegendLabel`: tres queries planas por
/// marcador con índice, nada de navegar `Children`).
#[derive(Component)]
struct MaterialRow(usize);

#[derive(Component)]
struct MaterialSwatch(usize);

#[derive(Component)]
struct MaterialLabel(usize);

#[derive(Component)]
struct MaterialTerrainRow;

#[derive(Component)]
struct MaterialTerrainSwatch;

#[derive(Component)]
struct MaterialTerrainLabel;

#[derive(Component)]
struct MaterialSummaryText;

#[derive(Component)]
struct MaterialOmittedText;

pub struct DebugUiPlugin;

impl Plugin for DebugUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugUiState>();
        app.init_resource::<hud_menu::HudMenuState>();
        app.add_systems(
            Startup,
            (
                view::spawn_debug_ui,
                overlay::spawn_overlay,
                hud_menu::spawn_hud_menu,
            ),
        );
        app.add_systems(
            Update,
            (
                toggle_hub,
                sync_visibility,
                view::sync_tab_panes.run_if(hub_is_open),
                view::sync_tab_buttons.run_if(hub_is_open),
                sync_labels.run_if(hub_is_open),
                sync_material_rows.run_if(hub_is_open),
                // Outside the `hub_is_open` gate on purpose: the panel closes
                // when a run starts, and that is exactly when the overlay has
                // something to say.
                overlay::update_overlay,
                overlay::update_overdraw_legend,
                overlay::update_terrain_legend,
            )
                .chain(),
        );
        // The F2 readout menu is an independent modal, so its systems form their
        // own chain rather than joining the hub's.
        app.add_systems(
            Update,
            (
                hud_menu::toggle_hud_menu,
                hud_menu::sync_hud_menu_visibility,
                hud_menu::sync_hud_menu_labels.run_if(hud_menu::menu_is_open),
            )
                .chain(),
        );
    }
}

fn hub_is_open(state: Res<DebugUiState>) -> bool {
    state.open
}

/// The one key. Everything else lives inside the panel.
fn toggle_hub(
    keys: Res<ButtonInput<KeyCode>>,
    benchmark: Res<Benchmark>,
    flythrough: Res<Flythrough>,
    mut state: ResMut<DebugUiState>,
    root: Single<Entity, With<DebugUiRoot>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
) {
    if benchmark.is_running() || flythrough.is_running() {
        set_open(&mut state, false, *root, &mut focus);
        return;
    }
    if !keys.just_pressed(KeyCode::F1) {
        return;
    }
    let wanted = !state.open;
    set_open(&mut state, wanted, *root, &mut focus);
}

/// El botón "Cerrar" del header es un `FeathersButton` desde el spike de
/// bevy_feathers (2026-08-12) y se cierra desde su propio observer de
/// `Activate` en `view.rs` (que ya puede ver esta función privada, por ser
/// un módulo hijo) — los widgets de Feathers no llenan `Interaction`, así
/// que la query vieja de `handle_clicks` nunca hubiera disparado.
fn set_open(
    state: &mut DebugUiState,
    open: bool,
    root: Entity,
    focus: &mut MessageWriter<ModalInputFocusRequest>,
) {
    if state.open == open {
        return;
    }
    state.open = open;
    focus.write(if open {
        ModalInputFocusRequest::Acquire(root)
    } else {
        ModalInputFocusRequest::Release(root)
    });
}

fn sync_visibility(state: Res<DebugUiState>, mut root: Single<&mut Node, With<DebugUiRoot>>) {
    let wanted = if state.open {
        Display::Flex
    } else {
        Display::None
    };
    if root.display != wanted {
        root.display = wanted;
    }
}

#[allow(clippy::type_complexity)]
fn sync_labels(
    perf: Res<PerfToggles>,
    config: DebugConfigView,
    benchmark: Res<Benchmark>,
    terrain_debug: Res<TerrainDebugState>,
    mut texts: ParamSet<(
        Query<(&mut Text, &KnobText)>,
        Query<(&mut Text, &ChannelText)>,
        Query<&mut Text, With<ReadoutText>>,
        Query<(&mut Text, &TerrainViewText)>,
    )>,
) {
    for (mut text, knob) in &mut texts.p0() {
        text.0 = perf.knob_value(knob.0);
    }
    for (mut text, channel) in &mut texts.p1() {
        text.0 = if config.is_enabled(channel.0) {
            "ON".to_string()
        } else {
            "off".to_string()
        };
    }
    for (mut text, view) in &mut texts.p3() {
        text.0 = if terrain_debug.view() == view.0 {
            format!("{} · ACTIVO", view.0.label())
        } else {
            view.0.label().to_string()
        };
    }
    // Button labels are static; only the readout reflects progress.
    for mut text in &mut texts.p2() {
        text.0 = benchmark.status().unwrap_or_else(|| {
            format!(
                "Perfil de arranque: {} · resultados al log al terminar",
                perf.profile.label()
            )
        });
    }
}

/// Pinta la pestaña "Materiales" desde el último `MaterialBreakdownSnapshot`
/// (poblado por `debug::material_report::log_material_breakdown` cuando se
/// corre la acción "Material breakdown" en la pestaña Acciones — esta
/// pestaña sólo lee, nunca dispara el escaneo por su cuenta). Filas fijas,
/// ocultas por índice cuando el snapshot tiene menos looks que
/// `MAX_MATERIAL_ROWS` — mismo patrón que `overlay::update_terrain_legend`.
// Cuatro marcadores de texto hermanos, todos con `&mut Text` en el mismo
// sistema: cada uno necesita `Without<>` de los otros tres para que Bevy
// pruebe que son disjuntos (mismo motivo que las *Filter en `inventory_ui`).
type SummaryFilter = (
    With<MaterialSummaryText>,
    Without<MaterialOmittedText>,
    Without<MaterialLabel>,
    Without<MaterialTerrainLabel>,
);
type OmittedFilter = (
    With<MaterialOmittedText>,
    Without<MaterialSummaryText>,
    Without<MaterialLabel>,
    Without<MaterialTerrainLabel>,
);
type RowLabelFilter = (
    Without<MaterialSummaryText>,
    Without<MaterialOmittedText>,
    Without<MaterialTerrainLabel>,
);
type TerrainLabelFilter = (
    With<MaterialTerrainLabel>,
    Without<MaterialSummaryText>,
    Without<MaterialOmittedText>,
    Without<MaterialLabel>,
);

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn sync_material_rows(
    snapshot: Res<MaterialBreakdownSnapshot>,
    mut summary: Single<&mut Text, SummaryFilter>,
    mut omitted: Single<&mut Text, OmittedFilter>,
    mut rows: Query<(&MaterialRow, &mut Node)>,
    mut swatches: Query<(&MaterialSwatch, &mut BackgroundColor), Without<MaterialTerrainSwatch>>,
    mut labels: Query<(&MaterialLabel, &mut Text), RowLabelFilter>,
    mut terrain_row: Query<&mut Node, (With<MaterialTerrainRow>, Without<MaterialRow>)>,
    mut terrain_swatch: Single<&mut BackgroundColor, With<MaterialTerrainSwatch>>,
    mut terrain_label: Single<&mut Text, TerrainLabelFilter>,
) {
    if !snapshot.is_changed() {
        return;
    }
    summary.0 = if snapshot.summary.is_empty() {
        "Sin datos — corré \"Material breakdown\" en Acciones.".to_string()
    } else {
        snapshot.summary.clone()
    };
    omitted.0 = if snapshot.omitted > 0 {
        format!("… {} looks más, omitidos por espacio", snapshot.omitted)
    } else {
        String::new()
    };
    for (row, mut node) in &mut rows {
        node.display = if row.0 < snapshot.rows.len() {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (swatch, mut color) in &mut swatches {
        if let Some(row) = snapshot.rows.get(swatch.0) {
            color.0 = row.color;
        }
    }
    for (label, mut text) in &mut labels {
        if let Some(row) = snapshot.rows.get(label.0) {
            text.0 = material_row_label(row);
        }
    }
    if let Ok(mut node) = terrain_row.single_mut() {
        node.display = if snapshot.terrain.is_some() {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let Some(terrain) = &snapshot.terrain {
        terrain_swatch.0 = terrain.color;
        terrain_label.0 = format!("terreno · {}", material_row_label(terrain));
    }
}

fn material_row_label(row: &crate::debug::MaterialLookRow) -> String {
    format!(
        "{} mats · {} mesh · rough {:.2} · metal {:.2}{}",
        row.handles,
        row.meshes,
        row.roughness,
        row.metallic,
        if row.textured { " · tex" } else { "" },
    )
}
