//! Aim motor — bow drawn while the aim button is held.
//!
//! `propose` keeps `Aiming` alive while `wants_aim` holds (release = silence
//! → `Idle`); `shoot_drawn_arrow` turns an attack press while `Aiming` into a
//! `projectiles::SpawnProjectileMessage` along the actor's control
//! orientation (where the aim camera looks). Draw-hold/slow-mo sub-states
//! stay an open decision in `combat.md`.

use bevy::prelude::*;

use crate::combat::intent::CombatIntents;
use crate::combat::proposal::{CombatProposalBuffer, Priority, TransitionProposal, weight};
use crate::combat::state::CombatState;
use crate::input::frame::ControlOrientation;
use crate::movement::Actor;
use crate::projectiles::SpawnProjectileMessage;

/// Initial arrow speed (m/s). First pass; tuned at the feeling checkpoint.
const ARROW_SPEED: f32 = 34.0;
const ARROW_BASE_DAMAGE: f32 = 12.0;
/// Where the arrow leaves the body, relative to its center.
const ARROW_MUZZLE_UP: f32 = 0.4;
const ARROW_MUZZLE_FORWARD: f32 = 0.6;

type ProposeQuery<'a> = (
    &'a CombatIntents,
    &'a CombatState,
    &'a mut CombatProposalBuffer,
);

/// Draw from `Idle`, keep drawn while held. A melee start the same tick
/// out-arbitrates the draw (`weight::ATTACK_CHAIN > weight::AIM`); an attack
/// press while already drawn is the release, handled by `shoot_drawn_arrow`.
pub fn propose(mut q: Query<ProposeQuery, With<Actor>>) {
    for (intents, state, mut buffer) in &mut q {
        if !intents.wants_aim {
            continue;
        }
        if matches!(*state, CombatState::Idle | CombatState::Aiming) {
            let _ = buffer.push(TransitionProposal::new(
                CombatState::Aiming,
                Priority::PlayerRequested,
                weight::AIM,
                "aim",
            ));
        }
    }
}

type ShootQuery<'a> = (
    Entity,
    &'a Transform,
    &'a ControlOrientation,
    &'a CombatIntents,
    &'a CombatState,
);

/// While `Aiming`, an attack press releases an arrow along the control
/// orientation — the exact direction the aim camera is looking.
pub fn shoot_drawn_arrow(
    q: Query<ShootQuery, With<Actor>>,
    mut spawns: MessageWriter<SpawnProjectileMessage>,
) {
    for (shooter, transform, orientation, intents, state) in &q {
        if *state != CombatState::Aiming || !intents.attack.pressed {
            continue;
        }
        let direction = aim_direction(orientation);
        let origin =
            transform.translation + Vec3::Y * ARROW_MUZZLE_UP + direction * ARROW_MUZZLE_FORWARD;
        spawns.write(SpawnProjectileMessage {
            shooter,
            origin,
            velocity: direction * ARROW_SPEED,
            damage: ARROW_BASE_DAMAGE,
        });
    }
}

/// The camera-forward implied by the control orientation (same rotation the
/// camera rig builds: yaw then pitch, looking down -Z).
pub(crate) fn aim_direction(orientation: &ControlOrientation) -> Vec3 {
    (Quat::from_rotation_y(orientation.yaw) * Quat::from_rotation_x(orientation.pitch))
        * Vec3::NEG_Z
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn aim_direction_matches_yaw_and_pitch() {
        let level = aim_direction(&ControlOrientation {
            yaw: 0.0,
            pitch: 0.0,
        });
        assert!((level - Vec3::NEG_Z).length() < 1e-5);

        let up = aim_direction(&ControlOrientation {
            yaw: 0.0,
            pitch: std::f32::consts::FRAC_PI_4,
        });
        assert!(up.y > 0.5, "positive pitch must aim upward, got {up}");

        let turned = aim_direction(&ControlOrientation {
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: 0.0,
        });
        assert!(
            turned.x < -0.99,
            "quarter yaw must aim along -X, got {turned}"
        );
    }

    #[test]
    fn aiming_holds_while_wanted_and_yields_to_a_melee_start() {
        let mut world = World::new();
        let drawing = world
            .spawn((
                Actor,
                CombatIntents {
                    wants_aim: true,
                    ..default()
                },
                CombatState::Idle,
                CombatProposalBuffer::default(),
            ))
            .id();
        let mid_swing = world
            .spawn((
                Actor,
                CombatIntents {
                    wants_aim: true,
                    ..default()
                },
                CombatState::Windup,
                CombatProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

        let proposals: Vec<_> = world
            .entity(drawing)
            .get::<CombatProposalBuffer>()
            .unwrap()
            .iter()
            .cloned()
            .collect();
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].target_state, CombatState::Aiming);
        assert!(
            proposals[0].override_weight < weight::ATTACK_CHAIN,
            "a same-tick melee start must beat the draw"
        );

        assert!(
            world
                .entity(mid_swing)
                .get::<CombatProposalBuffer>()
                .unwrap()
                .iter()
                .next()
                .is_none(),
            "the bow must not interrupt a committed swing"
        );
    }
}
