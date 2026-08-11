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

use super::budget::SceneInventory;
use super::data::PerfToggles;
use super::suite::{BenchSuite, BenchmarkStep};
use crate::visuals::material_registry::Subject;

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
/// Hard stop for lifecycle failures, per step, so a longer matrix gets
/// proportionally longer to finish instead of being cut off at a fixed wall.
const MAX_SECS_PER_STEP: f32 = WARMUP_SECS + SETTLE_SECS + MEASURE_SECS;
/// Slack on top, to tolerate long frames without stranding the toggles.
const RUN_SLACK_SECS: f32 = 15.0;
/// Time a manually selected suite gives its scene to finish spawning before the
/// runner parks the camera and starts warming pipelines.
pub(super) const SCENE_SETTLE_SECS: f32 = 2.0;

/// Movement past this (metres from the anchor) invalidates the step.
const STILLNESS_TOLERANCE: f32 = 0.75;
/// Looking around past this (radians of camera rotation) invalidates it too.
/// Position alone is not enough: standing still and turning swaps the whole set
/// of trees in frustum, which is precisely the variable under test.
const AIM_TOLERANCE: f32 = 0.12;

#[derive(Default)]
struct StepSamples {
    frame_ms: Vec<f64>,
    gpu_ms: Vec<f64>,
    invalid: bool,
}

/// Lo que hacía falta saber para que una tabla de milisegundos signifique algo
/// dentro de un mes. Se toma al **arrancar** la corrida, no al terminarla: al
/// terminar las perillas ya volvieron a su valor previo y el contexto
/// describiría otra cosa que la que se midió.
#[derive(Default)]
struct RunContext {
    suite: BenchSuite,
    scene: String,
    profile: &'static str,
    window: UVec2,
    msaa: String,
    render_scale: String,
    grass_density: String,
    grass_reach: String,
    shadow_map: String,
    shadow_range: String,
    leaf_shadows: String,
    /// Lo que el mirador **ve** durante el baseline, repartido por sistema.
    ///
    /// Es lo que convierte al mirador en algo verificable: una suite declara a
    /// qué apunta ([`BenchSuite::vantage_subject`]) y esto dice cuánto de eso
    /// hay realmente en cuadro. Se toma en el baseline y no al arrancar, porque
    /// antes de que la cámara se estacione el encuadre todavía es otro.
    framing: Option<SceneInventory>,
    /// Cuántos assets fallaron al cargar durante la corrida.
    broken_assets: usize,
}

impl RunContext {
    fn capture(
        suite: BenchSuite,
        toggles: &PerfToggles,
        scene: &crate::scene::AppState,
        window: UVec2,
    ) -> Self {
        use bof_domain::perf::PerfKnob;
        // Del baseline de la suite, no de las perillas de ahora mismo: lo que
        // el reporte tiene que declarar es contra qué se midió todo lo demás.
        let mut baseline = *toggles;
        apply_step(&mut baseline, &suite.steps()[0]);
        Self {
            suite,
            scene: scene_label(scene),
            profile: baseline.profile.label(),
            window,
            msaa: baseline.knob_value(PerfKnob::Msaa),
            render_scale: baseline.knob_value(PerfKnob::RenderScale),
            grass_density: baseline.knob_value(PerfKnob::GrassDensity),
            grass_reach: baseline.knob_value(PerfKnob::GrassReach),
            shadow_map: baseline.knob_value(PerfKnob::ShadowMap),
            shadow_range: baseline.knob_value(PerfKnob::ShadowRange),
            leaf_shadows: baseline.knob_value(PerfKnob::LeafShadows),
            framing: None,
            broken_assets: 0,
        }
    }

    fn unusable_reason(&self) -> Option<&'static str> {
        if self.broken_assets > 0 {
            return Some("assets failed to load");
        }
        let Some(framing) = self.framing else {
            return Some("the vantage framing was not sampled");
        };
        let subject = self.suite.vantage_subject();
        (framing
            .share_of(subject)
            .min(framing.triangle_share_of(subject))
            < SUBJECT_FLOOR)
            .then_some("the vantage does not contain its subject")
    }
}

/// Por debajo de esta fracción de las mallas visibles, un tema no está medido:
/// está de fondo. El caso que lo fija es el bosque desde el mirador canónico —
/// ocultarlo entero valía 0,34 ms, o sea nada, y nadie lo leyó como "el mirador
/// está mal apuntado" hasta semanas después.
const SUBJECT_FLOOR: f32 = 0.10;

fn scene_label(state: &crate::scene::AppState) -> String {
    match state {
        crate::scene::AppState::MainMenu => "menu".to_string(),
        crate::scene::AppState::Scene(id) => crate::scene::SCENES
            .iter()
            .find(|scene| scene.id == *id)
            .map_or_else(|| format!("{id:?}"), |scene| scene.label.to_string()),
    }
}

/// Result of one completed step.
pub struct StepResult {
    pub name: &'static str,
    pub frame_mean: f64,
    pub frame_min: f64,
    pub frame_max: f64,
    pub gpu_mean: f64,
    pub samples: usize,
    pub gpu_samples: usize,
    pub invalid: bool,
}

#[derive(Default)]
pub struct RunState {
    /// Which matrix this run is walking. Carried on the run rather than read
    /// from a resource, so a suite cannot change under a run in progress.
    suite: BenchSuite,
    /// `Some(i)` while priming configuration `i`; `None` once measuring.
    warmup: Option<usize>,
    index: usize,
    elapsed: f32,
    total_elapsed: f32,
    anchor: Option<(Vec3, Quat)>,
    current: StepSamples,
    results: Vec<StepResult>,
    /// Restored when the run finishes, so a benchmark never leaves the game in
    /// a configuration the operator did not choose.
    restore: PerfToggles,
    context: RunContext,
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
    pub aborted: Option<&'static str>,
}

#[derive(Resource, Default)]
pub struct Benchmark {
    pub(super) pending: Option<PendingRun>,
    pub run: Option<RunState>,
    pub finished: Option<FinishedRun>,
}

#[derive(Clone, Copy)]
pub(super) struct PendingRun {
    request: BenchmarkRequest,
    elapsed: f32,
}

impl Benchmark {
    pub fn is_running(&self) -> bool {
        self.pending.is_some() || self.run.is_some()
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
        if let Some(pending) = self.pending {
            let phase = if pending.elapsed == 0.0 {
                "entrando a su escena"
            } else {
                "asentando escena"
            };
            return Some(format!("{} · {phase}", pending.request.suite.label()));
        }
        let run = self.run.as_ref()?;
        let steps = run.suite.steps();
        let step = steps.get(run.index)?;
        let (phase, remaining) = if run.elapsed < SETTLE_SECS {
            ("asentando", SETTLE_SECS - run.elapsed)
        } else {
            ("MIDIENDO", SETTLE_SECS + MEASURE_SECS - run.elapsed)
        };
        if let Some(warming) = run.warmup {
            return Some(format!(
                "{} · precalentando pipelines {}/{}",
                run.suite.label(),
                warming + 1,
                steps.len()
            ));
        }
        Some(format!(
            "{} · paso {}/{} · {} · {phase} {remaining:.1}s",
            run.suite.label(),
            run.index + 1,
            steps.len(),
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
///
/// La suite viaja en el pedido y no en un recurso, porque quién mide y qué mide
/// son la misma decisión: el hub tiene un botón por suite.
#[derive(Message, Debug, Clone, Copy)]
pub struct BenchmarkRequest {
    pub vantage: VantageMode,
    pub suite: BenchSuite,
}

impl BenchmarkRequest {
    pub const fn new(suite: BenchSuite, vantage: VantageMode) -> Self {
        Self { vantage, suite }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "arrancar una corrida necesita el pedido, el estado, las perillas, el reloj, \
              la cámara, el diagnóstico que la bloquea, y la escena y la ventana que el \
              reporte declara como contexto"
)]
pub(super) fn start_requested_runs(
    mut requests: MessageReader<BenchmarkRequest>,
    mut benchmark: ResMut<Benchmark>,
    flythrough: Res<super::Flythrough>,
    mut toggles: ResMut<PerfToggles>,
    time: Res<Time<Real>>,
    camera: Option<Single<&GlobalTransform, With<Camera3d>>>,
    terrain_debug: Res<crate::visuals::terrain_material::TerrainDebugState>,
    scene: Res<State<crate::scene::AppState>>,
    mut next_scene: ResMut<NextState<crate::scene::AppState>>,
    window: Option<Single<&Window, With<bevy::window::PrimaryWindow>>>,
) {
    let incoming = requests.read().copied().next();
    if incoming.is_some() && benchmark.is_running() {
        if benchmark.run.is_some() {
            finalize_benchmark(
                &mut benchmark,
                &mut toggles,
                time.elapsed_secs(),
                FinishReason::Aborted("cancelled by operator"),
            );
        } else {
            reject_benchmark(&mut benchmark, time.elapsed_secs(), "cancelled by operator");
        }
        return;
    }
    if incoming.is_some() && flythrough.is_running() {
        warn!("[bench] cannot start — a flythrough is already running");
        reject_benchmark(
            &mut benchmark,
            time.elapsed_secs(),
            "flythrough is already running",
        );
        return;
    }

    let request = if let Some(request) = incoming {
        if request.vantage == VantageMode::Canonical {
            let wanted = crate::scene::AppState::Scene(request.suite.scene());
            if *scene.get() != wanted {
                benchmark.finished = None;
                benchmark.pending = Some(PendingRun {
                    request,
                    elapsed: 0.0,
                });
                next_scene.set(wanted);
                info!(
                    "[bench] entering suite '{}' scene before measuring",
                    request.suite.label()
                );
                return;
            }
        }
        request
    } else {
        let Some(pending) = benchmark.pending.as_mut() else {
            return;
        };
        let wanted = crate::scene::AppState::Scene(pending.request.suite.scene());
        if *scene.get() != wanted {
            pending.elapsed = 0.0;
            next_scene.set(wanted);
            return;
        }
        pending.elapsed += time.delta_secs();
        if pending.elapsed < SCENE_SETTLE_SECS {
            return;
        }
        let request = pending.request;
        benchmark.pending = None;
        request
    };
    let Some(camera) = camera else {
        warn!("[bench] cannot start — camera missing or ambiguous");
        reject_benchmark(
            &mut benchmark,
            time.elapsed_secs(),
            "camera missing or ambiguous",
        );
        return;
    };
    if terrain_debug.view() != crate::visuals::terrain_material::TerrainDebugView::Off {
        warn!("[bench] cannot start — switch the terrain view back to Arte first");
        reject_benchmark(
            &mut benchmark,
            time.elapsed_secs(),
            "terrain diagnostic view is active",
        );
        return;
    }
    let pose = match request.vantage {
        VantageMode::Here => (camera.translation(), camera.forward().as_vec3()),
        VantageMode::Canonical => request.suite.vantage(),
    };
    let restore = *toggles;
    benchmark.finished = None;
    let suite = request.suite;
    let context = RunContext::capture(
        suite,
        &toggles,
        scene.get(),
        window.map_or(UVec2::ZERO, |window| window.physical_size()),
    );
    benchmark.run = Some(RunState {
        suite,
        warmup: Some(0),
        restore,
        vantage: Some(pose),
        context,
        ..default()
    });
    // Vsync would quantise every step to the same refresh multiple.
    toggles.vsync = false;
    apply_step(&mut toggles, &suite.steps()[0]);
    info!(
        "[bench] start — suite '{}' ({}), priming {} configurations, then {:.1}s settle + {:.1}s measure each.",
        suite.label(),
        suite.question(),
        suite.steps().len(),
        SETTLE_SECS,
        MEASURE_SECS
    );
}

fn reject_benchmark(benchmark: &mut Benchmark, at: f32, reason: &'static str) {
    benchmark.pending = None;
    benchmark.run = None;
    benchmark.finished = Some(FinishedRun {
        at,
        valid: 0,
        total: 0,
        aborted: Some(reason),
    });
}

fn apply_step(toggles: &mut PerfToggles, step: &BenchmarkStep) {
    // Visual diagnostics add/replace render passes. They are useful for
    // inspection but would invalidate every timing in an attribution run.
    toggles.wireframe = false;
    toggles.overdraw = false;
    toggles.forest_visible = step.forest_visible;
    toggles.sun_shadows = step.sun_shadows;
    toggles.moon_shadows = step.moon_shadows;
    toggles.cull_step = step.cull_step;
    toggles.shadow_range_step = step.shadow_range_step;
    toggles.shadow_map_step = step.shadow_map_step;
    toggles.leaf_shadows = step.leaf_shadows;
    toggles.grass_density_step = step.grass_density_step;
    toggles.grass_reach_step = step.grass_reach_step;
    toggles.render_scale_step = step.render_scale_step;
    toggles.msaa_step = step.msaa_step;
}

#[derive(Clone, Copy)]
enum FinishReason {
    Completed,
    Aborted(&'static str),
}

fn finalize_benchmark(
    benchmark: &mut Benchmark,
    toggles: &mut PerfToggles,
    at: f32,
    reason: FinishReason,
) {
    let Some(run) = benchmark.run.take() else {
        error!("[bench] finalization requested without an active run");
        return;
    };
    *toggles = run.restore;
    let valid = run.results.iter().filter(|step| !step.invalid).count();
    let aborted = match reason {
        FinishReason::Completed => {
            report(&run.results, run.vantage, &run.context);
            let unusable = run.context.unusable_reason();
            if let Some(reason) = unusable {
                error!("[bench] corrida terminada pero no utilizable: {reason}");
            }
            unusable
        }
        FinishReason::Aborted(reason) => {
            warn!("[bench] aborted: {reason} — render configuration restored");
            Some(reason)
        }
    };
    benchmark.finished = Some(FinishedRun {
        at,
        valid,
        total: run.results.len(),
        aborted,
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn advance_benchmark(
    time: Res<Time<Real>>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    camera: Option<Single<&GlobalTransform, With<Camera3d>>>,
    inventory: Res<SceneInventory>,
    broken: Res<super::shot::BrokenAssets>,
    mut benchmark: ResMut<Benchmark>,
    mut toggles: ResMut<PerfToggles>,
) {
    if benchmark.run.is_none() {
        return;
    }
    let Some(camera) = camera else {
        finalize_benchmark(
            &mut benchmark,
            &mut toggles,
            time.elapsed_secs(),
            FinishReason::Aborted("camera missing or ambiguous"),
        );
        return;
    };
    let Some(run) = benchmark.run.as_mut() else {
        return;
    };

    run.elapsed += time.delta_secs();
    run.total_elapsed += time.delta_secs();
    let max_secs = MAX_SECS_PER_STEP * run.suite.steps().len() as f32 + RUN_SLACK_SECS;
    if run.total_elapsed > max_secs {
        finalize_benchmark(
            &mut benchmark,
            &mut toggles,
            time.elapsed_secs(),
            FinishReason::Aborted("maximum duration exceeded"),
        );
        return;
    }

    // Prime every configuration before the first measurement, so no step pays
    // another step's pipeline compilation.
    if let Some(warming) = run.warmup {
        if run.elapsed < WARMUP_SECS {
            return;
        }
        run.elapsed = 0.0;
        match run.suite.steps().get(warming + 1) {
            Some(step) => {
                run.warmup = Some(warming + 1);
                apply_step(&mut toggles, step);
            }
            None => {
                run.warmup = None;
                apply_step(&mut toggles, &run.suite.steps()[0]);
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
        // El encuadre se muestrea en el baseline, con la cámara ya estacionada
        // y antes de que ningún paso apague nada: es el único momento en que lo
        // que hay en cuadro **es** la escena que se dice medir. Gana la última
        // lectura del paso, que es la más asentada.
        if run.index == 0 {
            run.context.framing = Some(*inventory);
        }
        run.context.broken_assets = broken.count();
        let frame_ms = diagnostics
            .get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FRAME_TIME)
            .and_then(|d| d.value());
        if let Some(frame_ms) = frame_ms.filter(|value| value.is_finite() && *value > 0.0) {
            run.current.frame_ms.push(frame_ms);
            let (_, gpu_ms) = super::gpu_pass_costs(&diagnostics);
            if let Some(gpu_ms) = gpu_ms {
                run.current.gpu_ms.push(gpu_ms);
            }
        }
    }

    if run.elapsed < SETTLE_SECS + MEASURE_SECS {
        return;
    }

    run.results
        .push(summarise(run.suite.steps()[run.index].name, &run.current));
    run.index += 1;
    run.elapsed = 0.0;
    run.anchor = None;
    run.current = StepSamples::default();

    match run.suite.steps().get(run.index) {
        Some(step) => apply_step(&mut toggles, step),
        None => {
            finalize_benchmark(
                &mut benchmark,
                &mut toggles,
                time.elapsed_secs(),
                FinishReason::Completed,
            );
        }
    }
}

/// `None` cuando no hay baseline utilizable del que restar.
///
/// Existe como función y no como una resta suelta por el bug que ya ocurrió: un
/// baseline inválido igual tiene un `frame_mean`, así que restarle producía un
/// número inventado con el formato exacto de una medición. Una corrida sin
/// baseline utilizable tiene que **retener** los deltas, no fabricarlos.
fn delta_against(baseline: Option<f64>, value: f64) -> Option<f64> {
    Some(value - baseline?)
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
        gpu_samples: samples.gpu_ms.len(),
        invalid: samples.invalid || count == 0,
    }
}

/// The before/after table `AHORA.md` requires as the closing criterion.
/// Deltas are against the first step, and the repeated baseline at the end
/// bounds how much of any delta is drift rather than the change under test.
///
/// **Abre declarando qué se midió, y eso no es adorno.** Una tabla de
/// milisegundos sin su contexto no se puede comparar con otra de la semana que
/// viene: el 2026-08-06 hubo que deducir del delta del bosque en qué escena
/// había corrido una tabla, porque no lo decía. Perfil, escena, resolución y las
/// perillas del baseline son lo que hace que dos corridas hablen de lo mismo.
/// Qué hay realmente en cuadro, y si el tema declarado es uno de ellos.
///
/// **Esta es la línea que la suite no tenía.** Un mirador es una afirmación —
/// "desde acá se ve el bosque"— y hasta ahora ninguna corrida podía
/// desmentirla: los deltas de un sistema que casi no está en cuadro salen del
/// tamaño del ruido, que es indistinguible de "no cuesta nada".
fn report_framing(context: &RunContext) {
    let Some(framing) = context.framing else {
        warn!(
            "[bench] sin muestra de encuadre — no se puede afirmar que el mirador vea \
             '{}'",
            context.suite.vantage_subject().label()
        );
        return;
    };
    let mut parts: Vec<String> = Vec::new();
    for subject in Subject::ALL {
        let tally = framing.subject(subject);
        if tally.meshes == 0 {
            continue;
        }
        parts.push(format!(
            "{}={:.0}%/{:.0}% ({} draws~)",
            subject.label(),
            framing.share_of(subject) * 100.0,
            framing.triangle_share_of(subject) * 100.0,
            tally.draws,
        ));
    }
    info!(
        "[bench] en cuadro (mallas%/triángulos%): {} · total {} mallas, {} draws~",
        parts.join(" "),
        framing.visible_meshes,
        framing.draws,
    );
    let subject = context.suite.vantage_subject();
    let meshes = framing.share_of(subject);
    let triangles = framing.triangle_share_of(subject);
    // **Las dos fracciones, y basta con que una caiga.** La primera versión
    // miraba sólo las mallas y no disparó en el caso que la motivó: desde el
    // mirador canónico el bosque es el 15% de las mallas y el **0%** de los
    // triángulos, porque la pradera se lleva el 92%. Un tema que no pone costo
    // no se puede medir aunque esté en cuadro.
    if meshes.min(triangles) < SUBJECT_FLOOR {
        warn!(
            "[bench] ESTA CORRIDA NO PUEDE MEDIR SU TEMA: '{}' es {:.0}% de las mallas \
             y {:.0}% de los triángulos en cuadro. Sus deltas van a salir del tamaño \
             del ruido, que es indistinguible de 'no cuesta nada' — hay que reautorear \
             el mirador (F4) o medir este tema en su propia caja.",
            subject.label(),
            meshes * 100.0,
            triangles * 100.0,
        );
    }
}

fn report(results: &[StepResult], vantage: Option<(Vec3, Vec3)>, context: &RunContext) {
    let Some(first) = results.first() else {
        return;
    };
    // A delta against an invalid baseline is a fabricated number wearing the
    // costume of a measurement. Without a usable baseline the run reports its
    // absolute values and says so, rather than inviting a false comparison.
    let baseline = (!first.invalid).then_some(first);
    info!(
        "[bench] ================ {} ================",
        context.suite.label()
    );
    info!("[bench] pregunta: {}", context.suite.question());
    info!(
        "[bench] escena={} perfil={} ventana={}x{} msaa={} render={}",
        context.scene,
        context.profile,
        context.window.x,
        context.window.y,
        context.msaa,
        context.render_scale,
    );
    info!(
        "[bench] pasto: densidad={} alcance={} · sombras: mapa={} rango={} hoja={}",
        context.grass_density,
        context.grass_reach,
        context.shadow_map,
        context.shadow_range,
        context.leaf_shadows,
    );
    if let Some((position, facing)) = vantage {
        info!(
            "[bench] mirador pos=({:.1},{:.1},{:.1}) facing=({:.2},{:.2},{:.2}) — \
             los valores absolutos sólo comparan contra una corrida del mismo lugar",
            position.x, position.y, position.z, facing.x, facing.y, facing.z
        );
    }
    report_framing(context);
    // Un asset que no carga **abarata** la corrida: lo que no se dibuja no se
    // paga. Una tabla de milisegundos así no es optimista, es de otra escena — y
    // es peor que una foto rota, porque una foto rota se ve.
    if context.broken_assets > 0 {
        warn!(
            "[bench] ESTOS NÚMEROS NO SIRVEN: {} assets no cargaron. Lo que no se \
             dibuja no cuesta, así que esta tabla mide una escena incompleta.",
            context.broken_assets
        );
    }
    info!("[bench] ---- resultados (ms, menos es mejor) ----");
    info!(
        "[bench] {:<20} {:>9} {:>9} {:>9} {:>8} {:>9} {:>8} {:>6}",
        "step", "frame", "min", "max", "d-frame", "gpu", "d-gpu", "n"
    );
    for result in results {
        if result.invalid {
            info!(
                "[bench] {:<20} INVÁLIDO (se movió, miró para otro lado, o no hubo muestras)",
                result.name
            );
            continue;
        }
        let frame_delta =
            match delta_against(baseline.map(|base| base.frame_mean), result.frame_mean) {
                Some(delta) => format!("{delta:+8.2}"),
                None => format!("{:>8}", "n/a"),
            };
        // El delta de GPU es la columna que sobrevive cuando la presentación
        // clava el frame, que es la mitad de las corridas en esta máquina.
        let gpu_baseline = baseline
            .filter(|base| base.gpu_samples > 0)
            .map(|base| base.gpu_mean);
        let gpu_delta = match (result.gpu_samples > 0)
            .then(|| delta_against(gpu_baseline, result.gpu_mean))
            .flatten()
        {
            Some(delta) => format!("{delta:+8.2}"),
            None => format!("{:>8}", "n/a"),
        };
        let gpu = if result.gpu_samples > 0 {
            format!("{:>9.2}", result.gpu_mean)
        } else {
            format!("{:>9}", "n/a")
        };
        info!(
            "[bench] {:<20} {:>9.2} {:>9.2} {:>9.2} {frame_delta} {gpu} {gpu_delta} {:>6}",
            result.name, result.frame_mean, result.frame_min, result.frame_max, result.samples
        );
    }
    if baseline.is_none() {
        warn!("[bench] baseline INVÁLIDO — sin deltas; sólo valores absolutos");
    }

    if let (Some(first), Some(last)) = (baseline, results.last())
        && !last.invalid
    {
        let drift = last.frame_mean - first.frame_mean;
        let noise_floor = drift.abs();
        info!(
            "[bench] deriva entre los dos baselines: {drift:+.2} ms — todo delta de frame menor que {noise_floor:.2} ms es ruido",
        );
    }
    warn_if_presentation_capped(results);
}

/// Avisa cuando el **frame está clavado por la presentación** y sus deltas no
/// significan nada.
///
/// Pasó en la primera corrida automática, el 2026-08-06: once pasos que iban de
/// 2,15 a 7,98 ms de GPU reportaron todos 16,66-16,67 ms de frame. Con el
/// compositor sincronizando la presentación, el frame mide el ritmo de la
/// pantalla y no el trabajo que se hizo — y una tabla así **invita a concluir
/// que nada cuesta nada**, que es peor que no medir.
///
/// El criterio no nombra ningún refresh: si el trabajo de GPU se mueve mucho
/// entre pasos y el frame casi nada, el frame no está midiendo el trabajo. Eso
/// vale a 60 Hz, a 144 y en una máquina que no conocemos.
fn warn_if_presentation_capped(results: &[StepResult]) {
    if !presentation_capped(results) {
        return;
    }
    warn!(
        "[bench] el frame no siguió a la GPU: se movió mucho menos que ella entre pasos. \
         **Leé la columna d-gpu y descartá d-frame.** O la presentación lo está clavando \
         (típico con la ventana en segundo plano) o la CPU lo está topando; este criterio \
         no distingue cuál, y para leer la tabla da igual."
    );
}

/// El criterio, aparte para poder probarlo.
///
/// **Compara los dos recorridos en milisegundos, no en porcentaje**, y esa es la
/// tercera versión. Las dos anteriores fallaron por el mismo motivo de fondo —
/// medir "cuánto se movió" en relativo:
///
/// 1. Umbrales sueltos (frame <5%, GPU >20%) dejaron pasar una corrida con la
///    GPU al 203% y el frame al 6,3%: falló por un decimal en uno de los dos.
/// 2. La razón entre porcentajes marcó **clavada una corrida sana**, porque el
///    frame tiene un piso —no baja de lo que la CPU tarde— y la GPU no. Con un
///    piso, el porcentaje del frame se achica solo y la razón se dispara aunque
///    el frame esté siguiendo perfectamente bien.
///
/// En milisegundos no hay ese sesgo: si la GPU se movió 5 ms y el frame 4, el
/// frame está midiendo el trabajo; si la GPU se movió 5 y el frame 0,03, no.
///
/// Y el aviso **no afirma la causa**, porque este criterio no la distingue: un
/// frame que no sigue a la GPU puede estar clavado por la presentación o topado
/// por la CPU. Las dos cosas quieren la misma respuesta del lector — leer la
/// columna de GPU — así que decir las dos es más honesto que elegir una.
fn presentation_capped(results: &[StepResult]) -> bool {
    /// Cuántas veces más tiene que haberse movido la GPU que el frame, **en
    /// milisegundos**. Contra las cuatro corridas del 2026-08-06: las dos sanas
    /// dan 1,3 y 2,1; las dos clavadas, 4,8 y 194.
    const SWING_RATIO: f64 = 3.0;
    /// Debajo de esto la GPU no se movió lo suficiente como para que la
    /// comparación signifique algo. Una suite cuyos pasos no mueven la GPU
    /// (`shadows` desde el mirador de hoy) no está clavada: no tiene nada que
    /// medir, que es distinto.
    const MIN_GPU_SWING_MS: f64 = 0.5;

    let spread = |values: &mut dyn Iterator<Item = f64>| {
        let (mut low, mut high) = (f64::MAX, 0.0_f64);
        for value in values {
            low = low.min(value);
            high = high.max(value);
        }
        (low <= high).then_some(high - low)
    };
    let valid: Vec<&StepResult> = results
        .iter()
        .filter(|step| !step.invalid && step.gpu_samples > 0)
        .collect();
    if valid.len() < 3 {
        return false;
    }
    let (Some(frame_swing), Some(gpu_swing)) = (
        spread(&mut valid.iter().map(|step| step.frame_mean)),
        spread(&mut valid.iter().map(|step| step.gpu_mean)),
    ) else {
        return false;
    };
    if gpu_swing < MIN_GPU_SWING_MS {
        return false;
    }
    // Un frame perfectamente plano contra una GPU que se mueve es el caso más
    // claro de todos, y dividir por cero no debería perdérselo.
    frame_swing <= 0.0 || gpu_swing / frame_swing > SWING_RATIO
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Las reglas de las matrices (un eje por paso, baseline al principio y al
    /// final) viven con las matrices, en `suite.rs`. Acá se prueba el **motor**:
    /// que aplique un paso entero, que restaure lo que encontró, y que no deje
    /// un diagnóstico visual prendido contaminando las muestras.

    #[test]
    fn benchmark_steps_disable_visual_diagnostics() {
        let mut toggles = PerfToggles {
            wireframe: true,
            overdraw: true,
            ..default()
        };

        apply_step(&mut toggles, &BenchSuite::General.steps()[0]);

        assert!(!toggles.wireframe);
        assert!(!toggles.overdraw);
    }

    /// El motor tiene que aplicar **todos** los ejes de un paso. El agujero que
    /// esto cierra ya pasó una vez: `apply_step` copiaba cinco campos y la
    /// matriz declaraba seis, así que un paso pedía una cosa y medía otra sin
    /// que nada fallara. Se compara contra el juego entero de perillas que un
    /// paso puede mover.
    #[test]
    fn applying_a_step_moves_every_axis_the_step_declares() {
        for suite in BenchSuite::ALL {
            for step in suite.steps() {
                let mut toggles = PerfToggles::default();
                apply_step(&mut toggles, step);
                assert_eq!(toggles.forest_visible, step.forest_visible, "{}", step.name);
                assert_eq!(toggles.sun_shadows, step.sun_shadows, "{}", step.name);
                assert_eq!(toggles.moon_shadows, step.moon_shadows, "{}", step.name);
                assert_eq!(toggles.cull_step, step.cull_step, "{}", step.name);
                assert_eq!(
                    toggles.shadow_range_step, step.shadow_range_step,
                    "{}",
                    step.name
                );
                assert_eq!(
                    toggles.shadow_map_step, step.shadow_map_step,
                    "{}",
                    step.name
                );
                assert_eq!(toggles.leaf_shadows, step.leaf_shadows, "{}", step.name);
                assert_eq!(
                    toggles.grass_density_step, step.grass_density_step,
                    "{}",
                    step.name
                );
                assert_eq!(
                    toggles.grass_reach_step, step.grass_reach_step,
                    "{}",
                    step.name
                );
                assert_eq!(
                    toggles.render_scale_step, step.render_scale_step,
                    "{}",
                    step.name
                );
                assert_eq!(toggles.msaa_step, step.msaa_step, "{}", step.name);
            }
        }
    }

    #[test]
    fn every_finalization_path_restores_the_exact_render_configuration() {
        let restore = PerfToggles {
            vsync: true,
            wireframe: true,
            overdraw: false,
            tree_detail: true,
            ..default()
        };
        let mut benchmark = Benchmark {
            run: Some(RunState {
                restore,
                ..default()
            }),
            ..default()
        };
        let mut active = PerfToggles {
            vsync: false,
            wireframe: false,
            overdraw: false,
            ..default()
        };

        finalize_benchmark(
            &mut benchmark,
            &mut active,
            12.0,
            FinishReason::Aborted("test abort"),
        );

        assert_eq!(active, restore);
        assert!(!benchmark.is_running());
        assert_eq!(
            benchmark.finished.as_ref().and_then(|run| run.aborted),
            Some("test abort")
        );
    }

    #[test]
    fn completed_timings_with_broken_assets_still_fail_as_evidence() {
        let restore = PerfToggles::default();
        let mut benchmark = Benchmark {
            run: Some(RunState {
                restore,
                results: vec![result(8.0, 6.0)],
                context: RunContext {
                    broken_assets: 1,
                    ..default()
                },
                ..default()
            }),
            ..default()
        };
        let mut active = restore;

        finalize_benchmark(&mut benchmark, &mut active, 12.0, FinishReason::Completed);

        assert_eq!(
            benchmark.finished.as_ref().and_then(|run| run.aborted),
            Some("assets failed to load")
        );
    }

    #[test]
    fn losing_the_camera_aborts_instead_of_stranding_the_run() {
        let restore = PerfToggles {
            vsync: true,
            overdraw: true,
            ..default()
        };
        let mut app = App::new();
        app.insert_resource(Time::<Real>::default())
            .init_resource::<bevy::diagnostic::DiagnosticsStore>()
            .init_resource::<SceneInventory>()
            .init_resource::<crate::perf::shot::BrokenAssets>()
            .insert_resource(Benchmark {
                run: Some(RunState {
                    restore,
                    ..default()
                }),
                ..default()
            })
            .insert_resource(PerfToggles {
                vsync: false,
                overdraw: false,
                ..default()
            })
            .add_systems(Update, advance_benchmark);

        app.update();

        assert_eq!(*app.world().resource::<PerfToggles>(), restore);
        let benchmark = app.world().resource::<Benchmark>();
        assert!(!benchmark.is_running());
        assert_eq!(
            benchmark.finished.as_ref().and_then(|run| run.aborted),
            Some("camera missing or ambiguous")
        );
    }

    fn result(frame_mean: f64, gpu_mean: f64) -> StepResult {
        StepResult {
            name: "step",
            frame_mean,
            frame_min: frame_mean,
            frame_max: frame_mean,
            gpu_mean,
            samples: 240,
            gpu_samples: 240,
            invalid: false,
        }
    }

    /// The bug this guards: an invalid baseline still has a `frame_mean`, so
    /// subtracting from it produced a fabricated number formatted exactly like
    /// a real measurement. A run without a usable baseline must withhold
    /// deltas, not invent them.
    #[test]
    fn no_delta_is_reported_without_a_valid_baseline() {
        assert_eq!(delta_against(None, 18.45), None);
        assert_eq!(delta_against(Some(24.23), 18.45), Some(18.45 - 24.23));
    }

    /// La corrida automática del 2026-08-06: once pasos con la GPU entre 2,15 y
    /// 7,98 ms y **todos** los frames en 16,66-16,67. Sin este aviso la tabla
    /// invita a concluir que ningún paso cuesta nada.
    #[test]
    fn a_frame_pinned_by_presentation_is_called_out() {
        let pinned: Vec<StepResult> = [(16.67, 6.30), (16.66, 2.31), (16.67, 7.98)]
            .into_iter()
            .map(|(frame, gpu)| result(frame, gpu))
            .collect();
        assert!(presentation_capped(&pinned));

        // El caso que la primera versión dejó pasar, con sus números reales: la
        // GPU se movió 203% y el frame 6,3%, o sea justo por encima del 5% que
        // aquel criterio exigía. Un frame clavado con jitter sigue estando
        // clavado.
        let jittery: Vec<StepResult> = [(16.64, 5.43), (16.10, 2.39), (17.11, 7.24)]
            .into_iter()
            .map(|(frame, gpu)| result(frame, gpu))
            .collect();
        assert!(presentation_capped(&jittery));

        // Y una suite cuyos pasos no mueven la GPU no está clavada: no tiene
        // nada que medir, que es distinto. `shadows` del 2026-08-06.
        let flat: Vec<StepResult> = [(16.67, 4.13), (16.65, 3.68), (16.67, 4.14)]
            .into_iter()
            .map(|(frame, gpu)| result(frame, gpu))
            .collect();
        assert!(!presentation_capped(&flat));

        // **El falso positivo que costó la segunda versión del criterio.** Una
        // corrida sana con la máquina limpia: la GPU se movió de 1,55 a 6,81 y
        // el frame de 4,83 a 8,88, o sea que lo siguió. En porcentaje el frame
        // parecía plano —tiene un piso de CPU y la GPU no— y el aviso saltaba.
        let healthy_with_a_floor: Vec<StepResult> =
            [(5.68, 4.17), (4.83, 2.30), (8.88, 6.81), (6.47, 1.55)]
                .into_iter()
                .map(|(frame, gpu)| result(frame, gpu))
                .collect();
        assert!(!presentation_capped(&healthy_with_a_floor));

        // Y una corrida sana no dispara el aviso: acá el frame sigue a la GPU.
        let healthy: Vec<StepResult> = [(7.47, 5.82), (5.51, 4.90), (5.80, 4.25)]
            .into_iter()
            .map(|(frame, gpu)| result(frame, gpu))
            .collect();
        assert!(!presentation_capped(&healthy));
    }

    #[test]
    fn a_step_with_no_samples_is_invalid_rather_than_zero() {
        let empty = summarise("empty", &StepSamples::default());
        assert!(empty.invalid);
        assert_eq!(empty.samples, 0);
    }

    #[test]
    fn missing_gpu_diagnostics_are_unavailable_not_a_zero_cost() {
        let cpu_only = summarise(
            "cpu-only",
            &StepSamples {
                frame_ms: vec![16.0, 16.5],
                ..default()
            },
        );

        assert!(!cpu_only.invalid);
        assert_eq!(cpu_only.gpu_samples, 0);
        assert!(!presentation_capped(&[
            cpu_only,
            result(16.0, 2.0),
            result(16.0, 8.0),
        ]));
    }

    #[test]
    fn a_rejected_start_is_visible_to_the_automation_driver() {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<crate::scene::AppState>()
            .init_resource::<Benchmark>()
            .init_resource::<crate::perf::Flythrough>()
            .init_resource::<PerfToggles>()
            .init_resource::<crate::visuals::terrain_material::TerrainDebugState>()
            .init_resource::<Time<Real>>()
            .add_message::<BenchmarkRequest>()
            .add_systems(Update, start_requested_runs);
        app.world_mut().write_message(BenchmarkRequest::new(
            BenchSuite::General,
            VantageMode::Here,
        ));

        app.update();

        let benchmark = app.world().resource::<Benchmark>();
        let finished = benchmark.finished.as_ref().expect("rejection must finish");
        assert_eq!(finished.aborted, Some("camera missing or ambiguous"));
        assert!(!benchmark.is_running());
    }

    #[test]
    fn a_canonical_run_enters_its_declared_scene_before_measuring() {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<crate::scene::AppState>()
            .init_resource::<Benchmark>()
            .init_resource::<crate::perf::Flythrough>()
            .init_resource::<PerfToggles>()
            .init_resource::<crate::visuals::terrain_material::TerrainDebugState>()
            .init_resource::<Time<Real>>()
            .add_message::<BenchmarkRequest>()
            .add_systems(Update, start_requested_runs);
        app.world_mut().write_message(BenchmarkRequest::new(
            BenchSuite::Grass,
            VantageMode::Canonical,
        ));

        app.update();
        app.update();

        assert_eq!(
            *app.world()
                .resource::<State<crate::scene::AppState>>()
                .get(),
            crate::scene::AppState::Scene(crate::scene::SceneId::Grass)
        );
        let benchmark = app.world().resource::<Benchmark>();
        assert!(benchmark.pending.is_some());
        assert!(benchmark.run.is_none());
        assert!(benchmark.finished.is_none());
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
