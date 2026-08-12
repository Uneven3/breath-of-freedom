//! Shared visual vocabulary for the debug panels (the F1 hub and the F2 readout
//! menu), so a colour or a row shape is defined once instead of drifting apart
//! between two files.

use bevy::prelude::*;

pub(super) use crate::presentation::theme::{ACCENT, BORDER, PANEL, TEXT_BRIGHT, TEXT_MUTED};

pub(super) fn body_font() -> TextFont {
    crate::presentation::theme::body_font(15.0)
}

pub(super) fn heading_font() -> TextFont {
    TextFont {
        font_size: FontSize::Px(18.0),
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
