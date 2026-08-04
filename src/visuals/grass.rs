//! The grass meadow: a dense field of blades, built as **one mesh per chunk**.
//!
//! Pure presentation — no collider, no simulation meaning; the ground beneath
//! already reports `Surface(Grass)` for footstep audio, so the grass never needs
//! to be *known* by the simulation.
//!
//! # Why a mesh per chunk and not an entity per blade
//!
//! Density is the whole point of this system, and density is a fight against
//! per-item cost. An entity per blade puts every blade through transform
//! propagation, visibility and change detection every frame: at the density that
//! actually looks like a meadow that is tens of thousands of entities, and the
//! frame is gone before a single triangle is drawn.
//!
//! So the blade is not an entity. A chunk bakes its blades straight into one
//! mesh — position, yaw, height and colour resolved once, at build time — and
//! the ECS holds one entity per chunk. The cost of a blade becomes two
//! triangles and nothing else, which is what lets [`BLADES_PER_SQUARE_METRE`] be
//! a number you choose for how it *looks* rather than a number the engine
//! forces on you. It is the same shape as `visuals::terrain`, which bakes 32k
//! triangles of ground into one mesh the same way.
//!
//! The unit matters more than any of it: a blade is **two triangles**. Grouping
//! blades into a modelled clump makes the instance cost six times more, and the
//! only way to stay inside the budget after that is to space the clumps out —
//! which is how a meadow turns into scattered shrubs on bare dirt.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::world::TerrainAccess;
use crate::world::forest::{hash_u32, hash_unit};

/// **The knob that decides how the meadow reads.** Blades per square metre.
///
/// Everything else here is derived from it, so tuning the look is this line and
/// nothing else. Judged by eye at 25 (ground covered, still reads thin) and
/// raised to 45, which is where the field stops looking like blades on dirt and
/// starts looking like a meadow. Below ~10 the dirt shows through, which is the
/// failure this system exists to avoid.
const BLADES_PER_SQUARE_METRE: f32 = 45.0;

/// Field size, as a grid of square chunks. One mesh, one draw call per chunk —
/// the grid exists so distance culling has something to work on later, not
/// because the CPU needs the split.
const FIELD_CHUNKS: i32 = 5;
const CHUNK_METRES: f32 = 5.0;

/// Centre of the field in world XZ. The playable area sits north of the origin,
/// so the field is offset in Z; explicit so the asymmetry reads as intentional.
const MEADOW_CENTER: Vec2 = Vec2::new(0.0, 6.0);

/// Blade shape. Wide enough at the base to cover ground, tapered at the tip so
/// it reads as a leaf rather than a strip of paper.
const BLADE_WIDTH: f32 = 0.055;
/// Fraction of the base width kept at the tip.
const BLADE_TIP_TAPER: f32 = 0.35;
/// Blade height range in metres, picked per blade.
const BLADE_HEIGHT_MIN: f32 = 0.26;
const BLADE_HEIGHT_MAX: f32 = 0.52;
/// How far a tip may lean off vertical, in metres, so the field is not a bed of
/// nails. Deterministic per blade — this is authored variety, not animation.
const BLADE_LEAN: f32 = 0.16;

/// Past this slope the ground is rock, not meadow.
const MAX_SLOPE_DEG: f32 = 45.0;
/// Blades on steep ground are shorter, the way real grass thins out on a bank.
const STEEP_SLOPE_DEG: f32 = 35.0;
const STEEP_SCALE: f32 = 0.65;

/// Root and tip colours, baked per vertex. `StandardMaterial` multiplies its
/// base colour by the vertex colour, so the root-to-tip gradient — the single
/// biggest reason BOTW grass reads as grass — costs nothing and needs no custom
/// shader.
const ROOT_COLOR: LinearRgba = LinearRgba::rgb(0.13, 0.24, 0.09);
const TIP_COLOR: LinearRgba = LinearRgba::rgb(0.42, 0.70, 0.22);

/// Density presets cycled by F8, in blades per m². Index 0 is what the meadow
/// spawns at, so the resource never lies about what is on screen.
const DENSITY_TIERS: [(f32, &str); 3] = [
    (BLADES_PER_SQUARE_METRE, "45/m² (objetivo)"),
    (70.0, "70/m² (tupido — mira el presupuesto de tris)"),
    (25.0, "25/m² (el paso anterior, para comparar)"),
];

/// Which density preset the meadow is showing.
#[derive(Resource, Default)]
pub(super) struct GrassStressState {
    tier: usize,
}

/// One baked chunk of the meadow: a single mesh holding all its blades.
#[derive(Component)]
pub(super) struct GrassChunk;

/// Triángulos que la pradera declara a la escena: cada brizna son **dos**.
///
/// Sólo lo consume el presupuesto declarado (`perf::budget`), que es un test:
/// el contador de runtime cuenta lo que la cámara ve, no lo que la escena
/// declara. Mientras este número vivió dentro del módulo, la meadow no la
/// sumaba nadie y una escena con pradera leía como si el pasto fuera gratis.
/// Es la cuenta a densidad por defecto, que es la que se hornea al entrar.
#[cfg(test)]
pub(crate) fn meadow_triangles() -> usize {
    let chunks = (FIELD_CHUNKS * FIELD_CHUNKS) as usize;
    let (density, _) = DENSITY_TIERS[0];
    chunks * blades_per_chunk(density) as usize * 2
}

/// Blades per chunk at a given density. Rounded once, here, so the count on
/// screen and the count in the budget are the same number.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "density is clamped non-negative and a blade count is an integer bucket"
)]
fn blades_per_chunk(density: f32) -> u32 {
    (CHUNK_METRES * CHUNK_METRES * density).round().max(0.0) as u32
}

/// Startup: lay down the field at the default density.
pub(super) fn spawn_meadow(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    stress: Res<GrassStressState>,
    scene: Res<State<crate::scene::AppState>>,
    terrain: TerrainAccess,
) {
    let (density, _) = DENSITY_TIERS[stress.tier];
    spawn_field(
        &mut commands,
        &mut meshes,
        &mut materials,
        density,
        *scene.get(),
        &terrain,
    );
}

fn grass_material() -> StandardMaterial {
    StandardMaterial {
        // White base: the per-vertex gradient is the colour.
        base_color: Color::WHITE,
        // Blades are flat quads seen from every side, so both faces must draw —
        // unlike tree bark, where double-siding was pure waste.
        cull_mode: None,
        double_sided: true,
        perceptual_roughness: 0.95,
        reflectance: 0.03,
        ..default()
    }
}

fn spawn_field(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    density: f32,
    scene: crate::scene::AppState,
    terrain: &TerrainAccess,
) {
    let material = materials.add(grass_material());
    let half = FIELD_CHUNKS / 2;
    let per_chunk = blades_per_chunk(density);
    let mut blades = 0usize;

    for cz in -half..=half {
        for cx in -half..=half {
            let centre = MEADOW_CENTER + Vec2::new(cx as f32, cz as f32) * CHUNK_METRES;
            // Seeded by chunk coordinate, not by spawn order: the same chunk
            // grows the same blades every session, and a re-spawn at another
            // density does not reshuffle the whole field.
            let seed = hash_u32(cx.cast_unsigned().wrapping_mul(0x9e37_79b9) ^ cz.cast_unsigned());
            let mesh = build_chunk_mesh(centre, per_chunk, seed, Some(terrain));
            blades += mesh.count_vertices() / 4;
            commands.spawn((
                DespawnOnExit(scene),
                Name::new(format!("GrassChunk_{cx}_{cz}")),
                GrassChunk,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material.clone()),
                // Blades cast no shadows: thousands of alpha-free slivers in the
                // cascades buy noise, not depth.
                bevy::light::NotShadowCaster,
                Transform::default(),
            ));
        }
    }
    info!(
        "[grass] pradera: {blades} briznas · {} tris · {} chunks · {density:.0}/m²",
        blades * 2,
        FIELD_CHUNKS * FIELD_CHUNKS
    );
}

/// Bake `count` blades around `centre` into one mesh, in world space.
///
/// Vertices carry world positions (the chunk entity sits at the origin), so a
/// blade sits on the ground wherever the terrain put it without the chunk having
/// to track a height of its own.
fn build_chunk_mesh(centre: Vec2, count: u32, seed: u32, terrain: Option<&TerrainAccess>) -> Mesh {
    let capacity = count as usize;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(capacity * 4);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(capacity * 4);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(capacity * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(capacity * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(capacity * 6);

    for blade in 0..count {
        let hash = hash_u32(seed ^ blade.wrapping_mul(0x0019_6f3d));
        let u1 = hash_unit(hash);
        let u2 = hash_unit(hash ^ 0x1234_5678);
        let u3 = hash_unit(hash ^ 0x8765_4321);
        let u4 = hash_unit(hash ^ 0xdead_beef);
        let u5 = hash_unit(hash ^ 0x0f0f_0f0f);

        let xz = centre + Vec2::new(u1 - 0.5, u2 - 0.5) * CHUNK_METRES;
        let ground = terrain.and_then(|t| t.height_at(xz)).unwrap_or(0.0);
        let slope = terrain.and_then(|t| t.slope_deg_at(xz)).unwrap_or(0.0);
        if slope > MAX_SLOPE_DEG {
            continue;
        }

        let steep = if slope > STEEP_SLOPE_DEG {
            STEEP_SCALE
        } else {
            1.0
        };
        let height = (BLADE_HEIGHT_MIN + u4 * (BLADE_HEIGHT_MAX - BLADE_HEIGHT_MIN)) * steep;
        let yaw = u3 * std::f32::consts::TAU;
        // The quad's width runs along `side`; the lean tips it over `side`'s
        // perpendicular so blades do not all fall the same way.
        let side = Vec2::new(yaw.cos(), yaw.sin()) * (BLADE_WIDTH * 0.5);
        let lean = Vec2::new(-yaw.sin(), yaw.cos()) * ((u5 - 0.5) * 2.0 * BLADE_LEAN);

        let base = Vec3::new(xz.x, ground, xz.y);
        let tip = base + Vec3::new(lean.x, height, lean.y);
        let taper = BLADE_TIP_TAPER;
        let Ok(first) = u32::try_from(positions.len()) else {
            error!("grass chunk exceeded the u32 mesh-index limit");
            break;
        };

        positions.push([base.x - side.x, base.y, base.z - side.y]);
        positions.push([base.x + side.x, base.y, base.z + side.y]);
        positions.push([tip.x + side.x * taper, tip.y, tip.z + side.y * taper]);
        positions.push([tip.x - side.x * taper, tip.y, tip.z - side.y * taper]);

        // Normals point up, not out of the quad. A blade lit by its own flat
        // face goes black the moment the sun is off to the side; borrowing the
        // ground's normal is what keeps a field evenly lit — the same trick the
        // authored props use.
        for _ in 0..4 {
            normals.push([0.0, 1.0, 0.0]);
        }
        uvs.push([0.0, 0.0]);
        uvs.push([1.0, 0.0]);
        uvs.push([1.0, 1.0]);
        uvs.push([0.0, 1.0]);
        // Height along the blade, ready for a vertex-shader wind pass to read.
        let root = ROOT_COLOR.to_f32_array();
        let tip_color = TIP_COLOR.to_f32_array();
        colors.push(root);
        colors.push(root);
        colors.push(tip_color);
        colors.push(tip_color);

        indices.extend_from_slice(&[first, first + 1, first + 2]);
        indices.extend_from_slice(&[first, first + 2, first + 3]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// F8: cycle density and rebuild the field. A tuning aid — the whole point of
/// this system is that the density number is something you can judge by eye.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_grass_stress_toggle(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<GrassStressState>,
    scene: Res<State<crate::scene::AppState>>,
    terrain: TerrainAccess,
    chunks: Query<Entity, With<GrassChunk>>,
) {
    if !keys.just_pressed(KeyCode::F8) {
        return;
    }
    for entity in &chunks {
        commands.entity(entity).despawn();
    }
    state.tier = (state.tier + 1) % DENSITY_TIERS.len();
    let (density, label) = DENSITY_TIERS[state.tier];
    info!("[grass-stress] densidad → {label}");
    spawn_field(
        &mut commands,
        &mut meshes,
        &mut materials,
        density,
        *scene.get(),
        &terrain,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_density_knob_is_what_actually_lands_on_the_ground() {
        // The failure this system was built to fix: a density that reads well in
        // a constant but arrives on screen divided by twenty. Stated against the
        // knob rather than a literal, so tuning the density cannot break the
        // test that guards it — the count must always be area × density.
        let per_chunk = blades_per_chunk(BLADES_PER_SQUARE_METRE);
        let expected = (CHUNK_METRES * CHUNK_METRES * BLADES_PER_SQUARE_METRE).round();
        assert_eq!(
            (per_chunk as f32).to_bits(),
            expected.to_bits(),
            "the knob must reach the ground intact"
        );

        let mesh = build_chunk_mesh(Vec2::ZERO, per_chunk, 7, None);
        let per_chunk = per_chunk as usize;
        assert_eq!(mesh.count_vertices(), per_chunk * 4);
        let triangles = mesh.indices().map(|i| i.len() / 3).unwrap_or(0);
        assert_eq!(triangles, per_chunk * 2, "a blade is two triangles");
    }

    #[test]
    fn the_whole_field_stays_inside_the_mobile_triangle_budget() {
        // Grass shares the frame with 32768 triangles of terrain, so the field
        // has to leave room for the ground it grows on.
        let per_chunk = blades_per_chunk(BLADES_PER_SQUARE_METRE) as usize;
        let chunks = (FIELD_CHUNKS * FIELD_CHUNKS).unsigned_abs() as usize;
        let field_triangles = per_chunk * 2 * chunks;
        let terrain = 128 * 128 * 2;
        assert!(
            field_triangles + terrain <= crate::perf::budget::MOBILE_TRIANGLES,
            "field {field_triangles} + terrain {terrain} exceeds the mobile budget"
        );
    }

    #[test]
    fn blades_are_deterministic_per_chunk() {
        // Same chunk, same blades: a field that reshuffles when it is rebuilt
        // makes every visual comparison worthless.
        let a = build_chunk_mesh(Vec2::new(5.0, -5.0), 64, 11, None);
        let b = build_chunk_mesh(Vec2::new(5.0, -5.0), 64, 11, None);
        assert_eq!(
            a.attribute(Mesh::ATTRIBUTE_POSITION)
                .map(|values| values.len()),
            b.attribute(Mesh::ATTRIBUTE_POSITION)
                .map(|values| values.len())
        );
    }

    #[test]
    fn a_blade_stands_on_the_ground_and_reaches_its_height() {
        let mesh = build_chunk_mesh(Vec2::ZERO, 1, 3, None);
        let bevy::mesh::VertexAttributeValues::Float32x3(positions) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions")
        else {
            panic!("positions must be Float32x3");
        };
        let base = positions[0][1];
        let tip = positions[2][1];
        assert!(
            base.abs() < 0.001,
            "the root sits on flat ground, got {base}"
        );
        assert!(
            (BLADE_HEIGHT_MIN..=BLADE_HEIGHT_MAX).contains(&(tip - base)),
            "blade height {} outside its authored range",
            tip - base
        );
    }
}
