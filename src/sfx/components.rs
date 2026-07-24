use bevy::prelude::Component;

/// Component attached to actors to track the last logged continuous SFX state.
#[derive(Component, Debug, Clone)]
pub struct ContinuousSfxTracker {
    pub last_speed: f32,
    pub last_stamina: f32,
}

/// Distance walked since the last footstep cue, per actor. A stopgap for
/// footstep timing until the animation contract emits foot-plant events
/// (roadmap step 3): one cue per `STRIDE_LEN` of grounded travel.
#[derive(Component, Debug, Clone, Default)]
pub struct StrideAccumulator {
    pub distance: f32,
}
