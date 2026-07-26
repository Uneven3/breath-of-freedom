//! What is on screen while authoring.
//!
//! A brush you cannot see the settings of is a brush you tune by guessing, so
//! this is not decoration: the panel names the active layer and brush, what its
//! buttons do, and where radius, strength and history stand. Hidden entirely when
//! the tool is off, so it costs nothing during play.
//!
//! It matters more since the tool grew a second layer: the number keys mean a
//! sculpt brush in one and a terrain kind in the other, and painting leaves no
//! trace on flat ground, so the panel is the only thing telling you which of the
//! two your next click lands on.

use bevy::prelude::*;

use super::brush::BrushKind;
use super::history::SculptHistory;
use super::{EditorTool, ToolLayer};
use crate::presentation::theme::{
    ACCENT, BORDER, PANEL, TEXT_BRIGHT, TEXT_MUTED, body_font, emoji_font, icon_font,
};
use crate::world::TerrainKind;

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

/// The key legend. Rebuilt per frame rather than written once at spawn, because
/// the number keys mean different things in each layer and a legend that lies is
/// worse than none.
#[derive(Component)]
pub(super) struct LegendLine;

/// One panel line's text, excluded from the other two.
///
/// The `Without` bounds are not decoration: three mutable `Text` queries in one
/// system have to be provably disjoint or Bevy refuses to run it. Naming the
/// shape once keeps that guarantee readable instead of three lines of filter
/// tuple.
type LineText<'w, 's, Mine, A, B> =
    Query<'w, 's, &'static mut Text, (With<Mine>, Without<A>, Without<B>)>;

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
                    title.spawn((TextSpan::new(" Editor de mapa "), body_font(16.0)));
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
                LegendLine,
                Text::new(""),
                body_font(13.0),
                TextColor(TEXT_MUTED),
            ));
        });
}

pub(super) fn update_hud(
    tool: Res<EditorTool>,
    history: Res<SculptHistory>,
    mut panel: Query<&mut Visibility, With<SculptHud>>,
    mut brush_line: LineText<BrushLine, KnobLine, LegendLine>,
    mut knob_line: LineText<KnobLine, BrushLine, LegendLine>,
    mut legend_line: LineText<LegendLine, BrushLine, KnobLine>,
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
        **text = match tool.layer {
            ToolLayer::Relief => {
                let brush: BrushKind = tool.brush;
                format!(
                    "{}: {} — {}",
                    ToolLayer::Relief.label(),
                    brush.label(),
                    brush.hint()
                )
            }
            // Naming the properties, not just the kind: they are what the author
            // is actually placing, they come from a table the brush cannot show,
            // and today nothing in the world reacts to them yet — so this line is
            // the only confirmation that painting tall grass laid down something
            // that will burn.
            ToolLayer::Meaning => {
                let kind = tool.paint_kind;
                let mut traits: Vec<&str> = Vec::new();
                if kind.flammable() {
                    traits.push("inflamable");
                }
                if kind.cuttable() {
                    traits.push("cortable");
                }
                let traits = if traits.is_empty() {
                    "inerte".to_string()
                } else {
                    traits.join(", ")
                };
                format!(
                    "{}: {} — pisada {:?} · {traits}",
                    ToolLayer::Meaning.label(),
                    kind.label(),
                    kind.surface()
                )
            }
        };
    }
    if let Ok(mut text) = knob_line.single_mut() {
        let (undo, redo) = history.depth();
        // Strength is a relief-only knob (painting has no rate), so showing it in
        // the meaning layer would advertise a control that does nothing.
        let strength = match tool.layer {
            ToolLayer::Relief => format!(" · fuerza {:.1}×", tool.strength),
            ToolLayer::Meaning => String::new(),
        };
        **text = format!(
            "radio {:.0} m{strength} · deshacer {undo} / rehacer {redo}",
            tool.radius
        );
    }
    if let Ok(mut text) = legend_line.single_mut() {
        let palette = match tool.layer {
            ToolLayer::Relief => BrushKind::ALL
                .iter()
                .enumerate()
                .map(|(index, brush)| format!("{} {}", index + 1, brush.label()))
                .collect::<Vec<_>>()
                .join(" · "),
            ToolLayer::Meaning => TerrainKind::ALL
                .iter()
                .enumerate()
                .map(|(index, kind)| format!("{} {}", index + 1, kind.label()))
                .collect::<Vec<_>>()
                .join(" · "),
        };
        let layer_keys = match tool.layer {
            ToolLayer::Relief => "MMB suaviza siempre · rueda: radio · Shift+rueda o [ ]: fuerza\n",
            ToolLayer::Meaning => "LMB pinta · para borrar, pintá Tierra encima · rueda: radio\n",
        };
        **text = format!(
            "{palette}\n\
             {layer_keys}\
             F6 cambia de capa · Ctrl+Z/Ctrl+Y deshacer · Ctrl+S guardar · Ctrl+L recargar · F5 salir\n\
             En freecam (F3) el RMB mira, no baja · con un panel abierto el pincel calla"
        );
    }
}
