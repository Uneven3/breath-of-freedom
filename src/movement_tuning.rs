//! `BOF_TUNING` — the locomotion numbers under investigation, from the launch.
//!
//! The play sessions kept producing numbers nobody could choose from a desk:
//! how wide the ground hysteresis band should be, how fast a slide should seep,
//! how far the climb sensor should reach. Answering each one by editing a
//! constant costs a recompile per experiment, so they are seeded here instead
//! and stay writable while the game runs.
//!
//! The app layer owns this because it is the only one that may read the
//! environment; `bof_simulation` receives a resource, not a `std::env` call.

use bevy::prelude::*;
use bof_domain::movement::tuning::MovementTuning;

const TUNING_ENV: &str = "BOF_TUNING";

/// Reads `BOF_TUNING="slope_hysteresis_dot=0.06,ground_grace_ticks=3"`.
///
/// A bad entry **warns and continues**, naming the valid keys: this is a
/// diagnostic tool and aborting the launch over a typo protects nothing. What
/// it must never do is apply something other than what was asked for, which is
/// why every rejection is logged individually.
pub fn configured_tuning() -> MovementTuning {
    let mut tuning = MovementTuning::default();
    let raw = match std::env::var(TUNING_ENV) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => return tuning,
        Err(std::env::VarError::NotUnicode(_)) => {
            warn!("[tuning] {TUNING_ENV} no es Unicode; se ignora");
            return tuning;
        }
    };

    let report = tuning.apply_spec(&raw);
    for (field, value) in &report.applied {
        info!("[tuning] {} = {value}", field.key());
    }
    if !report.rejected.is_empty() {
        for problem in &report.rejected {
            warn!("[tuning] {TUNING_ENV}: {problem}");
        }
        warn!("[tuning] perillas válidas: {}", valid_keys());
    }
    tuning
}

fn valid_keys() -> String {
    bof_domain::movement::tuning::TuningField::ALL
        .map(|field| field.key())
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_list_names_every_field() {
        let keys = valid_keys();
        for field in bof_domain::movement::tuning::TuningField::ALL {
            assert!(keys.contains(field.key()), "falta {}", field.key());
        }
    }
}
