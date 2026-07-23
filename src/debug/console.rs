//! Console sink: writes the [`DebugSnapshot`] to the log.
//!
//! This is the channel that survives the playtest. The HUD is for judging
//! feeling in the moment; the log is what gets read afterwards to build the
//! before/after table (`AHORA.md`), so it must carry the same numbers without
//! anyone having to dictate them off the screen.
//!
//! Three modes, each answering a different question:
//!
//! - **Periodic** — the perf and scene sections on a fixed cadence. This is the
//!   A/B time series: several samples per configuration, so a value can be
//!   averaged and settling after a knob change is visible rather than guessed.
//!   The scene inventory (triangles, draw calls) rides the same cadence because
//!   it is the load that explains the frame cost logged beside it.
//! - **On change** — discrete values only, emitted when one moves. Continuous
//!   readings (frame time, velocity, sensor floats) are excluded per field:
//!   they drift every frame and would bury the transitions worth reading.
//! - **On demand (P)** — the full snapshot, to mark a specific moment
//!   ("this is where it felt wrong") without flooding the log.

use bevy::prelude::*;

use super::snapshot::{DebugSnapshot, SectionId};

/// How often the steady-state perf line lands in the log. Long enough for the
/// smoothed diagnostics to settle after a knob change, short enough that a
/// brief stop in one spot still yields several samples.
const PERIOD: f32 = 2.0;

pub(super) fn log_periodic(
    time: Res<Time<Real>>,
    snapshot: Res<DebugSnapshot>,
    mut next_at: Local<f32>,
) {
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
    snapshot: Res<DebugSnapshot>,
    mut previous: Local<Vec<Option<String>>>,
) {
    if previous.len() != SectionId::COUNT {
        previous.resize(SectionId::COUNT, None);
    }
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
