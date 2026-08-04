//! Construction bundles for movement-capable actors.
//!
//! These bundles only compose data. They do not enable systems: movement
//! systems remain registered by [`super::MovementPlugin`] and select actors
//! through their individual capability components.

use avian3d::prelude::*;
use bevy_ecs::prelude::*;
use bevy_transform::prelude::*;

use super::abilities::SneakMovement;
use super::body::{BodyDimensions, crouched_collider, standing_collider};
use super::motors::sneak::{CrouchCollider, Crouched, SneakLock, StandClearance, StandCollider};
use super::sensing::GroundSensing;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::abilities::{
        ClimbMovement, GlideMovement, GroundMovement, JumpMovement, LedgeTraversal, SneakMovement,
        SprintMovement, StairsMovement, WallJumpMovement,
    };
    use crate::movement::facts::{LadderFacts, LedgeFacts, StairsFacts};
    use crate::movement::motors::{
        auto_vault::VaultState, edge_leap::EdgeLeapState, glide::GlideLocal, jump::JumpLocal,
        jump::JumpPhase, mantle::MantleState, sneak::Crouched, sprint::SprintLock,
        stairs::StairsGrace, stairs::StairsLocal, wall_jump::WallJumpState,
    };
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

    /// Una capacidad trae su propio bookkeeping. Antes lo hacía un bundle por
    /// capacidad; ahora lo declara el tipo, así que agregar `SprintMovement` y
    /// olvidarse de `SprintLock` dejó de ser posible — que era el bug callado:
    /// la query no enganchaba y el sprint no andaba, sin ningún error.
    #[test]
    fn each_capability_brings_its_own_motor_state() {
        let dimensions = BodyDimensions::PLAYER;
        let mut world = World::new();
        let entity = world
            .spawn((
                GroundMovement::PLAYER,
                SprintMovement::PLAYER,
                SneakMovementBundle::new(SneakMovement::PLAYER, dimensions),
                StairsMovement::PLAYER,
                JumpMovement::PLAYER,
                GlideMovement::PLAYER,
                LedgeTraversal::PLAYER,
                WallJumpMovement::PLAYER,
            ))
            .id();
        let actor = world.entity(entity);

        assert!(actor.contains::<GroundMovement>());
        assert!(actor.contains::<SprintLock>());
        assert!(actor.contains::<Crouched>());
        assert!(actor.contains::<JumpMovement>());
        assert!(actor.contains::<JumpPhase>());
        assert!(actor.contains::<JumpLocal>());
        assert!(actor.contains::<StairsFacts>());
        assert!(actor.contains::<StairsLocal>());
        assert!(actor.contains::<StairsGrace>());
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
