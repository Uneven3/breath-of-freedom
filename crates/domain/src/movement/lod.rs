//! Sensing LOD — the per-actor decision, as data.
//!
//! The system that *assigns* it lives in simulation; what is here is the answer
//! it writes and everyone else reads, plus the tuning resource. Data, not
//! policy (§19).

use bevy_ecs::prelude::*;

/// Tuning for the sensing LOD. World- and encounter-scale dependent, so it is
/// a resource meant to be tweaked, not a set of scattered constants.
#[derive(Resource, Clone, Debug)]
pub struct SensingLodConfig {
    /// Distance (m) from the local player within which actors sense every tick.
    pub full_rate_radius: f32,
    /// Beyond the radius, sense once every this many ticks (>= 1). At 60 Hz a
    /// value of 4 means distant actors refresh their facts at 15 Hz.
    pub reduced_interval: u32,
}

impl Default for SensingLodConfig {
    fn default() -> Self {
        Self {
            full_rate_radius: 30.0,
            reduced_interval: 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SensingTier {
    #[default]
    Full,
    Reduced,
}

/// Per-actor LOD decision, rewritten every tick by `assign_sensing_lod`.
/// Defaults to sensing (full rate), so actors sense normally in worlds that
/// never run the assignment system.
#[derive(Component, Debug, Clone, Copy)]
pub struct SensingLod {
    pub tier: SensingTier,
    pub sense_this_tick: bool,
}

impl Default for SensingLod {
    fn default() -> Self {
        Self {
            tier: SensingTier::Full,
            sense_this_tick: true,
        }
    }
}

impl SensingLod {
    /// Guard used by the sensing services: `None` (no LOD component) senses.
    pub fn skips(lod: Option<&SensingLod>) -> bool {
        lod.is_some_and(|lod| !lod.sense_this_tick)
    }
}
