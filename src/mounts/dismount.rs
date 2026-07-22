use avian3d::prelude::*;
use bevy::prelude::*;

use super::lifecycle::{DISMOUNT_DISTANCE, FLOOR_MIN_UP_DOT};
use crate::movement::body::BodyDimensions;
use crate::movement::link::DetachSafety;

#[derive(Clone, Copy)]
pub(super) struct DismountPose {
    pub pose: Transform,
    pub safety: DetachSafety,
}

pub(super) fn find_dismount_pose(
    spatial: &SpatialQuery,
    collider: &Collider,
    body: BodyDimensions,
    rider: Entity,
    horse: Entity,
    horse_transform: &Transform,
    forced: bool,
) -> Option<DismountPose> {
    const DIRECTIONS: [Vec3; 8] = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Z,
        Vec3::NEG_Z,
        Vec3::new(0.707, 0.0, 0.707),
        Vec3::new(-0.707, 0.0, 0.707),
        Vec3::new(0.707, 0.0, -0.707),
        Vec3::new(-0.707, 0.0, -0.707),
    ];
    let filter = SpatialQueryFilter::DEFAULT;
    for radius in [DISMOUNT_DISTANCE, 2.8, 4.0] {
        if !forced && radius > DISMOUNT_DISTANCE {
            break;
        }
        for direction in DIRECTIONS {
            let world_direction = horse_transform.rotation * direction;
            let start = horse_transform.translation
                + world_direction * radius
                + Vec3::Y * (body.standing_half_height() + 1.0);
            let Some(hit) = spatial.cast_shape_predicate(
                collider,
                start,
                horse_transform.rotation,
                Dir3::NEG_Y,
                &ShapeCastConfig::from_max_distance(4.0),
                &filter,
                &|entity| entity != rider && entity != horse,
            ) else {
                continue;
            };
            if hit.normal1.y < FLOOR_MIN_UP_DOT {
                continue;
            }
            let position = start - Vec3::Y * hit.distance + Vec3::Y * 0.02;
            if clear_at(
                spatial,
                collider,
                position,
                horse_transform.rotation,
                &filter,
                rider,
                horse,
            ) {
                return Some(DismountPose {
                    pose: Transform::from_translation(position)
                        .with_rotation(horse_transform.rotation),
                    safety: DetachSafety::Validated,
                });
            }
        }
    }
    if forced {
        for height in [3.0, 5.0, 8.0] {
            let position = horse_transform.translation + Vec3::Y * height;
            if clear_at(
                spatial,
                collider,
                position,
                horse_transform.rotation,
                &filter,
                rider,
                horse,
            ) {
                return Some(DismountPose {
                    pose: Transform::from_translation(position)
                        .with_rotation(horse_transform.rotation),
                    safety: DetachSafety::Validated,
                });
            }
        }
        return Some(DismountPose {
            pose: Transform::from_translation(horse_transform.translation + Vec3::Y * 10.0)
                .with_rotation(horse_transform.rotation),
            safety: DetachSafety::NeedsRecovery,
        });
    }
    None
}

fn clear_at(
    spatial: &SpatialQuery,
    collider: &Collider,
    position: Vec3,
    rotation: Quat,
    filter: &SpatialQueryFilter,
    rider: Entity,
    horse: Entity,
) -> bool {
    let mut clear = true;
    spatial.shape_intersections_callback(collider, position, rotation, filter, |entity| {
        if entity == rider || entity == horse {
            true
        } else {
            clear = false;
            false
        }
    });
    clear
}
