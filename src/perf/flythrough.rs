//! Scripted flythrough: a reproducible camera route measured **per leg**.
//!
//! Where `sequence` holds the camera still and varies render knobs, this moves
//! the camera along an authored route and buckets the cost by segment, so a run
//! answers "*which zone* got expensive" instead of "which knob". Reproducibility
//! comes from two things a hand-flown pass cannot give: a fixed route (authored
//! as `const` waypoints, captured in freecam with F4) and a fixed dwell per leg,
//! so the same geometry is crossed at the same cadence every run, on any machine.
//!
//! A warm-up lap primes every pipeline along the route before the measured lap,
//! for the same reason `sequence` primes every configuration first: otherwise the
//! leg that first shows a material pays its shader compilation.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

use super::budget::{SceneBudgetGrade, SceneInventory, scene_budget_grade};
use super::data::PerfToggles;

/// One authored camera pose on the route. `leg` names the segment that
/// *departs* this waypoint; the final waypoint only terminates the route, so its
/// `leg` label is unused.
pub struct Waypoint {
    pub leg: &'static str,
    pub position: Vec3,
    pub facing: Vec3,
}

/// The reproducible route. **Placeholder** — re-author by flying the freecam
/// (F3) and pressing **F4** at each point of interest, then pasting the logged
/// `Waypoint { .. }` lines here. Kept as code (not RON) so the canonical route
/// is versioned and identical across sessions and machines, mirroring
/// `sequence::VANTAGE_POSITION`.
pub const ROUTE: &[Waypoint] = &[
    Waypoint {
        leg: "spawn→clearing",
        position: Vec3::new(0.0, 5.0, 12.0),
        facing: Vec3::new(0.0, -0.2, -1.0),
    },
    Waypoint {
        leg: "clearing→forest-edge",
        position: Vec3::new(-6.2, 4.7, -3.2),
        facing: Vec3::new(-0.78, -0.02, 0.63),
    },
    Waypoint {
        leg: "(end)",
        position: Vec3::new(-20.0, 5.0, -20.0),
        facing: Vec3::new(-0.7, -0.1, -0.7),
    },
];

/// Unmeasured priming lap: short, just enough to compile each leg's pipelines.
const WARMUP_SECS_PER_LEG: f32 = 0.5;
/// Measured dwell per leg. Equal across legs so sample counts are comparable.
const MEASURE_SECS_PER_LEG: f32 = 3.0;

fn leg_count() -> usize {
    ROUTE.len().saturating_sub(1)
}

fn max_run_secs() -> f32 {
    (WARMUP_SECS_PER_LEG + MEASURE_SECS_PER_LEG) * leg_count() as f32 * 2.0 + 15.0
}

/// Presentation asks; `perf` owns the run and starts it (mirrors `BenchmarkRequest`).
#[derive(Message, Debug, Clone, Copy)]
pub struct FlythroughRequest;

#[derive(Clone, Copy)]
enum Phase {
    /// Priming lap: fly the whole route once, compiling pipelines, no samples.
    Warmup,
    /// Measured lap: bucket every frame's cost into the current leg.
    Measure,
}

/// Running accumulators for one leg. Sums instead of vectors so the run never
/// allocates per frame.
#[derive(Default, Clone, Copy)]
struct LegAccum {
    frame_sum: f64,
    frame_max: f64,
    gpu_sum: f64,
    tris_sum: u64,
    draws_sum: u64,
    materials_sum: u64,
    samples: u32,
}

struct RunState {
    phase: Phase,
    leg: usize,
    leg_elapsed: f32,
    total_elapsed: f32,
    /// Restored when the run ends, so a flythrough never strands render config.
    restore: PerfToggles,
    accums: Vec<LegAccum>,
    /// The interpolated pose the camera should hold this frame.
    pose: (Vec3, Vec3),
}

/// Kept after a run so the overlay can announce the outcome (mirrors
/// `sequence::FinishedRun`).
pub struct FinishedRun {
    pub at: f32,
    pub legs: usize,
    pub aborted: Option<&'static str>,
}

#[derive(Resource, Default)]
pub struct Flythrough {
    run: Option<RunState>,
    pub finished: Option<FinishedRun>,
}

impl Flythrough {
    pub fn is_running(&self) -> bool {
        self.run.is_some()
    }

    /// The scripted pose for this frame, applied by the camera's park system.
    pub fn desired_pose(&self) -> Option<(Vec3, Vec3)> {
        Some(self.run.as_ref()?.pose)
    }

    /// Progress text for the overlay: warm-up, or which leg is being measured.
    pub fn status(&self) -> Option<String> {
        let run = self.run.as_ref()?;
        let leg = ROUTE.get(run.leg)?;
        Some(match run.phase {
            Phase::Warmup => format!("precalentando {}/{}", run.leg + 1, leg_count()),
            Phase::Measure => {
                let remaining = MEASURE_SECS_PER_LEG - run.leg_elapsed;
                format!(
                    "tramo {}/{} · {} · {remaining:.1}s",
                    run.leg + 1,
                    leg_count(),
                    leg.leg
                )
            }
        })
    }
}

/// Linear pose between waypoint `leg` and `leg + 1` at `t ∈ [0, 1]`.
fn pose_at(leg: usize, t: f32) -> (Vec3, Vec3) {
    let a = &ROUTE[leg];
    let b = &ROUTE[leg + 1];
    let position = a.position.lerp(b.position, t);
    let facing = a
        .facing
        .normalize_or_zero()
        .lerp(b.facing.normalize_or_zero(), t)
        .normalize_or(Vec3::NEG_Z);
    (position, facing)
}

pub(super) fn start_requested_flythrough(
    mut requests: MessageReader<FlythroughRequest>,
    mut flythrough: ResMut<Flythrough>,
    benchmark: Res<super::Benchmark>,
    mut toggles: ResMut<PerfToggles>,
    time: Res<Time<Real>>,
) {
    if requests.read().next().is_none() {
        return;
    }
    // A second request cancels, matching the benchmark's toggle-to-stop feel.
    if flythrough.is_running() {
        finalize(
            &mut flythrough,
            &mut toggles,
            time.elapsed_secs(),
            Finish::Aborted("cancelled by operator"),
        );
        return;
    }
    // Only one run may script the camera at a time.
    if benchmark.is_running() {
        warn!("[flythrough] ignored — a benchmark is already running");
        return;
    }
    if leg_count() == 0 {
        warn!("[flythrough] ROUTE needs at least two waypoints; nothing to measure");
        return;
    }

    let restore = *toggles;
    // Vsync would quantise every leg to the same refresh multiple; the visual
    // diagnostics replace passes/materials and would poison the timings.
    toggles.vsync = false;
    toggles.wireframe = false;
    toggles.overdraw = false;

    flythrough.finished = None;
    flythrough.run = Some(RunState {
        phase: Phase::Warmup,
        leg: 0,
        leg_elapsed: 0.0,
        total_elapsed: 0.0,
        restore,
        accums: vec![LegAccum::default(); leg_count()],
        pose: pose_at(0, 0.0),
    });
    info!(
        "[flythrough] start — {} legs, priming lap then {:.1}s measured per leg.",
        leg_count(),
        MEASURE_SECS_PER_LEG
    );
}

pub(super) fn advance_flythrough(
    time: Res<Time<Real>>,
    diagnostics: Res<DiagnosticsStore>,
    scene: Res<SceneInventory>,
    mut flythrough: ResMut<Flythrough>,
    mut toggles: ResMut<PerfToggles>,
) {
    let now = time.elapsed_secs();
    let dt = time.delta_secs();

    let Some(run) = flythrough.run.as_mut() else {
        return;
    };
    run.leg_elapsed += dt;
    run.total_elapsed += dt;
    if run.total_elapsed > max_run_secs() {
        finalize(
            &mut flythrough,
            &mut toggles,
            now,
            Finish::Aborted("maximum duration exceeded"),
        );
        return;
    }

    let duration = match run.phase {
        Phase::Warmup => WARMUP_SECS_PER_LEG,
        Phase::Measure => MEASURE_SECS_PER_LEG,
    };
    let t = (run.leg_elapsed / duration).clamp(0.0, 1.0);
    run.pose = pose_at(run.leg, t);

    if matches!(run.phase, Phase::Measure)
        && let Some(frame_ms) = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
            .and_then(|d| d.value())
    {
        let (_, gpu_ms) = super::gpu_pass_costs(&diagnostics);
        let accum = &mut run.accums[run.leg];
        accum.frame_sum += frame_ms;
        accum.frame_max = accum.frame_max.max(frame_ms);
        accum.gpu_sum += gpu_ms;
        accum.tris_sum += scene.triangles as u64;
        accum.draws_sum += scene.draws as u64;
        accum.materials_sum += scene.materials as u64;
        accum.samples += 1;
    }

    if run.leg_elapsed < duration {
        return;
    }

    // Leg finished: advance, switching to the measured lap after warm-up wraps.
    run.leg_elapsed = 0.0;
    run.leg += 1;
    if run.leg < leg_count() {
        run.pose = pose_at(run.leg, 0.0);
        return;
    }
    run.leg = 0;
    match run.phase {
        Phase::Warmup => {
            run.phase = Phase::Measure;
            run.pose = pose_at(0, 0.0);
            info!("[flythrough] warm-up done — measuring.");
        }
        Phase::Measure => finalize(&mut flythrough, &mut toggles, now, Finish::Completed),
    }
}

#[derive(Clone, Copy)]
enum Finish {
    Completed,
    Aborted(&'static str),
}

fn finalize(flythrough: &mut Flythrough, toggles: &mut PerfToggles, at: f32, reason: Finish) {
    let Some(run) = flythrough.run.take() else {
        error!("[flythrough] finalization requested without an active run");
        return;
    };
    *toggles = run.restore;
    let aborted = match reason {
        Finish::Completed => {
            report(&run.accums);
            None
        }
        Finish::Aborted(reason) => {
            warn!("[flythrough] aborted: {reason} — render configuration restored");
            Some(reason)
        }
    };
    flythrough.finished = Some(FinishedRun {
        at,
        legs: run.accums.iter().filter(|a| a.samples > 0).count(),
        aborted,
    });
}

/// Averaged inventory for a leg, used only to grade it against the mobile budget.
fn leg_inventory(accum: &LegAccum) -> SceneInventory {
    let n = accum.samples.max(1) as u64;
    SceneInventory {
        triangles: (accum.tris_sum / n) as usize,
        draws: (accum.draws_sum / n) as usize,
        materials: (accum.materials_sum / n) as usize,
        ..default()
    }
}

/// Higher = worse, so the worst leg can be picked without ordering the enum.
fn severity(grade: SceneBudgetGrade) -> u8 {
    match grade {
        SceneBudgetGrade::Good => 0,
        SceneBudgetGrade::Medium => 1,
        SceneBudgetGrade::Bad => 2,
        SceneBudgetGrade::Critical => 3,
    }
}

fn kilo(n: usize) -> String {
    if n >= 10_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

/// The per-leg table, deltas replaced by the budget grade the whole tool exists
/// to surface: a run tells you which zone is over budget and on which axis.
fn report(accums: &[LegAccum]) {
    info!("[flythrough] ---- results (frame ms, lower is better) ----");
    info!(
        "[flythrough] {:<22} {:>8} {:>8} {:>8} {:>8} {:>6} {:>5} {:>8} {:>5}",
        "leg", "frame", "max", "gpu", "tris", "draws", "mats", "grade", "n"
    );
    let mut worst: Option<(usize, u8)> = None;
    for (i, accum) in accums.iter().enumerate() {
        let name = ROUTE[i].leg;
        if accum.samples == 0 {
            info!("[flythrough] {name:<22} INVALID (no samples)");
            continue;
        }
        let n = accum.samples as f64;
        let inventory = leg_inventory(accum);
        let grade = scene_budget_grade(&inventory);
        if worst.is_none_or(|(_, s)| severity(grade) > s) {
            worst = Some((i, severity(grade)));
        }
        info!(
            "[flythrough] {name:<22} {:>8.2} {:>8.2} {:>8.2} {:>8} {:>6} {:>5} {:>8} {:>5}",
            accum.frame_sum / n,
            accum.frame_max,
            accum.gpu_sum / n,
            kilo(inventory.triangles),
            inventory.draws,
            inventory.materials,
            grade.label(),
            accum.samples,
        );
    }
    if let Some((i, _)) = worst {
        let grade = scene_budget_grade(&leg_inventory(&accums[i]));
        info!("[flythrough] worst: {} ({})", ROUTE[i].leg, grade.label());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_has_at_least_one_named_leg() {
        assert!(leg_count() >= 1, "route needs a leg to measure");
        for waypoint in &ROUTE[..leg_count()] {
            assert!(!waypoint.leg.is_empty(), "each leg must be named");
        }
    }

    #[test]
    fn interpolation_hits_the_endpoints_without_a_seam() {
        let (start, _) = pose_at(0, 0.0);
        assert_eq!(start, ROUTE[0].position);
        let (end, _) = pose_at(0, 1.0);
        assert_eq!(end, ROUTE[1].position);
        // The end of leg 0 is the start of leg 1: no jump between legs.
        assert_eq!(pose_at(0, 1.0).0, pose_at(1, 0.0).0);
    }

    #[test]
    fn finalization_restores_the_exact_render_configuration() {
        let restore = PerfToggles {
            vsync: true,
            wireframe: true,
            tree_detail: true,
            ..default()
        };
        let mut flythrough = Flythrough {
            run: Some(RunState {
                phase: Phase::Measure,
                leg: 0,
                leg_elapsed: 0.0,
                total_elapsed: 0.0,
                restore,
                accums: vec![LegAccum::default(); leg_count()],
                pose: pose_at(0, 0.0),
            }),
            finished: None,
        };
        let mut active = PerfToggles {
            vsync: false,
            wireframe: false,
            ..default()
        };

        finalize(&mut flythrough, &mut active, 4.0, Finish::Aborted("test"));

        assert_eq!(active, restore);
        assert!(!flythrough.is_running());
        assert_eq!(
            flythrough.finished.as_ref().and_then(|r| r.aborted),
            Some("test")
        );
    }

    #[test]
    fn a_leg_over_material_budget_grades_bad() {
        let accum = LegAccum {
            materials_sum: (super::super::budget::MOBILE_MATERIALS as u64 + 1) * 4,
            samples: 4,
            ..default()
        };
        let grade = scene_budget_grade(&leg_inventory(&accum));
        assert!(matches!(
            grade,
            SceneBudgetGrade::Bad | SceneBudgetGrade::Critical
        ));
    }
}
