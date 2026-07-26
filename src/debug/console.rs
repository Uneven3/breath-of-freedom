//! Console sink: writes the [`DebugSnapshot`] to the log.
//!
//! This is the channel that survives the playtest. The HUD is for judging
//! feeling in the moment; the log is what gets read afterwards to build the
//! before/after table (`AHORA.md`), so it must carry the same numbers without
//! anyone having to dictate them off the screen.
//!
//! Three modes, each answering a different question — and **each off until you
//! ask it**, because this sink is the loudest thing in the process: in a real
//! two-minute playtest it wrote 208 of the log's 240 lines and buried the 28
//! that were about the game. The channels are in the F1 hub.
//!
//! - **Periodic** (`Log: perf samples`) — the perf and scene sections on a fixed
//!   cadence. This is the A/B time series: several samples per configuration, so
//!   a value can be averaged and settling after a knob change is visible rather
//!   than guessed. The scene inventory (triangles, draw calls) rides the same
//!   cadence because it is the load that explains the frame cost logged beside
//!   it. The scripted benchmark and the flythrough do **not** depend on this —
//!   they print their own tables — so leaving it off costs no measurement.
//! - **On change** (`Log: state changes`) — discrete values only, emitted when
//!   one moves. Continuous readings (frame time, velocity, sensor floats) are
//!   excluded per field: they drift every frame and would bury the transitions
//!   worth reading. Still noisy while simply walking, because `strafe` and
//!   `state` genuinely flip on every step.
//! - **On demand (P)** — the full snapshot, to mark a specific moment
//!   ("this is where it felt wrong") without flooding the log. **Always
//!   available**: it is the one mode that cannot flood anything, and needing to
//!   enable a channel before you can mark a moment you just saw would defeat it.

use bevy::prelude::*;

use super::DebugConfig;
use super::snapshot::{DebugSnapshot, SectionId};

/// How often the steady-state perf line lands in the log. Long enough for the
/// smoothed diagnostics to settle after a knob change, short enough that a
/// brief stop in one spot still yields several samples.
const PERIOD: f32 = 2.0;

pub(super) fn log_periodic(
    config: Res<DebugConfig>,
    time: Res<Time<Real>>,
    snapshot: Res<DebugSnapshot>,
    mut next_at: Local<f32>,
) {
    if !config.log_perf_samples {
        return;
    }
    let now = time.elapsed_secs();
    if now < *next_at {
        return;
    }
    *next_at = now + PERIOD;

    // Before the first frames are timed there is nothing to report, and a
    // line of zeros would look like data.
    let Some(perf) = snapshot.line(SectionId::Perf) else {
        return;
    };
    info!("[debug] {perf}");
    // Scene inventory rides the same cadence so the draw-call / triangle load
    // lands in the log next to the frame cost it explains — and during a
    // benchmark run, once per step. Skipped until its first throttled sample.
    if let Some(scene) = snapshot.line(SectionId::Scene) {
        info!("[debug] {scene}");
    }
}

pub(super) fn log_on_change(
    config: Res<DebugConfig>,
    snapshot: Res<DebugSnapshot>,
    mut previous: Local<Vec<Option<String>>>,
) {
    if !config.log_state_changes {
        return;
    }
    if previous.len() != SectionId::COUNT {
        previous.resize(SectionId::COUNT, None);
    }
    // Note the early return above leaves `previous` untouched while the channel
    // is off, so switching it on logs every section once. That is the useful
    // behaviour, not a leak: the first thing you want after asking for changes is
    // what the values are *now*, to have something to read the changes against.
    for (index, id) in SectionId::ALL.into_iter().enumerate() {
        let line = snapshot.stable_line(id);
        if previous[index] != line {
            if let Some(line) = &line {
                info!("[debug] {line}");
            }
            previous[index] = line;
        }
    }
}

pub(super) fn log_on_demand(keys: Res<ButtonInput<KeyCode>>, snapshot: Res<DebugSnapshot>) {
    if !keys.just_pressed(KeyCode::KeyP) {
        return;
    }
    info!("[debug] --- snapshot ---");
    for line in snapshot.lines() {
        info!("[debug] {line}");
    }
}
