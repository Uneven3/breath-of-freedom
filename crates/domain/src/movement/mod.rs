use bevy_ecs::prelude::{Component, SystemSet};
use bevy_math::Vec3;

pub mod abilities;
pub mod body;
pub mod constraints;
pub mod diag;
pub mod facing;
pub mod facts;
pub mod intents;
pub mod lod;
pub mod motor_state;
pub mod probe_data;
pub mod proposal;
pub mod sensing;
pub mod stamina;
pub mod state;
pub mod tuning;

pub const GRAVITY: f32 = 9.8;

/// Ordered phases of the movement broker within `FixedUpdate`.
///
/// Vive en domain porque es un **contrato de orden**, no una implementación:
/// quien quiera correr antes de que se lean los intents, o después de que el
/// motor activo movió el cuerpo, necesita nombrar la fase. `debug` ordena así
/// sus trazas para observar el pipeline tick a tick, y con las capas hermanas
/// no puede nombrar nada de simulación.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MovementSet {
    ApplyExternal,
    ReadIntents,
    ControlRedirect,
    SenseWorld,
    GatherProposals,
    Arbitrate,
    TickActiveMotor,
    SyncAttachments,
}

/// Marker for the local player entity.
#[derive(Component)]
pub struct Player;

/// Marker for any movement-capable simulation actor.
///
/// Lo que sigue **no** es una lista de conveniencia: es lo que el broker
/// necesita para que un cuerpo exista. Sin `Intents` nadie lo mueve, sin
/// `ProposalBuffer` nadie arbitra por él, sin `GroundFacts` los motores no
/// saben si pisa suelo — y en ECS olvidarse de uno no da error: la query
/// simplemente no engancha y el actor se queda quieto, en silencio. Con
/// `#[require]` eso deja de poder pasar: poner `Actor` trae el resto.
///
/// Fuera quedan los que necesitan un valor o viven en otro crate: `ActorId`
/// (identidad authored), `BodyDimensions` y `GroundSensing` (perfiles), y el
/// cuerpo físico de Avian (`Collider`, `RigidBody`, `CollisionLayers`). Esos
/// siguen en `KinematicActorBundle`, que ya no puede olvidarse de nada más.
#[derive(Component)]
#[require(
    BodyVelocity,
    intents::Intents,
    state::LocomotionState,
    state::LocomotionEnabled,
    proposal::ProposalBuffer,
    facts::BodyContact,
    facts::GroundFacts,
    lod::SensingLod,
    constraints::LocomotionConstraintFacts
)]
pub struct Actor;

/// Stable authored simulation identity; never derived from Bevy's transient
/// entity allocation order.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActorId(u32);

impl ActorId {
    pub const PLAYER: Self = Self(1);
    pub const HORSE: Self = Self(2);
    pub const BOKOBO_MELEE: Self = Self(100);
    pub const BOKOBO_ARCHER: Self = Self(101);
    pub const TRAVERSAL_PROBE: Self = Self(1_000);

    pub const fn authored(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Velocity integrated by Movement's kinematic controller.
#[derive(Component, Default)]
pub struct BodyVelocity(pub Vec3);
