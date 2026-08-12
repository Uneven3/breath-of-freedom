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

pub mod auto;
pub(crate) mod budget;
pub mod data;
pub mod flythrough;
pub mod sequence;
pub mod shot;
mod shot_stats;
pub mod suite;

use bevy::diagnostic::DiagnosticsStore;
use bevy::prelude::*;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;

pub use data::{PerfKnob, PerfKnobCategory, PerfProfile, PerfToggles};
pub use flythrough::{Flythrough, FlythroughRequest};
pub use sequence::{Benchmark, BenchmarkRequest};
pub use suite::BenchSuite;

pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(data::configured_toggles());
        // Sólo existe cuando `BOF_BENCH` la pide, y los sistemas que la mueven
        // se gatean con `resource_exists`: sin la variable, nada de esto corre
        // ni cuesta un frame.
        let auto_bench = auto::configured_auto_bench();
        let auto_shot = shot::configured_auto_shot();
        match (auto_bench, auto_shot) {
            (Ok(Some(bench)), Ok(None)) => {
                app.insert_resource(bench);
            }
            (Ok(None), Ok(Some(shot))) => {
                app.insert_resource(shot);
            }
            (Ok(None), Ok(None)) => {}
            (Ok(Some(_)), Ok(Some(_))) => {
                error!("[perf] BOF_BENCH y BOF_SHOT son modos excluyentes");
                app.add_systems(Startup, exit_invalid_automation_config);
            }
            _ => {
                app.add_systems(Startup, exit_invalid_automation_config);
            }
        }
        app.init_resource::<Benchmark>();
        app.init_resource::<Flythrough>();
        app.init_resource::<ScriptedCameraPose>();
        app.init_resource::<budget::SceneInventory>();
        app.init_resource::<shot::BrokenAssets>();
        app.init_resource::<shot::ShotCaptureProgress>();
        app.init_resource::<shot_stats::ShotStatsLog>();
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
                auto::drive_auto_bench.run_if(resource_exists::<auto::AutoBench>),
                // Antes de cualquier captura: una foto sacada con assets rotos
                // no es evidencia de nada, y hasta el 2026-08-07 nada lo decía.
                shot::note_failed_assets,
                shot::drive_auto_shot.run_if(resource_exists::<shot::AutoShot>),
                shot::capture_on_request,
            )
                .chain(),
        );
        app.add_systems(Startup, log_active_profile);
    }
}

/// An invalid automation request must make `cargo run` fail. Merely omitting
/// its resource would launch the normal game, which is indistinguishable from
/// a typo in a script until somebody notices the window is still open.
fn exit_invalid_automation_config(mut exit: MessageWriter<AppExit>) {
    exit.write(AppExit::error());
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
///
/// The screenshot harness is last and needs no guard against the others: it only
/// exists when `BOF_SHOT` asked for it, and that run never starts a benchmark.
fn reconcile_scripted_camera_pose(
    benchmark: Res<Benchmark>,
    flythrough: Res<Flythrough>,
    shot: Option<Res<shot::AutoShot>>,
    mut pose: ResMut<ScriptedCameraPose>,
) {
    pose.0 = benchmark
        .parked_pose()
        .or_else(|| flythrough.desired_pose())
        .or_else(|| shot.as_deref().and_then(shot::shot_pose));
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
    flythrough: Res<Flythrough>,
) {
    for request in requests.read() {
        if benchmark.is_running() || flythrough.is_running() {
            warn!("[perf] ignoring knob change while a measurement runs");
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
pub fn gpu_pass_costs(diagnostics: &DiagnosticsStore) -> (Vec<PassCost>, Option<f64>) {
    const FIELD: &str = "/elapsed_gpu";

    let spans: Vec<(String, f64)> = diagnostics
        .iter()
        .filter_map(|diagnostic| {
            let path = diagnostic.path().as_str();
            let stem = path.strip_suffix(FIELD)?.strip_prefix("render/")?;
            // Benchmark runners average their own, bounded sample window. A
            // diagnostic EMA crosses step boundaries and would attribute some
            // of the previous configuration to the next one.
            let millis = diagnostic.value()?;
            (millis.is_finite() && millis >= 0.0).then(|| (stem.to_string(), millis))
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
    let total = (!passes.is_empty()).then(|| passes.iter().map(|pass| pass.millis).sum());
    (passes, total)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::diagnostic::{Diagnostic, DiagnosticMeasurement, DiagnosticPath};
    use bevy::platform::time::Instant;

    use super::*;

    fn measured(path: &str, values: &[f64]) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(DiagnosticPath::new(path.to_string()));
        let mut now = Instant::now();
        for value in values {
            diagnostic.add_measurement(DiagnosticMeasurement {
                time: now,
                value: *value,
            });
            now += Duration::from_millis(16);
        }
        diagnostic
    }

    /// A benchmark owns its averaging window. Reading Bevy's EMA here leaks a
    /// previous render configuration into the current step.
    #[test]
    fn gpu_cost_uses_the_latest_frame_not_cross_step_smoothing() {
        let mut diagnostics = DiagnosticsStore::default();
        diagnostics.add(measured(
            "render/core_3d/main_opaque_pass_3d/elapsed_gpu",
            &[2.0, 8.0],
        ));

        let (passes, total) = gpu_pass_costs(&diagnostics);

        assert_eq!(passes.len(), 1);
        assert_eq!(passes[0].millis, 8.0);
        assert_eq!(total, Some(8.0));
    }

    /// Nested render spans overlap. Keeping the parent would count the same
    /// GPU work twice and make the total larger than the frame it describes.
    #[test]
    fn gpu_cost_sums_only_leaf_spans() {
        let mut diagnostics = DiagnosticsStore::default();
        diagnostics.add(measured("render/core_3d/elapsed_gpu", &[10.0]));
        diagnostics.add(measured(
            "render/core_3d/main_opaque_pass_3d/elapsed_gpu",
            &[6.0],
        ));
        diagnostics.add(measured("render/core_3d/bloom/elapsed_gpu", &[2.0]));

        let (passes, total) = gpu_pass_costs(&diagnostics);

        assert_eq!(passes.len(), 2);
        assert_eq!(total, Some(8.0));
        assert!(passes.iter().all(|pass| pass.name != "core_3d"));
    }

    #[test]
    fn adapters_without_gpu_timestamps_do_not_report_zero() {
        let (passes, total) = gpu_pass_costs(&DiagnosticsStore::default());

        assert!(passes.is_empty());
        assert_eq!(total, None);
    }

    #[test]
    fn invalid_gpu_measurements_do_not_poison_a_report() {
        let mut diagnostics = DiagnosticsStore::default();
        diagnostics.add(measured("render/core_3d/elapsed_gpu", &[f64::NAN]));

        let (passes, total) = gpu_pass_costs(&diagnostics);

        assert!(passes.is_empty());
        assert_eq!(total, None);
    }
}
