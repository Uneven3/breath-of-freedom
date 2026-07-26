//! The semantic brush: painting **what the ground is**, cell by cell.
//!
//! The sibling of `brush`, and deliberately much smaller, because the hard parts
//! are already solved elsewhere and reused here rather than copied:
//! [`Terrain::paint_area`](crate::world::Terrain::paint_area) owns *how* a cell
//! changes, and `brush`'s [`pointer_state`] owns the question of whose click this
//! is — the one piece of this tool that has actually bitten us, when the sculpt
//! brush used to dig craters behind open UI panels.
//!
//! What is genuinely different from sculpting: a paint stroke has no rate. It is
//! a set of cells assigned a value, so holding the button still does nothing
//! after the first frame, and the undo history only gets an entry when a cell
//! actually changed kind. Repainting sand as sand is not something you did.

use avian3d::prelude::SpatialQuery;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::brush::{Pointer, cursor_terrain_hit, pointer_state};
use super::{EditorTool, SculptFocus, ToolLayer, history::SculptHistory};
use crate::camera::CameraRig;
use crate::input::ModalInputFocus;
use crate::world::Terrain;

/// While held: paint the selected kind under the cursor.
///
/// The right button is not an eraser here. There is no "no kind" to erase to —
/// every cell is always made of something — so clearing a patch means painting
/// `Soil` over it, which the palette already offers.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_terrain(
    mut tool: ResMut<EditorTool>,
    mut history: ResMut<SculptHistory>,
    buttons: Res<ButtonInput<MouseButton>>,
    spatial: SpatialQuery,
    focus: Res<ModalInputFocus>,
    owner: Query<Entity, With<SculptFocus>>,
    rig: Query<Entity, With<CameraRig>>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut terrain: Query<(Entity, &mut Terrain)>,
) {
    let Ok((entity, mut terrain)) = terrain.single_mut() else {
        return;
    };
    if !tool.active || tool.layer != ToolLayer::Meaning {
        return;
    }
    let painting = buttons.pressed(MouseButton::Left)
        && pointer_state(&focus, &owner, &rig) != Pointer::Theirs;
    if !painting {
        // Same rule as sculpting: losing the pointer to a panel *closes* the
        // stroke rather than pausing it. What you painted before reaching for the
        // hub is one finished thing you did.
        tool.anchor = None;
        history.end_stroke(&terrain);
        return;
    }
    let (Ok(window), Ok((camera, camera_transform))) = (window.single(), camera.single()) else {
        return;
    };
    let Some(hit) = cursor_terrain_hit(&spatial, window, camera, camera_transform, entity) else {
        return;
    };

    // Predates the first edit of the stroke, and taken on the first frame with a
    // hit rather than on the button-down edge, which may have landed off the
    // terrain. Idempotent, so repeating it every frame of the drag is free.
    history.begin_stroke(&terrain);

    let kind = tool.paint_kind;
    let radius = tool.radius;
    // Only flag `Changed<Terrain>` when a cell really changed kind. Painting is
    // idempotent, so a held button would otherwise re-trigger the collider and
    // mesh rebuild every frame for a grid nobody touched — the exact cost that
    // made sculpting expensive before it was measured.
    if terrain
        .bypass_change_detection()
        .paint_area(hit.xz(), radius, kind)
    {
        terrain.set_changed();
    }
}
