use bevy::prelude::*;

/// Discrete triggers for presentation effects.
#[allow(dead_code)] // `Step` is emitted by sfx (footsteps); `Jump` is reserved for a future emitter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueId {
    Step,
    Jump,
}

/// The target system for a cue.
#[allow(dead_code)] // `Audio` is consumed by sfx; `Vfx` is reserved for a future VFX consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueKind {
    Audio,
    Vfx,
}

/// A message indicating that a discrete presentation cue has occurred.
#[derive(Message, Clone, Debug)]
pub struct CueMessage {
    /// The actor the cue fired on. Lets the consumer read that actor's
    /// simulation facts — a footstep reads `GroundFacts::surface` to pick the
    /// right sound.
    pub source: Entity,
    pub id: CueId,
    pub kind: CueKind,
}
