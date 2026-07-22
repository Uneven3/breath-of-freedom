//! Local player composition.
//!
//! Movement provides capabilities; who gets them is a scene concern. This
//! plugin assembles the locally-controlled player out of the kinematic actor
//! contract, its movement capability bundles, and the local input binding —
//! the same pieces an AI or network controller composes differently (see
//! `docs/ARCHITECTURE.md`).

use bevy::prelude::*;

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
use crate::movement::{BodyVelocity, Player};

/// Authored spawn point; death teleports back here (graybox respawn).
const PLAYER_SPAWN: Vec3 = Vec3::new(0.0, 1.5, 0.0);
const PLAYER_HP: f32 = 100.0;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player);
        // Death consequences belong to the actor's owner (`docs/ARCHITECTURE.md`): the
        // graybox player respawns at the authored spawn with full health.
        app.add_systems(FixedUpdate, respawn_on_death.after(HealthSet::Apply));
    }
}

fn spawn_player(mut commands: Commands) {
    // The Player is an invisible kinematic collider; the mesh lives on a separate
    // PlayerVisual entity that interpolates toward this body (see `visuals.rs`).
    // Capsule dimensions live in `body` (shared with services and motors).
    let body_dimensions = BodyDimensions::PLAYER;
    commands.spawn((
        Player,
        crate::enemies::perception::Perceivable,
        crate::input::frame::InputControlledBy(crate::input::frame::LOCAL_INPUT_SOURCE),
        crate::input::frame::ControlOrientation::default(),
        Name::new("Player"),
        KinematicActorBundle::new(
            Transform::from_translation(PLAYER_SPAWN),
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
) {
    for death in deaths.read() {
        let Ok((mut transform, mut velocity, mut health)) = player.get_mut(death.entity) else {
            continue;
        };
        transform.translation = PLAYER_SPAWN;
        velocity.0 = Vec3::ZERO;
        health.heal_full();
        info!("[player] died — respawning at the authored spawn");
    }
}
