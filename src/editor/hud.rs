//! What is on screen while sculpting.
//!
//! A brush you cannot see the settings of is a brush you tune by guessing, so
//! this is not decoration: the panel names the active brush, what its buttons
//! do, and where radius, strength and history stand. Hidden entirely when the
//! tool is off, so it costs nothing during play.

use bevy::prelude::*;

use super::SculptTool;
use super::brush::BrushKind;
use super::history::SculptHistory;
use crate::presentation::theme::{
    ACCENT, BORDER, PANEL, TEXT_BRIGHT, TEXT_MUTED, body_font, emoji_font, icon_font,
};

/// `nf-md-terrain` from the Nerd Font symbol range (private use area, so it is
/// only meaningful in that face).
const ICON_TERRAIN: &str = "\u{f0509}";
/// Snow-capped mountain, rendered in colour from the emoji font.
const EMOJI_MOUNTAIN: &str = "\u{1f3d4}";

#[derive(Component)]
pub(super) struct SculptHud;

/// The line naming the active brush and its buttons.
#[derive(Component)]
pub(super) struct BrushLine;

/// The line with radius, strength and history depth.
#[derive(Component)]
pub(super) struct KnobLine;

pub(super) fn spawn_hud(mut commands: Commands) {
    commands
        .spawn((
            Name::new("SculptHud"),
            SculptHud,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                top: Val::Px(16.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                padding: UiRect::all(Val::Px(12.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(BORDER),
            // Above the gameplay HUD, below the F1 modal panel.
            GlobalZIndex(110),
            Visibility::Hidden,
        ))
        .with_children(|panel| {
            // Title in three spans, because three faces meet on this line: the
            // Nerd Font icon and the colour emoji have no glyphs in the body
            // face, and the body face has none of theirs. A span per face is
            // how Bevy composes them — `TextColor` on the icon, ignored by the
            // emoji, which arrives already coloured from its own bitmaps.
            panel
                .spawn((Text::new(""), body_font(16.0), TextColor(ACCENT)))
                .with_children(|title| {
                    title.spawn((
                        TextSpan::new(ICON_TERRAIN),
                        icon_font(16.0),
                        TextColor(ACCENT),
                    ));
                    title.spawn((TextSpan::new(" Esculpir terreno "), body_font(16.0)));
                    title.spawn((TextSpan::new(EMOJI_MOUNTAIN), emoji_font(15.0)));
                });
            panel.spawn((
                BrushLine,
                Text::new(""),
                body_font(15.0),
                TextColor(TEXT_BRIGHT),
            ));
            panel.spawn((
                KnobLine,
                Text::new(""),
                body_font(15.0),
                TextColor(TEXT_MUTED),
            ));
            panel.spawn((
                Text::new(
                    "1 Elevar · 2 Suavizar · 3 Aplanar · 4 Rampa · 5 Rugosidad · 6 Terrazas\n\
                     MMB suaviza siempre · rueda: radio · Shift+rueda o [ ]: fuerza\n\
                     Ctrl+Z/Ctrl+Y deshacer · Ctrl+S guardar · Ctrl+L recargar · F5 salir\n\
                     En freecam (F3) el RMB mira, no baja · con un panel abierto el pincel calla",
                ),
                body_font(13.0),
                TextColor(TEXT_MUTED),
            ));
        });
}

pub(super) fn update_hud(
    tool: Res<SculptTool>,
    history: Res<SculptHistory>,
    mut panel: Query<&mut Visibility, With<SculptHud>>,
    mut brush_line: Query<&mut Text, (With<BrushLine>, Without<KnobLine>)>,
    mut knob_line: Query<&mut Text, (With<KnobLine>, Without<BrushLine>)>,
) {
    let Ok(mut visibility) = panel.single_mut() else {
        return;
    };
    let wanted = if tool.active {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *visibility != wanted {
        *visibility = wanted;
    }
    if !tool.active {
        return;
    }
    if let Ok(mut text) = brush_line.single_mut() {
        let kind: BrushKind = tool.kind;
        **text = format!("{} — {}", kind.label(), kind.hint());
    }
    if let Ok(mut text) = knob_line.single_mut() {
        let (undo, redo) = history.depth();
        **text = format!(
            "radio {:.0} m · fuerza {:.1}× · deshacer {undo} / rehacer {redo}",
            tool.radius, tool.strength
        );
    }
}
