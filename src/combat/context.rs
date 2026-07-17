use bevy::prelude::*;

pub use super::context_data::{
    BowProfile, CombatContext, MountedCombatProfile, SetMountedCombatMessage,
};
use super::weapon::WeaponProfile;

pub fn apply_mounted_context(
    mut messages: MessageReader<SetMountedCombatMessage>,
    mut actors: Query<&mut CombatContext>,
) {
    for message in messages.read() {
        if let Ok(mut context) = actors.get_mut(message.actor) {
            context.mounted = message.mounted;
        }
    }
}

pub fn effective_weapon(
    base: WeaponProfile,
    context: Option<&CombatContext>,
    mounted: Option<&MountedCombatProfile>,
) -> WeaponProfile {
    if context.is_some_and(|value| value.is_mounted()) {
        mounted.map_or(base, |profile| profile.sword)
    } else {
        base
    }
}

pub fn effective_bow(
    context: Option<&CombatContext>,
    mounted: Option<&MountedCombatProfile>,
) -> BowProfile {
    if context.is_some_and(|value| value.is_mounted()) {
        mounted.map_or(BowProfile::ON_FOOT, |profile| profile.bow)
    } else {
        BowProfile::ON_FOOT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn context_switch_never_replaces_the_base_weapon() {
        let mut world = World::new();
        world.init_resource::<Messages<SetMountedCombatMessage>>();
        let actor = world
            .spawn((
                CombatContext::default(),
                MountedCombatProfile::HORSE,
                WeaponProfile::GRAYBOX_SWORD,
            ))
            .id();
        world.write_message(SetMountedCombatMessage {
            actor,
            mounted: true,
        });
        world.run_system_once(apply_mounted_context).unwrap();

        let entity = world.entity(actor);
        let base = *entity.get::<WeaponProfile>().unwrap();
        let context = entity.get::<CombatContext>().unwrap();
        let mounted = entity.get::<MountedCombatProfile>().unwrap();
        assert_eq!(base, WeaponProfile::GRAYBOX_SWORD);
        assert_eq!(
            effective_weapon(base, Some(context), Some(mounted)),
            mounted.sword
        );

        world.write_message(SetMountedCombatMessage {
            actor,
            mounted: false,
        });
        world.run_system_once(apply_mounted_context).unwrap();
        assert_eq!(
            *world.entity(actor).get::<WeaponProfile>().unwrap(),
            WeaponProfile::GRAYBOX_SWORD
        );
    }
}
