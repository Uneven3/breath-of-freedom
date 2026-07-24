use super::*;
use bevy::ecs::system::RunSystemOnce;

#[test]
fn redirect_transfers_only_supported_controls_and_neutralizes_controller() {
    let mut world = World::new();
    let controller = world
        .spawn((
            Actor,
            crate::movement::attachment::LocomotionEnabled,
            control::ControlRedirect {
                controlled: Entity::PLACEHOLDER,
                mask: control::ControlMask::MOUNT,
            },
            intents::Intents {
                planar: intents::PlanarMoveIntent {
                    direction: Vec2::X,
                    strength: 1.0,
                    local: Vec2::ZERO,
                },
                wants_sneak: true,
                jump: intents::JumpIntent {
                    held: true,
                    pressed: true,
                },
                climb: intents::ClimbIntent {
                    requested: true,
                    ..default()
                },
                ..default()
            },
        ))
        .id();
    let controlled = world.spawn((Actor, intents::Intents::default())).id();
    world
        .entity_mut(controller)
        .get_mut::<control::ControlRedirect>()
        .unwrap()
        .controlled = controlled;
    let unrelated = world
        .spawn((
            Actor,
            crate::movement::attachment::LocomotionEnabled,
            intents::Intents {
                wants_sprint: true,
                ..default()
            },
        ))
        .id();
    world
        .run_system_once(attachment_systems::redirect_controls)
        .unwrap();

    let target = world.entity(controlled).get::<intents::Intents>().unwrap();
    assert_eq!(target.planar.direction, Vec2::X);
    assert!(!target.wants_sneak);
    assert!(target.jump.held);
    assert!(!target.climb.requested);
    assert_eq!(
        world
            .entity(controller)
            .get::<intents::Intents>()
            .unwrap()
            .planar
            .direction,
        Vec2::ZERO
    );
    assert!(
        world
            .entity(unrelated)
            .get::<intents::Intents>()
            .unwrap()
            .wants_sprint
    );
}
