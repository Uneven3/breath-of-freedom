//! Gathers gameplay and render state into the [`DebugSnapshot`]. This is the
//! only place that turns values into strings; the HUD and the console sinks
//! only arrange what they find here.

use bevy::camera::visibility::{ViewVisibility, VisibilityRange};
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::mesh::Mesh3d;
use bevy::window::PrimaryWindow;

use super::snapshot::{DebugSnapshot, Field, SectionId};
use super::{DebugConfig, SimTick};
use crate::inventory::{Inventory, ItemKind, WeaponDurability};
use crate::perf::budget::{SceneInventory, scene_budget_grade};
use crate::perf::{PerfKnob, PerfToggles, gpu_pass_costs};
use crate::visuals::DiagnosticViewState;
use crate::visuals::material_registry::{SceneCensus, Subject, mesh_triangles};
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
use bof_domain::movement::state::{LocomotionEnabled, LocomotionState};
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
    player: Query<VitalsReport, With<Player>>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    let Ok((stamina, hp, inventory, weapon)) = player.single() else {
        snapshot.clear(SectionId::Vitals);
        return;
    };
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
    Has<LocomotionEnabled>,
);

pub(super) fn collect_locomotion(
    player: Query<LocomotionReport, With<Player>>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    let Ok((state, vel, ground, facing, intents, enabled)) = player.single() else {
        snapshot.clear(SectionId::Locomotion);
        return;
    };
    if !enabled {
        snapshot.set(SectionId::Locomotion, vec![Field::new("status", "paused")]);
        return;
    }
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
    player: Query<ContactReport, With<Player>>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    let Ok((contact, stairs, ladder, ledge)) = player.single() else {
        snapshot.clear(SectionId::Contact);
        return;
    };
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
    player: Query<CombatReport, With<Player>>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    let Ok((combat, draw)) = player.single() else {
        snapshot.clear(SectionId::Combat);
        return;
    };
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

/// Static scene inventory — the numbers a mobile budget is actually spent on,
/// distinct from the frame cost in `perf`. `draws` is a **lower-bound estimate**
/// of distinct `(mesh, material)` pairs among visible entities. The render
/// world can split those pairs further by pipeline, lightmap, allocator slab or
/// disabled batching; without render-world instrumentation this is not a draw
/// call counter. All fields are
/// volatile — they drift as the camera moves, so change-triggered console output
/// ignores them.
///
/// **Lo que este sistema ya no hace es enumerar tipos de material.** Eso vive en
/// `visuals::material_registry`, donde cada tipo se engancha al registrarse;
/// acá sólo se lee el resultado. La versión anterior tenía la lista escrita a
/// mano y por eso la pradera estuvo fuera del presupuesto desde que existe.
#[allow(clippy::too_many_arguments)]
pub(super) fn collect_scene(
    all_meshes: Query<AnySceneMesh>,
    ranged: Query<&ViewVisibility, With<VisibilityRange>>,
    mesh_assets: Res<Assets<Mesh>>,
    census: Res<SceneCensus>,
    perf: Res<PerfToggles>,
    diagnostic: Res<DiagnosticViewState>,
    mut inventory: ResMut<SceneInventory>,
    mut snapshot: ResMut<DebugSnapshot>,
    mut warned: Local<bool>,
) {
    // The diagnostic replaces StandardMaterial handles, so its temporary
    // render representation is not a valid production budget sample.
    if perf.overdraw || diagnostic.overdraw_material_override {
        if *inventory != SceneInventory::default() {
            *inventory = SceneInventory::default();
        }
        snapshot.clear(SectionId::Scene);
        return;
    }
    // **Triángulos y mallas visibles se cuentan acá y sólo acá**: un `Mesh3d` lo
    // tiene todo lo que se dibuja, sin mirar el material, así que es la única
    // cuenta que no puede quedarse corta. Justamente por eso sirve de patrón
    // contra el censo, que sí necesita el tipo.
    let mut visible_meshes = 0u32;
    let mut triangles = 0usize;
    for (visibility, mesh3d) in &all_meshes {
        if !visibility.get() {
            continue;
        }
        visible_meshes += 1;
        triangles += mesh_assets.get(&mesh3d.0).map_or(0, mesh_triangles);
    }

    let accounted = census.accounted_meshes();
    if accounted < visible_meshes && !*warned {
        *warned = true;
        warn!(
            "[budget] {visible_meshes} mallas visibles y sólo {accounted} contabilizadas: \
             hay un material sin `add_instrumented_material`, y ni sus draws ni su \
             atribución se cuentan"
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
        draws: census.draws(),
        materials: census.materials(),
        ranged_culled,
        ranged_total,
        subjects: census.tallies(),
    };
    if *inventory != scene {
        *inventory = scene;
    }

    let mut fields = vec![
        Field::volatile("meshes", scene.visible_meshes.to_string()),
        Field::volatile("tris", kilo(scene.triangles)),
        Field::volatile("draws~", scene.draws.to_string()),
        Field::volatile("mats", scene.materials.to_string()),
        Field::volatile("budget", scene_budget_grade(&scene).label()),
        Field::volatile(
            "lod_cull",
            format!("{}/{}", scene.ranged_culled, scene.ranged_total),
        ),
    ];
    // El reparto, y sólo de quien esté en cuadro: una fila en cero por cada
    // sistema que la escena no tiene es ruido, y el overlay se lee de un
    // vistazo o no se lee.
    for subject in Subject::ALL {
        let tally = scene.subject(subject);
        if tally.meshes == 0 {
            continue;
        }
        fields.push(Field::volatile(
            subject.label(),
            format!("{} tris {} draws", kilo(tally.triangles), tally.draws),
        ));
    }
    snapshot.set(SectionId::Scene, fields);
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
    if let Some(gpu_ms) = gpu_ms {
        fields.push(Field::volatile("gpu", format!("{gpu_ms:.2}ms")));
        for pass in passes.iter().take(4) {
            fields.push(Field::volatile(
                pass.name.clone(),
                format!("{:.2}ms", pass.millis),
            ));
        }
    } else {
        // An adapter without timestamp queries must say so; a zero here would
        // read as "the GPU is free" and send the whole A/B down a false trail.
        fields.push(Field::volatile("gpu", "unavailable"));
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
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::{Vec3, World};

    use super::{DebugSnapshot, Field, SectionId, collect_locomotion, format_clock, kilo};
    use bof_domain::movement::BodyVelocity;
    use bof_domain::movement::Player;
    use bof_domain::movement::facing::FacingSource;
    use bof_domain::movement::facts::GroundFacts;
    use bof_domain::movement::intents::Intents;
    use bof_domain::movement::state::LocomotionState;

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

    #[test]
    fn disabled_player_locomotion_is_reported_as_paused_not_as_stale_facts() {
        let mut world = World::new();
        world.init_resource::<DebugSnapshot>();
        world.spawn((
            Player,
            LocomotionState::Walk,
            BodyVelocity(Vec3::X * 4.0),
            GroundFacts {
                grounded: true,
                ..Default::default()
            },
            FacingSource::default(),
            Intents::default(),
        ));

        world.run_system_once(collect_locomotion).unwrap();

        assert_eq!(
            world
                .resource::<DebugSnapshot>()
                .line(SectionId::Locomotion)
                .as_deref(),
            Some("locomotion: status=paused")
        );
    }

    #[test]
    fn absent_player_clears_the_previous_locomotion_report() {
        let mut world = World::new();
        let mut snapshot = DebugSnapshot::default();
        snapshot.set(
            SectionId::Locomotion,
            vec![Field::new("state", "stale walk")],
        );
        world.insert_resource(snapshot);

        world.run_system_once(collect_locomotion).unwrap();

        assert!(
            world
                .resource::<DebugSnapshot>()
                .get(SectionId::Locomotion)
                .is_none()
        );
    }
}
