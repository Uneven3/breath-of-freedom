//! Construction bundles for movement-capable actors.
//!
//! These bundles only compose data. They do not enable systems: movement
//! systems remain registered by [`super::MovementPlugin`] and select actors
//! through their individual capability components.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::abilities::{
    GlideMovement, GroundMovement, JumpMovement, LedgeTraversal, WallJumpMovement,
};
use super::body::BodyDimensions;
use super::facts::{BodyContact, GroundFacts, LadderFacts, LedgeFacts, StairsFacts};
use super::intents::Intents;
use super::lod::SensingLod;
use super::motors::{
    auto_vault::VaultState,
    edge_leap::EdgeLeapState,
    glide::GlideLocal,
    jump::{JumpLocal, JumpPhase},
    mantle::MantleState,
    sneak::{Crouched, SneakLock, StandClearance, StandCollider},
    sprint::SprintLock,
    stairs::StairsLocal,
    wall_jump::WallJumpState,
};
use super::proposal::ProposalBuffer;
use super::sensing::GroundSensing;
use super::stamina::Stamina;
use super::state::LocomotionState;
use super::{Actor, BodyVelocity};
use crate::world::GameLayer;

/// Data every kinematic Movement actor needs, independent of who controls it
/// or which locomotion capabilities it receives.
#[derive(Bundle)]
pub struct KinematicActorBundle {
    pub actor: Actor,
    pub transform: Transform,
    pub rigid_body: RigidBody,
    pub collider: Collider,
    pub collision_layers: CollisionLayers,
    pub dimensions: BodyDimensions,
    pub velocity: BodyVelocity,
    pub intents: Intents,
    pub locomotion: LocomotionState,
    pub proposals: ProposalBuffer,
    pub stamina: Stamina,
    pub contact: BodyContact,
    pub ground: GroundFacts,
    pub ledge: LedgeFacts,
    pub stairs: StairsFacts,
    pub ladder: LadderFacts,
    pub ground_sensing: GroundSensing,
    pub sensing_lod: SensingLod,
}

impl KinematicActorBundle {
    pub fn new(transform: Transform, dimensions: BodyDimensions, sensing: GroundSensing) -> Self {
        Self {
            actor: Actor,
            transform,
            rigid_body: RigidBody::Kinematic,
            collider: dimensions.standing_collider(),
            // Member of `Actor`, collides with everything: layers don't change
            // physical contacts, they let spatial queries (ledge sensing) mask
            // actors out so no capsule reads as climbable wall.
            collision_layers: CollisionLayers::new(GameLayer::Actor, LayerMask::ALL),
            dimensions,
            velocity: BodyVelocity::default(),
            intents: Intents::default(),
            locomotion: LocomotionState::default(),
            proposals: ProposalBuffer::default(),
            stamina: Stamina::default(),
            contact: BodyContact::default(),
            ground: GroundFacts::default(),
            ledge: LedgeFacts::default(),
            stairs: StairsFacts::default(),
            ladder: LadderFacts::default(),
            ground_sensing: sensing,
            sensing_lod: SensingLod::default(),
        }
    }
}

/// Ground capability and the per-actor state used by its Sprint and Sneak
/// motors.
#[derive(Bundle)]
pub struct GroundMovementBundle {
    pub movement: GroundMovement,
    pub sprint_lock: SprintLock,
    pub sneak_lock: SneakLock,
    pub crouched: Crouched,
    pub stand_clearance: StandClearance,
    pub stand_collider: StandCollider,
    pub stairs_local: StairsLocal,
}

impl GroundMovementBundle {
    pub fn new(movement: GroundMovement, dimensions: BodyDimensions) -> Self {
        Self {
            movement,
            sprint_lock: SprintLock::default(),
            sneak_lock: SneakLock::default(),
            crouched: Crouched::default(),
            stand_clearance: StandClearance::default(),
            stand_collider: StandCollider(dimensions.standing_collider()),
            stairs_local: StairsLocal::default(),
        }
    }
}

/// Jump capability and its coyote-time/input-buffer bookkeeping.
#[derive(Bundle)]
pub struct JumpMovementBundle {
    pub movement: JumpMovement,
    pub phase: JumpPhase,
    pub local: JumpLocal,
}

impl JumpMovementBundle {
    pub fn new(movement: JumpMovement) -> Self {
        Self {
            movement,
            phase: JumpPhase::default(),
            local: JumpLocal::default(),
        }
    }
}

/// Glide capability and its per-actor press-memory bookkeeping.
#[derive(Bundle)]
pub struct GlideMovementBundle {
    pub movement: GlideMovement,
    pub local: GlideLocal,
}

impl GlideMovementBundle {
    pub fn new(movement: GlideMovement) -> Self {
        Self {
            movement,
            local: GlideLocal::default(),
        }
    }
}

/// Ledge traversal capability and the independent Mantle and AutoVault phase
/// machines that use it.
#[derive(Bundle)]
pub struct LedgeTraversalBundle {
    pub traversal: LedgeTraversal,
    pub mantle: MantleState,
    pub vault: VaultState,
}

impl LedgeTraversalBundle {
    pub fn new(traversal: LedgeTraversal) -> Self {
        Self {
            traversal,
            mantle: MantleState::default(),
            vault: VaultState::default(),
        }
    }
}

/// Wall-jump capability and the separate WallJump and EdgeLeap phase
/// machines that use it.
#[derive(Bundle)]
pub struct WallJumpMovementBundle {
    pub movement: WallJumpMovement,
    pub wall_jump: WallJumpState,
    pub edge_leap: EdgeLeapState,
}

impl WallJumpMovementBundle {
    pub fn new(movement: WallJumpMovement) -> Self {
        Self {
            movement,
            wall_jump: WallJumpState::default(),
            edge_leap: EdgeLeapState::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::abilities::ClimbMovement;

    #[test]
    fn kinematic_actor_bundle_supplies_the_movement_pipeline_contract() {
        let dimensions = BodyDimensions::PLAYER;
        let mut world = World::new();
        let entity = world
            .spawn(KinematicActorBundle::new(
                Transform::IDENTITY,
                dimensions,
                GroundSensing::PLAYER,
            ))
            .id();
        let actor = world.entity(entity);

        assert!(actor.contains::<Actor>());
        assert!(actor.contains::<Collider>());
        let layers = actor
            .get::<CollisionLayers>()
            .expect("actors must declare their physics layer");
        assert!(
            layers.memberships.has_all(GameLayer::Actor),
            "actors must be members of GameLayer::Actor so ledge sensing can mask them out"
        );
        assert_eq!(
            layers.filters,
            LayerMask::ALL,
            "layers must not change physical contacts"
        );
        assert_eq!(actor.get::<BodyDimensions>(), Some(&dimensions));
        assert!(actor.contains::<BodyVelocity>());
        assert!(actor.contains::<Intents>());
        assert!(actor.contains::<LocomotionState>());
        assert!(actor.contains::<ProposalBuffer>());
        assert!(actor.contains::<Stamina>());
        assert!(actor.contains::<BodyContact>());
        assert!(actor.contains::<GroundFacts>());
        assert!(actor.contains::<LedgeFacts>());
        assert!(actor.contains::<StairsFacts>());
        assert!(actor.contains::<LadderFacts>());
        assert_eq!(actor.get::<GroundSensing>(), Some(&GroundSensing::PLAYER));
    }

    #[test]
    fn capability_bundles_supply_only_their_motor_state() {
        let dimensions = BodyDimensions::PLAYER;
        let mut world = World::new();
        let entity = world
            .spawn((
                GroundMovementBundle::new(GroundMovement::PLAYER, dimensions),
                JumpMovementBundle::new(JumpMovement::PLAYER),
                GlideMovementBundle::new(GlideMovement::PLAYER),
                LedgeTraversalBundle::new(LedgeTraversal::PLAYER),
                WallJumpMovementBundle::new(WallJumpMovement::PLAYER),
            ))
            .id();
        let actor = world.entity(entity);

        assert!(actor.contains::<GroundMovement>());
        assert!(actor.contains::<SprintLock>());
        assert!(actor.contains::<Crouched>());
        assert!(actor.contains::<JumpMovement>());
        assert!(actor.contains::<JumpPhase>());
        assert!(actor.contains::<JumpLocal>());
        assert!(actor.contains::<GlideMovement>());
        assert!(actor.contains::<GlideLocal>());
        assert!(actor.contains::<LedgeTraversal>());
        assert!(actor.contains::<MantleState>());
        assert!(actor.contains::<VaultState>());
        assert!(actor.contains::<WallJumpMovement>());
        assert!(actor.contains::<WallJumpState>());
        assert!(actor.contains::<EdgeLeapState>());
        assert!(!actor.contains::<Actor>());
        assert!(!actor.contains::<ClimbMovement>());
    }
}
