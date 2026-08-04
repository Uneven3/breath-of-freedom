use bevy::prelude::*;

pub mod components;

use crate::presentation::cues::{CueId, CueKind, CueMessage};
use bof_simulation::movement::Actor;
use bof_simulation::movement::BodyVelocity;
use bof_simulation::movement::facts::GroundFacts;
use bof_simulation::movement::stamina::Stamina;
use components::{ContinuousSfxTracker, StrideAccumulator};

/// Log a modulation update only when the change is audible-sized. Stamina
/// drains/recovers 5–15 per second (≈0.1–0.25 per 60 Hz tick), so a threshold
/// below one tick's delta would fire every frame.
const SPEED_DELTA_THRESHOLD: f32 = 0.5;
const STAMINA_DELTA_THRESHOLD: f32 = 1.0;

/// Metres of grounded travel between footstep cues.
const STRIDE_LEN: f32 = 2.0;
/// Below this planar speed the actor is idling, not stepping.
const MIN_STEP_SPEED: f32 = 0.6;

/// Plugin managing SFX presentation systems, reacting to discrete cues
/// and modulating continuous audio parameters.
pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        // Chained so a step cue emitted this frame is heard the same frame.
        app.add_systems(
            Update,
            (emit_step_cues, play_audio_cues, modulate_continuous_sfx).chain(),
        );
    }
}

/// Emits a `Step` audio cue every `STRIDE_LEN` of grounded travel. A stopgap
/// for footstep timing until the animation contract ships foot-plant events;
/// reads simulation facts only, never writes them (§20).
fn emit_step_cues(
    mut commands: Commands,
    time: Res<Time>,
    mut cues: MessageWriter<CueMessage>,
    mut q: Query<
        (
            Entity,
            &GroundFacts,
            &BodyVelocity,
            Option<&mut StrideAccumulator>,
        ),
        With<Actor>,
    >,
) {
    let dt = time.delta_secs();
    for (entity, ground, velocity, accumulator) in &mut q {
        let planar_speed = Vec3::new(velocity.0.x, 0.0, velocity.0.z).length();
        let Some(mut accumulator) = accumulator else {
            // try_insert: the actor can be despawned this same frame.
            commands
                .entity(entity)
                .try_insert(StrideAccumulator::default());
            continue;
        };
        if !ground.grounded || planar_speed <= MIN_STEP_SPEED {
            accumulator.distance = 0.0;
            continue;
        }
        accumulator.distance += planar_speed * dt;
        if accumulator.distance >= STRIDE_LEN {
            accumulator.distance -= STRIDE_LEN;
            cues.write(CueMessage {
                source: entity,
                id: CueId::Step,
                kind: CueKind::Audio,
            });
        }
    }
}

/// Turns audio cues into sound. A `Step` reads the stepping actor's recorded
/// `GroundFacts::surface` so grass and stone sound different — the surface→sound
/// mapping lives here, never in simulation (§20). No clips are loaded yet, so
/// the graybox is validated by the log; a `SurfaceKind`-keyed sound table will
/// spawn an `AudioPlayer` at this seam.
fn play_audio_cues(mut cues: MessageReader<CueMessage>, grounds: Query<&GroundFacts>) {
    for cue in cues.read() {
        if cue.kind != CueKind::Audio {
            continue;
        }
        match cue.id {
            CueId::Step => {
                let surface = grounds
                    .get(cue.source)
                    .map(|ground| ground.surface)
                    .unwrap_or_default();
                debug!("[audio] step on {:?} (actor {:?})", surface, cue.source);
            }
            other => debug!("[audio] cue: {:?}", other),
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
            // try_insert: the actor can be despawned by another Update
            // system's buffer this same frame (F7 toggle, death reactions).
            commands.entity(entity).try_insert(ContinuousSfxTracker {
                last_speed: current_speed,
                last_stamina: current_stamina,
            });
        }
    }
}
