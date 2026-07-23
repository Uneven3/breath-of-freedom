//! Gathers gameplay and render state into the [`DebugSnapshot`]. This is the
//! only place that turns values into strings; the HUD and the console sinks
//! only arrange what they find here.

use bevy::asset::AssetId;
use bevy::camera::visibility::{ViewVisibility, VisibilityRange};
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::pbr::MeshMaterial3d;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::render::mesh::Mesh3d;
use bevy::window::PrimaryWindow;

use super::snapshot::{DebugSnapshot, Field, SectionId};
use super::{DebugConfig, SimTick};
use crate::combat::motors::aim::DrawStrength;
use crate::combat::state::CombatState;
use crate::health::Health;
use crate::inventory::{Inventory, ItemKind, WeaponDurability};
use crate::mounts::data::{Horse, HorseCharge, RiddenBy};
use crate::movement::facts::{BodyContact, GroundFacts, LadderFacts, LedgeFacts, StairsFacts};
use crate::movement::probe_data::TraversalProbe;
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{BodyVelocity, Player};
use crate::perf::{PerfKnob, PerfToggles, gpu_pass_costs};
use crate::visuals::{AnimationDebug, PlayerAnimations};
use crate::world::day_night::TimeOfDay;

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

type LocomotionReport<'a> = (&'a LocomotionState, &'a BodyVelocity, &'a GroundFacts);

pub(super) fn collect_locomotion(
    player: Single<LocomotionReport, With<Player>>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    let (state, vel, ground) = *player;
    let v = vel.0;
    snapshot.set(
        SectionId::Locomotion,
        vec![
            Field::new("state", format!("{state:?}")),
            Field::volatile("vel", format!("({:.2},{:.2},{:.2})", v.x, v.y, v.z)),
            Field::volatile("speed", format!("{:.2}", v.length())),
            Field::flag("grounded", ground.grounded),
            Field::flag("probe", ground.probe_hit),
            Field::flag("slope_ok", ground.slope_ok),
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
        .find(|(.., ridden)| ridden.0.is_some())
        .or_else(|| horses.iter().next());
    let Some((state, vel, charge, hp, stamina, ridden)) = report else {
        snapshot.clear(SectionId::Mount);
        return;
    };
    let v = vel.0;
    snapshot.set(
        SectionId::Mount,
        vec![
            Field::flag("ridden", ridden.0.is_some()),
            Field::new("state", format!("{state:?}")),
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

type SceneMesh<'a> = (
    &'a ViewVisibility,
    &'a Mesh3d,
    &'a MeshMaterial3d<StandardMaterial>,
);

/// Static scene inventory — the numbers a mobile budget is actually spent on,
/// distinct from the frame cost in `perf`. `draws` counts distinct
/// `(mesh, material)` pairs among visible entities: Bevy batches by exactly
/// that, so it approximates the draw-call count without a private wgpu hook and
/// drops the moment shared handles let the batcher instance. Covers the shipped
/// `StandardMaterial` path (world, foliage, imported glTF); experimental custom
/// materials are out of frame. All fields are volatile — they drift as the
/// camera moves, so change-triggered console output ignores them.
pub(super) fn collect_scene(
    meshes: Query<SceneMesh>,
    ranged: Query<&ViewVisibility, With<VisibilityRange>>,
    mesh_assets: Res<Assets<Mesh>>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    let mut visible_meshes = 0u32;
    let mut triangles = 0usize;
    let mut batches: HashSet<(AssetId<Mesh>, AssetId<StandardMaterial>)> = HashSet::default();
    let mut materials: HashSet<AssetId<StandardMaterial>> = HashSet::default();

    for (visibility, mesh3d, material) in &meshes {
        if !visibility.get() {
            continue; // Frustum-, hierarchy- or range-culled: never submitted.
        }
        visible_meshes += 1;
        batches.insert((mesh3d.0.id(), material.0.id()));
        materials.insert(material.0.id());
        if let Some(mesh) = mesh_assets.get(&mesh3d.0) {
            triangles += match mesh.indices() {
                Some(indices) => indices.len() / 3,
                // Non-indexed meshes list every vertex per triangle.
                None => mesh.count_vertices() / 3,
            };
        }
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

    snapshot.set(
        SectionId::Scene,
        vec![
            Field::volatile("meshes", visible_meshes.to_string()),
            Field::volatile("tris", kilo(triangles)),
            Field::volatile("draws", batches.len().to_string()),
            Field::volatile("mats", materials.len().to_string()),
            Field::volatile("lod_cull", format!("{ranged_culled}/{ranged_total}")),
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

    let hours = time_of_day.hours.floor() as u32;
    let minutes = ((time_of_day.hours - hours as f32) * 60.0).floor() as u32;
    let clock = if time_of_day.speed > 1.0 {
        format!("{hours:02}:{minutes:02} x{:.0}", time_of_day.speed)
    } else {
        format!("{hours:02}:{minutes:02}")
    };

    let mut fields = vec![
        Field::volatile("fps", format!("{fps:.1}")),
        Field::volatile("frame", format!("{frame_ms:.2}ms")),
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
    anim_debug: Res<AnimationDebug>,
    anims: Option<Res<PlayerAnimations>>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    let anim_status = match (&anims, anim_debug.enabled) {
        (Some(anims), true) if !anims.clips.is_empty() => {
            let index = anim_debug.index % anims.clips.len();
            format!(
                "{}/{} {}",
                index + 1,
                anims.clips.len(),
                anims.clips[index].0
            )
        }
        _ => "off".to_string(),
    };

    snapshot.set(
        SectionId::Toggles,
        vec![
            Field::flag("colliders", config.show_colliders),
            Field::flag("casts", config.show_casts),
            Field::flag("log:transitions", config.log_transitions),
            Field::flag("log:trace", config.log_verbose),
            Field::flag("log:flips", config.log_fact_flips),
            Field::flag("probe", !probe_alive.is_empty()),
            Field::new("anim", anim_status),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::kilo;

    #[test]
    fn kilo_keeps_small_counts_exact_and_abbreviates_large_ones() {
        assert_eq!(kilo(0), "0");
        assert_eq!(kilo(9_999), "9999"); // still exact just under the threshold
        assert_eq!(kilo(10_000), "10.0k"); // first abbreviated value
        assert_eq!(kilo(142_300), "142.3k");
    }
}
