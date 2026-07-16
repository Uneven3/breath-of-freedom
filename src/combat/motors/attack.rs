//! Attack motor — owner of the `Windup/Active/Recovery` phases and the combo
//! chain (see `rationale/combat-combo-chains.md`).
//!
//! `propose` decides transitions from `ComboLocal`'s clock; the dispatcher's
//! `tick_phase_clock` advances that clock; `sweep_active_swings` runs the
//! hitbox while `Active`; `resolve_melee_hits` turns candidates into damage
//! cues and enemy aggro.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::combat::intent::CombatIntents;
use crate::combat::motors::CombatMotorTickItem;
use crate::combat::proposal::{CombatProposalBuffer, Priority, TransitionProposal, weight};
use crate::combat::state::CombatState;
use crate::combat::weapon::WeaponProfile;
use crate::enemies::perception::{Awareness, DirectThreatMessage};
use crate::movement::Actor;
use crate::movement::state::LocomotionState;
use crate::world::GameLayer;

/// Sneakstrike multiplier: melee on an unaware target while sneaking.
/// First-pass value (BotW's is 8×); tuned at the feeling checkpoint.
const SNEAKSTRIKE_MULT: f32 = 8.0;

/// Per-actor combo bookkeeping. A component, never a system `Local`
/// (multi-actor contract).
#[derive(Component, Default)]
pub struct ComboLocal {
    pub(crate) step: usize,
    pub(crate) phase_elapsed: f32,
    buffered: bool,
    last_phase: CombatState,
}

/// Entities already struck by the swing in progress — one hit per target per
/// swing. Fixed capacity, no allocation in the hot path.
#[derive(Component, Default)]
pub struct ActiveSwing {
    hit: [Option<Entity>; 8],
    len: usize,
}

impl ActiveSwing {
    fn contains(&self, entity: Entity) -> bool {
        self.hit[..self.len].iter().flatten().any(|e| *e == entity)
    }

    fn insert(&mut self, entity: Entity) {
        if self.len == self.hit.len() {
            warn!("ActiveSwing full: dropping hit on {entity:?}");
            return;
        }
        self.hit[self.len] = Some(entity);
        self.len += 1;
    }

    pub(crate) fn clear(&mut self) {
        self.hit = [None; 8];
        self.len = 0;
    }
}

/// One connected melee hit, pending resolution (damage math + aggro).
/// Combat-internal.
#[derive(Message, Debug, Clone, Copy)]
pub struct MeleeHitMessage {
    pub attacker: Entity,
    pub target: Entity,
    pub attacker_pos: Vec3,
    pub step: usize,
}

/// A resolved hit, published for presentation (flash, burst, damage text,
/// camera shake, hitstop). Combat owns the type; consumers read it without
/// Combat knowing they exist. (No `attacker` field until a consumer reads
/// one — Health gets its own `DamageRequestMessage` with attribution.)
#[derive(Message, Debug, Clone, Copy)]
pub struct HitImpactMessage {
    pub target: Entity,
    /// Where the impact reads best (target center).
    pub position: Vec3,
    pub damage: f32,
    /// A critical connection (sneakstrike today, headshot with the bow):
    /// bigger feedback — hitstop, louder text.
    pub critical: bool,
}

/// Planar knockback speed added to the struck body (m/s); the target's own
/// motor friction reabsorbs it. First-pass values, tuned at the checkpoint.
const KNOCKBACK_SPEED: f32 = 6.5;
const KNOCKBACK_SPEED_CRIT: f32 = 10.0;

type ProposeQuery<'a> = (
    &'a CombatIntents,
    &'a CombatState,
    &'a WeaponProfile,
    &'a mut ComboLocal,
    &'a mut CombatProposalBuffer,
);

/// Drives the phase machine: start from `Idle`, hold running phases, advance
/// on elapsed timers, chain on a buffered press inside the window. Expired
/// recoveries propose nothing — `Idle` (Default) wins by silence.
pub fn propose(mut q: Query<ProposeQuery, With<Actor>>) {
    for (intents, state, weapon, mut local, mut buffer) in &mut q {
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
                if intents.attack.pressed && weapon.has_step(0) {
                    let _ = buffer.push(TransitionProposal::new(
                        CombatState::Windup,
                        Priority::PlayerRequested,
                        weight::ATTACK_CHAIN,
                        "attack_start",
                    ));
                }
            }
            CombatState::Windup => {
                let Some(step) = weapon.step(local.step) else {
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
                let Some(step) = weapon.step(local.step) else {
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
                let Some(step) = weapon.step(local.step) else {
                    continue;
                };
                let in_chain_window = local.phase_elapsed <= step.chain_window_secs;
                if local.buffered && in_chain_window && weapon.has_step(local.step + 1) {
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
    &'a WeaponProfile,
    &'a ComboLocal,
    &'a mut ActiveSwing,
);

/// While `Active`: sphere sweep in front of the attacker, masked to
/// `GameLayer::Actor` — the exact inverse of Movement's sensing masks (hits
/// only see bodies, sensors only see world). Candidates inside the swing arc
/// become `MeleeHitMessage`s, once per swing.
pub fn sweep_active_swings(
    spatial: SpatialQuery,
    mut attackers: Query<SweepAttackerQuery, With<Actor>>,
    // No `With<Actor>` on the target side: anything on `GameLayer::Actor`
    // (actors, practice targets) is hittable — the collision layer is the
    // filter of record.
    targets: Query<&Transform>,
    mut hits: MessageWriter<MeleeHitMessage>,
) {
    for (attacker, transform, state, weapon, local, mut swing) in &mut attackers {
        if *state != CombatState::Active {
            continue;
        }
        let Some(step) = weapon.step(local.step) else {
            continue;
        };

        let forward = flat_forward(transform);
        let radius = step.reach * 0.5;
        let center = transform.translation + forward * radius;
        let filter =
            SpatialQueryFilter::from_mask(GameLayer::Actor).with_excluded_entities([attacker]);

        spatial.shape_intersections_callback(
            &Collider::sphere(radius),
            center,
            Quat::IDENTITY,
            &filter,
            |candidate| {
                if swing.contains(candidate) {
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
                ) {
                    swing.insert(candidate);
                    hits.write(MeleeHitMessage {
                        attacker,
                        target: candidate,
                        attacker_pos: transform.translation,
                        step: local.step,
                    });
                }
                true
            },
        );
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

/// Damage math, pure: stealth rules read the *target's* awareness
/// (`docs/architecture/combat.md` § Relaciones). A target without an
/// `Awareness` component (another player) counts as alerted — no bonus.
pub(crate) fn final_damage(
    base: f32,
    step_mult: f32,
    target_alerted: bool,
    attacker_sneaking: bool,
) -> f32 {
    let raw = base * step_mult;
    if !target_alerted && attacker_sneaking {
        raw * SNEAKSTRIKE_MULT
    } else {
        raw
    }
}

type HitAttackerQuery<'a> = (
    &'a WeaponProfile,
    Option<&'a LocomotionState>,
    Option<&'a Name>,
);
type HitTargetQuery<'a> = (&'a Transform, Option<&'a Awareness>, Option<&'a Name>);

/// Turns swept candidates into consequences: damage cue (placeholder for
/// `health::DamageRequestMessage` until `health-core` lands), instant aggro
/// on the struck enemy, knockback, and the public impact for presentation.
pub fn resolve_melee_hits(
    mut messages: MessageReader<MeleeHitMessage>,
    attackers: Query<HitAttackerQuery, With<Actor>>,
    // Target side matches the sweep: layer-gated, not marker-gated.
    targets: Query<HitTargetQuery>,
    mut threats: MessageWriter<DirectThreatMessage>,
    mut impulses: MessageWriter<crate::movement::constraints::BodyImpulseMessage>,
    mut impacts: MessageWriter<HitImpactMessage>,
) {
    for hit in messages.read() {
        let Ok((weapon, locomotion, attacker_name)) = attackers.get(hit.attacker) else {
            continue;
        };
        let Some(step) = weapon.step(hit.step) else {
            continue;
        };
        let Ok((target_tf, awareness, target_name)) = targets.get(hit.target) else {
            continue;
        };

        let target_alerted = awareness.is_none_or(Awareness::is_alerted);
        let sneaking = matches!(locomotion, Some(LocomotionState::Sneak));
        let critical = !target_alerted && sneaking;
        let damage = final_damage(
            weapon.base_damage,
            step.damage_mult,
            target_alerted,
            sneaking,
        );

        let attacker_label = attacker_name.map(Name::as_str).unwrap_or("attacker");
        let target_label = target_name.map(Name::as_str).unwrap_or("target");
        let sneak_tag = if critical { " (SNEAKSTRIKE)" } else { "" };
        // Placeholder for health::DamageRequestMessage (ticket `health-core`).
        info!("[combat] cue: {attacker_label} hit {target_label} for {damage:.0}{sneak_tag}");

        // Knockback: shove the target away from the attacker (planar; the
        // target's own motor reabsorbs it).
        let away = target_tf.translation - hit.attacker_pos;
        let away = Vec3::new(away.x, 0.0, away.z).normalize_or_zero();
        let speed = if critical {
            KNOCKBACK_SPEED_CRIT
        } else {
            KNOCKBACK_SPEED
        };
        impulses.write(crate::movement::constraints::BodyImpulseMessage {
            entity: hit.target,
            impulse: away * speed,
        });

        impacts.write(HitImpactMessage {
            target: hit.target,
            position: target_tf.translation,
            damage,
            critical,
        });

        threats.write(DirectThreatMessage {
            enemy: hit.target,
            threat_position: hit.attacker_pos,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn armed_actor(state: CombatState, local: ComboLocal, pressed: bool) -> impl Bundle {
        (
            Actor,
            CombatIntents {
                attack: crate::combat::intent::AttackIntent {
                    pressed,
                    held: pressed,
                },
                ..default()
            },
            state,
            WeaponProfile::GRAYBOX_SWORD,
            local,
            CombatProposalBuffer::default(),
        )
    }

    fn proposals(world: &World, entity: Entity) -> Vec<TransitionProposal> {
        world
            .entity(entity)
            .get::<CombatProposalBuffer>()
            .unwrap()
            .iter()
            .cloned()
            .collect()
    }

    #[test]
    fn pressing_attack_from_idle_proposes_windup() {
        let mut world = World::new();
        let armed = world
            .spawn(armed_actor(CombatState::Idle, ComboLocal::default(), true))
            .id();
        let calm = world
            .spawn(armed_actor(CombatState::Idle, ComboLocal::default(), false))
            .id();

        world.run_system_once(propose).unwrap();

        let out = proposals(&world, armed);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].target_state, CombatState::Windup);
        assert_eq!(out[0].category, Priority::PlayerRequested);
        assert!(proposals(&world, calm).is_empty());
    }

    #[test]
    fn windup_holds_then_advances_to_active() {
        let step0 = WeaponProfile::GRAYBOX_SWORD.step(0).unwrap();

        let mut world = World::new();
        let holding = world
            .spawn(armed_actor(
                CombatState::Windup,
                ComboLocal {
                    phase_elapsed: step0.windup_secs * 0.5,
                    ..default()
                },
                false,
            ))
            .id();
        let done = world
            .spawn(armed_actor(
                CombatState::Windup,
                ComboLocal {
                    phase_elapsed: step0.windup_secs,
                    ..default()
                },
                false,
            ))
            .id();

        world.run_system_once(propose).unwrap();

        assert_eq!(
            proposals(&world, holding)[0].target_state,
            CombatState::Windup
        );
        assert_eq!(proposals(&world, done)[0].target_state, CombatState::Active);
        assert!(
            proposals(&world, done)[0].override_weight
                > proposals(&world, holding)[0].override_weight,
            "the advance must out-arbitrate a hold"
        );
    }

    #[test]
    fn recovery_chains_only_buffered_inside_the_window() {
        let step0 = WeaponProfile::GRAYBOX_SWORD.step(0).unwrap();

        let mut world = World::new();
        // Buffered press, inside the window → chain to step 1.
        let chains = world
            .spawn(armed_actor(
                CombatState::Recovery,
                ComboLocal {
                    buffered: true,
                    phase_elapsed: step0.chain_window_secs * 0.5,
                    ..default()
                },
                false,
            ))
            .id();
        // Buffered but too late → just holds recovery.
        let late = world
            .spawn(armed_actor(
                CombatState::Recovery,
                ComboLocal {
                    buffered: true,
                    phase_elapsed: step0.chain_window_secs + 0.01,
                    ..default()
                },
                false,
            ))
            .id();

        world.run_system_once(propose).unwrap();

        let chained = proposals(&world, chains);
        assert_eq!(chained[0].target_state, CombatState::Windup);
        assert_eq!(chained[0].source_id, "attack_chain");
        assert_eq!(
            world.entity(chains).get::<ComboLocal>().unwrap().step,
            1,
            "chaining advances the combo step"
        );

        assert_eq!(
            proposals(&world, late)[0].target_state,
            CombatState::Recovery
        );
        assert_eq!(world.entity(late).get::<ComboLocal>().unwrap().step, 0);
    }

    #[test]
    fn finisher_never_chains_and_expired_recovery_goes_silent() {
        let finisher_index = 2;
        let finisher = WeaponProfile::GRAYBOX_SWORD.step(finisher_index).unwrap();

        let mut world = World::new();
        let at_finisher = world
            .spawn(armed_actor(
                CombatState::Recovery,
                ComboLocal {
                    step: finisher_index,
                    buffered: true,
                    phase_elapsed: 0.01,
                    ..default()
                },
                false,
            ))
            .id();
        let expired = world
            .spawn(armed_actor(
                CombatState::Recovery,
                ComboLocal {
                    phase_elapsed: finisher.recovery_secs + 1.0,
                    ..default()
                },
                false,
            ))
            .id();

        world.run_system_once(propose).unwrap();

        assert_eq!(
            proposals(&world, at_finisher)[0].target_state,
            CombatState::Recovery,
            "the finisher's buffered press must not chain past the end"
        );
        assert!(
            proposals(&world, expired).is_empty(),
            "an expired recovery proposes nothing: Idle wins by silence"
        );
    }

    #[test]
    fn combo_state_does_not_bleed_between_actors() {
        let mut world = World::new();
        let fresh = world
            .spawn(armed_actor(CombatState::Idle, ComboLocal::default(), true))
            .id();
        let mid_combo = world
            .spawn(armed_actor(
                CombatState::Recovery,
                ComboLocal {
                    step: 1,
                    buffered: false,
                    phase_elapsed: 0.05,
                    ..default()
                },
                false,
            ))
            .id();

        world.run_system_once(propose).unwrap();

        assert_eq!(world.entity(fresh).get::<ComboLocal>().unwrap().step, 0);
        assert_eq!(world.entity(mid_combo).get::<ComboLocal>().unwrap().step, 1);
        assert_eq!(
            proposals(&world, fresh)[0].target_state,
            CombatState::Windup
        );
        assert_eq!(
            proposals(&world, mid_combo)[0].target_state,
            CombatState::Recovery
        );
    }

    #[test]
    fn sneakstrike_multiplies_only_unaware_targets_while_sneaking() {
        let base = 10.0;
        assert_eq!(
            final_damage(base, 1.0, true, true),
            10.0,
            "alerted: no bonus"
        );
        assert_eq!(
            final_damage(base, 1.0, false, false),
            10.0,
            "unaware but not sneaking: normal melee"
        );
        assert_eq!(
            final_damage(base, 1.0, false, true),
            10.0 * SNEAKSTRIKE_MULT,
            "unaware + sneaking = sneakstrike"
        );
        assert_eq!(final_damage(base, 1.6, true, false), 16.0);
    }

    #[test]
    fn swing_arc_accepts_front_and_rejects_flanks() {
        let pos = Vec3::new(0.0, 1.0, 0.0);
        let forward = Vec3::NEG_Z;
        assert!(within_swing_arc(
            pos,
            forward,
            Vec3::new(0.0, 1.0, -1.5),
            100.0
        ));
        assert!(within_swing_arc(
            pos,
            forward,
            Vec3::new(0.7, 1.0, -1.0),
            100.0
        ));
        assert!(!within_swing_arc(
            pos,
            forward,
            Vec3::new(1.5, 1.0, 0.0),
            100.0
        ));
        assert!(!within_swing_arc(
            pos,
            forward,
            Vec3::new(0.0, 1.0, 1.5),
            100.0
        ));
    }

    #[test]
    fn active_swing_deduplicates_targets() {
        let mut swing = ActiveSwing::default();
        let target = Entity::PLACEHOLDER;
        assert!(!swing.contains(target));
        swing.insert(target);
        assert!(swing.contains(target));
        swing.clear();
        assert!(!swing.contains(target));
    }
}
