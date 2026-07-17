use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlMask {
    pub planar: bool,
    pub sprint: bool,
    pub jump: bool,
}

impl ControlMask {
    pub const MOUNT: Self = Self {
        planar: true,
        sprint: true,
        jump: true,
    };
}

#[derive(Component, Debug, Clone, Copy)]
pub struct ControlRedirect {
    pub controlled: Entity,
    pub mask: ControlMask,
}
