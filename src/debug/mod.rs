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
//! [`mod@trace`] stays separate: it is a per-fixed-tick event stream (transitions,
//! flips, casts), not a snapshot of the present, so it goes straight to the
//! log without passing through the snapshot.
//!
//! Everything is reached from two panels (see `presentation::debug_ui`): the
//! **F1** hub (channels, render knobs, one-shot actions — including the bokobo
//! and horse spawns that used to be bare F7/F8 keys — and the scripted
//! benchmark) and the **F2** readout menu (which real-time groups the overlay
//! draws). Twelve unlabelled function keys were not a design, and they had run
//! out of room. **P** remains as the one direct debug key: it dumps the current
//! snapshot to the log so a moment can be marked without opening a modal over
//! the thing being observed.

pub mod channel;
mod collect;
mod console;
mod gizmos;
mod hud;
mod material_report;
pub mod snapshot;
mod toggles;
mod trace;

pub use material_report::{
    MAX_MATERIAL_ROWS, MaterialBreakdownSnapshot, MaterialLookRow, MaterialReportNotice,
};

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;

use bof_domain::movement::MovementSet;
use bof_domain::movement::proposal::ProposalBuffer;

use crate::visuals::material_registry::{CensusSet, census_is_open};

/// Which debug channels are active. Mirrored into `CastTrace.enabled` and
/// avian's `PhysicsGizmos` by `handle_toggles`.
///
/// **Everything defaults off.** A log that is always on is a log nobody reads:
/// the console sink alone was 208 of the 240 lines of a real playtest, which
/// buried the 28 that were about the game. What a channel costs is in
/// [`DebugChannel::hint`](channel::DebugChannel::hint); what it is *for* is a
/// question you are asking on purpose, so you turn it on when you ask it.
#[derive(Resource, Default)]
pub struct DebugConfig {
    pub show_colliders: bool,
    pub show_casts: bool,
    pub log_perf_samples: bool,
    pub log_state_changes: bool,
    pub log_transitions: bool,
    pub log_verbose: bool,
    pub log_fact_flips: bool,
}

impl DebugConfig {
    /// `BOF_DEBUG=flips,transitions cargo run` — the channels the hub toggles,
    /// asked for before the window exists. A name nobody answers to is an
    /// error naming the valid ones, never a silent nothing (same rule as
    /// `BOF_SCENE`).
    pub fn from_env() -> Self {
        let mut config = Self::default();
        let Ok(raw) = std::env::var("BOF_DEBUG") else {
            return config;
        };
        for key in raw.split(',').map(str::trim).filter(|k| !k.is_empty()) {
            match channel::DebugChannel::from_env_key(key) {
                Some(channel) => config.enable(channel),
                None => error!(
                    "[debug] BOF_DEBUG={key} no nombra ningún canal; hay: {}",
                    channel::DebugChannel::env_keys()
                ),
            }
        }
        config
    }

    fn enable(&mut self, channel: channel::DebugChannel) {
        use channel::DebugChannel as C;
        let flag = match channel {
            C::Colliders => &mut self.show_colliders,
            C::Casts => &mut self.show_casts,
            C::LogPerfSamples => &mut self.log_perf_samples,
            C::LogStateChanges => &mut self.log_state_changes,
            C::LogTransitions => &mut self.log_transitions,
            C::LogVerbose => &mut self.log_verbose,
            C::LogFactFlips => &mut self.log_fact_flips,
        };
        *flag = true;
    }
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
        app.insert_resource(DebugConfig::from_env());
        app.init_resource::<SimTick>();
        app.init_resource::<snapshot::DebugSnapshot>();
        app.init_resource::<snapshot::HudVisibility>();
        app.init_resource::<material_report::MaterialReportNotice>();
        app.init_resource::<material_report::MaterialBreakdownSnapshot>();
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
                    // collectors, así que corre a 4 Hz — pero la cadencia no es
                    // suya: la fija la ventana del censo
                    // (`visuals::material_registry`), porque contar y publicar
                    // tienen que ser **el mismo frame**. Dos `on_timer` del
                    // mismo período no disparan juntos: cada uno lleva su reloj.
                    collect::collect_scene
                        .in_set(CensusSet::Read)
                        .run_if(census_is_open),
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
                material_report::log_material_breakdown,
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
