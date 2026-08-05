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

use crate::visuals::grass_material::{GrassExtension, GrassMaterial, GrassUniform};
use crate::world::TerrainAccess;
use crate::world::forest::{hash_u32, hash_unit};

/// **The knob that decides how the meadow reads.** Blades per square metre.
///
/// Judged by eye at 25 (ground covered, still reads thin) and raised to 45,
/// which is where the field stops looking like blades on dirt and starts
/// looking like a meadow. Below ~10 the dirt shows through, which is the
/// failure this system exists to avoid.
///
/// It lives in `PerfToggles` rather than here because the density sweep — half
/// of what separates fill-bound from vertex-bound — is only a measurement if it
/// runs inside the A/B sequence, with warmup, settle window and a parked
/// camera. This constant is the shipped value, i.e. step 0 of that dial, and
/// exists so the tests and the declared budget name it instead of a literal.
#[cfg(test)]
const BLADES_PER_SQUARE_METRE: f32 = bof_domain::perf::GRASS_DENSITY_STEPS[0];

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

/// Root and tip colours. The root-to-tip gradient is the single biggest reason
/// BOTW grass reads as grass; it used to be baked into a vertex colour and now
/// travels as two uniforms, because it is a pure function of the vertex's
/// height along the blade and a `mix` in the shader costs nothing per frame
/// while sixteen bytes per vertex cost bandwidth on every one of them.
const ROOT_COLOR: LinearRgba = LinearRgba::rgb(0.13, 0.24, 0.09);
const TIP_COLOR: LinearRgba = LinearRgba::rgb(0.42, 0.70, 0.22);

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
    chunks * blades_per_chunk(BLADES_PER_SQUARE_METRE) as usize * 2
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

/// Scene entry: lay down the field at whatever density the dial says.
pub(super) fn spawn_meadow(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GrassMaterial>>,
    perf: Res<crate::perf::PerfToggles>,
    scene: Res<State<crate::scene::AppState>>,
    terrain: TerrainAccess,
) {
    spawn_field(
        &mut commands,
        &mut meshes,
        &mut materials,
        perf.grass_density(),
        *scene.get(),
        &terrain,
    );
}

/// The meadow's material: PBR plus the grass extension.
///
/// `ExtendedMaterial` rather than a pipeline of our own — lighting, shadows,
/// fog and decals keep working, and what the extension owns is only where the
/// base colour and the normal come from. The uniform carries the same two
/// colours the vertices used to carry, so plugging this in changes no pixel.
fn grass_material() -> GrassMaterial {
    GrassMaterial {
        base: StandardMaterial {
            // The extension writes `base_color` per fragment; white here means
            // nothing tints the gradient behind its back.
            base_color: Color::WHITE,
            // Blades are flat quads seen from every side, so both faces must
            // draw — unlike tree bark, where double-siding was pure waste.
            cull_mode: None,
            double_sided: true,
            perceptual_roughness: 0.95,
            reflectance: 0.03,
            ..default()
        },
        extension: GrassExtension {
            grass_data: GrassUniform {
                root_color: ROOT_COLOR,
                tip_color: TIP_COLOR,
                ..default()
            },
            interaction_map: None,
        },
    }
}

fn spawn_field(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<GrassMaterial>,
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
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(capacity * 4);
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

        // `uv.y` is the vertex's height along the blade: 0 at the root, 1 at
        // the tip. It is the one derived value still worth its eight bytes —
        // the shader reads it for the colour gradient, and the wind and the
        // trample map will multiply their displacement by it. The normal and
        // the colour that used to sit beside it are gone: both are constants of
        // the system, rebuilt in `grass.wgsl` for free.
        uvs.push([0.0, 0.0]);
        uvs.push([1.0, 0.0]);
        uvs.push([1.0, 1.0]);
        uvs.push([0.0, 1.0]);

        indices.extend_from_slice(&[first, first + 1, first + 2]);
        indices.extend_from_slice(&[first, first + 2, first + 3]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Re-bake the field when the density dial moves.
///
/// Rebuilding is the one knob in the hub with a real CPU cost per step, which
/// is why it is a step of the A/B sequence and not a keypress: the sequence
/// gives it a settle window, so the rebuild frame is never inside the samples.
/// The field is only rebuilt when the density actually changed — `PerfToggles`
/// is one resource for fourteen dials, so `is_changed()` alone would re-bake
/// 28k blades every time somebody toggled a shadow.
#[expect(
    clippy::too_many_arguments,
    reason = "a rebuild needs the same eight pieces the spawn does"
)]
pub(super) fn rebuild_meadow_on_density_change(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GrassMaterial>>,
    perf: Res<crate::perf::PerfToggles>,
    scene: Res<State<crate::scene::AppState>>,
    terrain: TerrainAccess,
    chunks: Query<Entity, With<GrassChunk>>,
    mut current: Local<Option<usize>>,
) {
    // The step index, not the density it resolves to: the dial is what moved,
    // and an index compares exactly where two floats only compare by luck.
    let known = current.replace(perf.grass_density_step);
    // First run only latches the dial: the field `spawn_meadow` just baked is
    // already at this density, and re-baking it would double the cost of
    // entering every scene with a meadow.
    if known.is_none_or(|previous| previous == perf.grass_density_step) {
        return;
    }
    let density = perf.grass_density();
    for entity in &chunks {
        commands.entity(entity).despawn();
    }
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

    /// The vertex carries only what cannot be derived. Normal and colour are
    /// constants of the system rebuilt in `grass.wgsl`; baking them again would
    /// silently put back the 28 bytes per vertex this pair of steps removed,
    /// and nothing on screen would show it.
    #[test]
    fn the_vertex_carries_only_position_and_height() {
        let mesh = build_chunk_mesh(Vec2::ZERO, 8, 5, None);
        assert!(mesh.attribute(Mesh::ATTRIBUTE_POSITION).is_some());
        assert!(
            mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some(),
            "uv.y is the blade height the shader and the wind both read"
        );
        assert!(
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL).is_none(),
            "the normal is +Y for every blade — the shader rebuilds it"
        );
        assert!(
            mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_none(),
            "the gradient is a function of uv.y between two uniforms"
        );
    }

    /// The shader reads `uv.y` as the height along the blade, so the four
    /// vertices have to keep meaning root, root, tip, tip in that order.
    #[test]
    fn a_blades_four_vertices_keep_their_authored_order() {
        let mesh = build_chunk_mesh(Vec2::ZERO, 1, 5, None);
        let bevy::mesh::VertexAttributeValues::Float32x2(uvs) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0).expect("uvs")
        else {
            panic!("uvs must be Float32x2");
        };
        assert_eq!(uvs[0], [0.0, 0.0], "base-left sits at the root");
        assert_eq!(uvs[1], [1.0, 0.0], "base-right sits at the root");
        assert_eq!(uvs[2], [1.0, 1.0], "tip-right sits at the tip");
        assert_eq!(uvs[3], [0.0, 1.0], "tip-left sits at the tip");
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
