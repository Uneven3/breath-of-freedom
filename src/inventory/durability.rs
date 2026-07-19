//! Weapon durability: reads Combat's public impact contract, never its
//! internals (§5). `HitImpactMessage` has three producers — melee, arrows,
//! horse charge — so `melee` is the load-bearing filter here: without it, a
//! bow shot or a charge would wear down the equipped melee weapon, because
//! `attacker` is the same actor in all three paths.

use bevy::prelude::*;

use crate::combat::motors::attack::HitImpactMessage;
use crate::combat::weapon::WeaponProfile;

use super::data::{WeaponBrokeMessage, WeaponDurability};

/// One melee hit connecting costs the equipped weapon one point of
/// durability. First-pass value, tuned at the feeling checkpoint like every
/// other combat constant.
const DURABILITY_COST_PER_HIT: u32 = 1;

pub fn track_weapon_durability(
    mut impacts: MessageReader<HitImpactMessage>,
    mut weapons: Query<&mut WeaponDurability>,
    mut broke: MessageWriter<WeaponBrokeMessage>,
) {
    for impact in impacts.read() {
        if !impact.melee {
            continue;
        }
        let Ok(mut durability) = weapons.get_mut(impact.attacker) else {
            continue;
        };
        if durability.is_broken() {
            continue;
        }
        durability.apply_hit(DURABILITY_COST_PER_HIT);
        if durability.is_broken() {
            info!("[inventory] {} broke", durability.label());
            broke.write(WeaponBrokeMessage {
                actor: impact.attacker,
            });
        }
    }
}

/// Unequips whatever `WeaponBrokeMessage` names — a broken weapon is
/// discarded, not returned to `Inventory` (same as BotW).
pub fn unequip_broken_weapons(
    mut breaks: MessageReader<WeaponBrokeMessage>,
    mut commands: Commands,
) {
    for message in breaks.read() {
        commands
            .entity(message.actor)
            .remove::<WeaponProfile>()
            .remove::<WeaponDurability>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    use crate::inventory::data::WeaponItem;

    fn impact(attacker: Entity, target: Entity, melee: bool) -> HitImpactMessage {
        HitImpactMessage {
            target,
            attacker,
            position: Vec3::ZERO,
            damage: 10.0,
            critical: false,
            melee,
        }
    }

    #[test]
    fn non_melee_impacts_never_wear_down_the_equipped_weapon() {
        let mut world = World::new();
        world.init_resource::<Messages<HitImpactMessage>>();
        world.init_resource::<Messages<WeaponBrokeMessage>>();
        let attacker = world
            .spawn(WeaponDurability::new(WeaponItem::GRAYBOX_SWORD))
            .id();
        let target = world.spawn_empty().id();
        world.write_message(impact(attacker, target, false));

        world.run_system_once(track_weapon_durability).unwrap();

        assert_eq!(
            world
                .entity(attacker)
                .get::<WeaponDurability>()
                .unwrap()
                .current(),
            40,
            "an arrow or a charge must not touch the melee weapon's durability"
        );
    }

    #[test]
    fn breaking_emits_weapon_broke_exactly_once() {
        let mut world = World::new();
        world.init_resource::<Messages<HitImpactMessage>>();
        world.init_resource::<Messages<WeaponBrokeMessage>>();
        let attacker = world
            .spawn(WeaponDurability::new(WeaponItem::LOOTABLE_CLUB))
            .id();
        let target = world.spawn_empty().id();
        for _ in 0..20 {
            world.write_message(impact(attacker, target, true));
        }

        world.run_system_once(track_weapon_durability).unwrap();

        assert!(
            world
                .entity(attacker)
                .get::<WeaponDurability>()
                .unwrap()
                .is_broken()
        );
        let messages = world.resource::<Messages<WeaponBrokeMessage>>();
        let mut cursor = messages.get_cursor();
        let breaks: Vec<_> = cursor.read(messages).collect();
        assert_eq!(breaks.len(), 1, "exactly once, not once per hit past zero");
        assert_eq!(breaks[0].actor, attacker);
    }

    #[test]
    fn unequip_removes_profile_and_durability_together() {
        let mut world = World::new();
        let actor = world
            .spawn((
                WeaponProfile::GRAYBOX_SWORD,
                WeaponDurability::new(WeaponItem::GRAYBOX_SWORD),
            ))
            .id();
        world.init_resource::<Messages<WeaponBrokeMessage>>();
        world.write_message(WeaponBrokeMessage { actor });

        world.run_system_once(unequip_broken_weapons).unwrap();

        let entity = world.entity(actor);
        assert!(!entity.contains::<WeaponProfile>());
        assert!(!entity.contains::<WeaponDurability>());
    }
}
