//! Debug suite: HUD, structured logging, and sensor visualization.
//!
//! Read-only over gameplay state (Constitution §7): it never mutates
//! simulation data — it only reads facts/state and draws/logs. The one
//! exception is flipping its own capture switches (`CastTrace.enabled`,
//! avian's `PhysicsGizmos`).
//!
//! Toggles (Update-schedule key edges):
//! - **F1** — avian collider wireframes (`PhysicsDebugPlugin`).
//! - **F2** — sensor gizmos: every shapecast/raycast the services actually
//!   issued this tick (ground probe, 6 ledge casts, mantle/vault down-casts,
//!   lateral rays), stairs/ladder trigger volumes, velocity / floor-normal /
//!   wall-normal arrows.
//! - **F3** — transition + grounded-flip logging (`info!`), with the full
//!   proposal list of the tick and the decomposed grounded conditions.
//! - **F4** — compact per-tick trace plus every sensor shape cast (`info!`).
//! - **F5** — semantic context-fact flip log: emits only when stairs, ladder,
//!   or ledge booleans change, without the per-tick or per-cast noise.
//! - **F6** — spawn/despawn the TraversalProbe dummy AI near the player.

use avian3d::prelude::*;
use bevy::color::palettes::css;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use std::fmt::Write;

use crate::movement::diag::{CastKind, CastTrace};
use crate::movement::facts::{BodyContact, GroundFacts, LadderFacts, LedgeFacts, StairsFacts};
use crate::movement::probe_data::TraversalProbe;
use crate::movement::proposal::ProposalBuffer;
use crate::movement::sensing::GroundSensing;
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity, MovementSet, Player};
use crate::world::{Ladder, Stairs};

/// Which debug channels are active. Mirrored into `CastTrace.enabled` and
/// avian's `PhysicsGizmos` by `handle_toggles`.
#[derive(Resource, Default)]
pub struct DebugConfig {
    pub show_colliders: bool,
    pub show_casts: bool,
    pub log_transitions: bool,
    pub log_verbose: bool,
    pub log_fact_flips: bool,
}

/// Fixed-tick counter so log lines from the same tick can be correlated.
#[derive(Resource, Default)]
pub struct SimTick(pub u64);

/// Snapshot of an actor's `ProposalBuffer` taken right before `Arbitrate`
/// clears it, so the transition log can show who competed.
#[derive(Component, Default)]
pub struct ProposalTrace(pub ProposalBuffer);

#[derive(Component)]
struct DebugText;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugConfig>();
        app.init_resource::<SimTick>();

        app.add_systems(Startup, (spawn_debug_text, apply_initial_toggles));

        app.add_systems(
            Update,
            (handle_toggles, update_debug_text, draw_sensor_gizmos),
        );

        app.add_systems(
            FixedUpdate,
            (
                advance_tick.before(MovementSet::ReadIntents),
                log_ground_flips
                    .after(MovementSet::SenseWorld)
                    .before(MovementSet::GatherProposals),
                log_shape_casts
                    .after(MovementSet::SenseWorld)
                    .before(MovementSet::GatherProposals),
                capture_proposals
                    .after(MovementSet::GatherProposals)
                    .before(MovementSet::Arbitrate),
                log_transitions.after(MovementSet::Arbitrate),
                log_verbose_tick.after(MovementSet::TickActiveMotor),
                log_context_fact_flips.after(MovementSet::TickActiveMotor),
            ),
        );
    }
}

fn advance_tick(mut tick: ResMut<SimTick>) {
    tick.0 += 1;
}

// ---------------------------------------------------------------------------
// Toggles
// ---------------------------------------------------------------------------

fn apply_initial_toggles(
    config: Res<DebugConfig>,
    mut trace: ResMut<CastTrace>,
    mut store: ResMut<GizmoConfigStore>,
) {
    trace.enabled = config.show_casts || config.log_verbose;
    store.config_mut::<PhysicsGizmos>().0.enabled = config.show_colliders;
}

fn handle_toggles(
    keys: Res<ButtonInput<KeyCode>>,
    mut config: ResMut<DebugConfig>,
    mut trace: ResMut<CastTrace>,
    mut store: ResMut<GizmoConfigStore>,
) {
    if keys.just_pressed(KeyCode::F1) {
        config.show_colliders = !config.show_colliders;
        store.config_mut::<PhysicsGizmos>().0.enabled = config.show_colliders;
    }
    if keys.just_pressed(KeyCode::F2) {
        config.show_casts = !config.show_casts;
    }
    if keys.just_pressed(KeyCode::F3) {
        config.log_transitions = !config.log_transitions;
        info!(
            "[debug] transition/grounded logging: {}",
            config.log_transitions
        );
    }
    if keys.just_pressed(KeyCode::F4) {
        config.log_verbose = !config.log_verbose;
        info!("[debug] verbose per-tick trace: {}", config.log_verbose);
    }
    if keys.just_pressed(KeyCode::F5) {
        config.log_fact_flips = !config.log_fact_flips;
        info!(
            "[debug] context-fact flip logging: {}",
            config.log_fact_flips
        );
    }
    trace.enabled = config.show_casts || config.log_verbose;
}

// ---------------------------------------------------------------------------
// Structured logging
// ---------------------------------------------------------------------------

/// Log every `GroundFacts.grounded` flip with the decomposed reason — this is
/// the primary tool for "why did I fall?" investigations.
type GroundFlipQuery<'a> = (
    Entity,
    &'a GroundFacts,
    &'a BodyVelocity,
    &'a GroundSensing,
    Option<&'a Name>,
);

fn log_ground_flips(
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
fn log_shape_casts(
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
fn capture_proposals(
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
fn log_transitions(
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
fn log_verbose_tick(
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
struct ContextFlags {
    stairs: bool,
    ladder: bool,
    can_climb: bool,
    continue_climb: bool,
    wall_left: bool,
    wall_right: bool,
}

#[allow(clippy::type_complexity)]
fn log_context_fact_flips(
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

/// Draw the sensor casts recorded by the services this fixed tick, the
/// stairs/ladder trigger volumes, and per-actor state arrows.
#[allow(clippy::type_complexity)]
fn draw_sensor_gizmos(
    config: Res<DebugConfig>,
    trace: Res<CastTrace>,
    mut gizmos: Gizmos,
    actors: Query<
        (
            &Transform,
            &BodyVelocity,
            &GroundFacts,
            &BodyContact,
            &crate::movement::body::BodyDimensions,
            Option<&StairsFacts>,
        ),
        With<Actor>,
    >,
    stairs: Query<&Stairs>,
    ladders: Query<&Ladder>,
) {
    if !config.show_casts {
        return;
    }

    // --- Recorded sensor casts: line to max range, sphere + normal on hit ---
    for rec in &trace.records {
        let color = cast_color(rec.label);
        let end = rec.origin + rec.dir * rec.max_dist;
        match rec.hit {
            Some((point, normal)) => {
                gizmos.line(rec.origin, point, color);
                // Remaining (unreached) range, dimmed.
                gizmos.line(point, end, color.with_alpha(0.15));
                gizmos.sphere(point, 0.05, color);
                gizmos.arrow(point, point + normal * 0.35, css::HOT_PINK);
            }
            None => gizmos.line(rec.origin, end, color.with_alpha(0.35)),
        }
    }

    // --- Trigger volumes (AABB overlap regions, not physics colliders) ---
    for s in &stairs {
        gizmos.cube(
            Transform::from_translation(s.trigger_center).with_scale(s.trigger_half_extents * 2.0),
            css::DODGER_BLUE,
        );
        // Authored slope line base → top.
        gizmos.line(s.base, s.top, css::DODGER_BLUE);
    }
    for l in &ladders {
        gizmos.cube(
            Transform::from_translation(l.trigger_center).with_scale(l.trigger_half_extents * 2.0),
            css::MEDIUM_PURPLE,
        );
        gizmos.line(l.bottom, l.top, css::MEDIUM_PURPLE);
    }

    // --- Per-actor arrows ---
    for (transform, vel, ground, contact, body, stairs_facts) in &actors {
        let pos = transform.translation;
        // Velocity (scaled down to stay readable).
        if vel.0.length_squared() > 0.001 {
            gizmos.arrow(pos, pos + vel.0 * 0.25, css::GOLD);
        }
        // Floor normal from the feet, green when grounded, red when not.
        let feet = pos - Vec3::Y * body.standing_half_height();
        let n_color = if ground.grounded { css::LIME } else { css::RED };
        gizmos.arrow(feet, feet + ground.floor_normal * 0.6, n_color);
        // Wall contact normal.
        if contact.on_wall {
            gizmos.arrow(pos, pos + contact.wall_normal * 0.6, css::ORANGE_RED);
        }
        // Where the stairs motor believes the feet should be.
        if let Some(sf) = stairs_facts
            && sf.on_stairs
        {
            let mut expected = pos;
            expected.y = crate::movement::motors::stairs::expected_feet_y(sf, pos);
            gizmos.sphere(expected, 0.08, css::DODGER_BLUE);
        }
    }
}

/// Stable color per sensor family so casts are tellable-apart at a glance.
fn cast_color(label: &str) -> Color {
    match label {
        "ground_probe" => css::LIME.into(),
        "mantle_down" => css::AQUA.into(),
        "vault_down" => css::ORANGE.into(),
        "wall_ray_left" | "wall_ray_right" => css::YELLOW.into(),
        // The 6 forward profiling casts.
        _ => css::WHITE.into(),
    }
}

// ---------------------------------------------------------------------------
// HUD
// ---------------------------------------------------------------------------

fn spawn_debug_text(mut commands: Commands) {
    commands.spawn((
        DebugText,
        Text::new("…"),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            ..default()
        },
    ));
}

#[allow(clippy::type_complexity)]
fn update_debug_text(
    player: Single<
        (
            &LocomotionState,
            &Stamina,
            &BodyVelocity,
            &GroundFacts,
            &BodyContact,
            &StairsFacts,
            &LadderFacts,
            &LedgeFacts,
        ),
        With<Player>,
    >,
    tick: Res<SimTick>,
    config: Res<DebugConfig>,
    mut text: Single<&mut Text, With<DebugText>>,
    probe_alive: Query<(), With<TraversalProbe>>,
) {
    let (state, stamina, vel, ground, contact, stairs, ladder, ledge) = *player;
    let speed = vel.0.length();
    let onoff = |b: bool| if b { "ON " } else { "off" };
    let probe_status = if probe_alive.is_empty() { "off" } else { "ON " };
    text.0 = format!(
        "state: {:?}   [t{:06}]\n\
         stamina: {:.0}/{:.0}\n\
         vel: ({:.2}, {:.2}, {:.2})  |v|={:.2}\n\
         grounded: {}  (probe={} slope={} ascend_dot={:.3})\n\
         slide_wall: {}  stairs: {}  ladder: {}\n\
         ledge: climb={} cont={} side={}/{} n=({:.2},{:.2},{:.2}) lip={:.2} mantle_edge={} vault={}\n\
         [F1] colliders:{}  [F2] casts:{}  [F3] log:{}  [F4] trace:{}  [F5] flips:{}  [F6] probe:{}",
        state,
        tick.0,
        stamina.current(),
        stamina.max(),
        vel.0.x,
        vel.0.y,
        vel.0.z,
        speed,
        ground.grounded,
        ground.probe_hit,
        ground.slope_ok,
        ground.ascend_dot,
        contact.on_wall,
        stairs.on_stairs,
        ladder.on_ladder,
        ledge.can_climb,
        ledge.can_continue_climb,
        ledge.has_wall_left,
        ledge.has_wall_right,
        ledge.climb_normal.unwrap_or(Vec3::ZERO).x,
        ledge.climb_normal.unwrap_or(Vec3::ZERO).y,
        ledge.climb_normal.unwrap_or(Vec3::ZERO).z,
        ledge.lip_height,
        ledge.is_at_mantle_edge,
        ledge.is_vaultable,
        onoff(config.show_colliders),
        onoff(config.show_casts),
        onoff(config.log_transitions),
        onoff(config.log_verbose),
        onoff(config.log_fact_flips),
        probe_status,
    );
}

