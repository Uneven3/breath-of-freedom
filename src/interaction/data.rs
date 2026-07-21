//! Interaction contract — pure data (§6, §19).

use bevy::prelude::*;

/// What an `Interact` press means for a given target. The arbiter never acts
/// on these; it only decides *which one* wins. Each domain matches on its own
/// variants, so adding a campfire or a dialogue costs a variant here and a
/// reader there — not another competing consumer of the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionKind {
    Mount,
    Dismount,
    Pickup,
}

/// Marks a world entity as a candidate. Whoever owns the entity inserts and
/// removes this as its availability changes (`mounts` drops it from a ridden
/// horse), so the arbiter never has to know what "available" means per domain
/// — that knowledge stays with the owner (§7).
#[derive(Component, Debug, Clone, Copy)]
pub struct Interactable {
    pub kind: InteractionKind,
    /// Reach measured from the actor's origin. Per-target rather than global:
    /// a horse and a dropped apple were never meant to share a radius.
    pub range: f32,
}

/// Placed on the *actor* while an ongoing relationship claims the key. While
/// present the arbiter stops searching and emits this directly — being mounted
/// means `Interact` dismounts, no matter what else is within reach.
///
/// This is what keeps priority declarative: without it, "dismount beats
/// everything" would live as an early `continue` inside one domain's system,
/// invisible to the others.
#[derive(Component, Debug, Clone, Copy)]
pub struct InteractionOverride {
    pub kind: InteractionKind,
}

/// The single resolved interaction for one press. Domains read this and match
/// on `kind`; nobody else consumes `IntentAction::Interact`.
#[derive(Message, Debug, Clone, Copy)]
pub struct InteractionRequest {
    pub actor: Entity,
    /// `None` when an [`InteractionOverride`] resolved it — the target is
    /// already implied by the relationship that installed the override.
    pub target: Option<Entity>,
    pub kind: InteractionKind,
}

/// Picks the nearest candidate within its own range.
///
/// Extracted because the same filter + `min_by(distance_squared)` had been
/// copied into `mounts::lifecycle` and `inventory::pickup`; a third contextual
/// system would have copied it again.
pub fn nearest_candidate<T>(
    origin: Vec3,
    candidates: impl Iterator<Item = (Entity, Vec3, T)>,
    reach: impl Fn(&T) -> f32,
) -> Option<(Entity, T)> {
    candidates
        .filter(|(_, position, data)| position.distance(origin) <= reach(data))
        .min_by(|(_, left, _), (_, right, _)| {
            left.distance_squared(origin)
                .total_cmp(&right.distance_squared(origin))
        })
        .map(|(entity, _, data)| (entity, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(x: f32, range: f32) -> (Entity, Vec3, f32) {
        (
            Entity::from_raw_u32(x as u32 + 1).unwrap(),
            Vec3::X * x,
            range,
        )
    }

    #[test]
    fn picks_the_closest_candidate_in_range() {
        let found = nearest_candidate(
            Vec3::ZERO,
            [
                candidate(5.0, 10.0),
                candidate(2.0, 10.0),
                candidate(8.0, 10.0),
            ]
            .into_iter(),
            |range| *range,
        );
        assert_eq!(found.map(|(_, range)| range), Some(10.0));
        let (entity, _) = found.expect("a candidate is in range");
        assert_eq!(entity, candidate(2.0, 10.0).0, "the nearest one wins");
    }

    /// Range is per-target on purpose: a far target with a generous reach must
    /// still qualify, and a near one with a tiny reach must not.
    #[test]
    fn range_is_evaluated_per_candidate() {
        let found = nearest_candidate(
            Vec3::ZERO,
            [candidate(1.0, 0.5), candidate(4.0, 9.0)].into_iter(),
            |range| *range,
        );
        assert_eq!(found.map(|(entity, _)| entity), Some(candidate(4.0, 9.0).0));
    }

    #[test]
    fn nothing_in_range_is_no_interaction() {
        let found = nearest_candidate(Vec3::ZERO, [candidate(50.0, 1.0)].into_iter(), |range| {
            *range
        });
        assert!(found.is_none());
    }
}
