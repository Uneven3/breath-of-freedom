//! Benchmark harness: the CPU/GPU split, the A/B knobs, and the scripted run
//! `docs/AHORA.md` requires before any optimisation is accepted.
//!
//! `RenderDiagnosticsPlugin` records per-render-graph-node GPU timings, so the
//! report can name the pass that costs the frame instead of guessing from
//! total frame time. Note it does *not* cover shadow passes: Bevy marks those
//! with `info_span!` rather than the diagnostics recorder, so shadow cost is
//! invisible to the timestamps and only the A/B can size it.
//!
//! Knobs are driven from the debug hub, not from keys — see
//! `presentation::debug_ui`. This module owns only the resources and the
//! sequence; applying a knob is the job of whoever owns the affected entity
//! (§7): `world::day_night` for the sun/moon and `visuals::forest` for tree
//! visuals.

pub(crate) mod budget;
pub mod data;
pub mod sequence;

use bevy::diagnostic::DiagnosticsStore;
use bevy::prelude::*;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;

pub use data::{PerfKnob, PerfProfile, PerfToggles};
pub use sequence::{Benchmark, BenchmarkRequest};

pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PerfToggles::configured());
        app.init_resource::<Benchmark>();
        app.init_resource::<budget::SceneInventory>();
        app.init_resource::<budget::SceneBudgetWarningState>();
        app.add_message::<BenchmarkRequest>();
        app.add_message::<PerfKnobToggle>();
        app.add_plugins(RenderDiagnosticsPlugin);
        app.add_systems(
            Update,
            (
                apply_knob_requests,
                sequence::start_requested_runs,
                sequence::advance_benchmark,
                apply_present_mode,
                budget::warn_scene_budget,
            )
                .chain(),
        );
        app.add_systems(Startup, log_active_profile);
    }
}

fn log_active_profile(perf: Res<PerfToggles>) {
    info!(
        "[perf] launch profile={} shadow-map={} msaa={}",
        perf.profile.label(),
        perf.shadow_map_size(),
        perf.profile.msaa_label(),
    );
}

/// Lifts the refresh-rate ceiling for attribution runs. Nothing else writes the
/// window's present mode, so this stays a single writer (§7).
///
/// `AutoNoVsync` rather than `Immediate`: it degrades to whatever the surface
/// actually supports instead of failing on drivers without immediate present.
fn apply_present_mode(
    perf: Res<PerfToggles>,
    mut window: Single<&mut bevy::window::Window, With<bevy::window::PrimaryWindow>>,
) {
    if !perf.is_changed() {
        return;
    }
    let wanted = if perf.vsync {
        bevy::window::PresentMode::AutoVsync
    } else {
        bevy::window::PresentMode::AutoNoVsync
    };
    if window.present_mode != wanted {
        window.present_mode = wanted;
    }
}

/// Presentation asks; `perf` owns the knobs and applies them (§7).
#[derive(Message, Debug, Clone, Copy)]
pub struct PerfKnobToggle(pub PerfKnob);

/// A running benchmark owns the knobs for its duration — a stray click
/// mid-run would silently corrupt the step being measured.
fn apply_knob_requests(
    mut requests: MessageReader<PerfKnobToggle>,
    mut toggles: ResMut<PerfToggles>,
    benchmark: Res<Benchmark>,
) {
    for request in requests.read() {
        if benchmark.is_running() {
            warn!("[perf] ignoring knob change while the benchmark runs");
            continue;
        }
        toggles.set_selected(request.0);
        toggles.step_selected();
        info!("[perf] {}", toggles.knob_text(request.0).trim_start());
    }
}

/// One top-level render graph node's GPU cost this frame.
pub struct PassCost {
    pub name: String,
    pub millis: f64,
}

/// GPU cost of the *leaf* render spans, most expensive first, plus their sum.
///
/// Bevy nests spans (`render/core_3d/main_opaque_pass_3d/elapsed_gpu` lives
/// inside `render/core_3d/elapsed_gpu`), so only leaves — spans that are not a
/// prefix of any other — are kept; summing every span would count the same
/// work several times. Values are already milliseconds.
///
/// Returns an empty list when the adapter has no timestamp queries, so the HUD
/// can say "unavailable" instead of reporting a fake zero.
pub fn gpu_pass_costs(diagnostics: &DiagnosticsStore) -> (Vec<PassCost>, f64) {
    const FIELD: &str = "/elapsed_gpu";

    let spans: Vec<(String, f64)> = diagnostics
        .iter()
        .filter_map(|diagnostic| {
            let path = diagnostic.path().as_str();
            let stem = path.strip_suffix(FIELD)?.strip_prefix("render/")?;
            Some((stem.to_string(), diagnostic.smoothed()?))
        })
        .collect();

    let mut passes: Vec<PassCost> = spans
        .iter()
        .filter(|(stem, _)| {
            !spans.iter().any(|(other, _)| {
                other.len() > stem.len() && other.starts_with(&format!("{stem}/"))
            })
        })
        .map(|(stem, millis)| PassCost {
            name: stem.rsplit('/').next().unwrap_or(stem).to_string(),
            millis: *millis,
        })
        .collect();

    passes.sort_by(|a, b| b.millis.total_cmp(&a.millis));
    let total = passes.iter().map(|pass| pass.millis).sum();
    (passes, total)
}
