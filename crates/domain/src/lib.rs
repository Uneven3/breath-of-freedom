//! Pure data contracts shared by Breath of Freedom's sibling simulation and
//! presentation crates.
//!
//! This crate deliberately does not depend on the Bevy facade, rendering, or
//! Avian. Owning the ECS components and messages here makes the layer boundary
//! enforceable by Cargo instead of relying on import discipline.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::float_cmp,
        clippy::unwrap_used,
        reason = "tests assert exact authored constants and may panic on broken fixtures"
    )
)]

pub mod asset_pipeline;
pub mod combat;
pub mod debug;
pub mod enemies;
pub mod health;
pub mod input;
pub mod interaction;
pub mod inventory;
pub mod mounts;
pub mod movement;
pub mod perf;
pub mod projectiles;
pub mod proposal;
pub mod scene;
pub mod visuals;
pub mod world;
