//! The instance brush: scattering and clearing props within a circle, the
//! same interaction model Relief and Meaning already use — one shared radius,
//! left button adds, right button removes. A click-per-instance model would
//! make populating anything that reads as a meadow impractical; a brush the
//! author already knows from the other two layers is what makes a third
//! layer "clear to use" instead of a third set of rules to learn.
//!
//! No undo here yet — see `docs/MAP_EDITOR.md`. Placing or clearing an
//! instance is not filed in [`super::history::SculptHistory`], which only
//! covers the height/kind grid. `Ctrl+L` (reload) can therefore discard
//! unsaved instance changes without a way to get them back; the HUD says so.

use avian3d::prelude::SpatialQuery;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::brush::{Pointer, cursor_terrain_hit, pointer_state};
use super::{EditorTool, SculptFocus, ToolLayer};
use crate::camera::CameraRig;
use crate::input::ModalInputFocus;
use crate::world::Terrain;
use crate::world::forest::{hash_u32, hash_unit};

/// Default footprint of every placed instance in this first cut — no scale
/// control yet, so every prop goes down at its authored size.
const DEFAULT_SCALE: f32 = 1.0;

/// While held: scatter (LMB) or clear (RMB) within the brush circle.
#[allow(clippy::too_many_arguments)]
pub(super) fn place_or_clear_instances(
    tool: Res<EditorTool>,
    mut attempt: Local<u32>,
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
    if !tool.active || tool.layer != ToolLayer::Instances {
        return;
    }
    let pointer = pointer_state(&focus, &owner, &rig);
    if pointer == Pointer::Theirs {
        return;
    }
    let placing = buttons.pressed(MouseButton::Left);
    let clearing = !placing && buttons.pressed(MouseButton::Right) && pointer == Pointer::Ours;
    if !placing && !clearing {
        return;
    }
    let (Ok(window), Ok((camera, camera_transform))) = (window.single(), camera.single()) else {
        return;
    };
    let Some(hit) = cursor_terrain_hit(&spatial, window, camera, camera_transform, |hit| {
        hit == entity
    }) else {
        return;
    };
    let centre = hit.xz();
    let radius = tool.radius;

    if clearing {
        if terrain
            .bypass_change_detection()
            .remove_instances_in_radius(centre, radius)
        {
            terrain.set_changed();
        }
        return;
    }

    // One placement attempt per frame: a uniformly random point inside the
    // circle, seeded by a counter local to this system so a held button
    // samples a different spot every frame instead of retrying the same one.
    // `Terrain::place_instance` owns the minimum-spacing rejection — the
    // circle self-limits once it is full, so holding the button longer does
    // not overpack it.
    *attempt = attempt.wrapping_add(1);
    let angle = hash_unit(*attempt ^ 0x2f6e_19a3) * std::f32::consts::TAU;
    let distance = hash_unit(*attempt ^ 0x7c15_8ad1).sqrt() * radius;
    let candidate = centre + Vec2::new(angle.cos(), angle.sin()) * distance;
    let yaw = hash_unit(hash_u32(*attempt) ^ 0x1d94_6b77) * std::f32::consts::TAU;
    if terrain.bypass_change_detection().place_instance(
        tool.prop_kind,
        candidate,
        yaw,
        DEFAULT_SCALE,
    ) {
        terrain.set_changed();
    }
}
