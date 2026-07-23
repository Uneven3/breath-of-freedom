"""Validate and export one convention-authored Blender asset.

Run from the repository root:
  blender -noaudio --background --factory-startup \
    --python tools/export_blender_asset.py -- \
    --source art/blender/trees/tree_pine_a.blend \
    --output assets/game/authored/trees/tree_pine_a.glb
"""

from __future__ import annotations

import argparse
from collections import defaultdict
from pathlib import Path
import re
import sys

import bpy

TOOLS_ROOT = Path(__file__).resolve().parent
REPO = TOOLS_ROOT.parent
sys.path.insert(0, str(TOOLS_ROOT))

from blender_export import export_selected_glb, select_hierarchy


CATEGORY_DIRECTORIES = {
    "char": "characters",
    "creature": "creatures",
    "prop": "props",
    "structure": "structures",
    "tree": "trees",
    "weapon": "weapons",
}
ASSET_KEY = re.compile(
    rf"^(?:{'|'.join(CATEGORY_DIRECTORIES)})_[a-z0-9]+(?:_[a-z0-9]+)*$"
)
RENDER_NAME = re.compile(r"^(SM|SK)_([A-Z][A-Za-z0-9]*)_LOD([0-2])$")
HELPER_PREFIXES = ("UCX_", "UBX_", "UCP_", "USP_", "UCY_")
SOCKET_NAME = re.compile(r"^SKT_[A-Z][A-Za-z0-9]*$")
ANIMATION_NAME = re.compile(r"^AN_[A-Z][A-Za-z0-9]*(?:_(?:[A-Z][A-Za-z0-9]*|[0-9]+))*$")
EPSILON = 1.0e-5


def fail(message: str) -> None:
    raise RuntimeError(f"[asset-export] {message}")


def arguments() -> argparse.Namespace:
    try:
        separator = sys.argv.index("--")
    except ValueError:
        fail("missing '--' before exporter arguments")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    return parser.parse_args(sys.argv[separator + 1 :])


def absolute_repo_path(path: Path) -> Path:
    return path if path.is_absolute() else REPO / path


def validate_paths(source: Path, output: Path) -> str:
    if source.suffix != ".blend" or not source.is_file():
        fail(f"source must be an existing .blend: {source}")
    if output.suffix != ".glb":
        fail(f"output must end in .glb: {output}")
    key = output.stem
    if ASSET_KEY.fullmatch(key) is None:
        fail(f"invalid asset key '{key}'")
    category = key.split("_", 1)[0]
    expected_parent = REPO / "assets" / "game" / "authored" / CATEGORY_DIRECTORIES[category]
    if output.parent.resolve() != expected_parent.resolve():
        fail(f"{key} must export under {expected_parent.relative_to(REPO)}")
    if source.stem != key:
        fail(f"source and output must share asset key '{key}'")
    return key


def near(value: float, expected: float) -> bool:
    return abs(value - expected) <= EPSILON


def validate_transform(obj: bpy.types.Object) -> None:
    if not all(near(value, 1.0) for value in obj.scale):
        fail(f"{obj.name}: apply scale before export")
    rotation = obj.rotation_euler
    if not all(near(value, 0.0) for value in rotation):
        fail(f"{obj.name}: apply rotation before export")


def validate_material(material: bpy.types.Material, owner: str) -> None:
    if not material.name.startswith("M_") or len(material.name) <= 2:
        fail(f"{owner}: material '{material.name}' must be M_<ClavePaleta>")
    if not material.use_nodes or material.node_tree is None:
        fail(f"{owner}: {material.name} must use a Principled BSDF node")
    principled = next(
        (node for node in material.node_tree.nodes if node.type == "BSDF_PRINCIPLED"),
        None,
    )
    if principled is None:
        fail(f"{owner}: {material.name} has no Principled BSDF node")
    if principled.inputs["Metallic"].default_value > EPSILON:
        fail(f"{owner}: {material.name} must have metallic = 0")
    if principled.inputs["Roughness"].default_value < 0.8 - EPSILON:
        fail(f"{owner}: {material.name} must have roughness >= 0.8")


def validate_scene(key: str) -> tuple[bpy.types.Object, bool]:
    scene = bpy.context.scene
    if scene.unit_settings.system != "METRIC" or not near(scene.unit_settings.scale_length, 1.0):
        fail("scene units must be Metric with Unit Scale = 1")

    roots = [obj for obj in scene.objects if obj.parent is None]
    expected_root = f"ROOT_{key}"
    if len(roots) != 1 or roots[0].name != expected_root:
        names = ", ".join(obj.name for obj in roots) or "<none>"
        fail(f"expected one root '{expected_root}', found {names}")
    root = roots[0]
    validate_transform(root)
    if not root.get("bof_license"):
        fail(f"{expected_root}: missing bof_license")

    names: set[str] = set()
    lods: defaultdict[str, set[int]] = defaultdict(set)
    render_kinds: set[str] = set()
    spatial_helpers = False
    for obj in [root, *root.children_recursive]:
        if obj.name in names:
            fail(f"duplicate object name '{obj.name}'")
        names.add(obj.name)
        if obj.name == expected_root:
            continue

        render = RENDER_NAME.fullmatch(obj.name)
        if render is not None:
            if obj.type != "MESH":
                fail(f"{obj.name}: render node must be a mesh object")
            if obj.data.name != obj.name:
                fail(f"{obj.name}: mesh datablock must share the object name")
            validate_transform(obj)
            if not obj.material_slots:
                fail(f"{obj.name}: render mesh has no material")
            for slot in obj.material_slots:
                if slot.material is None:
                    fail(f"{obj.name}: empty material slot")
                validate_material(slot.material, obj.name)
            kind, part, level = render.groups()
            render_kinds.add(kind)
            lods[part].add(int(level))
            continue

        if obj.name.startswith(HELPER_PREFIXES):
            spatial_helpers = True
            if obj.type != "MESH":
                fail(f"{obj.name}: collision helper must be a mesh object")
            if obj.data.name != obj.name:
                fail(f"{obj.name}: collision mesh datablock must share the object name")
            validate_transform(obj)
            continue

        if SOCKET_NAME.fullmatch(obj.name) is not None:
            spatial_helpers = True
            if obj.type != "EMPTY":
                fail(f"{obj.name}: socket must be an Empty")
            validate_transform(obj)
            continue

        if obj.type == "MESH":
            fail(f"{obj.name}: mesh lacks SM_/SK_/collision convention")

    if not lods:
        fail("asset has no conventional render meshes")
    if len(render_kinds) != 1:
        fail("an asset cannot mix SM_ and SK_ render meshes")
    for part, levels in lods.items():
        if levels != set(range(max(levels) + 1)):
            fail(f"{part}: LODs must be contiguous starting at LOD0")
    if spatial_helpers and not root.get("bof_profile"):
        fail(f"{expected_root}: spatial helpers require bof_profile")

    actions = list(bpy.data.actions)
    is_skinned = render_kinds == {"SK"}
    if is_skinned and not actions:
        fail("skinned asset requires named animation actions")
    if not is_skinned and actions:
        fail("static asset cannot contain animation actions")
    for action in actions:
        if ANIMATION_NAME.fullmatch(action.name) is None:
            fail(f"invalid animation action '{action.name}'")
    return root, is_skinned


def main() -> None:
    args = arguments()
    source = absolute_repo_path(args.source).resolve()
    output = absolute_repo_path(args.output).resolve()
    key = validate_paths(source, output)
    bpy.ops.wm.open_mainfile(filepath=str(source))
    root, is_skinned = validate_scene(key)
    select_hierarchy(root)
    export_selected_glb(output, export_animations=is_skinned)
    print(f"[asset-export] wrote {output.relative_to(REPO)}")


try:
    main()
except Exception as error:
    print(error, file=sys.stderr)
    raise
finally:
    bpy.ops.wm.quit_blender()
