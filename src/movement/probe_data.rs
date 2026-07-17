//! Data for the focused graybox traversal probe.

use bevy::prelude::*;

/// Marks the autonomous actor used to exercise the graybox traversal course.
#[derive(Component)]
pub struct TraversalProbe;

/// One phase of the full traversal scenario: climb the wall, mantle its top,
/// turn around, and glide back down. Historial: `docs/tickets/LOG.md` (probe-mantle-glide).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeStage {
    ApproachWall,
    AttachClimb,
    AscendClimb,
    /// Hold below the lip without an accidental mantle — the original
    /// checkpoint of `traversal-probe`, preserved as a settle gate.
    HoldAtLip,
    MantleOntoTop,
    SettleOnTop,
    TurnAround,
    JumpOff,
    GlideDown,
}

/// Per-actor script state. The brain owns phase transitions; motors remain
/// the only writers of body movement during the scenario.
#[derive(Component, Debug)]
pub struct ProbeScript {
    pub stage: ProbeStage,
    pub elapsed: f32,
    pub timeout_reported: bool,
    pub completed: bool,
    /// `GlideDown` only counts as completed if the pipeline actually reached
    /// `Glide` on the way down — landing without gliding is a failure.
    pub glide_observed: bool,
}

impl Default for ProbeScript {
    fn default() -> Self {
        Self {
            stage: ProbeStage::ApproachWall,
            elapsed: 0.0,
            timeout_reported: false,
            completed: false,
            glide_observed: false,
        }
    }
}

/// Bitset of phases that reached their real sensor/state condition.
#[derive(Component, Default, Debug)]
pub struct ProbeCoverage(pub u16);
