from pathlib import Path
import bpy
import math
import os
import sys

TOOLS_ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_ROOT))

from blender_export import export_selected_glb, select_hierarchy

def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)

def setup_units():
    scene = bpy.context.scene
    scene.unit_settings.system = 'METRIC'
    scene.unit_settings.scale_length = 1.0

def create_material(name, color):
    mat = bpy.data.materials.get(name)
    if not mat:
        mat = bpy.data.materials.new(name=name)
        mat.use_nodes = True
        nodes = mat.node_tree.nodes
        bsdf = nodes.get("Principled BSDF")
        if bsdf:
            bsdf.inputs['Base Color'].default_value = color
            bsdf.inputs['Roughness'].default_value = 0.85
            bsdf.inputs['Metallic'].default_value = 0.0
    return mat

def apply_upward_normals(mesh):
    """Transfer custom split normals pointing mostly upward (+Z in Blender)

    Gives the smooth, luminous cel-shading light response of Breath of the Wild
    without dark self-shadow seams between blades.
    """
    custom_normals = []
    for loop in mesh.loops:
        v_idx = loop.vertex_index
        co = mesh.vertices[v_idx].co
        # Blend upward (+Z) with subtle radial displacement from center
        nx = co.x * 0.25
        ny = co.y * 0.25
        nz = max(0.7, 1.0 - math.sqrt(co.x*co.x + co.y*co.y) * 0.15)
        length = math.sqrt(nx*nx + ny*ny + nz*nz)
        custom_normals.append((nx/length, ny/length, nz/length))

    mesh.normals_split_custom_set(custom_normals)

def generate_grass_asset(asset_key, variant_type, material_name, material_color):
    reset_scene()
    setup_units()

    mat = create_material(material_name, material_color)

    root = bpy.data.objects.new(f"ROOT_{asset_key}", None)
    bpy.context.collection.objects.link(root)
    root["bof_license"] = "CC0"
    root["bof_material_kind"] = "vegetation"

    part_name = "".join(x.capitalize() for x in asset_key.replace("prop_grass_", "").replace("prop_flower_", "").split("_"))

    # BOTW-style ultra-light wide fan configuration (8-14 tris at LOD0)
    if variant_type == "short_a":
        blade_count = 7
        height_range = (0.75, 0.95)
        base_width = 0.12
        curl_factor = 0.35
    elif variant_type == "short_b":
        blade_count = 6
        height_range = (0.70, 0.90)
        base_width = 0.14
        curl_factor = 0.45
    elif variant_type == "short_c":
        blade_count = 6
        height_range = (0.80, 1.05)
        base_width = 0.11
        curl_factor = 0.25
    elif variant_type == "very_short":
        blade_count = 8
        height_range = (0.35, 0.50)
        base_width = 0.12
        curl_factor = 0.50
    elif variant_type == "tall":
        blade_count = 6
        height_range = (1.10, 1.45)
        base_width = 0.14
        curl_factor = 0.30
    elif variant_type == "dry":
        blade_count = 6
        height_range = (0.80, 1.10)
        base_width = 0.11
        curl_factor = 0.40
    else: # flower
        blade_count = 6
        height_range = (0.75, 0.95)
        base_width = 0.11
        curl_factor = 0.30

    for lod_level in range(3):
        if lod_level == 0:
            subdivs = 2
            current_blades = blade_count
        elif lod_level == 1:
            subdivs = 1
            current_blades = min(4, blade_count)
        else:
            subdivs = 1
            current_blades = 3

        verts = []
        faces = []
        uvs = []

        vert_offset = 0

        for b in range(current_blades):
            # Fan out blades in 360-degree star pattern around center
            angle = (b / current_blades) * math.tau + (b * 0.45)
            radius = 0.04 + (b % 3) * 0.035
            h = height_range[0] + (b % 3) * ((height_range[1] - height_range[0]) / 2.0)

            bx = math.cos(angle) * radius
            by = math.sin(angle) * radius

            for step in range(subdivs + 1):
                t = step / subdivs
                z = t * h
                
                # Smooth parabolic curve away from center
                offset = (t * t) * curl_factor * h
                offset_x = bx + math.cos(angle) * offset
                offset_y = by + math.sin(angle) * offset

                # Taper broad width towards tip (BOTW wide blade profile)
                w = base_width * (1.0 - (t * 0.8))
                px = -math.sin(angle) * (w * 0.5)
                py = math.cos(angle) * (w * 0.5)

                verts.append((offset_x - px, offset_y - py, z))
                verts.append((offset_x + px, offset_y + py, z))

                # UV vertical mapping V: 0 (root at ground) -> 1 (blade tip)
                uvs.append((0.0, t))
                uvs.append((1.0, t))

            # Build quad faces
            for step in range(subdivs):
                v0 = vert_offset + step * 2
                v1 = vert_offset + step * 2 + 1
                v2 = vert_offset + (step + 1) * 2 + 1
                v3 = vert_offset + (step + 1) * 2

                faces.append((v0, v1, v2, v3))

            vert_offset += (subdivs + 1) * 2

        # Create mesh & object
        mesh_name = f"SM_{part_name}_LOD{lod_level}"
        mesh = bpy.data.meshes.new(mesh_name)
        mesh.from_pydata(verts, [], faces)
        mesh.update()

        # Apply UV Map
        uv_layer = mesh.uv_layers.new(name="UVMap")
        for polygon in mesh.polygons:
            for loop_index in polygon.loop_indices:
                v_idx = mesh.loops[loop_index].vertex_index
                uv_layer.data[loop_index].uv = uvs[v_idx]

        # Apply Upward Split Normals for Cel-Shading
        apply_upward_normals(mesh)

        # Apply Vertex Color Gradient (Root Dark -> Tip Sunny Green)
        vcol_layer = mesh.color_attributes.new(name="Color", type='FLOAT_COLOR', domain='POINT')
        for v_idx, v in enumerate(mesh.vertices):
            t = max(0.0, min(1.0, v.co.z / height_range[1]))
            if t < 0.25:
                factor = t / 0.25
                r = 0.14 + factor * (0.27 - 0.14)
                g = 0.28 + factor * (0.50 - 0.28)
                b = 0.11 + factor * (0.22 - 0.11)
            else:
                factor = (t - 0.25) / 0.75
                r = 0.27 + factor * (0.38 - 0.27)
                g = 0.50 + factor * (0.64 - 0.50)
                b = 0.22 + factor * (0.25 - 0.22)
            vcol_layer.data[v_idx].color = (r, g, b, 1.0)

        obj = bpy.data.objects.new(mesh_name, mesh)
        bpy.context.collection.objects.link(obj)
        obj.parent = root

        # Attach Material
        if len(obj.data.materials) == 0:
            obj.data.materials.append(mat)

    # Save .blend file
    blend_dir = "art/blender/props"
    os.makedirs(blend_dir, exist_ok=True)
    blend_path = os.path.join(blend_dir, f"{asset_key}.blend")
    bpy.ops.wm.save_as_mainfile(filepath=blend_path)

    # Export .glb file using repository blender_export standard
    glb_dir = Path("assets/game/authored/props")
    glb_dir.mkdir(parents=True, exist_ok=True)
    glb_path = glb_dir / f"{asset_key}.glb"

    select_hierarchy(root)
    export_selected_glb(glb_path, export_animations=False)
    print(f"Successfully generated {blend_path} and {glb_path}")

def generate_card_asset(asset_key, material_name, material_color):
    reset_scene()
    setup_units()

    mat = create_material(material_name, material_color)
    
    # Load T_GrassCard_Albedo.png texture if available
    tex_path = os.path.abspath("assets/textures/props/T_GrassCard_Albedo.png")
    if os.path.exists(tex_path):
        mat.use_nodes = True
        nodes = mat.node_tree.nodes
        bsdf = nodes.get("Principled BSDF")
        tex_node = nodes.new(type='ShaderNodeTexImage')
        tex_node.image = bpy.data.images.load(tex_path)
        mat.node_tree.links.new(tex_node.outputs['Color'], bsdf.inputs['Base Color'])
        mat.node_tree.links.new(tex_node.outputs['Alpha'], bsdf.inputs['Alpha'])
        if hasattr(mat, "blend_method"):
            mat.blend_method = 'CLIP'

    root = bpy.data.objects.new(f"ROOT_{asset_key}", None)
    bpy.context.collection.objects.link(root)
    root["bof_license"] = "CC0"
    root["bof_material_kind"] = "vegetation"

    part_name = "CardA"

    for lod_level in range(3):
        mesh_name = f"SM_{part_name}_LOD{lod_level}"
        mesh = bpy.data.meshes.new(mesh_name)

        # Single vertical 2D plane (2 tris exact - BOTW Single Quad Billboard)
        w = 1.20 if lod_level < 2 else 0.90
        h = 0.90 if lod_level < 2 else 0.75

        verts = [
            [-w*0.5, 0.0, 0.0], [w*0.5, 0.0, 0.0], [w*0.5, 0.0, h], [-w*0.5, 0.0, h],
        ]
        faces = [(0, 1, 2, 3)]
        uvs = [
            (0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0),
        ]

        mesh.from_pydata(verts, [], faces)
        mesh.update()

        uv_layer = mesh.uv_layers.new(name="UVMap")
        for polygon in mesh.polygons:
            for loop_index in polygon.loop_indices:
                v_idx = mesh.loops[loop_index].vertex_index
                uv_layer.data[loop_index].uv = uvs[v_idx]

        apply_upward_normals(mesh)

        obj = bpy.data.objects.new(mesh_name, mesh)
        bpy.context.collection.objects.link(obj)
        obj.parent = root
        if len(obj.data.materials) == 0:
            obj.data.materials.append(mat)

    blend_dir = "art/blender/props"
    os.makedirs(blend_dir, exist_ok=True)
    blend_path = os.path.join(blend_dir, f"{asset_key}.blend")
    bpy.ops.wm.save_as_mainfile(filepath=blend_path)

    glb_dir = Path("assets/game/authored/props")
    glb_dir.mkdir(parents=True, exist_ok=True)
    glb_path = glb_dir / f"{asset_key}.glb"

    select_hierarchy(root)
    export_selected_glb(glb_path, export_animations=False)
    print(f"Successfully generated {blend_path} and {glb_path}")

def main():
    assets_to_build = [
        ("prop_grass_a", "short_a", "M_FoliageCommon", (0.27, 0.50, 0.22, 1.0)),
        ("prop_grass_b", "short_b", "M_FoliageCommon", (0.27, 0.50, 0.22, 1.0)),
        ("prop_grass_c", "short_c", "M_FoliageCommon", (0.27, 0.50, 0.22, 1.0)),
        ("prop_grass_very_short_a", "very_short", "M_FoliageCommon", (0.27, 0.50, 0.22, 1.0)),
        ("prop_grass_tall_a", "tall", "M_FoliageCommon", (0.27, 0.50, 0.22, 1.0)),
        ("prop_grass_dry_a", "dry", "M_FoliageDry", (0.58, 0.52, 0.28, 1.0)),
        ("prop_flower_wild_a", "flower", "M_FoliageWildflowers", (0.35, 0.55, 0.25, 1.0)),
    ]

    for key, vtype, mname, mcolor in assets_to_build:
        generate_grass_asset(key, vtype, mname, mcolor)

    generate_card_asset("prop_grass_card_a", "M_FoliageCard", (0.27, 0.50, 0.22, 1.0))

if __name__ == "__main__":
    main()
