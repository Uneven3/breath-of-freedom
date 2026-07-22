//! Construction bundles for movement-capable actors.
//!
//! These bundles only compose data. They do not enable systems: movement
//! systems remain registered by [`super::MovementPlugin`] and select actors
//! through their individual capability components.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::abilities::{
    GlideMovement, GroundMovement, JumpMovement, LadderMovement, LedgeTraversal, SneakMovement,
    SprintMovement, StairsMovement, WallJumpMovement,
};
use super::attachment::LocomotionEnabled;
use super::body::BodyDimensions;
use super::constraints::LocomotionConstraintFacts;
use super::facts::{BodyContact, GroundFacts, LadderFacts, LedgeFacts, StairsFacts};
use super::intents::Intents;
use super::lod::SensingLod;
use super::motors::{
    auto_vault::VaultState,
    edge_leap::EdgeLeapState,
    glide::GlideLocal,
    jump::{JumpLocal, JumpPhase},
    mantle::MantleState,
    sneak::{CrouchCollider, Crouched, SneakLock, StandClearance, StandCollider},
    sprint::SprintLock,
    stairs::{StairsGrace, StairsLocal},
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
    pub locomotion_enabled: LocomotionEnabled,
    pub transform: Transform,
    pub rigid_body: RigidBody,
    pub collider: Collider,
    pub collision_layers: CollisionLayers,
    pub dimensions: BodyDimensions,
    pub velocity: BodyVelocity,
    pub intents: Intents,
    pub locomotion: LocomotionState,
    pub proposals: ProposalBuffer,
    pub contact: BodyContact,
    pub ground: GroundFacts,
    pub ground_sensing: GroundSensing,
    pub sensing_lod: SensingLod,
    pub constraint_facts: LocomotionConstraintFacts,
}

impl KinematicActorBundle {
    pub fn new(transform: Transform, dimensions: BodyDimensions, sensing: GroundSensing) -> Self {
        Self {
            actor: Actor,
            locomotion_enabled: LocomotionEnabled,
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
            contact: BodyContact::default(),
            ground: GroundFacts::default(),
            ground_sensing: sensing,
            sensing_lod: SensingLod::default(),
            constraint_facts: LocomotionConstraintFacts::default(),
        }
    }
}

#[derive(Bundle)]
pub struct GroundMovementBundle {
    pub movement: GroundMovement,
}

impl GroundMovementBundle {
    pub fn new(movement: GroundMovement) -> Self {
        Self { movement }
    }
}

#[derive(Bundle)]
pub struct SprintMovementBundle {
    pub movement: SprintMovement,
    pub sprint_lock: SprintLock,
}

impl SprintMovementBundle {
    pub fn new(movement: SprintMovement) -> Self {
        Self {
            movement,
            sprint_lock: default(),
        }
    }
}

#[derive(Bundle)]
pub struct SneakMovementBundle {
    pub movement: SneakMovement,
    pub sneak_lock: SneakLock,
    pub crouched: Crouched,
    pub stand_clearance: StandClearance,
    pub stand_collider: StandCollider,
    pub crouch_collider: CrouchCollider,
}

impl SneakMovementBundle {
    pub fn new(movement: SneakMovement, dimensions: BodyDimensions) -> Self {
        Self {
            movement,
            sneak_lock: SneakLock::default(),
            crouched: Crouched::default(),
            stand_clearance: StandClearance::default(),
            stand_collider: StandCollider(dimensions.standing_collider()),
            crouch_collider: CrouchCollider(dimensions.crouched_collider()),
        }
    }
}

#[derive(Bundle)]
pub struct StairsMovementBundle {
    pub movement: StairsMovement,
    pub facts: StairsFacts,
    pub local: StairsLocal,
    pub grace: StairsGrace,
}

impl StairsMovementBundle {
    pub fn new(movement: StairsMovement) -> Self {
        Self {
            movement,
            facts: default(),
            local: default(),
            grace: default(),
        }
    }
}

#[derive(Bundle, Default)]
pub struct StaminaBundle {
    pub stamina: Stamina,
}

#[derive(Bundle)]
pub struct LadderMovementBundle {
    pub movement: LadderMovement,
    pub facts: LadderFacts,
}

impl LadderMovementBundle {
    pub fn new(movement: LadderMovement) -> Self {
        Self {
            movement,
            facts: default(),
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
    pub facts: LedgeFacts,
    pub mantle: MantleState,
    pub vault: VaultState,
}

impl LedgeTraversalBundle {
    pub fn new(traversal: LedgeTraversal) -> Self {
        Self {
            traversal,
            facts: default(),
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
        assert!(actor.contains::<BodyContact>());
        assert!(actor.contains::<GroundFacts>());
        assert!(!actor.contains::<Stamina>());
        assert!(!actor.contains::<LedgeFacts>());
        assert!(!actor.contains::<StairsFacts>());
        assert!(!actor.contains::<LadderFacts>());
        assert_eq!(actor.get::<GroundSensing>(), Some(&GroundSensing::PLAYER));
    }

    #[test]
    fn capability_bundles_supply_only_their_motor_state() {
        let dimensions = BodyDimensions::PLAYER;
        let mut world = World::new();
        let entity = world
            .spawn((
                GroundMovementBundle::new(GroundMovement::PLAYER),
                SprintMovementBundle::new(SprintMovement::PLAYER),
                SneakMovementBundle::new(SneakMovement::PLAYER, dimensions),
                StairsMovementBundle::new(StairsMovement::PLAYER),
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
