//! Authoritative virtual-time control.
//!
//! Combat outcomes request a short global hitstop. This simulation service,
//! not presentation, owns `Time<Virtual>` so Update remains read-only (§20).

use bevy::prelude::*;

use crate::combat::motors::aim::BowFiredMessage;
use crate::combat::motors::attack::HitImpactMessage;

mod data;

use data::Hitstop;

const CRITICAL_HITSTOP_SECS: f32 = 0.09;
const BOW_FULL_CHARGE_HITSTOP_SECS: f32 = 0.06;
const FULL_CHARGE_THRESHOLD: f32 = 0.95;

pub struct TimeControlPlugin;

impl Plugin for TimeControlPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Hitstop>();
        app.add_systems(PreUpdate, apply_hitstop);
    }
}

fn apply_hitstop(
    real: Res<Time<Real>>,
    mut impacts: MessageReader<HitImpactMessage>,
    mut shots: MessageReader<BowFiredMessage>,
    mut hitstop: ResMut<Hitstop>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    let mut requested = 0.0_f32;
    for impact in impacts.read() {
        if impact.critical {
            requested = requested.max(CRITICAL_HITSTOP_SECS);
        }
    }
    for shot in shots.read() {
        if shot.charge > FULL_CHARGE_THRESHOLD {
            requested = requested.max(BOW_FULL_CHARGE_HITSTOP_SECS);
        }
    }
    hitstop.remaining = hitstop.remaining.max(requested);

    hitstop.remaining = (hitstop.remaining - real.delta_secs()).max(0.0);
    virtual_time.set_relative_speed(if hitstop.remaining > 0.0 { 0.0 } else { 1.0 });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    #[test]
    fn hitstop_resumes_on_the_tick_its_real_time_expires() {
        let mut world = World::new();
        world.insert_resource(Time::<Real>::default());
        world.insert_resource(Time::<Virtual>::default());
        world.insert_resource(Hitstop { remaining: 0.05 });
        world.init_resource::<Messages<HitImpactMessage>>();
        world.init_resource::<Messages<BowFiredMessage>>();
        world
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_millis(50));

        world.run_system_once(apply_hitstop).unwrap();

        assert_eq!(world.resource::<Hitstop>().remaining, 0.0);
        assert_eq!(world.resource::<Time<Virtual>>().relative_speed(), 1.0);
    }
}
