//! Presentation module exposing shared presentation cues and structures.

use bevy::prelude::*;

pub mod cues;

/// Registers the presentation-cue message so producers (Movement, Combat) and
/// consumers (SFX, VFX) share one channel. Lives here, next to the type,
/// rather than in `main.rs`.
pub struct PresentationPlugin;

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<cues::CueMessage>();
    }
}
