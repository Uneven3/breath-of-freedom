//! Data for the focused graybox traversal probe.

use bevy::prelude::*;

/// Marks the autonomous actor used to exercise the graybox traversal course.
#[derive(Component)]
pub struct TraversalProbe;

/// Ask Movement to spawn or despawn the probe.
///
/// Owned here rather than by `debug` because Movement is the receiver and owns
/// the entity (§7). The dependency has to point this way: simulation cannot
/// rely on a debug type existing, or every test app that builds
/// `MovementPlugin` without `DebugPlugin` fails to validate.
#[derive(Message, Debug, Clone, Copy)]
pub struct ProbeToggleRequest;

/// One phase of the full traversal scenario: climb the wall, mantle its top,
/// turn around, and glide back down. Historial: git history (probe-mantle-glide).
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
