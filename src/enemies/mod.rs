//! Enemies — AI-controlled actors on the shared Movement contract.
//!
//! An enemy is a normal kinematic `Actor` whose brain is AI instead of
//! hardware: the Perceive → Decide → Act systems below run in
//! `MovementSet::ReadIntents` (the same conceptual slot as
//! `movement::brain::read_intents`) and write **only** that entity's
//! `Intents`/`CombatIntents` (plus, for the archer, its own
//! `ControlOrientation`). Never `Transform`, `BodyVelocity`,
//! `LocomotionState`, or `CombatState` — the Broker pipelines own those.
//! See `docs/architecture/enemies.md` and the `bokobo-brain` /
//! `enemies-combat` tickets.

use bevy::prelude::*;

pub mod brain;
pub mod combat;
pub mod perception;
pub mod state;

use crate::health::{DeathMessage, Health, HealthSet};
use crate::movement::MovementSet;
use crate::movement::abilities::{
    AirborneMovement, GroundMovement, SprintMovement, StairsMovement,
};
use crate::movement::body::BodyDimensions;
use crate::movement::bundles::{
    GroundMovementBundle, KinematicActorBundle, SprintMovementBundle, StairsMovementBundle,
    StaminaBundle,
};
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

const BOKOBO_SPAWN_HEIGHT: f32 =
    BOKOBO_DIMENSIONS.radius + BOKOBO_DIMENSIONS.standing_capsule_length / 2.0;

/// Authored spawns: open ground east of the graybox course, clear of the
/// test wall, ramps, and stairs — world-fixed, never relative to the player,
/// so every run exercises the same scenario.
const MELEE_SPAWN_POSITION: Vec3 = Vec3::new(10.0, BOKOBO_SPAWN_HEIGHT, 8.0);
const ARCHER_SPAWN_POSITION: Vec3 = Vec3::new(16.0, BOKOBO_SPAWN_HEIGHT, 12.0);

/// First-pass hit points; tuned at the `enemies-combat` checkpoint.
const MELEE_BOKOBO_HP: f32 = 30.0;
const ARCHER_BOKOBO_HP: f32 = 20.0;

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
                combat::act_melee,
                combat::act_archer,
            )
                .chain()
                .in_set(MovementSet::ReadIntents),
        );
        // Death consequences belong to the actor's owner (health.md): a dead
        // enemy despawns; visuals cleans up the orphaned capsule itself.
        app.add_systems(FixedUpdate, despawn_dead.after(HealthSet::Apply));
    }
}

/// F7 toggle: spawns or despawns the graybox pair — a melee bokobo and an
/// archer bokobo — at their authored homes.
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
        info!("[debug] Bokobos despawned (F7)");
        return;
    }

    spawn_bokobo(
        &mut commands,
        "Bokobo",
        MELEE_SPAWN_POSITION,
        brain::EnemyBrainProfile::BOKOBO,
        MELEE_BOKOBO_HP,
        (
            crate::combat::weapon::WeaponProfile::BOKOBO_CLUB,
            crate::combat::motors::attack::ComboLocal::default(),
            crate::combat::motors::attack::ActiveSwing::default(),
        ),
    );
    spawn_bokobo(
        &mut commands,
        "BokoboArcher",
        ARCHER_SPAWN_POSITION,
        brain::EnemyBrainProfile::BOKOBO_ARCHER,
        ARCHER_BOKOBO_HP,
        (
            crate::combat::motors::aim::DrawStrength::default(),
            crate::input::frame::ControlOrientation::default(),
        ),
    );
    info!("[debug] Bokobo pair spawned: melee + archer (F7)");
}

/// The shared bokobo chassis; `loadout` is the combat archetype (club combo
/// vs. bow + own control orientation — capability is the component).
fn spawn_bokobo(
    commands: &mut Commands,
    name: &str,
    home: Vec3,
    profile: brain::EnemyBrainProfile,
    hit_points: f32,
    loadout: impl Bundle,
) {
    let mut ground = GroundMovement::PLAYER;
    ground.drive.max_forward_speed = 2.5;
    let mut sprint = SprintMovement::PLAYER;
    sprint.drive.max_forward_speed = 6.5;

    commands.spawn((
        Enemy,
        Name::new(name.to_string()),
        Home(home),
        KinematicActorBundle::new(
            Transform::from_translation(home),
            BOKOBO_DIMENSIONS,
            GroundSensing::PLAYER,
        ),
        (
            GroundMovementBundle::new(ground),
            SprintMovementBundle::new(sprint),
            StairsMovementBundle::new(StairsMovement::PLAYER),
            StaminaBundle::default(),
            AirborneMovement::PLAYER,
        ),
        perception::Perception::BOKOBO,
        perception::AggroTarget::default(),
        perception::Awareness::default(),
        state::EnemyAiState::default(),
        profile,
        brain::BrainLocal::default(),
        (
            Health::new(hit_points),
            crate::combat::intent::CombatIntents::default(),
            crate::combat::state::CombatState::default(),
            crate::combat::proposal::CombatProposalBuffer::default(),
            combat::EnemyCombatLocal::default(),
            loadout,
        ),
    ));
}

fn despawn_dead(
    mut commands: Commands,
    mut deaths: MessageReader<DeathMessage>,
    enemies: Query<Option<&Name>, With<Enemy>>,
) {
    for death in deaths.read() {
        let Ok(name) = enemies.get(death.entity) else {
            continue;
        };
        info!(
            "[enemies] {} died",
            name.map(Name::as_str).unwrap_or("enemy")
        );
        commands.entity(death.entity).despawn();
    }
}
