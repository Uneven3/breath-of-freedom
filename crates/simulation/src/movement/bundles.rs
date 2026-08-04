//! Construction bundles for movement-capable actors.
//!
//! These bundles only compose data. They do not enable systems: movement
//! systems remain registered by [`super::MovementPlugin`] and select actors
//! through their individual capability components.

use avian3d::prelude::*;
use bevy_ecs::prelude::*;
use bevy_transform::prelude::*;

use super::abilities::{
    GlideMovement, GroundMovement, JumpMovement, LadderMovement, LedgeTraversal, SneakMovement,
    SprintMovement, StairsMovement, WallJumpMovement,
};
use super::body::{BodyDimensions, crouched_collider, standing_collider};
use super::facts::{LadderFacts, LedgeFacts, StairsFacts};
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
use super::sensing::GroundSensing;
use super::stamina::Stamina;
use super::{Actor, ActorId};
use crate::physics::GameLayer;

/// Lo que un actor kinemático necesita **y no puede deducirse**: su identidad
/// authored, su pose, su cuerpo físico y sus perfiles.
///
/// El resto del núcleo (velocidad, intents, estado, buffer de propuestas,
/// facts, LOD) lo trae `Actor` por `#[require]`, así que ya no se puede
/// spawnear un actor a medias — antes eran nueve campos más acá, sostenidos
/// por disciplina.
#[derive(Bundle)]
pub struct KinematicActorBundle {
    pub actor: Actor,
    pub actor_id: ActorId,
    pub transform: Transform,
    pub rigid_body: RigidBody,
    pub collider: Collider,
    pub collision_layers: CollisionLayers,
    pub dimensions: BodyDimensions,
    pub ground_sensing: GroundSensing,
}

impl KinematicActorBundle {
    pub fn new(
        actor_id: ActorId,
        transform: Transform,
        dimensions: BodyDimensions,
        sensing: GroundSensing,
    ) -> Self {
        Self {
            actor: Actor,
            actor_id,
            transform,
            rigid_body: RigidBody::Kinematic,
            collider: standing_collider(dimensions),
            // Member of `Actor`, collides with everything: layers don't change
            // physical contacts, they let spatial queries (ledge sensing) mask
            // actors out so no capsule reads as climbable wall.
            collision_layers: CollisionLayers::new(GameLayer::Actor, LayerMask::ALL),
            dimensions,
            ground_sensing: sensing,
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
            sprint_lock: Default::default(),
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
            stand_collider: StandCollider(standing_collider(dimensions)),
            crouch_collider: CrouchCollider(crouched_collider(dimensions)),
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
            facts: Default::default(),
            local: Default::default(),
            grace: Default::default(),
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
            facts: Default::default(),
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
            facts: Default::default(),
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
    // Los trae `Actor` por `#[require]`, no este bundle: el test comprueba
    // justamente que aparezcan sin que nadie los liste.
    use crate::movement::BodyVelocity;
    use crate::movement::constraints::LocomotionConstraintFacts;
    use crate::movement::facts::{BodyContact, GroundFacts};
    use crate::movement::intents::Intents;
    use crate::movement::lod::SensingLod;
    use crate::movement::proposal::ProposalBuffer;
    use crate::movement::stamina::Stamina;
    use crate::movement::state::{LocomotionEnabled, LocomotionState};

    /// La razón de ser de `#[require]`: en ECS olvidarse de un componente no
    /// da error, sólo hace que la query no enganche y el actor se quede quieto
    /// sin decir nada. Acá se spawnea el marcador **solo**, sin bundle, y aun
    /// así el cuerpo queda completo.
    #[test]
    fn a_bare_actor_marker_still_arrives_with_the_whole_broker_contract() {
        let mut world = World::new();
        let actor = world.spawn(Actor).id();
        let actor = world.entity(actor);

        assert!(actor.contains::<BodyVelocity>());
        assert!(actor.contains::<Intents>());
        assert!(actor.contains::<LocomotionState>());
        assert!(actor.contains::<LocomotionEnabled>());
        assert!(actor.contains::<ProposalBuffer>());
        assert!(actor.contains::<BodyContact>());
        assert!(actor.contains::<GroundFacts>());
        assert!(actor.contains::<SensingLod>());
        assert!(actor.contains::<LocomotionConstraintFacts>());
    }

    #[test]
    fn kinematic_actor_bundle_supplies_the_movement_pipeline_contract() {
        let dimensions = BodyDimensions::PLAYER;
        let mut world = World::new();
        let entity = world
            .spawn(KinematicActorBundle::new(
                ActorId::authored(10_000),
                Transform::IDENTITY,
                dimensions,
                GroundSensing::PLAYER,
            ))
            .id();
        let actor = world.entity(entity);

        assert!(actor.contains::<Actor>());
        assert_eq!(actor.get::<ActorId>(), Some(&ActorId::authored(10_000)));
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
        assert!(actor.contains::<LocomotionEnabled>());
        assert!(actor.contains::<SensingLod>());
        assert!(actor.contains::<LocomotionConstraintFacts>());
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
