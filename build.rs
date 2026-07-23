#[path = "src/asset_pipeline/schema.rs"]
mod schema;

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

struct AssetRecord {
    key: String,
    runtime_path: String,
    profile: Option<String>,
    sockets: Vec<SocketRecord>,
    colliders: Vec<ColliderRecord>,
}

struct SocketRecord {
    name: String,
    translation: [f32; 3],
    rotation: [f32; 4],
}

struct ColliderRecord {
    name: String,
    kind: &'static str,
    translation: [f32; 3],
    rotation: [f32; 4],
    size: [f32; 3],
    points: Vec<[f32; 3]>,
    climbable: bool,
    material_kind: Option<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=assets/game/authored");
    println!("cargo:rerun-if-changed=src/asset_pipeline/schema.rs");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let authored_root = manifest_dir.join("assets/game/authored");
    let mut paths = Vec::new();
    collect_glbs(&authored_root, &mut paths)?;
    paths.sort();

    let mut seen_keys = BTreeSet::new();
    let mut seen_profiles = BTreeSet::new();
    let mut assets = Vec::with_capacity(paths.len());
    for path in paths {
        let asset = inspect_asset(&manifest_dir, &path)?;
        if !seen_keys.insert(asset.key.clone()) {
            return Err(format!("duplicate authored asset key {}", asset.key).into());
        }
        if let Some(profile) = &asset.profile
            && !seen_profiles.insert(profile.clone())
        {
            return Err(format!("duplicate authored spatial profile {profile}").into());
        }
        assets.push(asset);
    }

    let generated = emit_manifest(&assets);
    let output = PathBuf::from(env::var("OUT_DIR")?).join("authored_assets.rs");
    fs::write(output, generated)?;
    Ok(())
}

fn collect_glbs(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_glbs(&path, paths)?;
        } else if path.extension().is_some_and(|extension| extension == "glb") {
            paths.push(path);
        }
    }
    Ok(())
}

fn inspect_asset(manifest_dir: &Path, path: &Path) -> Result<AssetRecord, Box<dyn Error>> {
    let key = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| format!("{} has no UTF-8 file stem", path.display()))?
        .to_owned();
    if !schema::valid_asset_key(&key) {
        return Err(format!("{} is not a conventional asset key", path.display()).into());
    }

    let expected_category = key
        .split_once('_')
        .map(|(category, _)| category)
        .ok_or_else(|| format!("{key}: missing category"))?;
    let parent_category = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("{} has no category directory", path.display()))?;
    let expected_directory = category_directory(expected_category);
    if parent_category != expected_directory {
        return Err(format!(
            "{} must live in category directory {expected_directory}",
            path.display()
        )
        .into());
    }

    let (document, buffers, _) =
        gltf::import(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let scenes: Vec<_> = document.scenes().collect();
    if scenes.len() != 1 {
        return Err(format!("{key}: expected one scene, found {}", scenes.len()).into());
    }
    let roots: Vec<_> = scenes[0].nodes().collect();
    if roots.len() != 1 {
        return Err(format!("{key}: expected one scene root, found {}", roots.len()).into());
    }
    let root = roots[0].clone();
    let expected_root = format!("ROOT_{key}");
    if root.name() != Some(expected_root.as_str()) {
        return Err(format!("{key}: root must be named {expected_root}").into());
    }
    let root_extras = extras(root.extras().as_deref(), &format!("{key}: root"))?;
    let license = string_property(&root_extras, "bof_license");
    if license.is_none() {
        return Err(format!("{key}: root is missing bof_license").into());
    }
    let profile = string_property(&root_extras, "bof_profile").map(ToOwned::to_owned);

    validate_materials(&document, &key)?;

    let mut lods: BTreeMap<String, BTreeSet<u8>> = BTreeMap::new();
    let mut sockets = Vec::new();
    let mut colliders = Vec::new();
    let mut has_render = false;
    let mut has_skinned = false;

    for node in document.nodes() {
        let Some(name) = node.name() else {
            return Err(format!("{key}: every node must be named").into());
        };
        if name == expected_root {
            continue;
        }
        if let Some((part, level)) = schema::render_lod(name) {
            let mesh = node
                .mesh()
                .ok_or_else(|| format!("{key}: render node {name} has no mesh"))?;
            if mesh.name() != Some(name) {
                return Err(format!("{key}: mesh data for {name} must share the node name").into());
            }
            has_render = true;
            has_skinned |= name.starts_with("SK_");
            lods.entry(part.to_owned()).or_default().insert(level);
            continue;
        }
        if schema::is_collision_name(name) {
            colliders.push(inspect_collider(&key, &node, &buffers, &root_extras)?);
            continue;
        }
        if schema::valid_socket_name(name) {
            if node.mesh().is_some() {
                return Err(format!("{key}: socket {name} must be an empty node").into());
            }
            let (translation, rotation, scale) = node.transform().decomposed();
            require_unit_scale(&key, name, scale)?;
            sockets.push(SocketRecord {
                name: name.to_owned(),
                translation,
                rotation,
            });
            continue;
        }
        if !has_skinned && node.mesh().is_some() {
            return Err(format!("{key}: mesh node {name} has no recognized prefix").into());
        }
    }

    if !has_render {
        return Err(format!("{key}: no SM_/SK_ render meshes found").into());
    }
    for (part, levels) in lods {
        if !levels.contains(&0) {
            return Err(format!("{key}: {part} is missing LOD0").into());
        }
        let max = levels.iter().next_back().copied().unwrap_or(0);
        for level in 0..=max {
            if !levels.contains(&level) {
                return Err(format!("{key}: {part} skips LOD{level}").into());
            }
        }
    }

    let animations: Vec<_> = document.animations().collect();
    if has_skinned && animations.is_empty() {
        return Err(format!("{key}: skinned asset has no animations").into());
    }
    if !has_skinned && !animations.is_empty() {
        return Err(format!("{key}: static asset unexpectedly contains animations").into());
    }
    for animation in animations {
        let Some(name) = animation.name() else {
            return Err(format!("{key}: every animation must be named").into());
        };
        if !schema::valid_animation_name(name) {
            return Err(format!("{key}: invalid animation name {name}").into());
        }
    }

    if (!sockets.is_empty() || !colliders.is_empty()) && profile.is_none() {
        return Err(format!("{key}: spatial helpers require root bof_profile").into());
    }

    let relative = path.strip_prefix(manifest_dir.join("assets"))?;
    let runtime_path = relative
        .to_str()
        .ok_or_else(|| format!("{} is not UTF-8", relative.display()))?
        .replace('\\', "/");
    Ok(AssetRecord {
        key,
        runtime_path,
        profile,
        sockets,
        colliders,
    })
}

fn category_directory(category: &str) -> &'static str {
    match category {
        "char" => "characters",
        "creature" => "creatures",
        "prop" => "props",
        "structure" => "structures",
        "tree" => "trees",
        "weapon" => "weapons",
        _ => "",
    }
}

fn validate_materials(document: &gltf::Document, key: &str) -> Result<(), Box<dyn Error>> {
    for material in document.materials() {
        let Some(name) = material.name() else {
            return Err(format!("{key}: every material must be named").into());
        };
        let Some(palette_key) = name.strip_prefix("M_") else {
            return Err(format!("{key}: material {name} must start with M_").into());
        };
        if !schema::PALETTE_KEYS.contains(&palette_key) {
            return Err(format!("{key}: unknown palette material {name}").into());
        }
        let pbr = material.pbr_metallic_roughness();
        if pbr.metallic_factor() > 0.001 || pbr.roughness_factor() < 0.8 {
            return Err(format!("{key}: material {name} is not matte/non-metallic").into());
        }
    }
    Ok(())
}

fn inspect_collider(
    key: &str,
    node: &gltf::Node<'_>,
    buffers: &[gltf::buffer::Data],
    root_extras: &Value,
) -> Result<ColliderRecord, Box<dyn Error>> {
    let name = node
        .name()
        .ok_or_else(|| format!("{key}: unnamed collider"))?;
    let mesh = node
        .mesh()
        .ok_or_else(|| format!("{key}: collider {name} has no mesh"))?;
    if mesh.name() != Some(name) {
        return Err(format!("{key}: collider mesh data for {name} must share its name").into());
    }
    let points = mesh_positions(&mesh, buffers)?;
    if points.is_empty() {
        return Err(format!("{key}: collider {name} has no vertices").into());
    }
    let (translation, rotation, scale) = node.transform().decomposed();
    require_finite_transform(key, name, translation, rotation, scale)?;
    let scaled_points: Vec<[f32; 3]> = points
        .into_iter()
        .map(|point| {
            [
                point[0] * scale[0],
                point[1] * scale[1],
                point[2] * scale[2],
            ]
        })
        .collect();
    let (min, max) = bounds(&scaled_points);
    let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let kind = if name.starts_with("UCX_") {
        "ConvexHull"
    } else if name.starts_with("UBX_") {
        "Box"
    } else if name.starts_with("UCP_") {
        "Capsule"
    } else if name.starts_with("USP_") {
        "Sphere"
    } else {
        "Cylinder"
    };
    let node_extras = extras(node.extras().as_deref(), &format!("{key}: {name}"))?;
    let climbable = bool_property(&node_extras, "bof_climbable")
        .or_else(|| bool_property(root_extras, "bof_climbable"))
        .unwrap_or(true);
    let material_kind = string_property(&node_extras, "bof_material_kind")
        .or_else(|| string_property(root_extras, "bof_material_kind"))
        .map(ToOwned::to_owned);

    Ok(ColliderRecord {
        name: name.to_owned(),
        kind,
        translation,
        rotation,
        size,
        points: if kind == "ConvexHull" {
            scaled_points
        } else {
            Vec::new()
        },
        climbable,
        material_kind,
    })
}

fn mesh_positions(
    mesh: &gltf::Mesh<'_>,
    buffers: &[gltf::buffer::Data],
) -> Result<Vec<[f32; 3]>, Box<dyn Error>> {
    let mut points = Vec::new();
    for primitive in mesh.primitives() {
        let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|data| &data.0[..]));
        let positions = reader
            .read_positions()
            .ok_or_else(|| format!("mesh {} has no POSITION attribute", mesh.index()))?;
        points.extend(positions);
    }
    Ok(points)
}

fn bounds(points: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for point in points {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    (min, max)
}

fn extras(
    raw: Option<&serde_json::value::RawValue>,
    context: &str,
) -> Result<Value, Box<dyn Error>> {
    match raw {
        Some(raw) => serde_json::from_str(raw.get())
            .map_err(|error| format!("{context}: malformed extras: {error}").into()),
        None => Ok(Value::Object(Default::default())),
    }
}

fn string_property<'a>(extras: &'a Value, key: &str) -> Option<&'a str> {
    extras.get(key).and_then(Value::as_str)
}

fn bool_property(extras: &Value, key: &str) -> Option<bool> {
    extras.get(key).and_then(Value::as_bool)
}

fn require_unit_scale(key: &str, name: &str, scale: [f32; 3]) -> Result<(), Box<dyn Error>> {
    if scale
        .iter()
        .any(|value| !value.is_finite() || (value - 1.0).abs() > 0.0001)
    {
        return Err(format!("{key}: {name} has unapplied scale {scale:?}").into());
    }
    Ok(())
}

fn require_finite_transform(
    key: &str,
    name: &str,
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
) -> Result<(), Box<dyn Error>> {
    if translation
        .iter()
        .chain(rotation.iter())
        .chain(scale.iter())
        .any(|value| !value.is_finite())
    {
        return Err(format!("{key}: {name} has a non-finite transform").into());
    }
    Ok(())
}

fn emit_manifest(assets: &[AssetRecord]) -> String {
    let mut output = String::from(
        "// @generated by build.rs; authored GLBs are the source of truth.\n\
         pub const AUTHORED_ASSETS: &[GeneratedAsset] = &[\n",
    );
    for asset in assets {
        output.push_str("GeneratedAsset {\n");
        output.push_str(&format!("key: {:?},\n", asset.key));
        output.push_str(&format!("path: {:?},\n", asset.runtime_path));
        match &asset.profile {
            Some(profile) => output.push_str(&format!("profile: Some({profile:?}),\n")),
            None => output.push_str("profile: None,\n"),
        }
        output.push_str("sockets: &[\n");
        for socket in &asset.sockets {
            output.push_str(&format!(
                "GeneratedSocket {{ name: {:?}, translation: {:?}, rotation: {:?} }},\n",
                socket.name, socket.translation, socket.rotation
            ));
        }
        output.push_str("],\ncolliders: &[\n");
        for collider in &asset.colliders {
            output.push_str(&format!(
                "GeneratedCollider {{ name: {:?}, kind: GeneratedColliderKind::{}, \
                 translation: {:?}, rotation: {:?}, size: {:?}, points: &{:?}, \
                 climbable: {}, material_kind: {} }},\n",
                collider.name,
                collider.kind,
                collider.translation,
                collider.rotation,
                collider.size,
                collider.points,
                collider.climbable,
                collider
                    .material_kind
                    .as_ref()
                    .map_or_else(|| "None".to_owned(), |kind| format!("Some({kind:?})")),
            ));
        }
        output.push_str("],\n},\n");
    }
    output.push_str("];\n");
    output
}
