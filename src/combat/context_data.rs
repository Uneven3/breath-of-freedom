use bevy::prelude::*;

use super::weapon::WeaponProfile;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BowProfile {
    pub speed_min: f32,
    pub speed_max: f32,
    pub damage_min: f32,
    pub damage_max: f32,
    pub draw_time_secs: f32,
}

impl BowProfile {
    pub const ON_FOOT: Self = Self {
        speed_min: 30.0,
        speed_max: 95.0,
        damage_min: 6.0,
        damage_max: 18.0,
        draw_time_secs: 1.4,
    };
}

#[derive(Component, Debug, Clone, Copy)]
pub struct MountedCombatProfile {
    pub sword: WeaponProfile,
    pub bow: BowProfile,
}

impl MountedCombatProfile {
    pub const HORSE: Self = Self {
        sword: WeaponProfile::MOUNTED_SWORD,
        bow: BowProfile {
            speed_min: 35.0,
            speed_max: 85.0,
            damage_min: 7.0,
            damage_max: 16.0,
            draw_time_secs: 1.1,
        },
    };
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CombatContext {
    pub(crate) mounted: bool,
}

impl CombatContext {
    pub const fn is_mounted(self) -> bool {
        self.mounted
    }
}

#[derive(Message, Debug, Clone, Copy)]
pub struct SetMountedCombatMessage {
    pub actor: Entity,
    pub mounted: bool,
}
