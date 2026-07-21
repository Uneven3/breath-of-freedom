//! Fixed-tick trace stream.
//!
//! Unlike the snapshot sinks, this is an *event* stream: transitions, fact
//! flips and the shape casts actually issued on a given tick. Those describe
//! moments, not the present state, so they go straight to the log instead of
//! passing through `DebugSnapshot` — a snapshot would only ever show the last
//! one and lose the rest.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use std::fmt::Write;

use super::{DebugConfig, ProposalTrace, SimTick};
use crate::movement::diag::{CastKind, CastTrace};
use crate::movement::facts::{BodyContact, GroundFacts, LadderFacts, LedgeFacts, StairsFacts};
use crate::movement::proposal::ProposalBuffer;
use crate::movement::sensing::GroundSensing;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity};

/// the primary tool for "why did I fall?" investigations.
type GroundFlipQuery<'a> = (
    Entity,
    &'a GroundFacts,
    &'a BodyVelocity,
    &'a GroundSensing,
    Option<&'a Name>,
);

pub(super) fn log_ground_flips(
    config: Res<DebugConfig>,
    tick: Res<SimTick>,
    q: Query<GroundFlipQuery, With<Actor>>,
    mut prev: Local<HashMap<Entity, bool>>,
) {
    if !config.log_transitions {
        return;
    }
    for (entity, ground, vel, sensing, name) in &q {
        let was = prev.insert(entity, ground.grounded);
        if was == Some(ground.grounded) {
            continue;
        }
        let who = name
            .map(|n| n.as_str().to_owned())
            .unwrap_or(format!("{entity:?}"));
        info!(
            "[t{:06}] {who} GROUNDED {} → {} | probe_hit={} slope_ok={} ascend_dot={:.3} (eps {}) vel=({:.2},{:.2},{:.2}) n=({:.2},{:.2},{:.2})",
            tick.0,
            was.map(|b| b.to_string()).unwrap_or("∅".into()),
            ground.grounded,
            ground.probe_hit,
            ground.slope_ok,
            ground.ascend_dot,
            sensing.ascend_epsilon,
            vel.0.x,
            vel.0.y,
            vel.0.z,
            ground.floor_normal.x,
            ground.floor_normal.y,
            ground.floor_normal.z,
        );
    }
}

/// Emit a lossless compact form of the actual sensor shape casts issued this
/// fixed tick. The query configuration is static and keyed by the short code;
/// the actor trace supplies pose/facing, while each hit keeps distance, point,
/// and normal. F4 enables capture even with F2 gizmos disabled.
pub(super) fn log_shape_casts(
    config: Res<DebugConfig>,
    tick: Res<SimTick>,
    trace: Res<CastTrace>,
    actors: Query<(Entity, Option<&Name>), With<Actor>>,
) {
    if !config.log_verbose {
        return;
    }

    for (entity, name) in &actors {
        let mut casts = String::new();
        for (code, label) in SHAPE_CAST_CODES {
            let Some(rec) = trace.records.iter().find(|rec| {
                rec.entity == entity && rec.kind == CastKind::Shape && rec.label == label
            }) else {
                continue;
            };
            if !casts.is_empty() {
                casts.push(' ');
            }
            match rec.hit {
                Some((point, normal)) => {
                    let _ = write!(
                        casts,
                        "{code}=H{:.3}/{:.2},{:.2},{:.2}/{:.2},{:.2},{:.2}",
                        point.distance(rec.origin),
                        point.x,
                        point.y,
                        point.z,
                        normal.x,
                        normal.y,
                        normal.z,
                    );
                }
                None => {
                    let _ = write!(casts, "{code}=-");
                }
            }
        }
        let who = name.map(Name::as_str).unwrap_or("actor");
        info!("[t{:06}] {who} SC {casts}", tick.0,);
    }
}

/// Fixed-order dictionary for the compact `SC` trace. Query origin, direction,
/// range and shape are static for each code; hits retain the dynamic data.
const SHAPE_CAST_CODES: [(&str, &str); 9] = [
    ("g", "ground_probe"),
    ("a", "ledge_ankle"),
    ("k", "ledge_knee"),
    ("w", "ledge_waist"),
    ("c", "ledge_chest"),
    ("l", "ledge_limit"),
    ("h", "ledge_head"),
    ("m", "mantle_down"),
    ("v", "vault_down"),
];

/// Snapshot proposal buffers before `Arbitrate` clears them.
pub(super) fn capture_proposals(
    mut commands: Commands,
    config: Res<DebugConfig>,
    mut q: Query<(Entity, &ProposalBuffer, Option<&mut ProposalTrace>), With<Actor>>,
) {
    if !config.log_transitions {
        return;
    }
    for (entity, buffer, trace) in &mut q {
        match trace {
            Some(mut t) => t.0 = buffer.clone(),
            None => {
                commands
                    .entity(entity)
                    .insert(ProposalTrace(buffer.clone()));
            }
        }
    }
}

/// Log every `LocomotionState` transition together with the tick's competing
/// proposals and the facts that shaped them.
#[allow(clippy::type_complexity)]
pub(super) fn log_transitions(
    config: Res<DebugConfig>,
    tick: Res<SimTick>,
    q: Query<
        (
            Entity,
            &LocomotionState,
            Option<&ProposalTrace>,
            Option<&GroundFacts>,
            Option<&BodyContact>,
            Option<&Name>,
        ),
        (With<Actor>, Changed<LocomotionState>),
    >,
    mut prev: Local<HashMap<Entity, LocomotionState>>,
) {
    if !config.log_transitions {
        return;
    }
    for (entity, state, proposals, ground, contact, name) in &q {
        let old = prev.insert(entity, *state);
        if old == Some(*state) {
            continue; // Changed() also fires on insertion; skip non-changes.
        }
        let who = name
            .map(|n| n.as_str().to_owned())
            .unwrap_or(format!("{entity:?}"));
        let old = old.map(|s| format!("{s:?}")).unwrap_or("∅".into());

        let props = proposals
            .map(|t| {
                t.0.iter()
                    .map(|p| {
                        format!(
                            "{}({:?},{})→{:?}",
                            p.source_id, p.category, p.override_weight, p.target_state
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let ground_str = ground
            .map(|g| {
                format!(
                    " | grounded={} (probe={} slope={} dot={:.3})",
                    g.grounded, g.probe_hit, g.slope_ok, g.ascend_dot
                )
            })
            .unwrap_or_default();
        let wall_str = contact
            .filter(|c| c.on_wall)
            .map(|c| {
                format!(
                    " | on_wall n=({:.2},{:.2},{:.2})",
                    c.wall_normal.x, c.wall_normal.y, c.wall_normal.z
                )
            })
            .unwrap_or_default();

        info!(
            "[t{:06}] {who} {old} → {:?} | props: [{props}]{ground_str}{wall_str}",
            tick.0, state
        );
    }
}

/// One line per actor per fixed tick — heavy (60 lines/s), gated on F4.
#[allow(clippy::type_complexity)]
pub(super) fn log_verbose_tick(
    config: Res<DebugConfig>,
    tick: Res<SimTick>,
    q: Query<
        (
            &LocomotionState,
            &Transform,
            &BodyVelocity,
            &GroundFacts,
            &BodyContact,
            Option<&StairsFacts>,
            Option<&LadderFacts>,
            Option<&LedgeFacts>,
            Option<&Name>,
        ),
        With<Actor>,
    >,
) {
    if !config.log_verbose {
        return;
    }
    for (state, transform, vel, ground, contact, stairs, ladder, ledge, name) in &q {
        let who = name
            .map(|n| n.as_str().to_owned())
            .unwrap_or("actor".into());
        let p = transform.translation;
        let facing = transform.rotation * Vec3::NEG_Z;
        info!(
            "[t{:06}] {who} {:?} pos=({:.2},{:.2},{:.2}) face=({:.2},{:.2},{:.2}) vel=({:.2},{:.2},{:.2}) gnd={}({},{},{:.2}) slide_wall={} stairs={} ladder={} climb={}/{} side={}/{} n=({:.2},{:.2},{:.2}) lip={:.2}",
            tick.0,
            state,
            p.x,
            p.y,
            p.z,
            facing.x,
            facing.y,
            facing.z,
            vel.0.x,
            vel.0.y,
            vel.0.z,
            ground.grounded,
            ground.probe_hit,
            ground.slope_ok,
            ground.ascend_dot,
            contact.on_wall,
            stairs.map(|s| s.on_stairs).unwrap_or(false),
            ladder.map(|l| l.on_ladder).unwrap_or(false),
            ledge.map(|l| l.can_climb).unwrap_or(false),
            ledge.map(|l| l.can_continue_climb).unwrap_or(false),
            ledge.map(|l| l.has_wall_left).unwrap_or(false),
            ledge.map(|l| l.has_wall_right).unwrap_or(false),
            ledge.and_then(|l| l.climb_normal).unwrap_or(Vec3::ZERO).x,
            ledge.and_then(|l| l.climb_normal).unwrap_or(Vec3::ZERO).y,
            ledge.and_then(|l| l.climb_normal).unwrap_or(Vec3::ZERO).z,
            ledge.map(|l| l.lip_height).unwrap_or(0.0),
        );
    }
}

/// Semantic context flags are sensor observations. F5 turns them into a
/// transition stream without dumping all fixed ticks or every shape cast.
///
/// `BodyContact.on_wall` is intentionally excluded: it is a per-sweep solver
/// result, so tangential motion can alternate it while a wall remains nearby.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct ContextFlags {
    stairs: bool,
    ladder: bool,
    can_climb: bool,
    continue_climb: bool,
    wall_left: bool,
    wall_right: bool,
}

#[allow(clippy::type_complexity)]
pub(super) fn log_context_fact_flips(
    config: Res<DebugConfig>,
    tick: Res<SimTick>,
    q: Query<
        (
            Entity,
            &StairsFacts,
            &LadderFacts,
            &LedgeFacts,
            Option<&Name>,
        ),
        With<Actor>,
    >,
    mut previous: Local<HashMap<Entity, ContextFlags>>,
) {
    if !config.log_fact_flips {
        return;
    }

    for (entity, stairs, ladder, ledge, name) in &q {
        let current = ContextFlags {
            stairs: stairs.on_stairs,
            ladder: ladder.on_ladder,
            can_climb: ledge.can_climb,
            continue_climb: ledge.can_continue_climb,
            wall_left: ledge.has_wall_left,
            wall_right: ledge.has_wall_right,
        };
        let old = previous.insert(entity, current);
        let who = name.map(Name::as_str).unwrap_or("actor");

        let Some(old) = old else {
            info!(
                "[t{:06}] {who} FACTS ∅ → stairs={} ladder={} climb={}/{} side={}/{}",
                tick.0,
                current.stairs,
                current.ladder,
                current.can_climb,
                current.continue_climb,
                current.wall_left,
                current.wall_right,
            );
            continue;
        };
        if old == current {
            continue;
        }

        let mut changes = String::new();
        for (label, before, after) in [
            ("stairs", old.stairs, current.stairs),
            ("ladder", old.ladder, current.ladder),
            ("can_climb", old.can_climb, current.can_climb),
            ("continue", old.continue_climb, current.continue_climb),
            ("left", old.wall_left, current.wall_left),
            ("right", old.wall_right, current.wall_right),
        ] {
            if before != after {
                if !changes.is_empty() {
                    changes.push(' ');
                }
                let _ = write!(changes, "{label}={before}→{after}");
            }
        }
        info!("[t{:06}] {who} FACTS {changes}", tick.0);
    }
}

// ---------------------------------------------------------------------------
// Gizmos
// ---------------------------------------------------------------------------
