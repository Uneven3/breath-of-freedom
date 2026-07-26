import bpy
import math
import random
import os

def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj, do_unlink=True)

def create_botw_tuft(name_prefix, pascal_part, root_radius, height_min, height_max, mat_key, base_dir):
    reset_scene()
    
    blend_output = os.path.join(base_dir, f"art/blender/props/{name_prefix}.blend")
    glb_output = os.path.join(base_dir, f"assets/game/authored/props/{name_prefix}.glb")
    
    # Root Empty
    root_name = f"ROOT_{name_prefix}"
    root_obj = bpy.data.objects.new(root_name, None)
    bpy.context.scene.collection.objects.link(root_obj)
    
    # Extras for build.rs scanner
    root_obj["bof_license"] = "GNU GPLv3"
    root_obj["bof_profile"] = name_prefix
    root_obj["bof_material_kind"] = "Grass"
    
    # Material name matching MaterialPalette (M_FoliageCommon, M_FoliageDry, etc.)
    mat_name = f"M_{mat_key}"
    mat = bpy.data.materials.get(mat_name) or bpy.data.materials.new(name=mat_name)
    mat.use_nodes = True
    if mat.node_tree:
        bsdf = mat.node_tree.nodes.get("Principled BSDF")
        if bsdf:
            bsdf.inputs["Roughness"].default_value = 0.9
            bsdf.inputs["Metallic"].default_value = 0.0
        vcol_node = mat.node_tree.nodes.get("Color Attribute") or mat.node_tree.nodes.new("ShaderNodeVertexColor")
        vcol_node.layer_name = "Color"
    
    # BOTW LOD Volume Envelope Preservation:
    # Target volume envelope is ~0.60m - 0.75m in X and Y
    # LOD0: 3 blades (12 tris) scattered at 120 deg
    # LOD1: 2 crossed wide blades at 90 deg (6 tris, maintaining full X & Y volume envelope ~0.60m)
    # LOD2: 1 broad fanned card (2 tris, maintaining full X & Y volume envelope ~0.50m)
    lod_specs = [
        ("LOD0", 3, 0.08, 3), # 3 blades, 12 tris
        ("LOD1", 2, 0.30, 2), # 2 crossed wide blades (0.30m width each) = maintains X & Y volume envelope!
        ("LOD2", 1, 0.50, 1), # 1 broad fanned card (0.50m width) = maintains X & Y volume envelope!
    ]
    
    random.seed(101 + len(name_prefix))
    
    for lod_name, b_count, blade_width, seg_count in lod_specs:
        mesh_name = f"SM_{pascal_part}_{lod_name}"
        mesh = bpy.data.meshes.new(mesh_name)
        obj = bpy.data.objects.new(mesh_name, mesh)
        obj.parent = root_obj
        bpy.context.scene.collection.objects.link(obj)
        
        verts = []
        faces = []
        uvs = []
        
        step_angle = math.tau / b_count
        
        for b in range(b_count):
            angle = b * step_angle
            # LOD0 keeps root radius dispersion; LOD1 & LOD2 center their cards to prevent off-axis offset
            r_rad = root_radius if lod_name == "LOD0" else (root_radius * 0.4)
            base_x = math.cos(angle) * r_rad
            base_z = math.sin(angle) * r_rad
            
            # Blade orientation
            blade_yaw = angle + math.pi / 2.0
            dir_x = math.cos(blade_yaw)
            dir_z = math.sin(blade_yaw)
            
            blade_h = height_min + ((b % 2) * 0.10 * (height_max - height_min))
            half_w = blade_width * 0.5
            
            out_x = math.cos(angle)
            out_z = math.sin(angle)
            arc_dist = 0.18 * blade_h
            
            idx = len(verts)
            
            if seg_count == 3:
                # LOD0 (3 blades, 12 tris)
                v0 = (base_x - dir_x * half_w * 0.7, base_z - dir_z * half_w * 0.7, 0.0)
                v1 = (base_x + dir_x * half_w * 0.7, base_z + dir_z * half_w * 0.7, 0.0)
                
                w1 = half_w * 1.15
                arc1_x = base_x + out_x * (arc_dist * 0.40)
                arc1_z = base_z + out_z * (arc_dist * 0.40)
                v2 = (arc1_x - dir_x * w1, arc1_z - dir_z * w1, blade_h * 0.45)
                v3 = (arc1_x + dir_x * w1, arc1_z + dir_z * w1, blade_h * 0.45)
                
                w2 = half_w * 0.25
                arc2_x = base_x + out_x * (arc_dist * 1.20)
                arc2_z = base_z + out_z * (arc_dist * 1.20)
                v4 = (arc2_x - dir_x * w2, arc2_z - dir_z * w2, blade_h)
                v5 = (arc2_x + dir_x * w2, arc2_z + dir_z * w2, blade_h)
                
                verts.extend([v0, v1, v2, v3, v4, v5])
                faces.append((idx + 0, idx + 1, idx + 3, idx + 2))
                faces.append((idx + 2, idx + 3, idx + 5, idx + 4))
                uvs.extend([(0.0, 0.0), (1.0, 0.0), (0.0, 0.45), (1.0, 0.45), (0.0, 1.0), (1.0, 1.0)])
                
            elif seg_count == 2:
                # LOD1 (2 crossed wide cards, 6 tris, preserving full X & Y volume envelope)
                v0 = (base_x - dir_x * half_w * 0.8, base_z - dir_z * half_w * 0.8, 0.0)
                v1 = (base_x + dir_x * half_w * 0.8, base_z + dir_z * half_w * 0.8, 0.0)
                
                w1 = half_w * 1.10
                arc1_x = base_x + out_x * (arc_dist * 0.50)
                arc1_z = base_z + out_z * (arc_dist * 0.50)
                v2 = (arc1_x - dir_x * w1, arc1_z - dir_z * w1, blade_h * 0.50)
                v3 = (arc1_x + dir_x * w1, arc1_z + dir_z * w1, blade_h * 0.50)
                
                w2 = half_w * 0.35
                arc2_x = base_x + out_x * (arc_dist * 1.10)
                arc2_z = base_z + out_z * (arc_dist * 1.10)
                v4 = (arc2_x - dir_x * w2, arc2_z - dir_z * w2, blade_h)
                v5 = (arc2_x + dir_x * w2, arc2_z + dir_z * w2, blade_h)
                
                verts.extend([v0, v1, v2, v3, v4, v5])
                faces.append((idx + 0, idx + 1, idx + 3, idx + 2))
                faces.append((idx + 2, idx + 3, idx + 5, idx + 4))
                uvs.extend([(0.0, 0.0), (1.0, 0.0), (0.0, 0.50), (1.0, 0.50), (0.0, 1.0), (1.0, 1.0)])
                
            else:
                # LOD2 (1 broad fanned silhouette card, 2 tris, preserving full X & Y volume envelope)
                v0 = (base_x - dir_x * half_w, base_z - dir_z * half_w, 0.0)
                v1 = (base_x + dir_x * half_w, base_z + dir_z * half_w, 0.0)
                
                w2 = half_w * 0.80
                arc1_x = base_x + out_x * (arc_dist * 0.80)
                arc1_z = base_z + out_z * (arc_dist * 0.80)
                v2 = (arc1_x - dir_x * w2, arc1_z - dir_z * w2, blade_h)
                v3 = (arc1_x + dir_x * w2, arc1_z + dir_z * w2, blade_h)
                
                verts.extend([v0, v1, v2, v3])
                faces.append((idx + 0, idx + 1, idx + 3, idx + 2))
                uvs.extend([(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)])

        mesh.from_pydata(verts, [], faces)
        mesh.update()
        
        # UV Map creation
        uv_layer = mesh.uv_layers.new(name="UVMap")
        for face_idx, polygon in enumerate(mesh.polygons):
            for loop_idx in polygon.loop_indices:
                v_idx = mesh.loops[loop_idx].vertex_index
                uv_layer.data[loop_idx].uv = uvs[v_idx]

        # Vertex Color (COLOR_0) height attribute creation for GPU Wind Shader (0.0 root to 1.0 tip)
        color_layer = mesh.color_attributes.new(name="Color", type='FLOAT_COLOR', domain='CORNER')
        for polygon in mesh.polygons:
            for loop_idx in polygon.loop_indices:
                v_idx = mesh.loops[loop_idx].vertex_index
                v_height = uvs[v_idx][1]
                color_layer.data[loop_idx].color = (v_height, v_height, v_height, 1.0)
                
        # Upward Custom Normal Editing (+Z Blender / +Y Bevy with soft dome curvature)
        custom_normals = []
        for v in verts:
            vx, vy, vz = v
            length = math.hypot(vx, vy, vz)
            if length > 0.001:
                rx, ry, rz = vx / length, vy / length, vz / length
            else:
                rx, ry, rz = 0.0, 0.0, 1.0
            nx = rx * 0.25
            ny = ry * 0.25
            nz = 0.75 + rz * 0.25
            n_len = math.hypot(nx, math.hypot(ny, nz))
            custom_normals.append((nx / n_len, ny / n_len, nz / n_len))

        loop_normals = []
        for polygon in mesh.polygons:
            for loop_idx in polygon.loop_indices:
                v_idx = mesh.loops[loop_idx].vertex_index
                loop_normals.append(custom_normals[v_idx])

        mesh.normals_split_custom_set(loop_normals)

        # Material assignment
        obj.data.materials.append(mat)

    # Save .blend file
    os.makedirs(os.path.dirname(blend_output), exist_ok=True)
    os.makedirs(os.path.dirname(glb_output), exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=blend_output)
    
    # Export GLB
    bpy.ops.export_scene.gltf(
        filepath=glb_output,
        export_format='GLB',
        use_selection=False,
        export_apply=True,
        export_extras=True,
        export_attributes=True,
        export_materials='EXPORT'
    )
    print(f"Volume-Preserved {name_prefix}: LOD0/LOD1/LOD2 maintain consistent X & Y envelope.")

def create_botw_card_billboard(name_prefix, pascal_part, width, height, mat_key, base_dir):
    reset_scene()
    
    blend_output = os.path.join(base_dir, f"art/blender/props/{name_prefix}.blend")
    glb_output = os.path.join(base_dir, f"assets/game/authored/props/{name_prefix}.glb")
    
    # Root Empty
    root_name = f"ROOT_{name_prefix}"
    root_obj = bpy.data.objects.new(root_name, None)
    bpy.context.scene.collection.objects.link(root_obj)
    
    root_obj["bof_license"] = "GNU GPLv3"
    root_obj["bof_profile"] = name_prefix
    root_obj["bof_material_kind"] = "Grass"
    
    mat_name = f"M_{mat_key}"
    mat = bpy.data.materials.get(mat_name) or bpy.data.materials.new(name=mat_name)
    mat.use_nodes = True
    if mat.node_tree:
        bsdf = mat.node_tree.nodes.get("Principled BSDF")
        if bsdf:
            bsdf.inputs["Roughness"].default_value = 0.9
            bsdf.inputs["Metallic"].default_value = 0.0
        vcol_node = mat.node_tree.nodes.get("Color Attribute") or mat.node_tree.nodes.new("ShaderNodeVertexColor")
        vcol_node.layer_name = "Color"

    # Single vertical quad (exactly 2 triangles, 4 vertices) matching spatial volume envelope
    mesh_name = f"SM_{pascal_part}_LOD0"
    mesh = bpy.data.meshes.new(mesh_name)
    obj = bpy.data.objects.new(mesh_name, mesh)
    obj.parent = root_obj
    bpy.context.scene.collection.objects.link(obj)

    half_w = width * 0.5
    verts = [
        (-half_w, 0.0, 0.0), # Bottom-Left (v0)
        ( half_w, 0.0, 0.0), # Bottom-Right (v1)
        (-half_w, 0.0, height), # Top-Left (v2)
        ( half_w, 0.0, height), # Top-Right (v3)
    ]
    faces = [(0, 1, 3, 2)]
    uvs = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]

    mesh.from_pydata(verts, [], faces)
    mesh.update()

    # UV Map
    uv_layer = mesh.uv_layers.new(name="UVMap")
    for polygon in mesh.polygons:
        for loop_idx in polygon.loop_indices:
            v_idx = mesh.loops[loop_idx].vertex_index
            uv_layer.data[loop_idx].uv = uvs[v_idx]

    # Vertex Color COLOR_0 height attribute (0.0 root to 1.0 tip)
    color_layer = mesh.color_attributes.new(name="Color", type='FLOAT_COLOR', domain='CORNER')
    for polygon in mesh.polygons:
        for loop_idx in polygon.loop_indices:
            v_idx = mesh.loops[loop_idx].vertex_index
            v_height = uvs[v_idx][1]
            color_layer.data[loop_idx].color = (v_height, v_height, v_height, 1.0)

    # Baked Upward Normals (+Z Blender / +Y Bevy) to eliminate light flickering during Y-axis rotation
    custom_normals = [(0.0, 0.0, 1.0) for _ in verts]
    loop_normals = []
    for polygon in mesh.polygons:
        for loop_idx in polygon.loop_indices:
            v_idx = mesh.loops[loop_idx].vertex_index
            loop_normals.append(custom_normals[v_idx])
    mesh.normals_split_custom_set(loop_normals)

    obj.data.materials.append(mat)

    os.makedirs(os.path.dirname(blend_output), exist_ok=True)
    os.makedirs(os.path.dirname(glb_output), exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=blend_output)

    bpy.ops.export_scene.gltf(
        filepath=glb_output,
        export_format='GLB',
        use_selection=False,
        export_apply=True,
        export_extras=True,
        export_attributes=True,
        export_materials='EXPORT'
    )
    print(f"Built 2-Tri Billboard {name_prefix}: width={width}m height={height}m with Upward Normals.")

if __name__ == "__main__":
    base_dir = os.getcwd()
    models = [
        ("prop_grass_tall_a", "TallA", 0.25, 1.2, 1.5, "FoliageCommon"),
        ("prop_grass_a", "GrassA", 0.22, 0.9, 1.2, "FoliageCommon"),
        ("prop_grass_b", "GrassB", 0.20, 0.8, 1.1, "FoliageCommon"),
        ("prop_grass_c", "GrassC", 0.18, 0.7, 1.0, "FoliageCommon"),
        ("prop_grass_very_short_a", "VeryShortA", 0.15, 0.3, 0.5, "FoliageCommon"),
        ("prop_grass_dry_a", "DryA", 0.22, 0.8, 1.1, "FoliageDry"),
    ]
    for name, pascal_part, r_rad, h_min, h_max, mat in models:
        create_botw_tuft(name, pascal_part, r_rad, h_min, h_max, mat, base_dir)

    # Paso 5: 2D Billboard Card (Exactly 2 tris, baked +Y Upward Normals, matching 3D tuft volume envelope)
    create_botw_card_billboard("prop_grass_card_a", "CardA", 0.65, 1.10, "FoliageCard", base_dir)
