//! Contextual renderer lab for the grass test scene.
//!
//! F1 owns global diagnostics and benchmarks. This panel owns the facts needed
//! to change the grass architecture, and only exists where the scene table
//! explicitly grants that context. The controls are deliberately narrow: they
//! change the renderer's real values, but do not pretend a moved boundary can
//! repair the one-blade-for-one-card handover.

use bevy::prelude::*;

use crate::input::{GrassLabToggleRequest, ModalInputFocusRequest};
use crate::presentation::theme::{
    ACCENT, BORDER, PANEL, ROW_OR_SLOT_BG, TEXT_BRIGHT, TEXT_MUTED, body_font,
};
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
                send_setting_requests,
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
            panel
                .spawn((
                    Button,
                    GrassLabControl::Reset,
                    Node {
                        align_self: AlignSelf::FlexStart,
                        padding: UiRect::axes(Val::Px(9.0), Val::Px(5.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(ROW_OR_SLOT_BG),
                    BorderColor::all(BORDER),
                ))
                .with_child((Text::new("Restaurar baseline"), body_font(13.0), TextColor(TEXT_BRIGHT)));
            panel.spawn((
                Text::new(
                    "F9 cerrar · F1 sigue siendo diagnóstico global\n\
                     Estos controles rebakean la pradera real. El relevo púa→carta requiere grupos; mover la raya sólo permite medirlo.",
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
                row.spawn((
                    Button,
                    control,
                    Node {
                        width: Val::Px(28.0),
                        height: Val::Px(24.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(ROW_OR_SLOT_BG),
                    BorderColor::all(BORDER),
                ))
                .with_child((
                    Text::new(caption),
                    body_font(16.0),
                    TextColor(ACCENT),
                ));
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

fn send_setting_requests(
    buttons: Query<(&Interaction, &GrassLabControl), Changed<Interaction>>,
    mut requests: MessageWriter<GrassLabSettingRequest>,
) {
    for (interaction, control) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
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

fn update_settings_readout(
    settings: Res<GrassRendererSettings>,
    mut readout: Single<&mut Text, With<GrassLabSettingsReadout>>,
) {
    readout.0 = format!(
        "Activo · anillo 0→1 {:.0} m · anillo 1→2 {:.0} m\n\
         carta desde {:.2}px · ancho {:.2} m",
        settings.rings[0].reach_m,
        settings.rings[1].reach_m,
        settings.spike_min_pixels,
        settings.card_width_m,
    );
    debug_assert_eq!(settings.rings.len(), GRASS_RING_COUNT);
}
