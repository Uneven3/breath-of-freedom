use bevy_ecs::prelude::{Component, Entity};

/// What governs an actor's body yaw. Simulation resolves the target; the
/// component itself is a shared data contract for camera/debug presentation.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FacingSource {
    #[default]
    Free,
    Look,
    LockOn(Entity),
}

pub fn faces_movement(facing: Option<&FacingSource>) -> bool {
    facing.is_none_or(|source| *source == FacingSource::Free)
}
