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
pub mod flythrough;
pub mod sequence;

use bevy::diagnostic::DiagnosticsStore;
use bevy::prelude::*;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;

pub use data::{PerfKnob, PerfProfile, PerfToggles};
pub use flythrough::{Flythrough, FlythroughRequest};
pub use sequence::{Benchmark, BenchmarkRequest};

pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PerfToggles::configured());
        app.init_resource::<Benchmark>();
        app.init_resource::<Flythrough>();
        app.init_resource::<ScriptedCameraPose>();
        app.init_resource::<budget::SceneInventory>();
        app.init_resource::<budget::SceneBudgetWarningState>();
        app.add_message::<BenchmarkRequest>();
        app.add_message::<FlythroughRequest>();
        app.add_message::<PerfKnobToggle>();
        app.add_plugins(RenderDiagnosticsPlugin);
        app.add_systems(
            Update,
            (
                apply_knob_requests,
                sequence::start_requested_runs,
                sequence::advance_benchmark,
                flythrough::start_requested_flythrough,
                flythrough::advance_flythrough,
                // Reconciles both runs into one pose after they advance, so the
                // camera reads a single seam and never enumerates producers.
                reconcile_scripted_camera_pose,
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

/// The single pose any scripted run wants the camera to hold this frame, or
/// `None` when nothing is scripting it. The camera reads only this — it does not
/// know how many producers exist, so a third one (a cinematic, a replay) plugs
/// in here without editing the camera (§2). Reconciled by a single writer (§7).
#[derive(Resource, Default)]
pub struct ScriptedCameraPose(pub Option<(Vec3, Vec3)>);

/// Benchmark wins over the flythrough if both somehow ran; the cross-guards
/// mean only one runs at a time in practice.
fn reconcile_scripted_camera_pose(
    benchmark: Res<Benchmark>,
    flythrough: Res<Flythrough>,
    mut pose: ResMut<ScriptedCameraPose>,
) {
    pose.0 = benchmark
        .parked_pose()
        .or_else(|| flythrough.desired_pose());
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
