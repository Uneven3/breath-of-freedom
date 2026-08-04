//! Applies the hub's requests. The one place in `debug` allowed to mutate
//! anything, and only its own capture switches: `CastTrace.enabled`, avian's
//! `PhysicsGizmos` and the clock. Gameplay state is
//! never touched.
//!
//! Requests arrive as messages from `presentation::debug_ui` rather than as
//! key edges, so the set of channels can grow without hunting for a free key.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::DebugConfig;
use super::channel::{
    DebugAction, DebugActionRequest, DebugChannel, DebugChannelToggle, HudSectionToggle,
};
use super::snapshot::HudVisibility;
use crate::world::day_night::TimeOfDayRequest;
use bof_domain::enemies::BokoboSpawnRequest;
use bof_domain::mounts::HorseSpawnRequest;
use bof_domain::movement::diag::CastTrace;

pub(super) fn apply_initial_toggles(
    config: Res<DebugConfig>,
    mut trace: ResMut<CastTrace>,
    mut store: ResMut<GizmoConfigStore>,
) {
    trace.enabled = config.show_casts || config.log_verbose;
    store.config_mut::<PhysicsGizmos>().0.enabled = config.show_colliders;
}

pub(super) fn apply_channel_toggles(
    mut requests: MessageReader<DebugChannelToggle>,
    mut config: ResMut<DebugConfig>,
    mut trace: ResMut<CastTrace>,
    mut store: ResMut<GizmoConfigStore>,
) {
    for DebugChannelToggle(channel) in requests.read().copied() {
        let now = match channel {
            DebugChannel::Colliders => {
                config.show_colliders = !config.show_colliders;
                store.config_mut::<PhysicsGizmos>().0.enabled = config.show_colliders;
                config.show_colliders
            }
            DebugChannel::Casts => {
                config.show_casts = !config.show_casts;
                config.show_casts
            }
            DebugChannel::LogPerfSamples => {
                config.log_perf_samples = !config.log_perf_samples;
                config.log_perf_samples
            }
            DebugChannel::LogStateChanges => {
                config.log_state_changes = !config.log_state_changes;
                config.log_state_changes
            }
            DebugChannel::LogTransitions => {
                config.log_transitions = !config.log_transitions;
                config.log_transitions
            }
            DebugChannel::LogVerbose => {
                config.log_verbose = !config.log_verbose;
                config.log_verbose
            }
            DebugChannel::LogFactFlips => {
                config.log_fact_flips = !config.log_fact_flips;
                config.log_fact_flips
            }
        };
        info!(
            "[debug] {}: {}",
            channel.label(),
            if now { "ON" } else { "off" }
        );
    }

    trace.enabled = config.show_casts || config.log_verbose;
}

pub(super) fn apply_debug_actions(
    mut requests: MessageReader<DebugActionRequest>,
    mut time_of_day: MessageWriter<TimeOfDayRequest>,
    mut probe: MessageWriter<bof_domain::movement::probe_data::ProbeToggleRequest>,
    mut bokobos: MessageWriter<BokoboSpawnRequest>,
    mut horse: MessageWriter<HorseSpawnRequest>,
) {
    for DebugActionRequest(action) in requests.read().copied() {
        match action {
            // Each owning module holds the entity and its request type; debug
            // only translates the hub click into the message it already reads.
            DebugAction::ToggleProbe => {
                probe.write(bof_domain::movement::probe_data::ProbeToggleRequest);
            }
            DebugAction::AdvanceHour => {
                time_of_day.write(TimeOfDayRequest::AdvanceHour);
            }
            DebugAction::ToggleTimeSpeed => {
                time_of_day.write(TimeOfDayRequest::ToggleSpeed);
            }
            DebugAction::ToggleBokobos => {
                bokobos.write(BokoboSpawnRequest::Toggle);
            }
            DebugAction::ToggleHorse => {
                horse.write(HorseSpawnRequest::Toggle);
            }
            // Self-contained diagnostic; owns its own scan in `material_report`.
            DebugAction::MaterialBreakdown => {}
        }
    }
}

/// Applies the F2 readout menu's per-section toggles. The only writer of
/// [`HudVisibility`]; presentation just asks (§7).
pub(super) fn apply_hud_section_toggles(
    mut requests: MessageReader<HudSectionToggle>,
    mut visibility: ResMut<HudVisibility>,
) {
    for HudSectionToggle(section) in requests.read().copied() {
        let now = visibility.toggle(section);
        info!(
            "[debug] hud {}: {}",
            section.title(),
            if now { "shown" } else { "hidden" }
        );
    }
}
