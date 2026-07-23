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

use crate::debug::channel::{
    DebugAction, DebugActionRequest, DebugChannel, DebugChannelToggle, DebugConfigView,
};
use crate::input::ModalInputFocusRequest;
use crate::perf::{
    Benchmark, BenchmarkRequest, FlythroughRequest, PerfKnob, PerfKnobToggle, PerfToggles,
};

mod hud_menu;
mod overlay;
mod style;
mod view;

#[derive(Resource, Default)]
struct DebugUiState {
    open: bool,
}

#[derive(Component)]
struct DebugUiRoot;

/// The panel body. Bevy clips `Overflow::scroll_y` content but never scrolls it
/// on its own, so without a wheel handler the knobs past the fold were simply
/// unreachable — the panel looked complete and silently hid half of itself.
#[derive(Component)]
struct ScrollPanel;

#[derive(Component)]
struct CloseButton;

#[derive(Component)]
struct BenchmarkButton(crate::perf::sequence::VantageMode);

#[derive(Component)]
struct BenchmarkText;

#[derive(Component)]
struct FlythroughButton;

#[derive(Component)]
struct ReadoutText;

#[derive(Component)]
struct KnobButton(PerfKnob);

#[derive(Component)]
struct KnobText(PerfKnob);

#[derive(Component)]
struct ChannelButton(DebugChannel);

#[derive(Component)]
struct ChannelText(DebugChannel);

#[derive(Component)]
struct ActionButton(DebugAction);

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
                handle_clicks,
                sync_visibility,
                sync_labels.run_if(hub_is_open),
                scroll_panel.run_if(hub_is_open),
                // Outside the `hub_is_open` gate on purpose: the panel closes
                // when a run starts, and that is exactly when the overlay has
                // something to say.
                overlay::update_overlay,
                overlay::update_overdraw_legend,
            )
                .chain(),
        );
        // The F2 readout menu is an independent modal, so its systems form their
        // own chain rather than joining the hub's.
        app.add_systems(
            Update,
            (
                hud_menu::toggle_hud_menu,
                hud_menu::handle_hud_menu_clicks,
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
    mut state: ResMut<DebugUiState>,
    root: Single<Entity, With<DebugUiRoot>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
) {
    if !keys.just_pressed(KeyCode::F1) {
        return;
    }
    let wanted = !state.open;
    set_open(&mut state, wanted, *root, &mut focus);
}

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

#[allow(clippy::too_many_arguments)]
fn handle_clicks(
    mut state: ResMut<DebugUiState>,
    root: Single<Entity, With<DebugUiRoot>>,
    close: Query<&Interaction, (Changed<Interaction>, With<CloseButton>)>,
    bench: Query<(&Interaction, &BenchmarkButton), Changed<Interaction>>,
    flythrough: Query<&Interaction, (Changed<Interaction>, With<FlythroughButton>)>,
    knobs: Query<(&Interaction, &KnobButton), Changed<Interaction>>,
    channels: Query<(&Interaction, &ChannelButton), Changed<Interaction>>,
    actions: Query<(&Interaction, &ActionButton), Changed<Interaction>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
    mut knob_writer: MessageWriter<PerfKnobToggle>,
    mut channel_writer: MessageWriter<DebugChannelToggle>,
    mut action_writer: MessageWriter<DebugActionRequest>,
    mut bench_writer: MessageWriter<BenchmarkRequest>,
    mut flythrough_writer: MessageWriter<FlythroughRequest>,
) {
    if !state.open {
        return;
    }
    let pressed = |interaction: &Interaction| *interaction == Interaction::Pressed;

    for (interaction, knob) in &knobs {
        if pressed(interaction) {
            knob_writer.write(PerfKnobToggle(knob.0));
        }
    }
    for (interaction, channel) in &channels {
        if pressed(interaction) {
            channel_writer.write(DebugChannelToggle(channel.0));
        }
    }
    for (interaction, action) in &actions {
        if pressed(interaction) {
            action_writer.write(DebugActionRequest(action.0));
        }
    }
    for (interaction, button) in &bench {
        if pressed(interaction) {
            bench_writer.write(BenchmarkRequest(button.0));
            // The panel would be measured along with the scene; close it first.
            set_open(&mut state, false, *root, &mut focus);
        }
    }
    if flythrough.iter().any(pressed) {
        flythrough_writer.write(FlythroughRequest);
        // Same reason as the benchmark: the modal must not enter the measurement.
        set_open(&mut state, false, *root, &mut focus);
    }
    if close.iter().any(pressed) {
        set_open(&mut state, false, *root, &mut focus);
    }
}

/// Wheel scrolling for the panel body, clamped to the content that exists.
fn scroll_panel(
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut panel: Single<(&mut ScrollPosition, &ComputedNode), With<ScrollPanel>>,
) {
    const LINE_PX: f32 = 24.0;

    let delta: f32 = wheel
        .read()
        .map(|event| match event.unit {
            bevy::input::mouse::MouseScrollUnit::Line => event.y * LINE_PX,
            bevy::input::mouse::MouseScrollUnit::Pixel => event.y,
        })
        .sum();
    if delta == 0.0 {
        return;
    }
    let (position, node) = &mut *panel;
    let overflow = (node.content_size.y - node.size().y).max(0.0);
    position.0.y = (position.0.y - delta).clamp(0.0, overflow);
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
    mut texts: ParamSet<(
        Query<(&mut Text, &KnobText)>,
        Query<(&mut Text, &ChannelText)>,
        Query<&mut Text, With<ReadoutText>>,
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
