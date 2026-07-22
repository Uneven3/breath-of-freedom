//! Authoritative consequences of accepted melee sweep candidates.

use bevy::prelude::*;

use super::attack_data::{ComboLocal, HitImpactMessage, MeleeHitMessage};
use crate::enemies::perception::{Awareness, DirectThreatMessage};
use crate::movement::Actor;
use crate::movement::state::LocomotionState;

pub(super) const SNEAKSTRIKE_MULT: f32 = 8.0;
const KNOCKBACK_SPEED: f32 = 6.5;
const KNOCKBACK_SPEED_CRIT: f32 = 10.0;

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
    &'a ComboLocal,
    Option<&'a LocomotionState>,
    Option<&'a Name>,
);
type HitTargetQuery<'a> = (
    &'a Transform,
    Option<&'a Awareness>,
    Option<&'a Name>,
    Option<&'a crate::health::HostileInteractionImmunity>,
);

pub fn resolve_melee_hits(
    mut messages: MessageReader<MeleeHitMessage>,
    attackers: Query<HitAttackerQuery, With<Actor>>,
    targets: Query<HitTargetQuery>,
    mut damage_requests: MessageWriter<crate::health::DamageRequestMessage>,
    mut threats: MessageWriter<DirectThreatMessage>,
    mut impulses: MessageWriter<crate::movement::constraints::BodyImpulseMessage>,
    mut impacts: MessageWriter<HitImpactMessage>,
) {
    for hit in messages.read() {
        let Ok((combo, locomotion, attacker_name)) = attackers.get(hit.attacker) else {
            continue;
        };
        let Some(profile) = combo.snapshot else {
            continue;
        };
        let Some(step) = profile.step(hit.step) else {
            continue;
        };
        let Ok((target_tf, awareness, target_name, immunity)) = targets.get(hit.target) else {
            continue;
        };
        if immunity.is_some_and(|immunity| immunity.blocks(hit.attacker)) {
            continue;
        }

        let target_alerted = awareness.is_none_or(Awareness::is_alerted);
        let sneaking = matches!(locomotion, Some(LocomotionState::Sneak));
        let critical = !target_alerted && sneaking;
        let damage = final_damage(
            profile.base_damage,
            step.damage_mult,
            target_alerted,
            sneaking,
        );
        if critical {
            info!(
                "[combat] SNEAKSTRIKE: {} on {}",
                attacker_name.map(Name::as_str).unwrap_or("attacker"),
                target_name.map(Name::as_str).unwrap_or("target")
            );
        }
        damage_requests.write(crate::health::DamageRequestMessage {
            target: hit.target,
            amount: damage,
            source: Some(hit.attacker),
        });
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
            attacker: hit.attacker,
            position: target_tf.translation,
            damage,
            critical,
            melee: true,
        });
        threats.write(DirectThreatMessage {
            enemy: hit.target,
            threat_position: hit.attacker_pos,
        });
    }
}
