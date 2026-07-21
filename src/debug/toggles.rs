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
use super::channel::{DebugAction, DebugActionRequest, DebugChannel, DebugChannelToggle};
use crate::movement::diag::CastTrace;
use crate::visuals::{AnimationDebug, PlayerAnimations};
use crate::world::day_night::TimeOfDay;

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
    mut time_of_day: ResMut<TimeOfDay>,
    mut probe: MessageWriter<crate::movement::probe_data::ProbeToggleRequest>,
) {
    for DebugActionRequest(action) in requests.read().copied() {
        match action {
            // Movement owns the probe entity and its request type; debug only
            // translates the hub click into it.
            DebugAction::ToggleProbe => {
                probe.write(crate::movement::probe_data::ProbeToggleRequest);
            }
            DebugAction::AdvanceHour => {
                time_of_day.hours = (time_of_day.hours + 1.0).rem_euclid(24.0);
                info!("[debug] time jump: {:05.2}h", time_of_day.hours);
            }
            DebugAction::ToggleTimeSpeed => {
                time_of_day.speed = if time_of_day.speed > 1.0 { 1.0 } else { 120.0 };
                info!("[debug] time speed: x{}", time_of_day.speed);
            }
        }
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
