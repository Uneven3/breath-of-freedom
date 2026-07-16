//! Presentation module exposing shared presentation cues and structures.

use bevy::prelude::*;

pub mod cues;
pub mod juice;

/// Registers the presentation-cue message so producers (Movement, Combat) and
/// consumers (SFX, VFX) share one channel, plus the game-feel feedback layer
/// (`juice`). Lives here, next to the types, rather than in `main.rs`.
pub struct PresentationPlugin;

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<cues::CueMessage>();
        app.add_plugins(juice::JuicePlugin);
    }
}
