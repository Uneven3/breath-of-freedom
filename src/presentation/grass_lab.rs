//! Contextual renderer lab for the grass test scene.
//!
//! F1 owns global diagnostics and benchmarks. This panel owns the facts needed
//! to change the grass architecture, and only exists where the scene table
//! explicitly grants that context. The controls are deliberately narrow: they
//! change the renderer's real values, but do not pretend a moved boundary can
//! repair the one-blade-for-one-card handover.

use bevy::feathers::controls::FeathersButton;
use bevy::feathers::theme::ThemedText;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

use crate::input::{GrassLabToggleRequest, ModalInputFocusRequest};
use crate::presentation::theme::{ACCENT, BORDER, PANEL, TEXT_BRIGHT, TEXT_MUTED, body_font};
use crate::scene::{AppState, current_scene};
use crate::visuals::grass::{
    GRASS_RING_COUNT, GrassLabSettingRequest, GrassLabStats, GrassRendererSettings,
};

#[derive(Resource, Default)]
struct GrassLabState {
    open: bool,
}

#[derive(Component)]
struct GrassLabRoot;

#[derive(Component)]
struct GrassLabReadout;

#[derive(Component)]
struct GrassLabSettingsReadout;

#[derive(Component, Clone, Copy)]
enum GrassLabControl {
    Frontier { ring: usize, delta_m: f32 },
    Spike { delta_pixels: f32 },
    CardWidth { delta_m: f32 },
    Reset,
}

pub(super) struct GrassLabPlugin;

impl Plugin for GrassLabPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GrassLabState>();
        app.add_systems(Startup, spawn_grass_lab);
        app.add_systems(
            Update,
            (
                toggle_grass_lab,
                close_outside_grass_lab,
                sync_visibility,
                update_readout,
                update_settings_readout,
            )
                .chain(),
        );
    }
}

fn scene_hosts_grass_lab(state: &State<AppState>) -> bool {
    current_scene(state).is_some_and(|scene| scene.authoring.grass_lab)
}

fn toggle_grass_lab(
    mut requests: MessageReader<GrassLabToggleRequest>,
    state: Res<State<AppState>>,
    mut lab: ResMut<GrassLabState>,
    root: Single<Entity, With<GrassLabRoot>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
) {
    if requests.read().next().is_some() && scene_hosts_grass_lab(&state) {
        set_open(!lab.open, *root, &mut lab, &mut focus);
    }
}

/// A tool may never hold modal focus after its scene changed. This is separate
/// from the toggle because leaving Pasto need not press F9.
fn close_outside_grass_lab(
    state: Res<State<AppState>>,
    mut lab: ResMut<GrassLabState>,
    root: Single<Entity, With<GrassLabRoot>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
) {
    if lab.open && !scene_hosts_grass_lab(&state) {
        set_open(false, *root, &mut lab, &mut focus);
    }
}

fn set_open(
    open: bool,
    owner: Entity,
    state: &mut GrassLabState,
    focus: &mut MessageWriter<ModalInputFocusRequest>,
) {
    if state.open == open {
        return;
    }
    state.open = open;
    focus.write(if open {
        ModalInputFocusRequest::Acquire(owner)
    } else {
        ModalInputFocusRequest::Release(owner)
    });
}

/// Un solo observer para las 4 variantes de `GrassLabControl`: comparten
/// marcador y mensaje, así que no hace falta uno por instancia (mismo
/// patrón que `debug_ui::view::activate_knob`). `bevy_ui_widgets::Button`
/// no llena `Interaction` — este observer sobre `On<Activate>` es el único
/// camino real de click desde la migración a `bevy_feathers`.
fn activate_grass_lab_control(
    activate: On<Activate>,
    controls: Query<&GrassLabControl>,
    mut requests: MessageWriter<GrassLabSettingRequest>,
) {
    let Ok(control) = controls.get(activate.entity) else {
        return;
    };
    let request = match *control {
        GrassLabControl::Frontier { ring, delta_m } => {
            GrassLabSettingRequest::AdjustFrontier { ring, delta_m }
        }
        GrassLabControl::Spike { delta_pixels } => {
            GrassLabSettingRequest::AdjustSpikeThreshold { delta_pixels }
        }
        GrassLabControl::CardWidth { delta_m } => {
            GrassLabSettingRequest::AdjustCardWidth { delta_m }
        }
        GrassLabControl::Reset => GrassLabSettingRequest::Reset,
    };
    requests.write(request);
}

fn spawn_grass_lab(mut commands: Commands) {
    commands
        .spawn((
            Name::new("GrassLab"),
            GrassLabRoot,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                top: Val::Px(16.0),
                width: Val::Px(430.0),
                max_width: Val::Percent(92.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(14.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(BORDER),
            GlobalZIndex(110),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("Grass Lab · representación"),
                body_font(17.0),
                TextColor(ACCENT),
            ));
            panel.spawn((
                Text::new(
                    "Residente = la grilla lo mantiene. En frustum = Bevy lo envía a esta vista. \n\
                     Un chunk en frustum aún puede quedar tapado por una montaña: eso NO es oclusión.",
                ),
                body_font(13.0),
                TextColor(TEXT_MUTED),
            ));
            panel.spawn((
                GrassLabReadout,
                Text::new("Esperando los chunks de pradera…"),
                body_font(14.0),
                TextColor(TEXT_BRIGHT),
            ));
            panel.spawn((
                GrassLabSettingsReadout,
                Text::new("Configuración cargando…"),
                body_font(14.0),
                TextColor(TEXT_BRIGHT),
            ));
            control_row(panel, "Frontera anillo 0 → 1", GrassLabControl::Frontier { ring: 0, delta_m: -1.0 }, GrassLabControl::Frontier { ring: 0, delta_m: 1.0 });
            control_row(panel, "Frontera anillo 1 → 2", GrassLabControl::Frontier { ring: 1, delta_m: -1.0 }, GrassLabControl::Frontier { ring: 1, delta_m: 1.0 });
            control_row(panel, "Umbral de carta", GrassLabControl::Spike { delta_pixels: -0.1 }, GrassLabControl::Spike { delta_pixels: 0.1 });
            control_row(panel, "Ancho de carta", GrassLabControl::CardWidth { delta_m: -0.02 }, GrassLabControl::CardWidth { delta_m: 0.02 });
            panel.spawn(GrassLabControl::Reset).apply_scene(bsn! {
                @FeathersButton
                Node { align_self: AlignSelf::FlexStart }
                on(activate_grass_lab_control)
                Children [ (Text("Restaurar baseline") ThemedText TextFont { font_size: FontSize::Px(13.0) }) ]
            });
            panel.spawn((
                Text::new(
                    "F9 cerrar · F1 sigue siendo diagnóstico global, salvo grass-shape/grass-card:\n\
                     esas dos SÍ pisan estos controles — son el banco para medir sin jugador.\n\
                     El relevo púa→carta requiere grupos; mover la raya sólo permite medirlo.",
                ),
                body_font(12.0),
                TextColor(TEXT_MUTED),
            ));
        });
}

fn control_row(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    decrement: GrassLabControl,
    increment: GrassLabControl,
) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                body_font(13.0),
                TextColor(TEXT_MUTED),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            ));
            for (caption, control) in [("−", decrement), ("+", increment)] {
                row.spawn(control).apply_scene(bsn! {
                    @FeathersButton
                    Node { width: Val::Px(28.0), height: Val::Px(24.0) }
                    on(activate_grass_lab_control)
                    Children [ (Text({caption}) ThemedText TextFont { font_size: FontSize::Px(16.0) }) ]
                });
            }
        });
}

fn sync_visibility(lab: Res<GrassLabState>, mut root: Single<&mut Node, With<GrassLabRoot>>) {
    let wanted = if lab.open {
        Display::Flex
    } else {
        Display::None
    };
    if root.display != wanted {
        root.display = wanted;
    }
}

fn update_readout(
    stats: Res<GrassLabStats>,
    mut readout: Single<&mut Text, With<GrassLabReadout>>,
) {
    let mut lines = Vec::with_capacity(stats.resident_chunks.len() + 1);
    let resident_total: usize = stats.resident_chunks.iter().sum();
    let frustum_total: usize = stats.frustum_chunks.iter().sum();
    let resident_triangles: usize = stats.resident_triangles.iter().sum();
    let frustum_triangles: usize = stats.frustum_triangles.iter().sum();
    lines.push(format!(
        "Total  {resident_total} residentes · {frustum_total} en frustum\n\
         {resident_triangles} tris residentes · {frustum_triangles} tris en frustum"
    ));
    for ring in 0..stats.resident_chunks.len() {
        lines.push(format!(
            "Anillo {ring}  {} / {} chunks · {} / {} tris",
            stats.resident_chunks[ring],
            stats.frustum_chunks[ring],
            stats.resident_triangles[ring],
            stats.frustum_triangles[ring],
        ));
    }
    readout.0 = lines.join("\n");
}

/// **F9 no es la única mano en `GrassRendererSettings`.** El banco de F1
/// (`grass-shape`/`grass-card`, pensado para medir sin jugador) pisa una
/// copia de esta configuración en el mismo punto donde se renderiza — nunca
/// escribe el recurso, pero el resultado en pantalla es el suyo mientras esté
/// activo. Sin este aviso, mover "Umbral de carta" acá parecía no hacer nada
/// y no había forma de saber por qué.
fn update_settings_readout(
    settings: Res<GrassRendererSettings>,
    perf: Res<crate::perf::PerfToggles>,
    mut readout: Single<&mut Text, With<GrassLabSettingsReadout>>,
) {
    let effective = crate::visuals::grass::shape_bench_settings(&perf, *settings);
    let shape_label = perf.grass_shape_bench_label();
    let card_label = perf.grass_card_candidate_label();
    let mut lines = Vec::new();
    if shape_label != "auto" || card_label != "base" {
        lines.push(format!(
            "⚠ F1 grass-shape={shape_label} grass-card={card_label} — pisa lo de abajo. \
             Volvé a auto/base en el hub para editar acá.",
        ));
    }
    lines.push(format!(
        "Activo · anillo 0→1 {:.0} m · anillo 1→2 {:.0} m\n\
         carta desde {:.2}px · ancho {:.2} m",
        effective.rings[0].reach_m,
        effective.rings[1].reach_m,
        effective.spike_min_pixels,
        effective.card_width_m,
    ));
    readout.0 = lines.join("\n");
    debug_assert_eq!(effective.rings.len(), GRASS_RING_COUNT);
}
