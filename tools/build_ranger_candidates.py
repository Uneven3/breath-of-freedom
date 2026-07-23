"""Build head + Ranger outfit GLBs without modifying Quaternius sources.

Run from the repository root:
  blender --background --factory-startup --python tools/build_ranger_candidates.py
"""

from pathlib import Path
import sys

import bmesh
import bpy

TOOLS_ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_ROOT))

from blender_export import export_selected_glb


REPO = Path(__file__).resolve().parents[1]
BASE_ROOT = (
    REPO
    / "assets"
    / "Universal Base Characters[Standard]"
    / "Universal Base Characters[Standard]"
    / "Base Characters"
    / "Godot - UE"
)
OUTFIT_ROOT = (
    REPO
    / "assets"
    / "Modular Character Outfits - Fantasy[Standard]"
    / "Modular Character Outfits - Fantasy[Standard]"
    / "Exports"
    / "glTF (Godot-Unreal)"
    / "Outfits"
)
ANIMATION_SOURCE = (
    REPO
    / "assets"
    / "Universal Animation Library 2[Standard]"
    / "Universal Animation Library 2[Standard]"
    / "Unreal-Godot"
    / "UAL2_Standard.glb"
)
OUTPUT_ROOT = REPO / "assets" / "game" / "characters"
MAX_TEXTURE_SIZE = 1024


def reset_scene() -> None:
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    bpy.ops.outliner.orphans_purge(do_recursive=True)


def import_gltf(path: Path) -> set[bpy.types.Object]:
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(path))
    return set(bpy.data.objects) - before


def only_armature(objects: set[bpy.types.Object], label: str) -> bpy.types.Object:
    armatures = [obj for obj in objects if obj.type == "ARMATURE"]
    if len(armatures) != 1:
        raise RuntimeError(f"{label}: expected one armature, found {len(armatures)}")
    return armatures[0]


def cut_to_head(body: bpy.types.Object) -> None:
    kept_groups = {
        group.index
        for group in body.vertex_groups
        if group.name in {"Head", "head", "neck_01"}
    }
    if not kept_groups:
        raise RuntimeError(f"{body.name}: Head/neck vertex groups not found")
    kept_vertices = {
        vertex.index
        for vertex in body.data.vertices
        if any(group.group in kept_groups and group.weight > 0.001 for group in vertex.groups)
    }
    if not kept_vertices:
        raise RuntimeError(f"{body.name}: Head/neck vertex groups have no weights")

    mesh = body.data
    editable = bmesh.new()
    editable.from_mesh(mesh)
    editable.verts.ensure_lookup_table()
    removed = [vertex for vertex in editable.verts if vertex.index not in kept_vertices]
    bmesh.ops.delete(editable, geom=removed, context="VERTS")
    editable.to_mesh(mesh)
    editable.free()
    mesh.update()


def bind_to_armature(mesh_object: bpy.types.Object, armature: bpy.types.Object) -> None:
    world_transform = mesh_object.matrix_world.copy()
    mesh_object.parent = armature
    mesh_object.matrix_world = world_transform
    for modifier in mesh_object.modifiers:
        if modifier.type == "ARMATURE":
            modifier.object = armature


def repair_missing_images() -> None:
    replacements: dict[bpy.types.Image, bpy.types.Image] = {}
    for image in tuple(bpy.data.images):
        source = Path(bpy.path.abspath(image.filepath))
        if source.exists():
            continue
        corrected = Path(str(source).replace("_png.png", ".png"))
        if corrected.exists():
            replacements[image] = bpy.data.images.load(str(corrected), check_existing=True)

    for material in bpy.data.materials:
        if material.node_tree is None:
            continue
        for node in material.node_tree.nodes:
            if getattr(node, "image", None) in replacements:
                node.image = replacements[node.image]


def resize_textures() -> None:
    for image in bpy.data.images:
        width, height = image.size
        longest_side = max(width, height)
        if longest_side <= MAX_TEXTURE_SIZE:
            continue
        scale = MAX_TEXTURE_SIZE / longest_side
        image.scale(max(1, round(width * scale)), max(1, round(height * scale)))


def strip_unsupported_vertex_attributes(mesh_objects: list[bpy.types.Object]) -> None:
    for obj in mesh_objects:
        while len(obj.data.color_attributes) > 1:
            obj.data.color_attributes.remove(obj.data.color_attributes[-1])
        while len(obj.data.uv_layers) > 2:
            obj.data.uv_layers.remove(obj.data.uv_layers[-1])


def build_candidate(gender: str) -> Path:
    reset_scene()
    animation_objects = import_gltf(ANIMATION_SOURCE)
    animation_armature = only_armature(animation_objects, "UAL2 Standard")
    for obj in tuple(animation_objects):
        if obj.type == "MESH":
            bpy.data.objects.remove(obj, do_unlink=True)

    outfit_objects = import_gltf(OUTFIT_ROOT / f"{gender}_Ranger.gltf")
    outfit_armature = only_armature(outfit_objects, f"{gender} Ranger")
    outfit_meshes = [obj for obj in outfit_objects if obj.type == "MESH"]
    for outfit_mesh in outfit_meshes:
        bind_to_armature(outfit_mesh, animation_armature)

    base_objects = import_gltf(BASE_ROOT / f"Superhero_{gender}_FullBody.gltf")
    base_armature = only_armature(base_objects, f"{gender} base")
    base_meshes = [obj for obj in base_objects if obj.type == "MESH"]
    if not base_meshes:
        raise RuntimeError(f"{gender}: base body mesh not found")
    body = max(base_meshes, key=lambda obj: len(obj.data.vertices))

    cut_to_head(body)
    head_objects = base_meshes
    for head_object in head_objects:
        bind_to_armature(head_object, animation_armature)

    repair_missing_images()
    resize_textures()
    strip_unsupported_vertex_attributes([*outfit_meshes, *head_objects])
    bpy.data.objects.remove(base_armature, do_unlink=True)
    bpy.data.objects.remove(outfit_armature, do_unlink=True)
    animation_armature.name = "Armature"
    for obj in bpy.data.objects:
        obj.select_set(False)
    export_objects = [animation_armature, *outfit_meshes, *head_objects]
    for obj in export_objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = animation_armature

    OUTPUT_ROOT.mkdir(parents=True, exist_ok=True)
    output = OUTPUT_ROOT / f"ranger_{gender.lower()}.glb"
    export_selected_glb(output, export_animations=True)
    print(f"Built {output.relative_to(REPO)}")
    return output


requested_genders = [
    value.capitalize()
    for value in sys.argv[sys.argv.index("--") + 1 :]
    if value.lower() in {"female", "male"}
] if "--" in sys.argv else []

try:
    for candidate_gender in requested_genders or ("Female", "Male"):
        build_candidate(candidate_gender)
finally:
    bpy.ops.wm.quit_blender()
