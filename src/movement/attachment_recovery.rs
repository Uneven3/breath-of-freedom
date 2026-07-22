//! Safe recovery when an attachment carrier disappears or a forced detach has
//! no validated floor pose.

use avian3d::prelude::{Collider, ColliderDisabled, SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

use super::attachment::{KinematicAttachment, LocomotionEnabled, PendingSafeRecovery};
use super::control::ControlRedirect;
use super::intents::Intents;
use super::{Actor, BodyVelocity};

pub fn recover_pending_safe_poses(
    mut commands: Commands,
    spatial: SpatialQuery,
    mut actors: Query<(Entity, &Collider, &mut Transform, &mut PendingSafeRecovery), With<Actor>>,
) {
    let filter = SpatialQueryFilter::DEFAULT;
    for (actor, collider, mut transform, mut recovery) in &mut actors {
        let mut recovered = None;
        for _ in 0..4 {
            let candidate = recovery.origin + Vec3::Y * recovery.next_height;
            recovery.next_height += 2.0;
            let mut blocked = false;
            spatial.shape_intersections_callback(
                collider,
                candidate,
                recovery.rotation,
                &filter,
                |entity| {
                    if entity != actor {
                        blocked = true;
                        return false;
                    }
                    true
                },
            );
            if !blocked {
                recovered = Some(candidate);
                break;
            }
        }
        let Some(position) = recovered else {
            continue;
        };
        transform.translation = position;
        transform.rotation = recovery.rotation;
        commands.entity(actor).remove::<PendingSafeRecovery>();
        commands.entity(actor).remove::<ColliderDisabled>();
        commands.entity(actor).insert(LocomotionEnabled);
    }
}

pub fn recover_orphaned_attachments(
    mut commands: Commands,
    attached: Query<
        (
            Entity,
            &KinematicAttachment,
            Option<&ControlRedirect>,
            &Transform,
        ),
        With<Actor>,
    >,
    carriers: Query<(), With<Actor>>,
    mut bodies: Query<(&mut BodyVelocity, &mut Intents), With<Actor>>,
) {
    for (actor, attachment, redirect, transform) in &attached {
        if carriers.get(attachment.carrier).is_ok() {
            continue;
        }
        if let Ok((mut velocity, mut intents)) = bodies.get_mut(actor) {
            velocity.0 = Vec3::ZERO;
            *intents = Intents::default();
        }
        if let Some(redirect) = redirect
            && let Ok((_, mut controlled_intents)) = bodies.get_mut(redirect.controlled)
        {
            *controlled_intents = Intents::default();
        }
        commands
            .entity(actor)
            .remove::<(KinematicAttachment, ControlRedirect)>();
        commands.entity(actor).insert((
            ColliderDisabled,
            PendingSafeRecovery {
                origin: transform.translation,
                rotation: transform.rotation,
                next_height: 0.0,
            },
        ));
        commands.entity(actor).remove::<LocomotionEnabled>();
    }
}
