//! Shared visual vocabulary for the debug panels (the F1 hub and the F2 readout
//! menu), so a colour or a row shape is defined once instead of drifting apart
//! between two files.

use bevy::prelude::*;

pub(super) const PANEL: Color = Color::srgba(0.055, 0.065, 0.07, 0.98);
pub(super) const PANEL_INSET: Color = Color::srgb(0.09, 0.105, 0.11);
pub(super) const ROW: Color = Color::srgb(0.115, 0.13, 0.135);
pub(super) const ACCENT: Color = Color::srgb(0.25, 0.82, 0.67);
pub(super) const ACCENT_DARK: Color = Color::srgb(0.08, 0.34, 0.29);
pub(super) const BORDER: Color = Color::srgb(0.25, 0.28, 0.29);
pub(super) const TEXT_BRIGHT: Color = Color::srgb(0.94, 0.96, 0.93);
pub(super) const TEXT_MUTED: Color = Color::srgb(0.62, 0.67, 0.65);

pub(super) fn body_font() -> TextFont {
    TextFont {
        font_size: FontSize::Px(15.0),
        ..default()
    }
}

pub(super) fn heading_font() -> TextFont {
    TextFont {
        font_size: FontSize::Px(18.0),
        ..default()
    }
}

pub(super) fn row_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)),
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        column_gap: Val::Px(12.0),
        border_radius: BorderRadius::all(Val::Px(4.0)),
        ..default()
    }
}

/// An accent heading with a muted subtitle underneath — the section separator
/// both panels use.
pub(super) fn section_title(panel: &mut ChildSpawnerCommands, title: &str, subtitle: &str) {
    panel.spawn((
        Text::new(title),
        heading_font(),
        TextColor(ACCENT),
        Node {
            margin: UiRect::top(Val::Px(6.0)),
            ..default()
        },
    ));
    panel.spawn((Text::new(subtitle), body_font(), TextColor(TEXT_MUTED)));
}
