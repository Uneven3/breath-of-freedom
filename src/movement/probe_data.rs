//! Data for the focused graybox climbing probe.

use bevy::prelude::*;

/// Marks the autonomous actor used to exercise the graybox traversal course.
#[derive(Component)]
pub struct TraversalProbe;

/// One phase of the continuous climb scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeStage {
    ApproachWall,
    AttachClimb,
    AscendClimb,
    HoldAtLip,
}

/// Per-actor script state. The brain owns phase transitions; motors remain
/// the only writers of body movement during the scenario.
#[derive(Component, Debug)]
pub struct ProbeScript {
    pub stage: ProbeStage,
    pub elapsed: f32,
    pub timeout_reported: bool,
    pub completed: bool,
}

impl Default for ProbeScript {
    fn default() -> Self {
        Self {
            stage: ProbeStage::ApproachWall,
            elapsed: 0.0,
            timeout_reported: false,
            completed: false,
        }
    }
}

/// Bitset of phases that reached their real sensor/state condition.
#[derive(Component, Default, Debug)]
pub struct ProbeCoverage(pub u8);
