//! Enemies — AI-controlled actors on the shared Movement contract.
//!
//! An enemy is a normal kinematic `Actor` whose brain is AI instead of
//! hardware: the Perceive → Decide → Act systems below run in
//! `MovementSet::ReadIntents` (the same conceptual slot as
//! `movement::brain::read_intents`) and write **only** that entity's
//! `Intents`. Never `Transform`, `BodyVelocity`, or `LocomotionState` — the
//! Broker pipeline owns those. See `docs/architecture/enemies.md` and
//! `docs/tickets/bokobo-brain.md`.

use bevy::prelude::*;

pub mod brain;
pub mod perception;
pub mod state;

use crate::movement::MovementSet;
use crate::movement::abilities::{AirborneMovement, GroundMovement};
use crate::movement::body::BodyDimensions;
use crate::movement::bundles::{GroundMovementBundle, KinematicActorBundle};
use crate::movement::sensing::GroundSensing;

/// Marker for an AI-controlled actor, analogous to `Player`.
#[derive(Component)]
pub struct Enemy;

/// The world-space point this enemy patrols around and returns to.
#[derive(Component, Clone, Copy)]
pub struct Home(pub Vec3);

/// Graybox bokobo body: a bit smaller than the player.
const BOKOBO_DIMENSIONS: BodyDimensions = BodyDimensions {
    radius: 0.45,
    standing_capsule_length: 0.9,
    crouched_capsule_length: 0.4,
};

/// Authored spawn: open ground east of the graybox course, clear of the test
/// wall, ramps, and stairs — world-fixed, never relative to the player, so
/// every run exercises the same scenario.
const BOKOBO_SPAWN_POSITION: Vec3 = Vec3::new(
    10.0,
    BOKOBO_DIMENSIONS.radius + BOKOBO_DIMENSIONS.standing_capsule_length / 2.0,
    8.0,
);

pub struct EnemiesPlugin;

impl Plugin for EnemiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<perception::DirectThreatMessage>();
        app.add_systems(Update, toggle_spawn);
        app.add_systems(
            FixedUpdate,
            (
                perception::perceive,
                perception::receive_direct_threats,
                brain::decide,
                brain::act,
            )
                .chain()
                .in_set(MovementSet::ReadIntents),
        );
    }
}

/// F7 toggle: spawns or despawns the graybox bokobo at its authored home.
fn toggle_spawn(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    existing: Query<Entity, With<Enemy>>,
) {
    if !keys.just_pressed(KeyCode::F7) {
        return;
    }

    if !existing.is_empty() {
        for entity in &existing {
            commands.entity(entity).despawn();
        }
        info!("[debug] Bokobo despawned (F7)");
        return;
    }

    let mut ground = GroundMovement::PLAYER;
    ground.walk.max_speed = 2.5;
    ground.sprint.max_speed = 6.5;

    commands.spawn((
        Enemy,
        Name::new("Bokobo"),
        Home(BOKOBO_SPAWN_POSITION),
        KinematicActorBundle::new(
            Transform::from_translation(BOKOBO_SPAWN_POSITION),
            BOKOBO_DIMENSIONS,
            GroundSensing::PLAYER,
        ),
        (
            GroundMovementBundle::new(ground, BOKOBO_DIMENSIONS),
            AirborneMovement::PLAYER,
        ),
        perception::Perception::BOKOBO,
        perception::AggroTarget::default(),
        perception::Awareness::default(),
        state::EnemyAiState::default(),
        brain::EnemyBrainProfile::BOKOBO,
        brain::BrainLocal::default(),
    ));
    info!(
        "[debug] Bokobo spawned at ({:.1}, {:.1}, {:.1}) (F7)",
        BOKOBO_SPAWN_POSITION.x, BOKOBO_SPAWN_POSITION.y, BOKOBO_SPAWN_POSITION.z
    );
}
