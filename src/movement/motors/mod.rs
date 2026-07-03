//! Motors — one module per locomotion state. Each exposes `propose` (always runs)
//! and `tick` (runs only when active).

pub mod auto_vault;
pub mod climb;
pub mod edge_leap;
pub mod fall;
pub mod glide;
pub mod jump;
pub mod ladder;
pub mod mantle;
pub mod sneak;
pub mod sprint;
pub mod stairs;
pub mod walk;
pub mod wall_jump;
