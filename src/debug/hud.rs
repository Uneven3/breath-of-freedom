//! Screen sink: renders the [`DebugSnapshot`] into the on-screen `Text`.
//! Formats nothing itself — it only arranges the lines the snapshot produced,
//! which is what keeps it from drifting away from the console sink.

use bevy::prelude::*;

use super::snapshot::{DebugSnapshot, HudVisibility};

#[derive(Component)]
pub(super) struct DebugText;

pub(super) fn spawn_debug_text(mut commands: Commands) {
    commands.spawn((
        DebugText,
        Text::new("…"),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            ..default()
        },
    ));
}

pub(super) fn render_hud(
    snapshot: Res<DebugSnapshot>,
    visibility: Res<HudVisibility>,
    mut text: Single<&mut Text, With<DebugText>>,
) {
    text.0 = snapshot.visible_lines(&visibility).join("\n");
}
