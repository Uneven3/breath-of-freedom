//! Applies [`MovementTuning`] onto the per-actor components.
//!
//! Runs whenever the resource changes, so a value moved at runtime reaches the
//! body on the next frame without a relaunch — and, because it *writes the
//! components* instead of being read by the motors, nothing in the fixed-tick
//! hot path gains a `Res<>` and no headless world can panic for missing one.

use bevy_ecs::prelude::*;

pub use bof_domain::movement::tuning::{MovementTuning, TuningField, TuningNudge, TuningReport};

use bevy_log::info;

use super::abilities::SlideMovement;
use super::sensing::{GroundSensing, LedgeSensing};
use crate::movement::Actor;

/// The profile a knob reads before it moves it.
type LiveProfile<'a> = (
    &'a GroundSensing,
    Option<&'a LedgeSensing>,
    Option<&'a SlideMovement>,
);

/// Turns a panel click into a value, and says out loud what it became — a knob
/// moved without a record is an experiment nobody can repeat.
pub fn apply_nudges(
    mut requests: bevy_ecs::prelude::MessageReader<TuningNudge>,
    mut tuning: ResMut<MovementTuning>,
    actors: Query<LiveProfile, With<Actor>>,
) {
    for nudge in requests.read() {
        let current = actors.iter().next().map_or_else(
            || {
                bof_domain::movement::tuning::live_value(
                    nudge.field,
                    &GroundSensing::PLAYER,
                    Some(&LedgeSensing::PLAYER),
                    SlideMovement::PLAYER,
                )
            },
            |(ground, ledge, slide)| {
                bof_domain::movement::tuning::live_value(
                    nudge.field,
                    ground,
                    ledge,
                    slide.copied().unwrap_or(SlideMovement::PLAYER),
                )
            },
        );
        let value = tuning.nudge(nudge.field, current, nudge.steps);
        info!("[tuning] {} = {value}", nudge.field.key());
    }
}

type TunedActor<'a> = (
    &'a mut GroundSensing,
    Option<&'a mut LedgeSensing>,
    Option<&'a mut SlideMovement>,
);

pub fn apply_tuning(tuning: Res<MovementTuning>, mut actors: Query<TunedActor, With<Actor>>) {
    let tuning_moved = tuning.is_changed();
    for (mut ground, ledge, slide) in &mut actors {
        // Either the knob just moved, or this actor was not born yet when it
        // did. The player spawns on scene entry, several frames after the
        // resource is inserted, so without the second case a knob would apply
        // to nobody while the launch log reported it applied.
        if !tuning_moved && !ground.is_added() {
            continue;
        }
        if let Some(dot) = tuning.slope_hysteresis_dot {
            ground.slope_hysteresis_dot = dot;
        }
        if let Some(ticks) = tuning.ground_grace_ticks {
            ground.ground_grace_ticks = ticks;
        }
        if let Some(mut ledge) = ledge {
            if let Some(reach) = tuning.wall_detection_reach {
                ledge.wall_detection_reach = reach;
            }
            if let Some(cone) = tuning.climb_wall_angle_max_deg {
                ledge.climb_wall_angle_max_deg = cone;
            }
        }
        if let Some(mut slide) = slide {
            if let Some(gravity) = tuning.slide_gravity_factor {
                slide.gravity_factor = gravity;
            }
            if let Some(speed) = tuning.slide_max_speed {
                slide.max_speed = speed;
            }
            if let Some(friction) = tuning.slide_contour_friction {
                slide.contour_friction = friction;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::RunSystemOnce;

    fn world_with_actor() -> (World, Entity) {
        let mut world = World::new();
        world.init_resource::<MovementTuning>();
        let actor = world
            .spawn((
                Actor,
                GroundSensing::PLAYER,
                LedgeSensing::PLAYER,
                SlideMovement::PLAYER,
            ))
            .id();
        (world, actor)
    }

    #[test]
    fn an_untouched_tuning_leaves_every_profile_alone() {
        let (mut world, actor) = world_with_actor();
        let _ = world.run_system_once(apply_tuning);
        assert_eq!(
            world.entity(actor).get::<GroundSensing>(),
            Some(&GroundSensing::PLAYER)
        );
        assert_eq!(
            world.entity(actor).get::<LedgeSensing>(),
            Some(&LedgeSensing::PLAYER)
        );
    }

    #[test]
    fn a_set_field_reaches_the_actor_and_the_others_do_not_move() {
        let (mut world, actor) = world_with_actor();
        world
            .resource_mut::<MovementTuning>()
            .set(TuningField::SlopeHysteresisDot, 0.06);
        let _ = world.run_system_once(apply_tuning);
        let sensing = world.entity(actor).get::<GroundSensing>().copied();
        assert_eq!(sensing.map(|s| s.slope_hysteresis_dot), Some(0.06));
        assert_eq!(
            sensing.map(|s| s.probe_distance),
            Some(GroundSensing::PLAYER.probe_distance),
            "una perilla no debe arrastrar a las demás"
        );
    }

    /// An actor without the optional profiles must not be skipped for the ones
    /// it does have.
    #[test]
    fn an_actor_missing_the_optional_profiles_still_gets_its_ground_sensing() {
        let mut world = World::new();
        world.init_resource::<MovementTuning>();
        let bare = world.spawn((Actor, GroundSensing::PLAYER)).id();
        world
            .resource_mut::<MovementTuning>()
            .set(TuningField::GroundGraceTicks, 4.0);
        let _ = world.run_system_once(apply_tuning);
        assert_eq!(
            world
                .entity(bare)
                .get::<GroundSensing>()
                .map(|s| s.ground_grace_ticks),
            Some(4)
        );
    }
}

#[cfg(test)]
mod ordering_tests {
    use super::*;
    use bevy_app::{App, Update};

    /// The bug this guards: the resource is only `is_changed()` for the frame
    /// after it is inserted, but the player spawns on scene entry — later. An
    /// actor born after that frame would keep its defaults while the log
    /// cheerfully reported the knob as applied.
    #[test]
    fn an_actor_spawned_after_the_first_frame_still_gets_tuned() {
        let mut app = App::new();
        app.init_resource::<MovementTuning>();
        app.add_systems(Update, apply_tuning);
        app.world_mut()
            .resource_mut::<MovementTuning>()
            .set(TuningField::GroundGraceTicks, 5.0);

        // Frames pass with no actor in the world at all.
        app.update();
        app.update();

        let late = app.world_mut().spawn((Actor, GroundSensing::PLAYER)).id();
        app.update();

        assert_eq!(
            app.world()
                .entity(late)
                .get::<GroundSensing>()
                .map(|s| s.ground_grace_ticks),
            Some(5),
            "una perilla que no alcanza al actor es peor que no tenerla: miente"
        );
    }
}
