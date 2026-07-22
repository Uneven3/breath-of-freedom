//! Center-screen bow crosshair presentation.

use bevy::prelude::*;

use super::{CameraRig, Crosshair, CrosshairRing};

pub(super) fn spawn(mut commands: Commands) {
    commands
        .spawn((
            Crosshair,
            Name::new("Crosshair"),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                width: Val::Px(0.0),
                height: Val::Px(0.0),
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                Name::new("CrosshairDot"),
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(4.0),
                    height: Val::Px(4.0),
                    margin: UiRect {
                        left: Val::Px(-2.0),
                        top: Val::Px(-2.0),
                        ..default()
                    },
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.95)),
            ));
            parent.spawn((
                CrosshairRing,
                Name::new("CrosshairRing"),
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(64.0),
                    height: Val::Px(64.0),
                    margin: UiRect {
                        left: Val::Px(-32.0),
                        top: Val::Px(-32.0),
                        ..default()
                    },
                    border_radius: BorderRadius::MAX,
                    border: UiRect::all(Val::Px(1.5)),
                    ..default()
                },
                BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.75)),
            ));
        });
}

pub(super) fn toggle(
    rig: Single<&CameraRig>,
    mut crosshair: Single<&mut Visibility, With<Crosshair>>,
) {
    **crosshair = if rig.aim_blend > 0.5 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
}
