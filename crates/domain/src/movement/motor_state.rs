//! Estado por actor de los motores de locomoción.
//!
//! Latches and counters kept beside domain capabilities so their `#[require]`
//! contracts cannot spawn incomplete actors. `KinematicArc` alone hides fields
//! because it maintains `elapsed <= duration`.

use bevy_ecs::prelude::*;
use bevy_math::Vec3;

use super::facts::StairsFacts;

/// Sprint bloqueado hasta que la stamina se recupere por encima del umbral.
#[derive(Component, Default)]
pub struct SprintLock(pub bool);

/// Sneak bloqueado por stamina, igual que [`SprintLock`].
#[derive(Component, Default)]
pub struct SneakLock(pub bool);

/// Si la cápsula de pie entra sin chocar contra un techo. Un actor agachado
/// sigue agachado mientras esto sea falso, aunque suelte el botón.
#[derive(Component, Default)]
pub struct StandClearance(pub bool);

/// Si el airtime actual lo inició un salto del jugador (y no una caída).
#[derive(Component, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct JumpPhase {
    pub is_player_jump: bool,
}

/// Contabilidad del salto: coyote time, buffer de input y detección de flanco.
#[derive(Component, Default, Debug, Clone, Copy, PartialEq)]
pub struct JumpLocal {
    pub coyote: f32,
    pub buffer: f32,
    pub was_on_floor: bool,
    pub prev_wants: bool,
    pub needs_release: bool,
}

impl JumpLocal {
    /// Mete al actor en su ventana de coyote sin esperar a que el suelo se le
    /// vaya. `mounts` lo usa para armar el caso de desmontar y saltar.
    pub fn grant_coyote(&mut self, seconds: f32) {
        self.coyote = seconds;
    }
}

/// Memoria de pulsación del planeo, por actor.
#[derive(Component, Default)]
pub struct GlideLocal {
    pub prev_wants: bool,
    pub was_glide: bool,
}

/// Última geometría de escalera válida vista mientras Stairs estuvo activo.
/// Cubre la ventana de gracia en la que `on_stairs` parpadea entre escalones.
#[derive(Component, Default)]
pub struct StairsLocal(pub Option<StairsFacts>);

/// Frames que Stairs sigue proponiéndose tras perder el trigger, para no
/// oscilar en el borde del volumen.
#[derive(Component, Default)]
pub struct StairsGrace(pub u32);

/// Máquina de fases del mantle.
#[derive(Component, Default)]
pub struct MantleState {
    pub arc: KinematicArc,
    pub needs_release: bool,
}

/// Máquina de fases del vault automático.
#[derive(Component, Default)]
pub struct VaultState {
    pub arc: KinematicArc,
}

/// Máquina de fases del salto de pared.
#[derive(Component, Default)]
pub struct WallJumpState {
    pub is_jumping: bool,
    pub timer: f32,
    pub needs_release: bool,
    /// La arma `propose` y la consume el primer tick activo de `tick` (el
    /// impulso de lanzamiento). Un flag explícito, no comparar el timer contra
    /// su duración: ese float no identifica de forma confiable "el primer tick".
    pub launch_pending: bool,
}

/// Máquina de fases del salto desde borde. Ver [`WallJumpState`].
#[derive(Component, Default)]
pub struct EdgeLeapState {
    pub is_leaping: bool,
    pub timer: f32,
    pub needs_release: bool,
    pub launch_pending: bool,
}

/// Arco de posición compartido por Mantle y AutoVault: smoothstep de `start` a
/// `target` con una joroba senoidal de altura.
///
/// Único de este módulo que conserva campos privados: `elapsed <= duration` es
/// un invariante real, y `step` es quien lo sostiene.
#[derive(Default)]
pub struct KinematicArc {
    pub running: bool,
    elapsed: f32,
    duration: f32,
    start: Vec3,
    target: Vec3,
}

impl KinematicArc {
    pub fn begin(&mut self, start: Vec3, target: Vec3, duration: f32) {
        self.start = start;
        self.target = target;
        self.duration = duration;
        self.elapsed = 0.0;
        self.running = true;
    }

    /// Avanza `dt` y devuelve la próxima posición del cuerpo; en el último paso
    /// aterriza exactamente en `target` y apaga `running`.
    pub fn step(&mut self, dt: f32, arc_height: f32) -> Vec3 {
        self.elapsed = (self.elapsed + dt).min(self.duration);
        let raw = self.elapsed / self.duration;
        if raw >= 1.0 {
            self.running = false;
            return self.target;
        }
        let mut next = self.start.lerp(self.target, smoothstep(raw));
        next.y += (raw * core::f32::consts::PI).sin() * arc_height;
        next
    }
}

/// `smoothstep(0, 1, x)` = x²(3 − 2x).
fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}
