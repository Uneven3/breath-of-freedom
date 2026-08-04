//! Physics-wide collision policy shared by simulation systems.

use avian3d::prelude::PhysicsLayer;

#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
pub enum GameLayer {
    #[default]
    Default,
    Actor,
}
