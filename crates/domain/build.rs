#[path = "src/asset_pipeline/schema.rs"]
mod schema;
#[path = "../../src/asset_pipeline/terrain_textures.rs"]
mod terrain_textures;

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
    /// Triangles per LOD level, index = level. Counted here because the GLB is
    /// already open and parsed: a triangle count that only exists at runtime can
    /// warn, but it cannot fail a build — and a budget that cannot fail is a
    /// comment. Index 0 is the one that matters; it is what a close-up costs.
    triangles: Vec<u32>,
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
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let project_root = manifest_dir.join("../..");
    println!(
        "cargo:rerun-if-changed={}",
        project_root.join("assets/game/authored").display()
    );
    println!("cargo:rerun-if-changed=src/asset_pipeline/schema.rs");
    println!(
        "cargo:rerun-if-changed={}",
        project_root
            .join("src/asset_pipeline/terrain_textures.rs")
            .display()
    );

    validate_terrain_textures(&project_root)?;
    let authored_root = project_root.join("assets/game/authored");
    let mut paths = Vec::new();
    collect_glbs(&authored_root, &mut paths)?;
    paths.sort();

    let mut seen_keys = BTreeSet::new();
    let mut seen_profiles = BTreeSet::new();
    let mut assets = Vec::with_capacity(paths.len());
    for path in paths {
        let asset = inspect_asset(&project_root, &path)?;
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

fn validate_terrain_textures(manifest_dir: &Path) -> Result<(), Box<dyn Error>> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

    for spec in terrain_textures::TERRAIN_TEXTURES {
        let path = manifest_dir.join("assets").join(spec.path);
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = fs::read(&path).map_err(|error| {
            format!(
                "{}: terrain texture cannot be read: {error}",
                path.display()
            )
        })?;
        if bytes.len() < 33 || &bytes[..8] != PNG_SIGNATURE || &bytes[12..16] != b"IHDR" {
            return Err(format!("{}: expected a PNG with an IHDR chunk", path.display()).into());
        }

        let width = u32::from_be_bytes(bytes[16..20].try_into()?);
        let height = u32::from_be_bytes(bytes[20..24].try_into()?);
        let bit_depth = bytes[24];
        let color_type = bytes[25];
        if width != terrain_textures::TERRAIN_TEXTURE_EDGE
            || height != terrain_textures::TERRAIN_TEXTURE_EDGE
            || bit_depth != 8
            || color_type != 2
        {
            return Err(format!(
                "{}: terrain array layers must be {}x{} RGB8 PNG without alpha; found \
                 {width}x{height}, bit depth {bit_depth}, PNG color type {color_type}",
                path.display(),
                terrain_textures::TERRAIN_TEXTURE_EDGE,
                terrain_textures::TERRAIN_TEXTURE_EDGE,
            )
            .into());
        }
    }
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
    let mut triangles_by_lod: BTreeMap<u8, u32> = BTreeMap::new();
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
            // Every part of a level adds to that level's cost: an asset split
            // into trunk and canopy still draws both.
            *triangles_by_lod.entry(level).or_default() += count_triangles(&key, &mesh)?;
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
    let mut animation_names: Vec<&str> = Vec::new();
    for animation in animations {
        let Some(name) = animation.name() else {
            return Err(format!("{key}: every animation must be named").into());
        };
        if !schema::valid_animation_name(name) {
            return Err(format!("{key}: invalid animation name {name}").into());
        }
        animation_names.push(name);
    }

    // Compile-time guardrail: a character that declares itself the player rig
    // must ship every required locomotion clip, named exactly. Missing one
    // fails the build listing precisely what to author — no silent runtime gap.
    if string_property(&root_extras, "bof_animset") == Some("player") {
        let missing = schema::missing_required_player_clips(&animation_names);
        if !missing.is_empty() {
            return Err(format!(
                "{key}: bof_animset=player is missing required clips: {}",
                missing.join(", ")
            )
            .into());
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
    // Dense by level: `lods` already proved the levels run 0..=max with no
    // gaps, so indexing by level is safe and the manifest stays a plain slice.
    let triangles: Vec<u32> = (0..=triangles_by_lod.keys().copied().max().unwrap_or(0))
        .map(|level| triangles_by_lod.get(&level).copied().unwrap_or(0))
        .collect();
    // The polygon budget, enforced where it is cheapest to fix: at export, on
    // the asset that broke it, before it is ever spawned in a scene.
    let budget = schema::lod0_triangle_budget(expected_category);
    let lod0 = triangles.first().copied().unwrap_or(0);
    if lod0 > budget {
        return Err(format!(
            "{key}: LOD0 has {lod0} triangles, over the {expected_category} budget of {budget}. \
             Decimate it, or raise the budget in schema.rs on purpose."
        )
        .into());
    }
    // A farther LOD may never cost *more* than a nearer one — that is a swap
    // that makes the distance more expensive, always a mistake. Equality is
    // allowed: a grass card is two triangles at every level because there is
    // nothing left to remove.
    for level in 1..triangles.len() {
        if triangles[level] > triangles[level - 1] {
            return Err(format!(
                "{key}: LOD{level} has {} triangles, more than LOD{}'s {} — a LOD swap must \
                 never cost more further away",
                triangles[level],
                level - 1,
                triangles[level - 1]
            )
            .into());
        }
    }
    Ok(AssetRecord {
        key,
        runtime_path,
        profile,
        triangles,
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
    // Exhaustive on purpose. This used to fall through to "Cylinder" for
    // anything unrecognised, which quietly turned a typo — `UBOX_Body`,
    // `UCYL_Trunk` — into a cylinder collider of the wrong shape, with no error
    // anywhere. The prefix *is* the contract (`ASSET_PIPELINE.md`): a helper
    // whose name Bevy cannot read is a broken asset, not a cylinder.
    let kind = match name.split('_').next().unwrap_or_default() {
        "UCX" => "ConvexHull",
        "UBX" => "Box",
        "UCP" => "Capsule",
        "USP" => "Sphere",
        "UCY" => "Cylinder",
        _ => {
            return Err(format!(
                "{key}: collider {name} has no known shape prefix \
                 (expected UCX_/UBX_/UCP_/USP_/UCY_)"
            )
            .into());
        }
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

/// Triangles in a mesh, summed over its primitives.
///
/// Indexed primitives cost `indices / 3`; a non-indexed one lists every corner,
/// so it is `positions / 3` — the same rule the runtime watchdog applies, kept
/// identical on purpose so the compile-time budget and the in-game counter
/// cannot disagree about what a triangle is.
fn count_triangles(key: &str, mesh: &gltf::Mesh<'_>) -> Result<u32, Box<dyn Error>> {
    let mut total = 0u32;
    for primitive in mesh.primitives() {
        if primitive.mode() != gltf::mesh::Mode::Triangles {
            return Err(format!(
                "{key}: {} is {:?}, but only triangle meshes are exported",
                mesh.name().unwrap_or("<unnamed>"),
                primitive.mode()
            )
            .into());
        }
        let corners = match primitive.indices() {
            Some(indices) => indices.count(),
            None => primitive
                .get(&gltf::Semantic::Positions)
                .map(|positions| positions.count())
                .ok_or_else(|| format!("{key}: a primitive has no positions"))?,
        };
        let triangles = u32::try_from(corners / 3)
            .map_err(|_| format!("{key}: triangle count does not fit in the manifest"))?;
        total = total
            .checked_add(triangles)
            .ok_or_else(|| format!("{key}: total triangle count overflowed the manifest"))?;
    }
    Ok(total)
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
        output.push_str(&format!("triangles: &{:?},\n", asset.triangles));
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
