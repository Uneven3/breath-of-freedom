"""Shared, deterministic Blender glTF export settings.

This module deliberately contains no asset-specific construction logic.  Both
the conventional exporter and temporary migration tools call this one seam.
"""

from pathlib import Path

import bpy


def select_hierarchy(root: bpy.types.Object) -> None:
    """Select only ``root`` and all of its descendants."""
    bpy.ops.object.select_all(action="DESELECT")
    objects = [root, *root.children_recursive]
    for obj in objects:
        obj.hide_set(False)
        obj.hide_viewport = False
        obj.select_set(True)
    bpy.context.view_layer.objects.active = root


def export_selected_glb(
    output: Path,
    *,
    export_animations: bool,
) -> None:
    """Export the current selection using the repository's fixed GLB recipe."""
    output.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=str(output),
        check_existing=False,
        export_format="GLB",
        use_selection=True,
        export_animations=export_animations,
        export_animation_mode="ACTIONS",
        export_extras=True,
        export_yup=True,
    )

