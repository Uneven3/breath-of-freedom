//! The world clock.
//!
//! Only the clock: the sun, the moon, the sky discs and the shadow cascades are
//! presentation and stay with the app, reading [`TimeOfDay`] like any other
//! fact. The split exists because this advances state every fixed tick, and
//! presentation is only ever allowed to read (§20) — so the writer has to live
//! on this side even while nothing but lighting consumes the hour yet.

use bevy_app::{App, FixedUpdate, Plugin};
use bevy_ecs::prelude::*;
use bevy_log::info;
use bevy_time::prelude::*;

/// Real minutes for one in-game day.
const REAL_MINUTES_PER_GAME_DAY: f32 = 24.0;

/// Simulation clock: `hours` in `0.0..24.0`, advanced on the fixed step.
/// `speed` is a debug affordance (the F1 hub's fast-forward action); 1.0 in
/// normal play.
#[derive(Resource)]
pub struct TimeOfDay {
    pub hours: f32,
    pub speed: f32,
}

impl Default for TimeOfDay {
    fn default() -> Self {
        Self {
            hours: 9.0,
            speed: 1.0,
        }
    }
}

/// External request to alter the world-owned simulation clock. Debug/UI may
/// emit it, but only World mutates `TimeOfDay` (§7).
#[derive(Message, Debug, Clone, Copy)]
pub enum TimeOfDayRequest {
    AdvanceHour,
    ToggleSpeed,
}

pub fn apply_time_requests(
    mut requests: MessageReader<TimeOfDayRequest>,
    mut time_of_day: ResMut<TimeOfDay>,
) {
    for request in requests.read() {
        match request {
            TimeOfDayRequest::AdvanceHour => {
                time_of_day.hours = (time_of_day.hours + 1.0).rem_euclid(24.0);
                info!("[debug] time jump: {:05.2}h", time_of_day.hours);
            }
            TimeOfDayRequest::ToggleSpeed => {
                time_of_day.speed = if time_of_day.speed > 1.0 { 1.0 } else { 120.0 };
                info!("[debug] time speed: x{}", time_of_day.speed);
            }
        }
    }
}

pub fn advance_time(time: Res<Time>, mut tod: ResMut<TimeOfDay>) {
    let game_hours_per_real_second = 24.0 / (REAL_MINUTES_PER_GAME_DAY * 60.0);
    tod.hours =
        (tod.hours + time.delta_secs() * game_hours_per_real_second * tod.speed).rem_euclid(24.0);
}

/// Installs the clock and the only two systems allowed to write it.
pub struct DayNightClockPlugin;

impl Plugin for DayNightClockPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TimeOfDay>();
        app.add_message::<TimeOfDayRequest>();
        app.add_systems(FixedUpdate, (apply_time_requests, advance_time).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::RunSystemOnce;

    #[test]
    fn time_advances_and_wraps_at_midnight() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(1.0));
        world.insert_resource(TimeOfDay {
            hours: 23.99,
            speed: 1.0,
        });

        world.run_system_once(advance_time).unwrap();

        let tod = world.resource::<TimeOfDay>();
        let expected_step = 24.0 / (REAL_MINUTES_PER_GAME_DAY * 60.0);
        assert!(tod.hours < expected_step + 1e-4, "must wrap past 24:00");

        // Debug fast-forward multiplies the same step.
        world.insert_resource(TimeOfDay {
            hours: 0.0,
            speed: 60.0,
        });
        world.run_system_once(advance_time).unwrap();
        let tod = world.resource::<TimeOfDay>();
        assert!((tod.hours - expected_step * 60.0).abs() < 1e-4);
    }
}
