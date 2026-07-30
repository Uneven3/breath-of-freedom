"""Distill Elf_MaleCharacter-Free + Universal Animation Library clips into a
working character .blend, re-skinned onto the donor UAL armature so the
vendor clips play natively (no bone-to-bone retargeting).

Run from the repository root:
  blender -noaudio --background --factory-startup \
    --python tools/distill_char_elf.py -- \
    --elf "assets/Elf_MaleCharacter-Free.blend" \
    --ual1 "assets/Universal Animation Library[Standard]/Unreal-Godot/UAL1_Standard.glb" \
    --ual2 "assets/Universal Animation Library 2[Standard]/Unreal-Godot/UAL2_Standard.glb" \
    --out art/blender/char/char_elf.blend \
    --shots /tmp/char_elf_qa

Output is a *working* source .blend, not yet a `bof_animset="player"`-ready
asset: it still needs `bof_license` (no license file ships with the elf
source), a palette material in place of its own `M_Atlas-1.002`, and a pass
through `tools/export_blender_asset.py` once those are decided. See
docs/ASSET_PIPELINE.md for the contract this feeds.
"""
import bpy
import sys
import argparse
import mathutils
from pathlib import Path


def args():
    sep = sys.argv.index("--")
    p = argparse.ArgumentParser()
    p.add_argument("--elf", required=True)
    p.add_argument("--ual1", required=True)
    p.add_argument("--ual2", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--shots", required=True)
    return p.parse_args(sys.argv[sep + 1:])


A = args()
SHOTS = Path(A.shots)
SHOTS.mkdir(parents=True, exist_ok=True)

# ---------------------------------------------------------------------------
# 1. Fresh scene, import the UAL1 donor (armature + Mannequin skin + clips).
# ---------------------------------------------------------------------------
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=A.ual1)

donor_arm = None
donor_mesh = None
for obj in list(bpy.context.scene.objects):
    if obj.type == "ARMATURE":
        donor_arm = obj
    elif obj.name == "Mannequin":
        donor_mesh = obj
    elif obj.name == "Icosphere":
        bpy.data.objects.remove(obj, do_unlink=True)

assert donor_arm and donor_mesh, "UAL1 import missing Armature/Mannequin"
donor_arm.name = "Armature"

# Clip -> role mapping (see docs/ASSET_PIPELINE.md contract + ROLE_TABLE in
# src/visuals/animation.rs). One source clip may feed several roles
# (duplicated as separate Actions, since the contract is 1 Action = 1
# AN_<Role> clip). Neither library has a real Glide, Climb/Ladder/Mantle/
# Vault, WallJump/EdgeLeap, or directional-strafe clip: these reuse the
# closest available pose, matching what ROLE_TABLE already falls back to at
# runtime for the vendor placeholder today.
ROLE_SOURCES = {
    "AN_Idle": "Idle_Loop",
    "AN_Walk": "Walk_Loop",
    "AN_Run": "Sprint_Loop",
    "AN_Sneak": "Crouch_Fwd_Loop",
    "AN_Jump": "Jump_Start",
    "AN_Fall": "Jump_Loop",
    "AN_Glide": "NinjaJump_Idle_Loop",
    "AN_Climb": "ClimbUp_1m",
    "AN_Ladder": "ClimbUp_1m",
    "AN_Mantle": "ClimbUp_1m",
    "AN_Vault": "ClimbUp_1m",
    "AN_WallJump": "NinjaJump_Start",
    "AN_EdgeLeap": "NinjaJump_Start",
    "AN_Swim": "Swim_Fwd_Loop",
}
NEEDED_SOURCES = set(ROLE_SOURCES.values())

for action in list(bpy.data.actions):
    if action.name in NEEDED_SOURCES:
        action.use_fake_user = True
    else:
        bpy.data.actions.remove(action)

print(f"[distill] kept {len(bpy.data.actions)} source actions after UAL1 pass")

# ---------------------------------------------------------------------------
# 2. Import UAL2 for its action library only (Climb/NinjaJump), then drop its
#    rig/mesh objects — the actions survive via fake_user.
# ---------------------------------------------------------------------------
before = set(bpy.data.objects.keys())
bpy.ops.import_scene.gltf(filepath=A.ual2)
new_objects = [o for o in bpy.context.scene.objects if o.name not in before]

for action in list(bpy.data.actions):
    if action.name in NEEDED_SOURCES:
        action.use_fake_user = True
    elif not action.use_fake_user:
        bpy.data.actions.remove(action)

for obj in new_objects:
    bpy.data.objects.remove(obj, do_unlink=True)

missing = NEEDED_SOURCES - {a.name for a in bpy.data.actions}
assert not missing, f"missing source clips after UAL2 pass: {missing}"
print(f"[distill] have all {len(NEEDED_SOURCES)} needed source clips")

# ---------------------------------------------------------------------------
# 3. Append the elf meshes + its own rig (rig only used transiently, to
#    un-pose the elf into the donor's T-pose using its own correct weights;
#    deleted once that bake is done).
# ---------------------------------------------------------------------------
with bpy.data.libraries.load(A.elf, link=False) as (data_from, data_to):
    data_to.objects = list(data_from.objects)

elf_objects = {}
elf_rig = None
for obj in data_to.objects:
    if obj is None:
        continue
    bpy.context.scene.collection.objects.link(obj)
    elf_objects[obj.name] = obj
    if obj.type == "ARMATURE":
        elf_rig = obj

assert elf_rig, "elf import missing armature"
bpy.context.view_layer.update()

ELF_BODY_PARTS = {
    "Elf_MaleCharacter-free": "SK_Body_LOD0",
    "Basis_Belt-free": "SK_Belt_LOD0",
    "Basic_Pant-free": "SK_Pants_LOD0",
}

# ---------------------------------------------------------------------------
# 4. Un-pose the ELF from its authored A-pose into the donor's T-pose, using
#    the elf's OWN (correctly authored) rig and weights, then bake that
#    deformation permanently into the mesh.
#
#    Why: Blender's Armature modifier always binds relative to the target
#    armature's rest matrix (`bone.matrix_local`) — at identity pose that
#    transform is a no-op for every bone regardless of vertex weights, which
#    is why a first attempt (nearest-surface transfer onto a *donor* merely
#    posed to match the elf's A-pose, weights left otherwise untouched) looked
#    perfect at rest and only exploded once a clip actually rotated the arms:
#    the mesh's raw vertex data was A-pose-shaped while the bind reference
#    (donor rest = T-pose) assumed T-pose-shaped data, so posing away from
#    identity re-interpreted every arm vertex through the wrong frame. The
#    fix has to make the *mesh data itself* T-pose-shaped before binding, not
#    paper over it with donor posing or weight smoothing (both tried; neither
#    addresses the actual reference-frame mismatch, confirmed by the same
#    Walk_Loop clip playing perfectly on the donor's own untouched mesh).
# ---------------------------------------------------------------------------
ARM_BONE_PAIRS = [
    ("L.Shoulder", "clavicle_l"),
    ("L.Arm", "upperarm_l"),
    ("L.ForeArm", "lowerarm_l"),
    ("R.Shoulder", "clavicle_r"),
    ("R.Arm", "upperarm_r"),
    ("R.ForeArm", "lowerarm_r"),
]


def bone_dir_local(armature_obj, bone_name):
    """Rest-pose bone direction (head->tail), in the armature's own local
    space. Both armature objects sit at identity world transform, so local
    space doubles as a shared comparison frame."""
    b = armature_obj.data.bones[bone_name]
    return (b.tail_local - b.head_local).normalized()


bpy.context.view_layer.objects.active = elf_rig
bpy.ops.object.mode_set(mode="POSE")
for pose_bone in elf_rig.pose.bones:
    pose_bone.rotation_mode = "QUATERNION"

for elf_name, donor_name in ARM_BONE_PAIRS:
    bpy.context.view_layer.update()
    pose_bone = elf_rig.pose.bones[elf_name]
    current_matrix = pose_bone.matrix.copy()  # armature space, parent already posed
    current_dir = (current_matrix.to_3x3() @ mathutils.Vector((0.0, 1.0, 0.0))).normalized()
    target_dir = bone_dir_local(donor_arm, donor_name)  # shared armature-space frame

    # World-space rotation that swings the bone's current direction onto the
    # donor's T-pose target direction, pivoting about the bone's own head so
    # the joint doesn't translate: Translation(head) @ Rot @ Translation(-head).
    q_world = current_dir.rotation_difference(target_dir)
    head = current_matrix.translation
    rot4 = q_world.to_matrix().to_4x4()
    new_matrix = (
        mathutils.Matrix.Translation(head)
        @ rot4
        @ mathutils.Matrix.Translation(-head)
        @ current_matrix
    )
    pose_bone.matrix = new_matrix

bpy.context.view_layer.update()
bpy.ops.object.mode_set(mode="OBJECT")
print("[distill] posed elf's own rig from A-pose into the donor's T-pose")

# ---------------------------------------------------------------------------
# 5. QA render: compare the elf now posed into T-pose against the donor's
#    native T-pose Mannequin, before trusting anything to nearest-surface.
# ---------------------------------------------------------------------------
def setup_camera_and_shade():
    bpy.context.scene.render.engine = "BLENDER_WORKBENCH"
    bpy.context.scene.display.shading.light = "FLAT"
    bpy.context.scene.display.shading.color_type = "MATERIAL"
    bpy.context.scene.render.resolution_x = 512
    bpy.context.scene.render.resolution_y = 768
    cam_data = bpy.data.cameras.new("QACam")
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = 2.4
    cam_obj = bpy.data.objects.new("QACam", cam_data)
    cam_obj.location = (0, -5, 0.95)
    cam_obj.rotation_euler = (1.5708, 0, 0)
    bpy.context.scene.collection.objects.link(cam_obj)
    bpy.context.scene.camera = cam_obj
    return cam_obj


def render_to(path):
    bpy.context.scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True)
    print(f"[distill] wrote {path}")


cam = setup_camera_and_shade()
render_to(SHOTS / "01_pose_match_donor_vs_elf.png")

# ---------------------------------------------------------------------------
# 6. Bake the T-pose deformation into each elf mesh's base data (apply the
#    Armature modifier while elf_rig is posed), so the geometry itself is now
#    genuinely T-pose-shaped — no more reference-frame mismatch downstream.
# ---------------------------------------------------------------------------
for name in ELF_BODY_PARTS:
    obj = elf_objects[name]
    bpy.context.view_layer.objects.active = obj
    for mod in list(obj.modifiers):
        if mod.type == "ARMATURE":
            bpy.ops.object.modifier_apply(modifier=mod.name)
    for vg in list(obj.vertex_groups):
        obj.vertex_groups.remove(vg)

print("[distill] baked T-pose into elf mesh geometry; elf_rig no longer needed")
bpy.data.objects.remove(elf_rig, do_unlink=True)

# ---------------------------------------------------------------------------
# 7. Weight transfer: donor Mannequin (pristine, native T-pose) -> each elf
#    mesh (now also T-pose), nearest-surface interpolated. Both meshes share
#    the same reference pose this time, so no donor posing/reset needed.
# ---------------------------------------------------------------------------
for name in ELF_BODY_PARTS:
    obj = elf_objects[name]
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    donor_mesh.select_set(True)
    bpy.context.view_layer.objects.active = donor_mesh  # source = active
    bpy.ops.object.data_transfer(
        data_type="VGROUP_WEIGHTS",
        use_create=True,
        vert_mapping="POLYINTERP_NEAREST",
        layers_select_src="ALL",
        layers_select_dst="NAME",
    )
    print(f"[distill] transferred weights: {len(obj.vertex_groups)} groups on {name}")

# ---------------------------------------------------------------------------
# 8. Rebuild the hierarchy: ROOT_char_elf -> Armature -> SK_*_LOD0 meshes.
#    Drop the donor's own skin mesh (elf_rig was already removed in step 6).
# ---------------------------------------------------------------------------
bpy.data.objects.remove(donor_mesh, do_unlink=True)

root = bpy.data.objects.new("ROOT_char_elf", None)
bpy.context.scene.collection.objects.link(root)
donor_arm.parent = root

for src_name, render_name in ELF_BODY_PARTS.items():
    obj = elf_objects[src_name]
    obj.name = render_name
    obj.data.name = render_name
    obj.parent = donor_arm
    mod = obj.modifiers.new(name="Armature", type="ARMATURE")
    mod.object = donor_arm

bpy.context.view_layer.update()
print("[distill] rebuilt hierarchy under ROOT_char_elf")

# ---------------------------------------------------------------------------
# 9. Rename/duplicate source clips into the contract's AN_<Role> vocabulary.
# ---------------------------------------------------------------------------
by_source = {a.name: a for a in bpy.data.actions}
seen_sources = set()
for role_name, source_name in ROLE_SOURCES.items():
    source_action = by_source[source_name]
    if source_name in seen_sources:
        action = source_action.copy()
    else:
        action = source_action
        seen_sources.add(source_name)
    action.name = role_name
    action.use_fake_user = True

for action in list(bpy.data.actions):
    if action.name not in ROLE_SOURCES:
        bpy.data.actions.remove(action)

print(f"[distill] final action set: {sorted(a.name for a in bpy.data.actions)}")

# ---------------------------------------------------------------------------
# 10. QA renders: rest pose, plus a couple of clips mid-frame, to check for
#     obvious skinning blow-ups.
# ---------------------------------------------------------------------------
render_to(SHOTS / "02_rest_pose.png")


def preview_action(action_name, frame_fraction=0.5):
    if donor_arm.animation_data is None:
        donor_arm.animation_data_create()
    action = bpy.data.actions[action_name]
    donor_arm.animation_data.action = action
    start, end = action.frame_range
    bpy.context.scene.frame_set(int(start + (end - start) * frame_fraction))
    bpy.context.view_layer.update()
    render_to(SHOTS / f"03_{action_name}.png")


for name in ["AN_Walk", "AN_Run", "AN_Jump", "AN_Climb"]:
    preview_action(name)

donor_arm.animation_data.action = None
bpy.context.scene.frame_set(0)

# ---------------------------------------------------------------------------
# 11. Drop the QA camera (render-only helper, not part of the asset) and save.
# ---------------------------------------------------------------------------
bpy.data.objects.remove(cam, do_unlink=True)
bpy.ops.wm.save_as_mainfile(filepath=A.out)
print(f"[distill] saved {A.out}")
