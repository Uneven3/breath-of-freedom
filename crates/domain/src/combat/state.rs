//! The single source of truth for the active combat mode.
//!
//! Mutually-exclusive phases as an enum, with one writer (`combat::arbitrate`).
//! Combo step stays in the motor because its phases repeat for every strike.

use bevy_ecs::prelude::*;
use bevy_math::Vec3;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CombatState {
    #[default]
    Idle,
    Windup,
    Active,
    Recovery,
    Aiming,
}

impl CombatState {
    /// Every variant; the compile-time guard below keeps the list exhaustive.
    pub const ALL: [CombatState; 5] = [
        CombatState::Idle,
        CombatState::Windup,
        CombatState::Active,
        CombatState::Recovery,
        CombatState::Aiming,
    ];

    /// Committed to an action: Movement must not sprint through it.
    pub fn commits_the_body(self) -> bool {
        match self {
            CombatState::Idle => false,
            CombatState::Windup
            | CombatState::Active
            | CombatState::Recovery
            | CombatState::Aiming => true,
        }
    }
}

const _: () = {
    fn assert_all_is_exhaustive(state: CombatState) {
        match state {
            CombatState::Idle
            | CombatState::Windup
            | CombatState::Active
            | CombatState::Recovery
            | CombatState::Aiming => {}
        }
    }
    let _ = assert_all_is_exhaustive;
    let _ = CombatState::ALL.len();
};

/// Geometría del golpe activo, publicada para que presentación no abra estado
/// interno del motor. `None` fuera de `CombatState::Active`.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct SwingFacts(pub Option<SwingArc>);

/// Alcance y apertura del golpe en curso.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwingArc {
    pub reach: f32,
    pub arc_deg: f32,
}

/// Bow socket shared by projectile simulation and bow presentation.
pub const BOW_SOCKET_LOCAL: Vec3 = Vec3::new(0.35, 0.4, -0.55);

/// Per-actor draw charge. Resets when leaving `Aiming`. Presentation reads
/// this to contract the crosshair, tint the bow, etc.
#[derive(Component, Default)]
pub struct DrawStrength {
    pub factor: f32,
    pub charging: bool,
    pub cooldown: f32,
    /// Tuning captured on the first charging tick; only the motor writes it.
    pub tuning: Option<crate::combat::context::BowProfile>,
}

/// Shared aim pivot: simulation launches here and the camera converges on it.
pub const AIM_PIVOT_HEIGHT: f32 = 0.7;
pub const AIM_SHOULDER_OFFSET: f32 = 0.72;
