//! Gathers gameplay and render state into the [`DebugSnapshot`]. This is the
//! only place that turns values into strings; the HUD and the console sinks
//! only arrange what they find here.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::snapshot::{DebugSnapshot, Field, SectionId};
use super::{DebugConfig, SimTick};
use crate::combat::motors::aim::DrawStrength;
use crate::combat::state::CombatState;
use crate::inventory::{Inventory, ItemKind, WeaponDurability};
use crate::movement::facts::{BodyContact, GroundFacts, LadderFacts, LedgeFacts, StairsFacts};
use crate::movement::probe_data::TraversalProbe;
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{BodyVelocity, Player};
use crate::perf::{PerfKnob, PerfToggles, gpu_pass_costs};
use crate::visuals::{AnimationDebug, PlayerAnimations};
use crate::world::day_night::TimeOfDay;

type PlayerReport<'a> = (
    &'a LocomotionState,
    &'a CombatState,
    &'a Stamina,
    &'a BodyVelocity,
    &'a GroundFacts,
    &'a BodyContact,
    &'a StairsFacts,
    &'a LadderFacts,
    &'a LedgeFacts,
    &'a DrawStrength,
    &'a crate::health::Health,
    &'a Inventory,
    Option<&'a WeaponDurability>,
);

pub(super) fn collect_player(
    player: Single<PlayerReport, With<Player>>,
    mut snapshot: ResMut<DebugSnapshot>,
) {
    let (
        state,
        combat,
        stamina,
        vel,
        ground,
        contact,
        stairs,
        ladder,
        ledge,
        draw,
        hp,
        inventory,
        weapon,
    ) = *player;

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

    snapshot.set(
        SectionId::Combat,
        vec![
            Field::new("state", format!("{combat:?}")),
            Field::volatile("draw", format!("{:.0}%", draw.factor * 100.0)),
        ],
    );
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
    fields.push(Field::new("F11 selected", perf.selected_label()));

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
