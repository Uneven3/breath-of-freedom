//! Gathers gameplay and render state into the [`DebugSnapshot`]. This is the
//! only place that turns values into strings; the HUD and the console sinks
//! only arrange what they find here.

use bevy::asset::{AssetId, UntypedAssetId};
use bevy::camera::visibility::{ViewVisibility, VisibilityRange};
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::pbr::{Material, MeshMaterial3d};
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::render::mesh::Mesh3d;
use bevy::window::PrimaryWindow;

use super::snapshot::{DebugSnapshot, Field, SectionId};
use super::{DebugConfig, SimTick};
use crate::inventory::{Inventory, ItemKind, WeaponDurability};
use crate::perf::budget::{SceneInventory, scene_budget_grade};
use crate::perf::{PerfKnob, PerfToggles, gpu_pass_costs};
use crate::visuals::DiagnosticViewState;
use crate::visuals::grass_material::GrassMaterial;
use crate::visuals::terrain_material::TerrainMaterial;
use crate::world::day_night::TimeOfDay;
use bof_domain::combat::state::CombatState;
use bof_domain::combat::state::DrawStrength;
use bof_domain::health::Health;
use bof_domain::mounts::{Horse, HorseCharge, RiddenBy};
use bof_domain::movement::facing::FacingSource;
use bof_domain::movement::facts::{BodyContact, GroundFacts, LadderFacts, LedgeFacts, StairsFacts};
use bof_domain::movement::intents::Intents;
use bof_domain::movement::probe_data::TraversalProbe;
use bof_domain::movement::stamina::Stamina;
use bof_domain::movement::state::LocomotionState;
use bof_domain::movement::{BodyVelocity, Player};

// Each section has its own focused producer (§1): a system reads exactly the
// components its slot needs and writes only that slot (§7). Adding a debug datum
// is one more `Field` or one small system — never a change to a shared monolith.
// The player-focused producers deliberately do not reach for the horse: it is a
// separate actor with its own producer (`collect_mount`).

type VitalsReport<'a> = (
    &'a Stamina,
    &'a Health,
    &'a Inventory,
    Option<&'a WeaponDurability>,
);

pub(super) fn collect_vitals(
    player: Single<VitalsReport, With<Player>>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    let (stamina, hp, inventory, weapon) = *player;
    let weapon_status = match weapon {
        Some(durability) => format!(
            "{} {}/{}",
            durability.label(),
            durability.current(),
            durability.max()
        ),
        None => "unarmed".to_string(),
    };
    let materials: u32 = inventory
        .iter()
        .filter(|stack| matches!(stack.kind, ItemKind::Material(_)))
        .map(|stack| stack.quantity)
        .sum();
    let food: u32 = inventory
        .iter()
        .filter(|stack| matches!(stack.kind, ItemKind::Food { .. }))
        .map(|stack| stack.quantity)
        .sum();

    snapshot.set(
        SectionId::Vitals,
        vec![
            Field::volatile("hp", format!("{:.0}/{:.0}", hp.current(), hp.max())),
            Field::volatile(
                "stamina",
                format!("{:.0}/{:.0}", stamina.current(), stamina.max()),
            ),
            Field::new("weapon", weapon_status),
            Field::new("materials", materials.to_string()),
            Field::new("food", food.to_string()),
        ],
    );
}

type LocomotionReport<'a> = (
    &'a LocomotionState,
    &'a BodyVelocity,
    &'a GroundFacts,
    &'a FacingSource,
    &'a Intents,
);

pub(super) fn collect_locomotion(
    player: Single<LocomotionReport, With<Player>>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    let (state, vel, ground, facing, intents) = *player;
    let v = vel.0;
    let facing = match facing {
        FacingSource::Free => "free".to_owned(),
        FacingSource::Look => "look".to_owned(),
        FacingSource::LockOn(target) => format!("lockon({target})"),
    };
    snapshot.set(
        SectionId::Locomotion,
        vec![
            Field::new("state", format!("{state:?}")),
            Field::new("facing", facing),
            Field::new("strafe", format!("{:?}", intents.planar.strafe_dir())),
            Field::volatile("vel", format!("({:.2},{:.2},{:.2})", v.x, v.y, v.z)),
            Field::volatile("speed", format!("{:.2}", v.length())),
            Field::flag("grounded", ground.grounded),
            Field::flag("probe", ground.probe_hit),
            Field::flag("slope_ok", ground.slope_ok),
            // Discrete, so it lands on screen *and* in the change log the moment
            // you walk from one painted patch onto another. That is the only
            // end-to-end proof the terrain's semantic layer reaches simulation:
            // the footstep sound is presentation's job and there are no clips
            // loaded yet, so without this the data arrives silently.
            Field::new("surface", format!("{:?}", ground.surface)),
            Field::volatile("ascend_dot", format!("{:.3}", ground.ascend_dot)),
        ],
    );
}

type ContactReport<'a> = (
    &'a BodyContact,
    &'a StairsFacts,
    &'a LadderFacts,
    &'a LedgeFacts,
);

pub(super) fn collect_contact(
    player: Single<ContactReport, With<Player>>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    let (contact, stairs, ladder, ledge) = *player;
    let n = ledge.climb_normal.unwrap_or(Vec3::ZERO);
    snapshot.set(
        SectionId::Contact,
        vec![
            Field::flag("slide_wall", contact.on_wall),
            Field::flag("stairs", stairs.on_stairs),
            Field::flag("ladder", ladder.on_ladder),
            Field::flag("climb", ledge.can_climb),
            Field::flag("continue", ledge.can_continue_climb),
            Field::new(
                "side",
                format!("{}/{}", ledge.has_wall_left, ledge.has_wall_right),
            ),
            Field::volatile("normal", format!("({:.2},{:.2},{:.2})", n.x, n.y, n.z)),
            Field::volatile("lip", format!("{:.2}", ledge.lip_height)),
            Field::flag("mantle_edge", ledge.is_at_mantle_edge),
            Field::flag("vault", ledge.is_vaultable),
        ],
    );
}

type CombatReport<'a> = (&'a CombatState, &'a DrawStrength);

pub(super) fn collect_combat(
    player: Single<CombatReport, With<Player>>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    let (combat, draw) = *player;
    snapshot.set(
        SectionId::Combat,
        vec![
            Field::new("state", format!("{combat:?}")),
            Field::volatile("draw", format!("{:.0}%", draw.factor * 100.0)),
        ],
    );
}

type HorseReport<'a> = (
    &'a LocomotionState,
    &'a BodyVelocity,
    &'a HorseCharge,
    &'a Health,
    &'a Stamina,
    &'a RiddenBy,
    &'a GroundFacts,
);

/// The mount is a separate actor, so the player-focused producers never saw it —
/// the blind spot this section closes. Reports the ridden horse (or the first
/// spawned) and clears the slot when no horse exists, so it never lingers stale
/// after a despawn.
pub(super) fn collect_mount(
    horses: Query<HorseReport, With<Horse>>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    let report = horses
        .iter()
        .find(|(.., ridden, _)| ridden.0.is_some())
        .or_else(|| horses.iter().next());
    let Some((state, vel, charge, hp, stamina, ridden, ground)) = report else {
        snapshot.clear(SectionId::Mount);
        return;
    };
    let v = vel.0;
    snapshot.set(
        SectionId::Mount,
        vec![
            Field::flag("ridden", ridden.0.is_some()),
            Field::new("state", format!("{state:?}")),
            // El caballo sensa el suelo como cualquier actor, pero el `surface`
            // de la sección de locomoción es el del **player**, que al montar
            // pierde `LocomotionEnabled` y deja de actualizarse: montado se lee
            // congelado en el último paso a pie. Éste es el del caballo.
            Field::flag("grounded", ground.grounded),
            Field::new("surface", format!("{:?}", ground.surface)),
            Field::volatile("speed", format!("{:.2}", Vec3::new(v.x, 0.0, v.z).length())),
            Field::flag("charge", charge.active),
            Field::new("charge_gen", charge.generation.to_string()),
            Field::volatile("hp", format!("{:.0}/{:.0}", hp.current(), hp.max())),
            Field::volatile(
                "stamina",
                format!("{:.0}/{:.0}", stamina.current(), stamina.max()),
            ),
        ],
    );
}

/// Toda malla dibujada, sin mirar su material.
///
/// **Triángulos y mallas visibles se cuentan acá y sólo acá**, y eso es el
/// arreglo del 2026-08-06: antes había una consulta por tipo de material, así
/// que un material nuevo desaparecía del presupuesto sin que nada avisara.
/// `GrassMaterial` fue el tercero y estuvo invisible desde que existe — el
/// grader calificaba la escena sin la pradera, que es lo más caro que hay en
/// ella. Un `Mesh3d` lo tiene todo lo que se dibuja, así que preguntarle a él es
/// lo único que no puede quedarse corto.
type AnySceneMesh<'a> = (&'a ViewVisibility, &'a Mesh3d);

/// Una malla con su material, para lo único que sí necesita el tipo: agrupar por
/// `(malla, material)`, que es como Bevy batchea.
type TypedSceneMesh<'a, M> = (&'a ViewVisibility, &'a Mesh3d, &'a MeshMaterial3d<M>);

/// Cuenta lotes y materiales de un tipo, en conjuntos *untyped* compartidos.
///
/// Untyped para que los tres tipos sumen en el mismo par de conjuntos: así
/// `draws` es un número y no una suma que hay que acordarse de extender.
/// Devuelve cuántas mallas visibles vio, que es con lo que
/// [`collect_scene`] detecta un material sin contabilizar.
fn tally<M: Material>(
    query: &Query<TypedSceneMesh<M>>,
    batches: &mut HashSet<(AssetId<Mesh>, UntypedAssetId)>,
    materials: &mut HashSet<UntypedAssetId>,
) -> u32 {
    let mut seen = 0;
    for (visibility, mesh3d, material) in query {
        if !visibility.get() {
            continue; // Frustum-, hierarchy- or range-culled: never submitted.
        }
        seen += 1;
        batches.insert((mesh3d.0.id(), material.0.id().untyped()));
        materials.insert(material.0.id().untyped());
    }
    seen
}

/// Static scene inventory — the numbers a mobile budget is actually spent on,
/// distinct from the frame cost in `perf`. `draws` counts distinct
/// `(mesh, material)` pairs among visible entities: Bevy batches by exactly
/// that, so it approximates the draw-call count without a private wgpu hook and
/// drops the moment shared handles let the batcher instance. Covers the shipped
/// `StandardMaterial` path plus the production layered terrain material. All
/// fields are volatile — they drift as the camera moves, so change-triggered
/// console output ignores them.
#[allow(clippy::too_many_arguments)]
pub(super) fn collect_scene(
    all_meshes: Query<AnySceneMesh>,
    standard: Query<TypedSceneMesh<StandardMaterial>>,
    terrain: Query<TypedSceneMesh<TerrainMaterial>>,
    meadow: Query<TypedSceneMesh<GrassMaterial>>,
    ranged: Query<&ViewVisibility, With<VisibilityRange>>,
    mesh_assets: Res<Assets<Mesh>>,
    perf: Res<PerfToggles>,
    diagnostic: Res<DiagnosticViewState>,
    mut inventory: ResMut<SceneInventory>,
    mut snapshot: ResMut<DebugSnapshot>,
    mut warned: Local<bool>,
) {
    // The diagnostic replaces StandardMaterial handles, so its temporary
    // render representation is not a valid production budget sample.
    if perf.overdraw || diagnostic.overdraw_material_override {
        return;
    }
    let mut visible_meshes = 0u32;
    let mut triangles = 0usize;
    for (visibility, mesh3d) in &all_meshes {
        if !visibility.get() {
            continue;
        }
        visible_meshes += 1;
        if let Some(mesh) = mesh_assets.get(&mesh3d.0) {
            triangles += match mesh.indices() {
                Some(indices) => indices.len() / 3,
                // Non-indexed meshes list every vertex per triangle.
                None => mesh.count_vertices() / 3,
            };
        }
    }

    let mut batches: HashSet<(AssetId<Mesh>, UntypedAssetId)> = HashSet::default();
    let mut materials: HashSet<UntypedAssetId> = HashSet::default();
    let accounted = tally(&standard, &mut batches, &mut materials)
        + tally(&terrain, &mut batches, &mut materials)
        + tally(&meadow, &mut batches, &mut materials);

    // El guardia que faltaba. `draws` y `materials` siguen necesitando el tipo,
    // o sea que siguen siendo una lista que alguien tiene que extender — pero
    // ahora olvidarse hace ruido en vez de mentir en silencio durante meses.
    if accounted < visible_meshes && !*warned {
        *warned = true;
        warn!(
            "[budget] {} mallas visibles y sólo {accounted} con material contabilizado: \
             hay un tipo de material fuera de `collect_scene`, y sus draws no se cuentan",
            visible_meshes,
        );
    }

    // The distance-LOD ledger: how many range-gated meshes the camera dropped
    // this frame, so the cull can be trusted to be working rather than assumed.
    let mut ranged_total = 0u32;
    let mut ranged_culled = 0u32;
    for visibility in &ranged {
        ranged_total += 1;
        if !visibility.get() {
            ranged_culled += 1;
        }
    }

    let scene = SceneInventory {
        visible_meshes,
        triangles,
        draws: batches.len(),
        materials: materials.len(),
        ranged_culled,
        ranged_total,
    };
    if *inventory != scene {
        *inventory = scene;
    }

    snapshot.set(
        SectionId::Scene,
        vec![
            Field::volatile("meshes", scene.visible_meshes.to_string()),
            Field::volatile("tris", kilo(scene.triangles)),
            Field::volatile("draws", scene.draws.to_string()),
            Field::volatile("mats", scene.materials.to_string()),
            Field::volatile("budget", scene_budget_grade(&scene).label()),
            Field::volatile(
                "lod_cull",
                format!("{}/{}", scene.ranged_culled, scene.ranged_total),
            ),
        ],
    );
}

/// Raw triangle digits are unreadable at scene scale; abbreviate over 10k so
/// the overlay stays glanceable (`142.3k`) while small counts stay exact.
fn kilo(n: usize) -> String {
    if n >= 10_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

fn format_clock(hours: f32, speed: f32) -> String {
    let whole_hours = hours.floor();
    let minutes = ((hours - whole_hours) * 60.0).floor();
    if speed > 1.0 {
        format!("{whole_hours:02.0}:{minutes:02.0} x{speed:.0}")
    } else {
        format!("{whole_hours:02.0}:{minutes:02.0}")
    }
}

/// Frame cost plus the benchmark knobs. Kept in one section so a console line
/// is self-describing: the numbers and the configuration that produced them
/// never get separated.
pub(super) fn collect_perf(
    diagnostics: Res<DiagnosticsStore>,
    perf: Res<PerfToggles>,
    window: Single<&Window, With<PrimaryWindow>>,
    tick: Res<SimTick>,
    time_of_day: Res<TimeOfDay>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let frame_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);

    let clock = format_clock(time_of_day.hours, time_of_day.speed);

    let mut fields = vec![
        Field::volatile("fps", format!("{fps:.1}")),
        Field::volatile("frame", format!("{frame_ms:.2}ms")),
        Field::new("profile", perf.profile.label()),
        Field::new("present", format!("{:?}", window.present_mode)),
        Field::volatile("tick", format!("{:06}", tick.0)),
        Field::volatile("time", clock),
    ];

    let (passes, gpu_ms) = gpu_pass_costs(&diagnostics);
    if passes.is_empty() {
        // An adapter without timestamp queries must say so; a zero here would
        // read as "the GPU is free" and send the whole A/B down a false trail.
        fields.push(Field::volatile("gpu", "unavailable"));
    } else {
        fields.push(Field::volatile("gpu", format!("{gpu_ms:.2}ms")));
        for pass in passes.iter().take(4) {
            fields.push(Field::volatile(
                pass.name.clone(),
                format!("{:.2}ms", pass.millis),
            ));
        }
    }

    for knob in PerfKnob::ALL {
        fields.push(Field::new(knob.label(), perf.knob_value(knob)));
    }

    snapshot.set(SectionId::Perf, fields);
}

/// Only the actual toggles live here. Anything that moves on its own — the
/// tick counter, the clock — belongs in the volatile perf section: this
/// section drives change-triggered console output, and a monotonic counter in
/// it would emit a line every single frame.
pub(super) fn collect_toggles(
    config: Res<DebugConfig>,
    probe_alive: Query<(), With<TraversalProbe>>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    snapshot.set(
        SectionId::Toggles,
        vec![
            Field::flag("colliders", config.show_colliders),
            Field::flag("casts", config.show_casts),
            Field::flag("log:transitions", config.log_transitions),
            Field::flag("log:trace", config.log_verbose),
            Field::flag("log:flips", config.log_fact_flips),
            Field::flag("probe", !probe_alive.is_empty()),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::{format_clock, kilo};

    #[test]
    fn kilo_keeps_small_counts_exact_and_abbreviates_large_ones() {
        assert_eq!(kilo(0), "0");
        assert_eq!(kilo(9_999), "9999"); // still exact just under the threshold
        assert_eq!(kilo(10_000), "10.0k"); // first abbreviated value
        assert_eq!(kilo(142_300), "142.3k");
    }

    #[test]
    fn clock_keeps_two_digit_hours_and_minutes() {
        assert_eq!(format_clock(8.5, 1.0), "08:30");
        assert_eq!(format_clock(18.25, 4.0), "18:15 x4");
    }
}
