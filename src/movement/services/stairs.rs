//! Stairs service — reports whether the body overlaps an authored stair volume.

#[cfg(test)]
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::facts::StairsFacts;
use crate::world::Stairs;

pub fn stairs_service(
    mut actors: Query<(&Transform, &mut StairsFacts), With<Actor>>,
    stairs: Query<&Stairs>,
) {
    for (transform, mut facts) in &mut actors {
        let pos = transform.translation;

        let mut nearest = None;
        for stair in &stairs {
            if contains(stair, pos) {
                let distance_sq = pos.distance_squared(stair.trigger_center);
                if nearest.is_none_or(|(_, best_distance_sq)| distance_sq < best_distance_sq) {
                    nearest = Some((stair, distance_sq));
                }
            }
        }
        *facts = nearest.map_or_else(StairsFacts::default, |(stair, _)| StairsFacts {
            on_stairs: true,
            base: stair.base,
            top: stair.top,
            step_count: stair.step_count,
            step_depth: stair.step_depth,
            step_rise: stair.step_rise,
        });
    }
}

fn contains(stairs: &Stairs, point: Vec3) -> bool {
    let local = stairs.trigger_rotation.conjugate() * (point - stairs.trigger_center);
    let d = local.abs();
    d.x <= stairs.trigger_half_extents.x
        && d.y <= stairs.trigger_half_extents.y
        && d.z <= stairs.trigger_half_extents.z
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stair(base_x: f32, center_x: f32) -> Stairs {
        Stairs {
            base: Vec3::new(base_x, 0.0, 0.0),
            top: Vec3::new(base_x + 1.0, 0.25, 0.0),
            step_count: 1,
            step_depth: 1.0,
            step_rise: 0.25,
            trigger_center: Vec3::new(center_x, 1.0, 0.0),
            trigger_half_extents: Vec3::splat(1.0),
            trigger_rotation: Quat::IDENTITY,
        }
    }

    #[test]
    fn overlapping_segments_choose_the_nearest_center() {
        let mut world = World::new();
        world.spawn((
            Actor,
            Transform::from_xyz(0.4, 1.0, 0.0),
            StairsFacts::default(),
        ));
        world.spawn(stair(-1.0, -0.5));
        world.spawn(stair(1.0, 0.5));

        world.run_system_once(stairs_service).unwrap();

        let facts = world.query::<&StairsFacts>().single(&world).unwrap();
        assert!(facts.on_stairs);
        assert_eq!(facts.base.x, 1.0);
    }
}
