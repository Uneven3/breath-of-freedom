//! Scripted A/B runner: the thing that makes a measurement *measured*.
//!
//! Doing attribution by hand fails in three ways at once — the operator times
//! the dwell inconsistently, samples taken right after a knob moves are still
//! settling, and any drift in the scene between the first and last
//! configuration is invisible. This runs the whole matrix on a fixed clock,
//! discards the settling window, and repeats the baseline at the end so drift
//! shows up as a number instead of hiding in the comparison.
//!
//! Two rules it enforces that a human cannot:
//!
//! - **Vsync off for the whole run.** Frame time quantised to the refresh rate
//!   cannot size a win; it only says whether a threshold was crossed.
//! - **The camera must hold still.** Displacement past a threshold marks the
//!   step invalid rather than silently averaging two different scenes. This is
//!   why the earlier mounted run could show a direction but never a magnitude.

use bevy::prelude::*;

use super::data::PerfToggles;

/// Discarded after each switch: shaders recompile, caches refill, and the
/// smoothed diagnostics still carry the previous configuration.
const SETTLE_SECS: f32 = 1.5;
/// Collected per step once settled.
const MEASURE_SECS: f32 = 4.0;
/// Every configuration is applied briefly before any measuring starts.
///
/// Bevy compiles render pipelines lazily, so the first frames of a
/// never-before-seen material/pass combination pay for compilation. Without a
/// warm-up that cost lands inside whichever step happens to introduce it, and a
/// run taken right after launch reads slower than the same run on a warm
/// process — which is exactly how two otherwise identical runs disagreed.
const WARMUP_SECS: f32 = 0.5;

/// Where the camera stands while a run measures.
///
/// Authored from the operator's own ritual — walking to the stairs and looking
/// into the forest — averaged over two hand-placed runs. Standing there by hand
/// was reproducible to about half a metre, and half a metre turned out to be
/// worth ~1 ms: enough to swamp the very differences the sequence was being
/// asked to resolve. Pinning it makes absolute frame times comparable across
/// sessions, profiles and days, which is what "reproducible baseline" means.
///
/// Presentation only: the camera is not simulation, so parking it changes no
/// `FixedUpdate` result. Gameplay keeps running underneath, unwatched.
pub const VANTAGE_POSITION: Vec3 = Vec3::new(-6.2, 4.7, -3.2);
pub const VANTAGE_FACING: Vec3 = Vec3::new(-0.78, -0.02, 0.63);

/// Movement past this (metres from the anchor) invalidates the step.
const STILLNESS_TOLERANCE: f32 = 0.75;
/// Looking around past this (radians of camera rotation) invalidates it too.
/// Position alone is not enough: standing still and turning swaps the whole set
/// of trees in frustum, which is precisely the variable under test.
const AIM_TOLERANCE: f32 = 0.12;

/// One configuration under test. Only the fields a step actually varies are
/// listed; everything else stays at the baseline for that run.
#[derive(Clone, Copy)]
pub struct BenchmarkStep {
    pub name: &'static str,
    pub forest_visible: bool,
    pub sun_shadows: bool,
    pub moon_shadows: bool,
    pub outline: bool,
    pub cull_step: usize,
    pub shadow_range_step: usize,
}

impl BenchmarkStep {
    const fn baseline(name: &'static str) -> Self {
        Self {
            name,
            forest_visible: true,
            sun_shadows: true,
            moon_shadows: true,
            outline: true,
            cull_step: 0,
            shadow_range_step: 0,
        }
    }
}

/// Index of the "unbounded" entry in `SHADOW_CASTER_STEPS`, i.e. the old
/// behaviour the budget is measured against.
const SHADOW_BUDGET_OFF: usize = 3;

/// The 70 m entry in `CULL_STEPS`. The matrix reset culling to off for every
/// step and so never measured the one lever that visibly moves the frame.
const CULL_MID: usize = 2;

/// The matrix. Ordered so each step changes exactly one thing from the
/// baseline — a step that changed two would make its delta unattributable.
/// The trailing baseline repeat is the drift check.
pub const STEPS: [BenchmarkStep; 8] = [
    BenchmarkStep::baseline("baseline"),
    BenchmarkStep {
        moon_shadows: false,
        ..BenchmarkStep::baseline("moon-shadow off")
    },
    BenchmarkStep {
        sun_shadows: false,
        moon_shadows: false,
        ..BenchmarkStep::baseline("all shadows off")
    },
    BenchmarkStep {
        outline: false,
        ..BenchmarkStep::baseline("outline off")
    },
    BenchmarkStep {
        shadow_range_step: SHADOW_BUDGET_OFF,
        ..BenchmarkStep::baseline("shadow budget off")
    },
    BenchmarkStep {
        cull_step: CULL_MID,
        ..BenchmarkStep::baseline("cull 70m")
    },
    BenchmarkStep {
        forest_visible: false,
        ..BenchmarkStep::baseline("forest hidden")
    },
    BenchmarkStep::baseline("baseline repeat"),
];

#[derive(Default)]
struct StepSamples {
    frame_ms: Vec<f64>,
    gpu_ms: Vec<f64>,
    invalid: bool,
}

/// Result of one completed step.
pub struct StepResult {
    pub name: &'static str,
    pub frame_mean: f64,
    pub frame_min: f64,
    pub frame_max: f64,
    pub gpu_mean: f64,
    pub samples: usize,
    pub invalid: bool,
}

#[derive(Default)]
pub struct RunState {
    /// `Some(i)` while priming configuration `i`; `None` once measuring.
    warmup: Option<usize>,
    index: usize,
    elapsed: f32,
    anchor: Option<(Vec3, Quat)>,
    current: StepSamples,
    results: Vec<StepResult>,
    /// Restored when the run finishes, so a benchmark never leaves the game in
    /// a configuration the operator did not choose.
    restore_vsync: bool,
    /// Where the camera stood when measuring began. Absolute frame times only
    /// mean something relative to a viewpoint — two runs from different spots
    /// see different amounts of forest and are not comparable, which is not
    /// visible from the table unless the vantage is written down.
    vantage: Option<(Vec3, Vec3)>,
}

/// Kept after a run so the overlay can announce the outcome. Without it the
/// sequence ended in silence on screen and the operator had no way to know it
/// was over except by reading the log.
pub struct FinishedRun {
    /// `Time<Real>` seconds when the run ended, so the notice can expire.
    pub at: f32,
    pub valid: usize,
    pub total: usize,
}

#[derive(Resource, Default)]
pub struct Benchmark {
    pub run: Option<RunState>,
    pub finished: Option<FinishedRun>,
}

impl Benchmark {
    pub fn is_running(&self) -> bool {
        self.run.is_some()
    }

    /// The pose the camera holds for the duration, so presentation can park it
    /// there without knowing how the run chose it.
    pub fn parked_pose(&self) -> Option<(Vec3, Vec3)> {
        self.run.as_ref()?.vantage
    }

    /// True once the current step has been spoiled by movement. Surfaced live
    /// so a ruined run can be abandoned at second 8 instead of second 33.
    pub fn current_step_spoiled(&self) -> bool {
        self.run.as_ref().is_some_and(|run| run.current.invalid)
    }

    /// Progress text: which step, and how far into it.
    pub fn status(&self) -> Option<String> {
        let run = self.run.as_ref()?;
        let step = STEPS.get(run.index)?;
        let (phase, remaining) = if run.elapsed < SETTLE_SECS {
            ("asentando", SETTLE_SECS - run.elapsed)
        } else {
            ("MIDIENDO", SETTLE_SECS + MEASURE_SECS - run.elapsed)
        };
        if let Some(warming) = run.warmup {
            return Some(format!(
                "precalentando pipelines {}/{}",
                warming + 1,
                STEPS.len()
            ));
        }
        Some(format!(
            "paso {}/{} · {} · {phase} {remaining:.1}s",
            run.index + 1,
            STEPS.len(),
            step.name
        ))
    }
}

/// Which viewpoint a run holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VantageMode {
    /// Wherever the camera already is. Needed because the worst spots are
    /// found by playing, and a run that always teleports to one authored pose
    /// cannot measure them — the numbers would keep describing a comfortable
    /// corner while the game stutters somewhere else.
    Here,
    /// The authored pose, for comparing across sessions and profiles.
    Canonical,
}

/// Presentation asks for a run; `perf` owns the sequence and starts it.
#[derive(Message, Debug, Clone, Copy)]
pub struct BenchmarkRequest(pub VantageMode);

pub(super) fn start_requested_runs(
    mut requests: MessageReader<BenchmarkRequest>,
    mut benchmark: ResMut<Benchmark>,
    mut toggles: ResMut<PerfToggles>,
    camera: Option<Single<&GlobalTransform, With<Camera3d>>>,
) {
    let Some(mode) = requests.read().map(|request| request.0).next() else {
        return;
    };
    if benchmark.is_running() {
        return;
    }
    let Some(camera) = camera else {
        return;
    };
    let pose = match mode {
        VantageMode::Here => (camera.translation(), camera.forward().as_vec3()),
        VantageMode::Canonical => (VANTAGE_POSITION, VANTAGE_FACING),
    };
    benchmark.finished = None;
    benchmark.run = Some(RunState {
        warmup: Some(0),
        restore_vsync: toggles.vsync,
        vantage: Some(pose),
        ..default()
    });
    // Vsync would quantise every step to the same refresh multiple.
    toggles.vsync = false;
    apply_step(&mut toggles, &STEPS[0]);
    info!(
        "[bench] start — priming {} configurations, then {:.1}s settle + {:.1}s measure each.",
        STEPS.len(),
        SETTLE_SECS,
        MEASURE_SECS
    );
}

fn apply_step(toggles: &mut PerfToggles, step: &BenchmarkStep) {
    toggles.forest_visible = step.forest_visible;
    toggles.sun_shadows = step.sun_shadows;
    toggles.moon_shadows = step.moon_shadows;
    toggles.outline = step.outline;
    toggles.cull_step = step.cull_step;
    toggles.shadow_range_step = step.shadow_range_step;
}

#[allow(clippy::too_many_arguments)]
pub(super) fn advance_benchmark(
    time: Res<Time<Real>>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    camera: Option<Single<&GlobalTransform, With<Camera3d>>>,
    mut benchmark: ResMut<Benchmark>,
    mut toggles: ResMut<PerfToggles>,
) {
    let Some(run) = benchmark.run.as_mut() else {
        return;
    };
    let Some(camera) = camera else {
        return;
    };

    run.elapsed += time.delta_secs();

    // Prime every configuration before the first measurement, so no step pays
    // another step's pipeline compilation.
    if let Some(warming) = run.warmup {
        if run.elapsed < WARMUP_SECS {
            return;
        }
        run.elapsed = 0.0;
        match STEPS.get(warming + 1) {
            Some(step) => {
                run.warmup = Some(warming + 1);
                apply_step(&mut toggles, step);
            }
            None => {
                run.warmup = None;
                apply_step(&mut toggles, &STEPS[0]);
                info!("[bench] warm-up done — measuring.");
            }
        }
        return;
    }

    // Anchored at the end of settling, not at the start: the camera is still
    // easing out of the previous configuration during the settle window, and
    // counting that as player movement would invalidate honest steps.
    if run.elapsed >= SETTLE_SECS {
        let (position, rotation) = (camera.translation(), camera.rotation());
        let (anchor_position, anchor_rotation) = *run.anchor.get_or_insert((position, rotation));
        if position.distance(anchor_position) > STILLNESS_TOLERANCE
            || rotation.angle_between(anchor_rotation) > AIM_TOLERANCE
        {
            run.current.invalid = true;
        }
    }

    if run.elapsed >= SETTLE_SECS {
        let frame_ms = diagnostics
            .get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FRAME_TIME)
            .and_then(|d| d.value());
        if let Some(frame_ms) = frame_ms {
            run.current.frame_ms.push(frame_ms);
            let (_, gpu_ms) = super::gpu_pass_costs(&diagnostics);
            run.current.gpu_ms.push(gpu_ms);
        }
    }

    if run.elapsed < SETTLE_SECS + MEASURE_SECS {
        return;
    }

    run.results
        .push(summarise(STEPS[run.index].name, &run.current));
    run.index += 1;
    run.elapsed = 0.0;
    run.anchor = None;
    run.current = StepSamples::default();

    match STEPS.get(run.index) {
        Some(step) => apply_step(&mut toggles, step),
        None => {
            let Some(run) = benchmark.run.take() else {
                error!("[bench] active run disappeared before completion");
                return;
            };
            toggles.vsync = run.restore_vsync;
            apply_step(&mut toggles, &STEPS[0]);
            let valid = run.results.iter().filter(|step| !step.invalid).count();
            report(&run.results, run.vantage);
            benchmark.finished = Some(FinishedRun {
                at: time.elapsed_secs(),
                valid,
                total: run.results.len(),
            });
        }
    }
}

/// `None` when there is no usable baseline to subtract from.
fn delta_against(baseline: Option<&StepResult>, result: &StepResult) -> Option<f64> {
    Some(result.frame_mean - baseline?.frame_mean)
}

fn summarise(name: &'static str, samples: &StepSamples) -> StepResult {
    let count = samples.frame_ms.len();
    let mean = |values: &[f64]| {
        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f64>() / values.len() as f64
        }
    };
    StepResult {
        name,
        frame_mean: mean(&samples.frame_ms),
        frame_min: samples.frame_ms.iter().copied().fold(f64::MAX, f64::min),
        frame_max: samples.frame_ms.iter().copied().fold(0.0, f64::max),
        gpu_mean: mean(&samples.gpu_ms),
        samples: count,
        invalid: samples.invalid || count == 0,
    }
}

/// The before/after table `AHORA.md` requires as the closing criterion.
/// Deltas are against the first step, and the repeated baseline at the end
/// bounds how much of any delta is drift rather than the change under test.
fn report(results: &[StepResult], vantage: Option<(Vec3, Vec3)>) {
    let Some(first) = results.first() else {
        return;
    };
    // A delta against an invalid baseline is a fabricated number wearing the
    // costume of a measurement. Without a usable baseline the run reports its
    // absolute values and says so, rather than inviting a false comparison.
    let baseline = (!first.invalid).then_some(first);
    if let Some((position, facing)) = vantage {
        info!(
            "[bench] vantage pos=({:.1},{:.1},{:.1}) facing=({:.2},{:.2},{:.2}) — \
             absolute values only compare against a run from the same spot",
            position.x, position.y, position.z, facing.x, facing.y, facing.z
        );
    }
    info!("[bench] ---- results (frame ms, lower is better) ----");
    info!(
        "[bench] {:<20} {:>9} {:>9} {:>9} {:>9} {:>7} {:>7}",
        "step", "mean", "min", "max", "gpu", "delta", "n"
    );
    for result in results {
        if result.invalid {
            info!(
                "[bench] {:<20} INVALID (moved, looked around, or no samples)",
                result.name
            );
            continue;
        }
        let delta = match delta_against(baseline, result) {
            Some(delta) => format!("{delta:+7.2}"),
            None => format!("{:>7}", "n/a"),
        };
        info!(
            "[bench] {:<20} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {delta} {:>7}",
            result.name,
            result.frame_mean,
            result.frame_min,
            result.frame_max,
            result.gpu_mean,
            result.samples
        );
    }
    if baseline.is_none() {
        warn!("[bench] baseline INVALID — deltas withheld; absolute values only");
    }

    if let (Some(first), Some(last)) = (baseline, results.last())
        && !last.invalid
    {
        let drift = last.frame_mean - first.frame_mean;
        info!(
            "[bench] drift between the two baselines: {drift:+.2} ms — any delta smaller than this is noise",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A step that changed two things at once would produce a delta nobody can
    /// attribute. Only the trailing baseline repeat may match the first.
    #[test]
    fn every_step_varies_exactly_one_axis_from_the_baseline() {
        let base = &STEPS[0];
        for step in &STEPS[1..STEPS.len() - 1] {
            let changed = [
                step.forest_visible != base.forest_visible,
                step.outline != base.outline,
                step.cull_step != base.cull_step,
                step.shadow_range_step != base.shadow_range_step,
                // Shadows count as one axis: "all shadows off" deliberately
                // moves both lights together.
                step.sun_shadows != base.sun_shadows || step.moon_shadows != base.moon_shadows,
            ]
            .iter()
            .filter(|changed| **changed)
            .count();
            assert_eq!(changed, 1, "{} varies {changed} axes", step.name);
        }
    }

    /// Without a repeated baseline there is no way to tell a real win from the
    /// machine warming up over the course of the run.
    #[test]
    fn the_run_starts_and_ends_on_the_baseline() {
        let first = STEPS.first().expect("non-empty");
        let last = STEPS.last().expect("non-empty");
        assert_eq!(first.forest_visible, last.forest_visible);
        assert_eq!(first.sun_shadows, last.sun_shadows);
        assert_eq!(first.moon_shadows, last.moon_shadows);
        assert_eq!(first.outline, last.outline);
        assert_eq!(first.cull_step, last.cull_step);
        assert_eq!(first.shadow_range_step, last.shadow_range_step);
        assert_ne!(first.name, last.name, "the table must tell them apart");
    }

    fn result(name: &'static str, frame_mean: f64, invalid: bool) -> StepResult {
        StepResult {
            name,
            frame_mean,
            frame_min: frame_mean,
            frame_max: frame_mean,
            gpu_mean: 0.0,
            samples: 10,
            invalid,
        }
    }

    /// The bug this guards: an invalid baseline still has a `frame_mean`, so
    /// subtracting from it produced a fabricated number formatted exactly like
    /// a real measurement. A run without a usable baseline must withhold
    /// deltas, not invent them.
    #[test]
    fn no_delta_is_reported_without_a_valid_baseline() {
        let measured = result("outline off", 18.45, false);
        assert_eq!(delta_against(None, &measured), None);

        let usable = result("baseline", 24.23, false);
        assert_eq!(delta_against(Some(&usable), &measured), Some(18.45 - 24.23));
    }

    #[test]
    fn a_step_with_no_samples_is_invalid_rather_than_zero() {
        let empty = summarise("empty", &StepSamples::default());
        assert!(empty.invalid);
        assert_eq!(empty.samples, 0);
    }

    #[test]
    fn movement_invalidates_a_step_even_when_samples_exist() {
        let moved = summarise(
            "moved",
            &StepSamples {
                frame_ms: vec![16.0, 16.5],
                gpu_ms: vec![9.0, 9.2],
                invalid: true,
            },
        );
        assert!(moved.invalid);
    }
}
