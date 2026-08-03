//! Shared visual theme and typography tokens across UI presentation layers.
//!
//! **Fonts live here or the UI lies.** Bevy's built-in fallback is a 95-glyph
//! subset of FiraMono — ASCII and nothing else, not even `ó`. Every accent and
//! every `·`, `—`, `→` in a Spanish-language HUD rendered as an empty box. So
//! the body face is installed over the default font handle, which every
//! `TextFont` in the project already points at: one seam, no call site changes.

use bevy::prelude::*;
use bevy::text::{Font, FontSource};

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

/// The UI face, compiled in rather than loaded from `assets/`.
///
/// 404 KB against a guarantee: text can never render before its font arrives,
/// and a build that lost its `assets/` folder still has a readable menu. It is
/// the same choice — and the same trade — Bevy makes for its own default.
/// Fira Sans is the sibling of that default, so the UI keeps its character and
/// only stops breaking. SIL OFL 1.1, see `assets/fonts/LICENSE-FiraSans.txt`.
const BODY_FONT_DATA: &[u8] = include_bytes!("../../assets/fonts/FiraSans-Regular.ttf");

/// Family names as embedded in the font files. `FontSource::Family` resolves
/// through these once the asset is registered, so a typo here is a silent
/// fallback rather than an error — they are checked by a test.
const ICON_FAMILY: &str = "Symbols Nerd Font";
const EMOJI_FAMILY: &str = "Noto Color Emoji";

/// Keeps the icon and emoji faces alive. They are assets rather than compiled
/// in: 2.4 MB and 10.7 MB have no business inside the binary, and no UI needs
/// them in the first frame.
#[derive(Resource)]
pub struct UiFonts {
    #[expect(
        dead_code,
        reason = "held so the assets stay loaded; used by family name"
    )]
    icons: Handle<Font>,
    #[expect(
        dead_code,
        reason = "held so the assets stay loaded; used by family name"
    )]
    emoji: Handle<Font>,
}

/// Plain UI text.
pub fn body_font(size_px: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(size_px),
        ..default()
    }
}

/// Nerd Font icon glyphs (the private-use range: Powerline, Font Awesome,
/// Devicons). Monochrome — they take the span's `TextColor` like any letter.
/// Use it on a span of its own; the body face has none of these codepoints.
pub fn icon_font(size_px: f32) -> TextFont {
    TextFont {
        font: FontSource::Family(ICON_FAMILY.into()),
        font_size: FontSize::Px(size_px),
        ..default()
    }
}

/// Colour emoji. Bevy rasterises these through swash's colour-bitmap source
/// into an RGBA atlas, so they arrive in colour and ignore `TextColor`.
pub fn emoji_font(size_px: f32) -> TextFont {
    TextFont {
        font: FontSource::Family(EMOJI_FAMILY.into()),
        font_size: FontSize::Px(size_px),
        ..default()
    }
}

pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        // Overwrite the asset behind `Handle<Font>::default()`. `bevy_text`
        // seeds it with its own subset when its plugin builds; doing this here,
        // in `build`, lands before any text is laid out — no first frame of
        // boxes, and no system to keep ordered.
        if let Err(error) = app.world_mut().resource_mut::<Assets<Font>>().insert(
            AssetId::default(),
            Font::from_bytes(BODY_FONT_DATA.to_vec()),
        ) {
            error!("failed to install the embedded UI font: {error}");
        }
        app.add_systems(PreStartup, load_symbol_fonts);
    }
}

fn load_symbol_fonts(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(UiFonts {
        icons: assets.load("fonts/SymbolsNerdFont-Regular.ttf"),
        emoji: assets.load("fonts/NotoColorEmoji.ttf"),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads the `name` table of a TTF and returns its family name (name ID 1).
    /// Hand-rolled because the alternative is a font-parsing dependency for one
    /// assertion — and this is the assertion that matters: `FontSource::Family`
    /// fails *silently* on a typo, falling back to some other face.
    fn family_name(data: &[u8]) -> Option<String> {
        let table_count = u16::from_be_bytes(data[4..6].try_into().ok()?) as usize;
        let mut name_table = None;
        for i in 0..table_count {
            let record = 12 + i * 16;
            if &data[record..record + 4] == b"name" {
                name_table = Some(
                    u32::from_be_bytes(data[record + 8..record + 12].try_into().ok()?) as usize,
                );
            }
        }
        let table = name_table?;
        let count = u16::from_be_bytes(data[table + 2..table + 4].try_into().ok()?) as usize;
        let strings =
            table + u16::from_be_bytes(data[table + 4..table + 6].try_into().ok()?) as usize;
        for i in 0..count {
            let record = table + 6 + i * 12;
            let platform = u16::from_be_bytes(data[record..record + 2].try_into().ok()?);
            let name_id = u16::from_be_bytes(data[record + 6..record + 8].try_into().ok()?);
            let length =
                u16::from_be_bytes(data[record + 8..record + 10].try_into().ok()?) as usize;
            let offset =
                u16::from_be_bytes(data[record + 10..record + 12].try_into().ok()?) as usize;
            if name_id != 1 {
                continue;
            }
            let raw = &data[strings + offset..strings + offset + length];
            return Some(if platform == 3 {
                // Windows platform: UTF-16BE.
                let units: Vec<u16> = raw
                    .chunks_exact(2)
                    .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                    .collect();
                String::from_utf16_lossy(&units)
            } else {
                raw.iter().map(|byte| *byte as char).collect()
            });
        }
        None
    }

    #[test]
    fn the_icon_and_emoji_families_reach_the_text_engine() {
        // The name test above catches a typo in the constant; this catches the
        // rest of the path — that the files load as font assets and that Parley
        // registers them under the family name `FontSource::Family` looks up. A
        // miss here is silent at runtime: the span just renders in some other
        // face.
        use bevy::asset::AssetPlugin;
        use bevy::text::{FontCx, TextPlugin};

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), TextPlugin));
        let (icons, emoji) = {
            let assets = app.world().resource::<AssetServer>();
            (
                assets.load::<Font>("fonts/SymbolsNerdFont-Regular.ttf"),
                assets.load::<Font>("fonts/NotoColorEmoji.ttf"),
            )
        };
        // Loading is asynchronous; pump until both land, with a bound so a
        // failure is a failed assert rather than a hung suite.
        for _ in 0..2_000 {
            app.update();
            let fonts = app.world().resource::<Assets<Font>>();
            if fonts.contains(&icons) && fonts.contains(&emoji) {
                break;
            }
        }
        let fonts = app.world().resource::<Assets<Font>>();
        assert!(fonts.contains(&icons), "the icon font never loaded");
        assert!(fonts.contains(&emoji), "the emoji font never loaded");

        let mut font_cx = app.world_mut().resource_mut::<FontCx>();
        assert!(
            font_cx.collection.family_id(ICON_FAMILY).is_some(),
            "Parley does not know the family {ICON_FAMILY}"
        );
        assert!(
            font_cx.collection.family_id(EMOJI_FAMILY).is_some(),
            "Parley does not know the family {EMOJI_FAMILY}"
        );
    }

    #[test]
    fn the_icon_and_emoji_family_names_match_the_shipped_fonts() {
        let icons = include_bytes!("../../assets/fonts/SymbolsNerdFont-Regular.ttf");
        let emoji = include_bytes!("../../assets/fonts/NotoColorEmoji.ttf");
        assert_eq!(family_name(icons).as_deref(), Some(ICON_FAMILY));
        assert_eq!(family_name(emoji).as_deref(), Some(EMOJI_FAMILY));
    }

    #[test]
    fn the_body_font_covers_every_character_the_ui_writes() {
        // The bug this file exists for: Bevy's default face has 95 glyphs, so
        // `ó` and `·` came out as empty boxes. Pin the coverage instead of
        // trusting it — a future font swap that regresses this is a HUD full of
        // tofu that no test would otherwise catch.
        let coverage = cmap_coverage(BODY_FONT_DATA);
        let used = "áéíóúñÁÉÍÓÚÑü¿¡«»—–…·×°²§→↔≈−’";
        let missing: String = used
            .chars()
            .filter(|character| !coverage.contains(&(*character as u32)))
            .collect();
        assert!(missing.is_empty(), "the body font lacks: {missing}");
    }

    /// Every codepoint the font's character map claims to render. Handles the
    /// two `cmap` formats real fonts ship: 4 (BMP) and 12 (full range).
    fn cmap_coverage(data: &[u8]) -> std::collections::HashSet<u32> {
        let mut codes = std::collections::HashSet::new();
        let table_count = u16::from_be_bytes(data[4..6].try_into().unwrap()) as usize;
        let mut cmap = None;
        for i in 0..table_count {
            let record = 12 + i * 16;
            if &data[record..record + 4] == b"cmap" {
                cmap = Some(
                    u32::from_be_bytes(data[record + 8..record + 12].try_into().unwrap()) as usize,
                );
            }
        }
        let cmap = cmap.expect("a font without a cmap renders nothing");
        let subtables = u16::from_be_bytes(data[cmap + 2..cmap + 4].try_into().unwrap()) as usize;
        for i in 0..subtables {
            let record = cmap + 4 + i * 8;
            let sub = cmap
                + u32::from_be_bytes(data[record + 4..record + 8].try_into().unwrap()) as usize;
            let read16 = |at: usize| u16::from_be_bytes(data[at..at + 2].try_into().unwrap());
            match read16(sub) {
                4 => {
                    let segments = read16(sub + 6) as usize / 2;
                    for segment in 0..segments {
                        let end = read16(sub + 14 + segment * 2) as u32;
                        let start = read16(sub + 16 + segments * 2 + segment * 2) as u32;
                        if end != 0xFFFF {
                            codes.extend(start..=end);
                        }
                    }
                }
                12 => {
                    let groups =
                        u32::from_be_bytes(data[sub + 12..sub + 16].try_into().unwrap()) as usize;
                    for group in 0..groups {
                        let at = sub + 16 + group * 12;
                        let start = u32::from_be_bytes(data[at..at + 4].try_into().unwrap());
                        let end = u32::from_be_bytes(data[at + 4..at + 8].try_into().unwrap());
                        codes.extend(start..=end);
                    }
                }
                _ => {}
            }
        }
        codes
    }
}
