//! Shared visual theme and typography tokens across UI presentation layers.

use bevy::prelude::*;

pub const PANEL: Color = Color::srgba(0.055, 0.065, 0.07, 0.98);
pub const PANEL_INSET: Color = Color::srgb(0.09, 0.105, 0.11);
pub const ROW_OR_SLOT_BG: Color = Color::srgb(0.115, 0.13, 0.135);
pub const SELECTED_SLOT: Color = Color::srgb(0.12, 0.21, 0.20);
pub const ACCENT: Color = Color::srgb(0.25, 0.82, 0.67);
pub const ACCENT_DARK: Color = Color::srgb(0.08, 0.34, 0.29);
pub const BORDER: Color = Color::srgb(0.25, 0.28, 0.29);
pub const DISABLED: Color = Color::srgb(0.11, 0.115, 0.12);
pub const TEXT_BRIGHT: Color = Color::srgb(0.94, 0.96, 0.93);
pub const TEXT_MUTED: Color = Color::srgb(0.62, 0.67, 0.65);

pub fn body_font(size_px: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(size_px),
        ..default()
    }
}
