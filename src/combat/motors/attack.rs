//! Attack motor — owner of the `Windup/Active/Recovery` phases and the combo
//! chain (see `docs/ARCHITECTURE.md`).
//!
//! `propose` decides transitions from `ComboLocal`'s clock; the dispatcher's
//! `tick_phase_clock` advances that clock; `sweep_active_swings` runs the
//! hitbox while `Active`; `resolve_melee_hits` turns candidates into damage
//! cues and enemy aggro.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::combat::context::effective_weapon;
use crate::combat::context_data::{CombatContext, MountedCombatProfile};
use crate::combat::intent::CombatIntents;
use crate::combat::motors::CombatMotorTickItem;
use crate::combat::proposal::{CombatProposalBuffer, Priority, TransitionProposal, weight};
use crate::combat::state::CombatState;
use crate::combat::weapon::WeaponProfile;
use crate::movement::Actor;
use crate::world::GameLayer;

pub use super::attack_data::{
    ActiveSwing, ComboLocal, HitImpactMessage, MeleeHitMessage, MeleeSweepShapes,
};
pub use super::attack_resolution::resolve_melee_hits;

type ProposeQuery<'a> = (
    &'a CombatIntents,
    &'a CombatState,
    &'a WeaponProfile,
    Option<&'a CombatContext>,
    Option<&'a MountedCombatProfile>,
    &'a mut ComboLocal,
    &'a mut CombatProposalBuffer,
);

/// Drives the phase machine: start from `Idle`, hold running phases, advance
/// on elapsed timers, chain on a buffered press inside the window. Expired
/// recoveries propose nothing — `Idle` (Default) wins by silence.
pub fn propose(mut q: Query<ProposeQuery, With<Actor>>) {
    for (intents, state, base_weapon, context, mounted, mut local, mut buffer) in &mut q {
        // Buffer only inside the melee phases: an attack press while Aiming
        // is the bow release (`aim::shoot_drawn_arrow`), not a queued swing.
        let in_melee_phase = matches!(
            *state,
            CombatState::Windup | CombatState::Active | CombatState::Recovery
        );
        if intents.attack.pressed && in_melee_phase {
            local.buffered = true;
        }

        match *state {
            CombatState::Idle => {
                local.step = 0;
                local.buffered = false;
                local.snapshot = None;
                let effective = effective_weapon(*base_weapon, context, mounted);
                if intents.attack.pressed && effective.has_step(0) {
                    local.snapshot = Some(effective);
                    let _ = buffer.push(TransitionProposal::new(
                        CombatState::Windup,
                        Priority::PlayerRequested,
                        weight::ATTACK_CHAIN,
                        "attack_start",
                    ));
                }
            }
            CombatState::Windup => {
                let Some(step) = local.current_step() else {
                    continue;
                };
                let (target, w, id) = if local.phase_elapsed >= step.windup_secs {
                    (CombatState::Active, weight::ATTACK_ADVANCE, "attack_active")
                } else {
                    (CombatState::Windup, weight::ATTACK_HOLD, "attack_windup")
                };
                let _ = buffer.push(TransitionProposal::new(target, Priority::Forced, w, id));
            }
            CombatState::Active => {
                let Some(step) = local.current_step() else {
                    continue;
                };
                let (target, w, id) = if local.phase_elapsed >= step.active_secs {
                    (
                        CombatState::Recovery,
                        weight::ATTACK_ADVANCE,
                        "attack_recovery",
                    )
                } else {
                    (CombatState::Active, weight::ATTACK_HOLD, "attack_active")
                };
                let _ = buffer.push(TransitionProposal::new(target, Priority::Forced, w, id));
            }
            CombatState::Recovery => {
                let Some(profile) = local.snapshot else {
                    continue;
                };
                let Some(step) = profile.step(local.step) else {
                    continue;
                };
                let in_chain_window = local.phase_elapsed <= step.chain_window_secs;
                if local.buffered && in_chain_window && profile.has_step(local.step + 1) {
                    local.buffered = false;
                    local.step += 1;
                    let _ = buffer.push(TransitionProposal::new(
                        CombatState::Windup,
                        Priority::Forced,
                        weight::ATTACK_CHAIN,
                        "attack_chain",
                    ));
                } else if local.phase_elapsed < step.recovery_secs {
                    let _ = buffer.push(TransitionProposal::new(
                        CombatState::Recovery,
                        Priority::Forced,
                        weight::ATTACK_HOLD,
                        "attack_recovery",
                    ));
                }
                // Recovery over: silence → Idle wins.
            }
            // The bow release is the aim motor's job, not a melee phase.
            CombatState::Aiming => {}
        }
    }
}

/// Advances the per-phase clock and detects phase entry (reset + swing clear
/// on entering `Active`). Called by the dispatcher for every row.
pub(super) fn tick_phase_clock(row: &mut CombatMotorTickItem, time: &Time) {
    let Some(combo) = row.combo.as_mut() else {
        return;
    };
    if combo.last_phase != *row.state {
        combo.last_phase = *row.state;
        combo.phase_elapsed = 0.0;
        if *row.state == CombatState::Active
            && let Some(swing) = row.swing.as_mut()
        {
            swing.clear();
        }
    }
    combo.phase_elapsed += time.delta_secs();
}

type SweepAttackerQuery<'a> = (
    Entity,
    &'a Transform,
    &'a CombatState,
    &'a ComboLocal,
    &'a mut ActiveSwing,
);

/// While `Active`: sphere sweep in front of the attacker, masked to
/// `GameLayer::Actor` — the exact inverse of Movement's sensing masks (hits
/// only see bodies, sensors only see world). Candidates inside the swing arc
/// become `MeleeHitMessage`s, once per swing.
pub fn sweep_active_swings(
    spatial: SpatialQuery,
    shapes: Res<MeleeSweepShapes>,
    mut attackers: Query<SweepAttackerQuery, With<Actor>>,
    // No `With<Actor>` on the target side: anything on `GameLayer::Actor`
    // (actors, practice targets) is hittable — the collision layer is the
    // filter of record.
    targets: Query<&Transform>,
    mut hits: MessageWriter<MeleeHitMessage>,
) {
    for (attacker, transform, state, local, mut swing) in &mut attackers {
        if *state != CombatState::Active {
            continue;
        }
        let Some(step) = local.current_step() else {
            continue;
        };

        let forward = flat_forward(transform);
        let radius = step.reach * 0.5;
        let Some(shape) = shapes.get(radius) else {
            error!("missing cached melee sweep shape for reach {}", step.reach);
            continue;
        };
        let center = transform.translation + forward * radius;
        // Mask-only filter: excluding the attacker with `with_excluded_entities`
        // would allocate an `EntityHashSet` every Active tick (§18). The
        // attacker is already in scope, so it is skipped in the callback
        // instead — the same way the arrow pool reuses one filter to avoid
        // this exact per-tick allocation.
        let filter = SpatialQueryFilter::from_mask(GameLayer::Actor);

        spatial.shape_intersections_callback(shape, center, Quat::IDENTITY, &filter, |candidate| {
            if candidate == attacker || swing.contains(candidate) {
                return true;
            }
            let Ok(target_tf) = targets.get(candidate) else {
                return true;
            };
            if within_swing_arc(
                transform.translation,
                forward,
                target_tf.translation,
                step.arc_deg,
            ) && swing.insert(candidate)
            {
                hits.write(MeleeHitMessage {
                    attacker,
                    target: candidate,
                    attacker_pos: transform.translation,
                    step: local.step,
                });
            }
            true
        });
    }
}

fn flat_forward(transform: &Transform) -> Vec3 {
    let forward = transform.rotation * Vec3::NEG_Z;
    Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero()
}

/// Horizontal arc check: the target's center must be within `arc_deg`
/// centered on facing (the sweep sphere already bounded the distance).
pub(crate) fn within_swing_arc(
    attacker_pos: Vec3,
    flat_forward: Vec3,
    target_pos: Vec3,
    arc_deg: f32,
) -> bool {
    let to_target = target_pos - attacker_pos;
    let Some(flat_dir) = Vec3::new(to_target.x, 0.0, to_target.z).try_normalize() else {
        // Directly above/below: inside any arc.
        return true;
    };
    flat_forward.dot(flat_dir) >= (arc_deg.to_radians() / 2.0).cos()
}
