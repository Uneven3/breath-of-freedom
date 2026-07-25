import bpy

def optimize_grass(key, target_poly_count, height_scale, source_fbx):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.unit_settings.system = "METRIC"
    scene.unit_settings.scale_length = 1.0

    bpy.ops.import_scene.fbx(filepath=source_fbx)
    mesh_objs = [o for o in scene.objects if o.type == "MESH"]
    orig_obj = mesh_objs[0]

    bpy.ops.object.select_all(action="DESELECT")
    orig_obj.select_set(True)
    bpy.context.view_layer.objects.active = orig_obj

    current_h = orig_obj.dimensions.z
    if current_h > 0:
        scale_factor = height_scale / current_h
        orig_obj.scale = (scale_factor, scale_factor, scale_factor)

    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)

    current_polys = len(orig_obj.data.polygons)
    if current_polys > target_poly_count:
        ratio = target_poly_count / current_polys
        dec = orig_obj.modifiers.new(name="Decimate0", type="DECIMATE")
        dec.ratio = ratio
        bpy.ops.object.modifier_apply(modifier="Decimate0")

    mesh = orig_obj.data
    mesh.normals_split_custom_set_from_vertices([(0.0, 0.0, 1.0)] * len(mesh.vertices))

    mat_name = "M_FoliageCommon"
    mat = bpy.data.materials.get(mat_name) or bpy.data.materials.new(name=mat_name)
    mat.use_nodes = True
    principled = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    principled.inputs["Base Color"].default_value = (0.27, 0.50, 0.22, 1.0)
    principled.inputs["Metallic"].default_value = 0.0
    principled.inputs["Roughness"].default_value = 0.9

    orig_obj.data.materials.clear()
    orig_obj.data.materials.append(mat)

    orig_obj.name = "SM_Grass_LOD0"
    orig_obj.data.name = "SM_Grass_LOD0"

    bpy.ops.object.select_all(action="DESELECT")
    orig_obj.select_set(True)
    bpy.context.view_layer.objects.active = orig_obj

    bpy.ops.object.duplicate()
    lod1_obj = bpy.context.active_object
    lod1_obj.name = "SM_Grass_LOD1"
    lod1_mesh = lod1_obj.data
    lod1_mesh.name = "SM_Grass_LOD1"

    dec_mod = lod1_obj.modifiers.new(name="Decimate1", type="DECIMATE")
    dec_mod.ratio = 0.4
    bpy.ops.object.modifier_apply(modifier="Decimate1")
    lod1_mesh.normals_split_custom_set_from_vertices([(0.0, 0.0, 1.0)] * len(lod1_mesh.vertices))

    h = height_scale
    w = (orig_obj.dimensions.x + orig_obj.dimensions.y) * 0.25
    verts = [
        (-w, 0.0, 0.0), (w, 0.0, 0.0), (w, 0.0, h), (-w, 0.0, h),
        (0.0, -w, 0.0), (0.0, w, 0.0), (0.0, w, h), (0.0, -w, h)
    ]
    faces = [(0, 1, 2, 3), (4, 5, 6, 7)]
    lod2_mesh = bpy.data.meshes.new(name="SM_Grass_LOD2")
    lod2_mesh.from_pydata(verts, [], faces)
    lod2_mesh.update()
    lod2_mesh.normals_split_custom_set_from_vertices([(0.0, 0.0, 1.0)] * len(lod2_mesh.vertices))

    lod2_obj = bpy.data.objects.new(name="SM_Grass_LOD2", object_data=lod2_mesh)
    scene.collection.objects.link(lod2_obj)
    lod2_obj.data.materials.append(mat)

    root_name = f"ROOT_{key}"
    root_obj = bpy.data.objects.new(root_name, None)
    scene.collection.objects.link(root_obj)
    root_obj["bof_license"] = "CC0-1.0"
    root_obj["bof_material_kind"] = "grass"

    for child in [orig_obj, lod1_obj, lod2_obj]:
        child.parent = root_obj
        child.matrix_parent_inverse = root_obj.matrix_world.inverted()

    out_blend = f"art/blender/props/{key}.blend"
    bpy.ops.wm.save_as_mainfile(filepath=out_blend)
    print(f"[optimize] {key}: LOD0 polys={len(orig_obj.data.polygons)}, saved {out_blend}")

optimize_grass("prop_grass_b", 100, 0.45, "assets/Stylized Nature MegaKit[Standard]/FBX/Grass_Wispy_Short.fbx")
optimize_grass("prop_grass_c", 100, 0.65, "assets/Stylized Nature MegaKit[Standard]/FBX/Grass_Common_Tall.fbx")
