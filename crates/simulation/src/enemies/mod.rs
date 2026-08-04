//! Enemies — AI-controlled actors on the shared Movement contract.
//!
//! An enemy is a normal kinematic `Actor` whose brain is AI instead of
//! hardware: the Perceive → Decide → Act systems below run in
//! `MovementSet::ReadIntents` (the same conceptual slot as
//! `movement::brain::read_intents`) and write **only** that entity's
//! `Intents`/`CombatIntents` (plus, for the archer, its own
//! `ControlOrientation`). Never `Transform`, `BodyVelocity`,
//! `LocomotionState`, or `CombatState` — the Broker pipelines own those.
//! See `docs/ARCHITECTURE.md` and the `bokobo-brain` /
//! `enemies-combat` tickets.

use bevy_app::{App, FixedUpdate, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_log::info;
use bevy_math::prelude::*;
use bevy_transform::prelude::*;
use bof_domain::scene::SceneScoped;

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

pub use bof_domain::enemies::Enemy;

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
        app.add_message::<BokoboSpawnRequest>();
        app.add_systems(Update, process_spawn_requests);
        // Who gets enemies and when is composition, so the app decides it: it
        // writes `BokoboSpawnRequest` on entering a scene that declares them —
        // the same request the debug hub writes, so there is one spawn path,
        // not two.
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
        // Death consequences belong to the actor's owner (`docs/ARCHITECTURE.md`): a dead
        // enemy despawns; visuals cleans up the orphaned capsule itself.
        app.add_systems(FixedUpdate, despawn_dead.after(HealthSet::Apply));
    }
}

pub use bof_domain::enemies::BokoboSpawnRequest;

/// Ask for the graybox pair when a scene that wants enemies starts.
fn process_spawn_requests(
    mut requests: MessageReader<BokoboSpawnRequest>,
    mut commands: Commands,
    existing: Query<(Entity, &crate::movement::ActorId), With<Enemy>>,
) {
    let exists = !existing.is_empty();
    let mut wanted = exists;
    let mut received = false;
    for request in requests.read().copied() {
        received = true;
        match request {
            BokoboSpawnRequest::Ensure => wanted = true,
            BokoboSpawnRequest::Toggle => wanted = !wanted,
        }
    }
    if !received || (!wanted && !exists) {
        return;
    }

    if !wanted {
        for (entity, _) in &existing {
            commands.entity(entity).despawn();
        }
        info!("[debug] Bokobos despawned");
        return;
    }

    let has_melee = existing
        .iter()
        .any(|(_, actor_id)| *actor_id == crate::movement::ActorId::BOKOBO_MELEE);
    let has_archer = existing
        .iter()
        .any(|(_, actor_id)| *actor_id == crate::movement::ActorId::BOKOBO_ARCHER);
    if !has_melee {
        spawn_bokobo(
            &mut commands,
            crate::movement::ActorId::BOKOBO_MELEE,
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
    }
    if !has_archer {
        spawn_bokobo(
            &mut commands,
            crate::movement::ActorId::BOKOBO_ARCHER,
            "BokoboArcher",
            ARCHER_SPAWN_POSITION,
            brain::EnemyBrainProfile::BOKOBO_ARCHER,
            ARCHER_BOKOBO_HP,
            (
                crate::combat::motors::aim::DrawStrength::default(),
                crate::combat::motors::aim::ShotSpreadRng::BOKOBO_ARCHER,
                bof_domain::input::frame::ControlOrientation::default(),
            ),
        );
    }
    info!("[debug] Bokobo pair ensured: melee + archer");
}

/// The shared bokobo chassis; `loadout` is the combat archetype (club combo
/// vs. bow + own control orientation — capability is the component).
fn spawn_bokobo(
    commands: &mut Commands,
    actor_id: crate::movement::ActorId,
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
        SceneScoped,
        Enemy,
        Name::new(name.to_string()),
        Home(home),
        KinematicActorBundle::new(
            actor_id,
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

#[cfg(test)]
mod spawn_tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_message::<BokoboSpawnRequest>();
        app.add_systems(Update, process_spawn_requests);
        app
    }

    fn enemy_count(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<Enemy>>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn scene_ensure_is_idempotent_and_debug_toggle_is_explicit() {
        let mut app = app();

        app.world_mut().write_message(BokoboSpawnRequest::Ensure);
        app.update();
        assert_eq!(enemy_count(&mut app), 2);
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, (With<Enemy>, With<SceneScoped>)>()
                .iter(app.world())
                .count(),
            2
        );

        app.world_mut().write_message(BokoboSpawnRequest::Ensure);
        app.update();
        assert_eq!(enemy_count(&mut app), 2, "Ensure must not toggle them off");

        let melee = app
            .world_mut()
            .query_filtered::<(Entity, &crate::movement::ActorId), With<Enemy>>()
            .iter(app.world())
            .find_map(|(entity, actor_id)| {
                (*actor_id == crate::movement::ActorId::BOKOBO_MELEE).then_some(entity)
            })
            .unwrap();
        app.world_mut().entity_mut(melee).despawn();
        app.world_mut().write_message(BokoboSpawnRequest::Ensure);
        app.update();
        assert_eq!(enemy_count(&mut app), 2, "Ensure must replenish the pair");

        app.world_mut().write_message(BokoboSpawnRequest::Toggle);
        app.update();
        assert_eq!(enemy_count(&mut app), 0);
    }
}
