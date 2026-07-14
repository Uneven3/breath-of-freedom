use bevy::prelude::*;

pub mod components;

use crate::movement::Actor;
use crate::movement::BodyVelocity;
use crate::movement::stamina::Stamina;
use crate::presentation::cues::{CueKind, CueMessage};
use components::ContinuousSfxTracker;

/// Log a modulation update only when the change is audible-sized. Stamina
/// drains/recovers 5–15 per second (≈0.1–0.25 per 60 Hz tick), so a threshold
/// below one tick's delta would fire every frame.
const SPEED_DELTA_THRESHOLD: f32 = 0.5;
const STAMINA_DELTA_THRESHOLD: f32 = 1.0;

/// Plugin managing SFX presentation systems, reacting to discrete cues
/// and modulating continuous audio parameters.
pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (log_audio_cue, modulate_continuous_sfx));
    }
}

/// Listens for `CueMessage` and logs those targeted for audio.
fn log_audio_cue(mut cues: MessageReader<CueMessage>) {
    for cue in cues.read() {
        if cue.kind == CueKind::Audio {
            debug!("[audio] cue: {:?}", cue.id);
        }
    }
}

/// Dynamically reads `BodyVelocity` and `Stamina` of all `Actor` entities,
/// logging changes when deltas exceed configured thresholds.
fn modulate_continuous_sfx(
    mut commands: Commands,
    mut q: Query<
        (
            Entity,
            &BodyVelocity,
            &Stamina,
            Option<&mut ContinuousSfxTracker>,
        ),
        With<Actor>,
    >,
) {
    for (entity, body_velocity, stamina, tracker) in &mut q {
        let current_speed = body_velocity.0.length();
        let current_stamina = stamina.current();

        if let Some(mut tracker) = tracker {
            let speed_delta = (current_speed - tracker.last_speed).abs();
            let stamina_delta = (current_stamina - tracker.last_stamina).abs();

            if speed_delta > SPEED_DELTA_THRESHOLD || stamina_delta > STAMINA_DELTA_THRESHOLD {
                debug!(
                    "[audio] continuous modulation update for entity {:?}: speed = {:.2}, stamina = {:.2}",
                    entity, current_speed, current_stamina
                );
                tracker.last_speed = current_speed;
                tracker.last_stamina = current_stamina;
            }
        } else {
            debug!(
                "[audio] initial baseline for entity {:?}: speed = {:.2}, stamina = {:.2}",
                entity, current_speed, current_stamina
            );
            commands.entity(entity).insert(ContinuousSfxTracker {
                last_speed: current_speed,
                last_stamina: current_stamina,
            });
        }
    }
}
