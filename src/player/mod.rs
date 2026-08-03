//! Local player composition.
//!
//! Movement provides capabilities; who gets them is a scene concern. This
//! plugin assembles the locally-controlled player out of the kinematic actor
//! contract, its movement capability bundles, and the local input binding —
//! the same pieces an AI or network controller composes differently (see
//! `docs/ARCHITECTURE.md`).

use bevy::prelude::*;

mod lock_on;

use crate::health::{DeathMessage, Health, HealthSet};
use crate::movement::abilities::{
    AirborneMovement, ClimbMovement, GlideMovement, GroundMovement, JumpMovement, LadderMovement,
    LedgeTraversal, SneakMovement, SprintMovement, StairsMovement, WallJumpMovement,
};
use crate::movement::body::BodyDimensions;
use crate::movement::brain::ClimbInputState;
use crate::movement::bundles::{
    GlideMovementBundle, GroundMovementBundle, JumpMovementBundle, KinematicActorBundle,
    LadderMovementBundle, LedgeTraversalBundle, SneakMovementBundle, SprintMovementBundle,
    StairsMovementBundle, StaminaBundle, WallJumpMovementBundle,
};
use crate::movement::sensing::{GroundSensing, LedgeCastShape, LedgeSensing};
use crate::movement::{ActorId, BodyVelocity, Player};

/// Authored spawn point in world XZ; death teleports back here (graybox
/// respawn). **Only the horizontal position is authored** — the height comes
/// from the terrain, because the ground is sculpted data now. A constant `y`
/// here put the player 6.6 m *under* a hill someone sculpted over the spawn,
/// and since a heightfield is one-sided it does not catch you from below: you
/// fall forever.
const PLAYER_SPAWN_XZ: Vec2 = Vec2::ZERO;
/// How far above the ground the body's origin starts, enough to clear the
/// capsule and settle rather than start intersecting.
const PLAYER_SPAWN_CLEARANCE: f32 = 1.5;
const PLAYER_HP: f32 = 100.0;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        // Scene content: a player is built when a scene is entered and dies
        // with it (`crate::scene`).
        for id in crate::scene::SceneId::ALL {
            app.add_systems(
                OnEnter(crate::scene::AppState::Scene(id)),
                spawn_player.in_set(crate::scene::SceneBuild::Actors),
            );
        }
        // Death consequences belong to the actor's owner (`docs/ARCHITECTURE.md`): the
        // graybox player respawns at the authored spawn with full health.
        app.add_systems(FixedUpdate, respawn_on_death.after(HealthSet::Apply));
        // Choose the lock-on target after actors move, before facing resolves it.
        app.add_systems(
            FixedUpdate,
            lock_on::update_lock_on
                .after(crate::movement::MovementSet::TickActiveMotor)
                .before(crate::movement::facing::resolve_facing),
        );
    }
}

/// Where the player's body starts: authored XZ, terrain height plus clearance.
/// Falls back to the old flat-floor height when there is no terrain, so this
/// stays correct for a scene that has none.
fn spawn_position(ground: Option<f32>) -> Vec3 {
    let ground = ground.unwrap_or(0.0);
    Vec3::new(
        PLAYER_SPAWN_XZ.x,
        ground + PLAYER_SPAWN_CLEARANCE,
        PLAYER_SPAWN_XZ.y,
    )
}

fn spawn_player(
    mut commands: Commands,
    state: Res<State<crate::scene::AppState>>,
    terrain: crate::world::TerrainAccess,
) {
    // The Player is an invisible kinematic collider; the mesh lives on a separate
    // PlayerVisual entity that interpolates toward this body (see `visuals.rs`).
    // Capsule dimensions live in `body` (shared with services and motors).
    let body_dimensions = BodyDimensions::PLAYER;
    commands.spawn((
        DespawnOnExit(*state.get()),
        Player,
        crate::enemies::perception::Perceivable,
        crate::input::frame::InputControlledBy(crate::input::frame::LOCAL_INPUT_SOURCE),
        crate::input::frame::ControlOrientation::default(),
        crate::movement::facing::FacingSource::default(),
        lock_on::LockOnInputCursor::default(),
        Name::new("Player"),
        KinematicActorBundle::new(
            ActorId::PLAYER,
            Transform::from_translation(spawn_position(terrain.height_at(PLAYER_SPAWN_XZ))),
            body_dimensions,
            GroundSensing::PLAYER,
        ),
        (
            GroundMovementBundle::new(GroundMovement::PLAYER),
            SprintMovementBundle::new(SprintMovement::PLAYER),
            SneakMovementBundle::new(SneakMovement::PLAYER, body_dimensions),
            StairsMovementBundle::new(StairsMovement::PLAYER),
            StaminaBundle::default(),
            AirborneMovement::PLAYER,
            JumpMovementBundle::new(JumpMovement::PLAYER),
            GlideMovementBundle::new(GlideMovement::PLAYER),
            ClimbMovement::PLAYER,
            LadderMovementBundle::new(LadderMovement::PLAYER),
            LedgeTraversalBundle::new(LedgeTraversal::PLAYER),
            WallJumpMovementBundle::new(WallJumpMovement::PLAYER),
            (
                LedgeSensing::PLAYER,
                LedgeCastShape::new(LedgeSensing::PLAYER),
            ),
            ClimbInputState::default(),
            (
                crate::input::InputConsumeCursor::default(),
                crate::interaction::InteractionInputCursor::default(),
            ),
        ),
        // Combat contract: the starting sword is a breakable instance of
        // `WeaponItem::GRAYBOX_SWORD` — Inventory owns the swap/durability
        // contract from here on (`inventory::equip`), Combat only reads
        // `WeaponProfile` as the armed boolean.
        (
            Health::new(PLAYER_HP),
            crate::combat::intent::CombatIntents::default(),
            crate::combat::state::CombatState::default(),
            crate::combat::proposal::CombatProposalBuffer::default(),
            crate::combat::weapon::WeaponProfile::GRAYBOX_SWORD,
            crate::combat::context_data::CombatContext::default(),
            crate::combat::context_data::MountedCombatProfile::HORSE,
            crate::combat::motors::attack::ComboLocal::default(),
            crate::combat::motors::attack::ActiveSwing::default(),
            crate::combat::brain::CombatInputCursor::default(),
            crate::combat::motors::aim::DrawStrength::default(),
            crate::combat::motors::aim::ShotSpreadRng::PLAYER,
        ),
        (
            crate::inventory::Inventory::default(),
            crate::inventory::WeaponDurability::new(crate::inventory::WeaponItem::GRAYBOX_SWORD),
            crate::inventory::InventoryInputCursor::default(),
        ),
    ));
}

type RespawnQuery<'a> = (&'a mut Transform, &'a mut BodyVelocity, &'a mut Health);

/// Graybox death rule: teleport to the authored spawn, kill momentum, heal
/// to full. The same discrete placement as the initial spawn — a game rule
/// owned by the Player's owner, not a control-pipeline bypass.
fn respawn_on_death(
    mut deaths: MessageReader<DeathMessage>,
    mut player: Query<RespawnQuery, With<Player>>,
    terrain: crate::world::TerrainAccess,
) {
    for death in deaths.read() {
        let Ok((mut transform, mut velocity, mut health)) = player.get_mut(death.entity) else {
            continue;
        };
        transform.translation = spawn_position(terrain.height_at(PLAYER_SPAWN_XZ));
        velocity.0 = Vec3::ZERO;
        health.heal_full();
        info!("[player] died — respawning at the authored spawn");
    }
}

#[cfg(test)]
mod spawn_tests {
    use super::*;
    use crate::world::Terrain;

    #[test]
    fn the_spawn_sits_on_top_of_sculpted_ground() {
        // The bug this pins: with a constant spawn height, sculpting a hill over
        // the spawn buried the player 6.6 m underground — and a heightfield does
        // not catch you from below, so the fall never ends.
        let mut terrain = Terrain::flat_for_test();
        terrain.raise_area(PLAYER_SPAWN_XZ, 30.0, 8.0);
        let ground = terrain.height_at(PLAYER_SPAWN_XZ);
        assert!(ground > 5.0, "the test hill should be tall: {ground}");

        let spawn = spawn_position(Some(ground));
        assert!(
            spawn.y > ground,
            "spawn {} must be above the ground {ground}",
            spawn.y
        );
        assert_eq!(spawn.xz(), PLAYER_SPAWN_XZ, "XZ stays authored");
    }

    #[test]
    fn a_scene_without_terrain_still_spawns_at_the_authored_height() {
        let spawn = spawn_position(None);
        assert_eq!(spawn, Vec3::new(0.0, PLAYER_SPAWN_CLEARANCE, 0.0));
    }
}
