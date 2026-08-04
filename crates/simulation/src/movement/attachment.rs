use bevy_ecs::prelude::*;
use bevy_math::{Quat, Vec3};
use bevy_transform::components::Transform;

#[derive(Component, Debug, Clone)]
pub struct KinematicAttachment {
    pub carrier: Entity,
    pub local_pose: Transform,
}

pub use bof_domain::movement::state::LocomotionEnabled;

#[derive(Component, Debug, Clone, Copy)]
pub struct PendingSafeRecovery {
    pub origin: Vec3,
    pub rotation: Quat,
    pub next_height: f32,
}

/// Shared filter for systems that participate in the physical locomotion
/// pipeline. Attached actors remain `Actor`s but intentionally fail this
/// filter until Movement detaches them.
pub type LocomotionActorFilter = (With<super::Actor>, With<LocomotionEnabled>);
