//! The single source of truth for the active combat mode.
//!
//! Same contract as `movement::state::LocomotionState`: mutually-exclusive
//! phases as an enum, one writer (`combat::arbitrate`). The combo *step*
//! deliberately does NOT live here — `Windup/Active/Recovery` repeat per
//! step and `ComboLocal.step` says which one (see
//! `docs/ARCHITECTURE.md`).
//!
//! Guarding, Parrying and Staggered remain future work. Adding any variant is
//! a compile error until the dispatcher handles it.

use bevy_ecs::prelude::*;
use bevy_math::Vec3;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CombatState {
    /// Not committed to any combat action.
    #[default]
    Idle,
    /// Committed to a strike, hitbox not yet live. Not cancelable by attack.
    Windup,
    /// Hitbox live: the swing sweep runs.
    Active,
    /// Follow-through: vulnerable; the chain window for the next step.
    Recovery,
    /// Bow drawn: attack releases an arrow along the control orientation.
    Aiming,
}

impl CombatState {
    /// Every variant, for exhaustive audits. The compile-time guard below
    /// fails the build if a variant is added without being listed here.
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

/// Geometría del golpe que está activo **ahora**, publicada por el motor de
/// ataque en cada tick de `CombatState::Active`.
///
/// Existe para que presentación pueda dibujar el arco del swing sin preguntarle
/// nada al motor: el alcance y la apertura son dato derivado del `AttackStep` en
/// curso, y §19 dice que el dato se separa del sistema que lo produce. Antes el
/// VFX llamaba a `ComboLocal::current_step`, lo que obligaba a abrir estado
/// interno del motor para que lo leyera otra capa.
///
/// `None` entre golpes: fuera de la fase activa no hay arco que dibujar.
///
/// (`ActiveSwing`, en simulación, es otra cosa: la lista de entidades ya
/// golpeadas por este barrido.)
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct SwingFacts(pub Option<SwingArc>);

/// Alcance y apertura del golpe en curso.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwingArc {
    /// Metros desde el cuerpo hasta la punta del barrido.
    pub reach: f32,
    /// Apertura total del abanico, en grados.
    pub arc_deg: f32,
}

/// Bow socket relative to the body (shoulder level — where a drawn bow is
/// held — slightly right and forward). The projectile origin is simulation
/// (§20); presentación coloca la malla del arco en este mismo offset para que
/// la flecha salga visiblemente del arco. Compartido, por eso vive acá.
pub const BOW_SOCKET_LOCAL: Vec3 = Vec3::new(0.35, 0.4, -0.55);

/// Per-actor draw charge. Resets when leaving `Aiming`. Presentation reads
/// this to contract the crosshair, tint the bow, etc.
#[derive(Component, Default)]
pub struct DrawStrength {
    /// 0.0 = uncharged, 1.0 = full draw.
    pub factor: f32,
    /// Whether the player is actively holding the attack button to charge.
    pub charging: bool,
    /// Delay after firing before you can notch and pull another arrow.
    pub cooldown: f32,
    /// Effective tuning captured on the first charging tick. Context changes
    /// cannot retune a draw already in progress. Sólo el motor lo escribe.
    pub tuning: Option<crate::combat::context::BowProfile>,
}

/// Altura del pivote de apuntado sobre el centro del cuerpo (~1.7 m sobre los
/// pies con un centro a ~1.0 m), y su desplazamiento a la derecha.
///
/// Es simulación quien define desde dónde sale la flecha (§20), pero la cámara
/// tiene que converger con esa línea: si el pivote de la vista y el origen del
/// proyectil se separan, las flechas vuelan visiblemente diagonales. Compartido,
/// por eso vive acá y no en el motor.
pub const AIM_PIVOT_HEIGHT: f32 = 0.7;
pub const AIM_SHOULDER_OFFSET: f32 = 0.72;
