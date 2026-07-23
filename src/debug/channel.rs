//! The debug channels as data, so the hub can list them instead of every
//! surface hard-coding its own key binding.
//!
//! Twelve function keys with no hierarchy was not a design, it was accumulated
//! history: nothing told you what existed, and the space ran out. One enum
//! plus one message means adding a channel costs a variant, and the UI picks
//! it up for free.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::DebugConfig;
use super::snapshot::SectionId;
use crate::visuals::AnimationDebug;

/// Read-only view of which channels are on, so presentation can render state
/// it is not allowed to mutate (§20). The switches live in two resources; this
/// hides that split from the UI.
#[derive(SystemParam)]
pub struct DebugConfigView<'w> {
    config: Res<'w, DebugConfig>,
    anim: Res<'w, AnimationDebug>,
}

impl DebugConfigView<'_> {
    pub fn is_enabled(&self, channel: DebugChannel) -> bool {
        match channel {
            DebugChannel::Colliders => self.config.show_colliders,
            DebugChannel::Casts => self.config.show_casts,
            DebugChannel::LogTransitions => self.config.log_transitions,
            DebugChannel::LogVerbose => self.config.log_verbose,
            DebugChannel::LogFactFlips => self.config.log_fact_flips,
            DebugChannel::AnimBrowser => self.anim.enabled,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DebugChannel {
    Colliders,
    Casts,
    LogTransitions,
    LogVerbose,
    LogFactFlips,
    AnimBrowser,
}

impl DebugChannel {
    pub const ALL: [DebugChannel; 6] = [
        DebugChannel::Colliders,
        DebugChannel::Casts,
        DebugChannel::LogTransitions,
        DebugChannel::LogVerbose,
        DebugChannel::LogFactFlips,
        DebugChannel::AnimBrowser,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DebugChannel::Colliders => "Colliders",
            DebugChannel::Casts => "Sensor casts",
            DebugChannel::LogTransitions => "Log: transitions",
            DebugChannel::LogVerbose => "Log: per-tick trace",
            DebugChannel::LogFactFlips => "Log: fact flips",
            DebugChannel::AnimBrowser => "Animation browser",
        }
    }

    /// What the channel costs, so an operator can tell a free toggle from one
    /// that will distort the very frame time they are measuring.
    pub fn hint(self) -> &'static str {
        match self {
            DebugChannel::Colliders => "wireframes — adds draw calls",
            DebugChannel::Casts => "gizmos — adds draw calls",
            DebugChannel::LogTransitions => "quiet unless state changes",
            DebugChannel::LogVerbose => "~60 lines/s per actor",
            DebugChannel::LogFactFlips => "quiet unless facts flip",
            DebugChannel::AnimBrowser => "bypasses the state machine",
        }
    }
}

/// Presentation asks; `debug` owns the switches and applies them (§7).
#[derive(Message, Debug, Clone, Copy)]
pub struct DebugChannelToggle(pub DebugChannel);

/// One-shot debug actions that are not a persistent switch. The two spawn
/// toggles replaced bare F7/F8 keys: debug translates the click into the
/// owning module's request message (same pattern as `ToggleProbe`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DebugAction {
    ToggleProbe,
    AdvanceHour,
    ToggleTimeSpeed,
    ToggleBokobos,
    ToggleHorse,
}

impl DebugAction {
    pub const ALL: [DebugAction; 5] = [
        DebugAction::ToggleProbe,
        DebugAction::AdvanceHour,
        DebugAction::ToggleTimeSpeed,
        DebugAction::ToggleBokobos,
        DebugAction::ToggleHorse,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DebugAction::ToggleProbe => "Probe dummy",
            DebugAction::AdvanceHour => "Advance 1 hour",
            DebugAction::ToggleTimeSpeed => "Fast-forward time",
            DebugAction::ToggleBokobos => "Bokobos on/off",
            DebugAction::ToggleHorse => "Horse on/off",
        }
    }
}

#[derive(Message, Debug, Clone, Copy)]
pub struct DebugActionRequest(pub DebugAction);

/// Presentation asks; `debug` owns [`HudVisibility`](super::snapshot::HudVisibility)
/// and applies the toggle (§7). Carries which readout group the F2 menu flipped.
#[derive(Message, Debug, Clone, Copy)]
pub struct HudSectionToggle(pub SectionId);
