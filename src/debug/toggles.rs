//! Applies the hub's requests. The one place in `debug` allowed to mutate
//! anything, and only its own capture switches: `CastTrace.enabled`, avian's
//! `PhysicsGizmos`, the animation browser and the clock. Gameplay state is
//! never touched.
//!
//! Requests arrive as messages from `presentation::debug_ui` rather than as
//! key edges, so the set of channels can grow without hunting for a free key.
//! The clip-cycling keys stay direct: they only mean anything while the
//! browser is open, and they are a continuous nudge, not a discrete toggle.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::DebugConfig;
use super::channel::{
    DebugAction, DebugActionRequest, DebugChannel, DebugChannelToggle, HudSectionToggle,
};
use super::snapshot::HudVisibility;
use crate::enemies::SpawnBokobosRequest;
use crate::mounts::data::MountDebugRequest;
use crate::movement::diag::CastTrace;
use crate::visuals::{AnimationDebug, PlayerAnimations};
use crate::world::day_night::TimeOfDayRequest;

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
    mut anim_debug: ResMut<AnimationDebug>,
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
            DebugChannel::AnimBrowser => {
                anim_debug.enabled = !anim_debug.enabled;
                anim_debug.enabled
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
    mut probe: MessageWriter<crate::movement::probe_data::ProbeToggleRequest>,
    mut bokobos: MessageWriter<SpawnBokobosRequest>,
    mut horse: MessageWriter<MountDebugRequest>,
) {
    for DebugActionRequest(action) in requests.read().copied() {
        match action {
            // Each owning module holds the entity and its request type; debug
            // only translates the hub click into the message it already reads.
            DebugAction::ToggleProbe => {
                probe.write(crate::movement::probe_data::ProbeToggleRequest);
            }
            DebugAction::AdvanceHour => {
                time_of_day.write(TimeOfDayRequest::AdvanceHour);
            }
            DebugAction::ToggleTimeSpeed => {
                time_of_day.write(TimeOfDayRequest::ToggleSpeed);
            }
            DebugAction::ToggleBokobos => {
                bokobos.write(SpawnBokobosRequest);
            }
            DebugAction::ToggleHorse => {
                horse.write(MountDebugRequest::ToggleHorse);
            }
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

/// Clip cycling while the browser is open.
pub(super) fn cycle_animation_clips(
    keys: Res<ButtonInput<KeyCode>>,
    mut anim_debug: ResMut<AnimationDebug>,
    anims: Option<Res<PlayerAnimations>>,
) {
    let Some(anims) = anims else { return };
    if !anim_debug.enabled || anims.clips.is_empty() {
        return;
    }
    let len = anims.clips.len();
    let forward = keys.just_pressed(KeyCode::BracketRight);
    let back = keys.just_pressed(KeyCode::BracketLeft);
    if !forward && !back {
        return;
    }
    anim_debug.index = if forward {
        (anim_debug.index + 1) % len
    } else {
        (anim_debug.index + len - 1) % len
    };
    info!(
        "[debug] animation clip {}/{}: {}",
        anim_debug.index + 1,
        len,
        anims.clips[anim_debug.index].0
    );
}
