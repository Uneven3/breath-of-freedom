//! Debug suite: one snapshot, two sinks, plus the fixed-tick trace stream.
//!
//! Read-only over gameplay state (§7): it never mutates simulation data — it
//! only reads facts/state and draws/logs. The one exception is flipping its
//! own capture switches (`CastTrace.enabled`, avian's `PhysicsGizmos`).
//!
//! The split exists because the console and the screen answer different
//! questions and must not disagree: [`collect`] gathers state into a
//! [`snapshot::DebugSnapshot`] of pure data, then [`hud`] renders it to screen
//! and [`console`] writes it to the log. Neither sink formats anything itself.
//!
//! [`trace`] stays separate: it is a per-fixed-tick event stream (transitions,
//! flips, casts), not a snapshot of the present, so it goes straight to the
//! log without passing through the snapshot.
//!
//! Everything is reached from two panels (see `presentation::debug_ui`): the
//! **F1** hub (channels, render knobs, one-shot actions — including the bokobo
//! and horse spawns that used to be bare F7/F8 keys — and the scripted
//! benchmark) and the **F2** readout menu (which real-time groups the overlay
//! draws). Twelve unlabelled function keys were not a design, and they had run
//! out of room. The keys that remain are the ones a menu cannot serve:
//! **[** / **]** cycle animation clips while the browser is open, and **P**
//! dumps the current snapshot to the log so a moment can be marked without
//! opening a modal over the thing being observed.

pub mod channel;
mod collect;
mod console;
mod gizmos;
mod hud;
pub mod snapshot;
mod toggles;
mod trace;

use std::time::Duration;

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy::time::common_conditions::on_timer;

use crate::movement::MovementSet;
use crate::movement::proposal::ProposalBuffer;

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

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugConfig>();
        app.init_resource::<SimTick>();
        app.init_resource::<snapshot::DebugSnapshot>();
        app.init_resource::<snapshot::HudVisibility>();
        // FPS / frame-time source for the perf section.
        app.add_plugins(FrameTimeDiagnosticsPlugin::default());

        app.add_systems(
            Startup,
            (hud::spawn_debug_text, toggles::apply_initial_toggles),
        );

        // Collect first, then render: both sinks read the same snapshot in the
        // same frame, which is the invariant the whole split exists to hold.
        app.add_systems(
            Update,
            (
                (
                    collect::collect_vitals,
                    collect::collect_locomotion,
                    collect::collect_contact,
                    collect::collect_combat,
                    collect::collect_mount,
                    // A full-scene scan is heavier than the single-actor
                    // collectors; throttling to 4 Hz keeps the measurement tool
                    // from showing up in the frame time it exists to measure.
                    collect::collect_scene.run_if(on_timer(Duration::from_millis(250))),
                    collect::collect_perf,
                    collect::collect_toggles,
                ),
                (
                    hud::render_hud,
                    console::log_periodic,
                    console::log_on_change,
                    console::log_on_demand,
                ),
            )
                .chain(),
        );

        app.add_message::<channel::DebugChannelToggle>();
        app.add_message::<channel::DebugActionRequest>();
        app.add_message::<channel::HudSectionToggle>();
        app.add_systems(
            Update,
            (
                toggles::apply_channel_toggles,
                toggles::apply_debug_actions,
                toggles::apply_hud_section_toggles,
                toggles::cycle_animation_clips,
                gizmos::draw_sensor_gizmos,
            ),
        );

        app.add_systems(
            FixedUpdate,
            (
                advance_tick.before(MovementSet::ReadIntents),
                trace::log_ground_flips
                    .after(MovementSet::SenseWorld)
                    .before(MovementSet::GatherProposals),
                trace::log_shape_casts
                    .after(MovementSet::SenseWorld)
                    .before(MovementSet::GatherProposals),
                trace::capture_proposals
                    .after(MovementSet::GatherProposals)
                    .before(MovementSet::Arbitrate),
                trace::log_transitions.after(MovementSet::Arbitrate),
                trace::log_verbose_tick.after(MovementSet::TickActiveMotor),
                trace::log_context_fact_flips.after(MovementSet::TickActiveMotor),
            ),
        );
    }
}

fn advance_tick(mut tick: ResMut<SimTick>) {
    tick.0 += 1;
}
