use bevy::prelude::*;

/// Discrete triggers for presentation effects.
#[allow(dead_code)] // Public contract API: variants will be constructed by other gameplay systems (Movement, Combat)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueId {
    Step,
    Jump,
}

/// The target system for a cue.
#[allow(dead_code)] // Public contract API: variants will be matched/constructed by other presentation systems (VFX)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueKind {
    Audio,
    Vfx,
}

/// A message indicating that a discrete presentation cue has occurred.
#[derive(Message, Clone, Debug)]
pub struct CueMessage {
    pub id: CueId,
    pub kind: CueKind,
}
