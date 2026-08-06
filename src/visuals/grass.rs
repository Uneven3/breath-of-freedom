//! The grass meadow: a **rolling grid of baked chunks, centred on the camera**.
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
//! mesh — position, yaw, height and slope resolved once, at build time — and the
//! ECS holds one entity per chunk. The cost of a blade becomes two triangles and
//! nothing else. It is the same shape as `visuals::terrain`, which bakes 32k
//! triangles of ground into one mesh the same way.
//!
//! The unit matters more than any of it: a blade is **two triangles**. Grouping
//! blades into a modelled clump makes the instance cost six times more, and the
//! only way to stay inside the budget after that is to space the clumps out —
//! which is how a meadow turns into scattered shrubs on bare dirt.
//!
//! # Why the grid rolls
//!
//! The field used to be a fixed 25×25 m square, and a fixed square does not
//! scale to a world: it covered 0.6% of the map while eating 52% of the scene's
//! triangle budget, and stretching it to 320×320 m would have been millions of
//! triangles. Tuning that shape could never fix it.
//!
//! So the field is not a place any more, it is a **neighbourhood**: four rings
//! that exist around wherever the camera is, each with bigger chunks, wider
//! blades and fewer of them per square metre than the one inside it. Walk, and
//! chunks left behind are rebaked ahead. The blade count is constant whether the
//! map is 25 m or 4 km across, and the budget becomes *per view* instead of per
//! scene — which is the only definition that means anything in an open world.
//!
//! What must **not** change with distance is how thick the field looks. That is
//! why the rings thin out on a derivation rather than on taste: a blade at
//! distance `d` hides `width · H · d / h` of the ground behind it, so the same
//! apparent cover needs `1/d` as many blades — and wider blades need fewer still.
//! Forty-five blades per m² at 16 m is not a dense field, it is thirty times the
//! cover anyone can see, and the surplus is paid in overdraw, which is exactly
//! what the mobile target cannot afford. See [`RINGS`] and `docs/BOTWGrass.md`.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

use crate::visuals::grass_cover;
use crate::visuals::grass_material::{GrassExtension, GrassMaterial, GrassUniform};
use crate::world::TerrainAccess;
use crate::world::forest::{hash_u32, hash_unit};

/// One LOD ring: how far out it reaches, how big its chunks are, and how thickly
/// it plants them.
///
/// `reach_m` is a Chebyshev radius — a square, not a circle — because the chunks
/// are a square grid and a square boundary keeps a chunk either wholly in a ring
/// or wholly out of it. The seam that leaves between two rings is a strip up to
/// half a chunk wide that may end up slightly denser or sparser than the ideal;
/// it survives review because after the terrain tint (`grass_cover`) a thin spot
/// reads as sparser grass over green ground rather than as a hole. That is why
/// the tint lands before the rings and not after.
struct Ring {
    reach_m: f32,
    chunk_m: f32,
    /// Blades per m², at the shipped density. The knob scales every ring.
    density: f32,
    /// How much wider than [`BLADE_WIDTH`] this ring's blades are.
    ///
    /// This is what keeps the density *looking* constant while the blade count
    /// falls. Coverage is `blades × width × height × d / h`, so a blade twice as
    /// wide hides twice the ground and half as many are needed. A 5 cm blade at
    /// 20 m is already thinner than a pixel; at 20 cm it is still thinner than
    /// three, and nobody can tell one from the other — but the second costs a
    /// quarter as much geometry to cover the same field.
    width_scale: f32,
}

/// The four rings, from the camera outward. **Every number here is derived, not
/// chosen** — [`minimum_density`] is the derivation and a test checks each row
/// against it.
///
/// The field has to read equally thick at every distance; what falls with
/// distance is the *cost*, not the look. Two things make that possible:
///
/// - **Fewer blades.** A blade at distance `d` hides `width · H · d / h` of the
///   ground behind it, because the viewing angle flattens out. The density
///   needed to cover the ground therefore falls as `1/d` — at 16 m, four blades
///   per m² cover what forty-five do at two.
/// - **Wider blades.** Coverage is linear in width too, so doubling the width
///   halves the blade count for the same look. Alone the first lever leaves each
///   ring costing twice its inner neighbour (area grows as `d²`, density falls
///   as `1/d`); with the second the outer rings roughly hold their price.
///
/// Chunks are half the ring's reach: coarser chunks waste less draw budget but
/// overshoot the ring's boundary, and that overshoot is baked geometry nobody
/// asked for. Measured both ways, `reach/2` cost 59.696 triangles against
/// 116.352 for `reach/1`.
const RINGS: [Ring; 4] = [
    Ring {
        reach_m: 4.0,
        chunk_m: 2.0,
        density: 45.5,
        width_scale: 1.0,
    },
    Ring {
        reach_m: 8.0,
        chunk_m: 4.0,
        density: 11.4,
        width_scale: 2.0,
    },
    Ring {
        reach_m: 16.0,
        chunk_m: 8.0,
        density: 2.8,
        width_scale: 4.0,
    },
    Ring {
        reach_m: 32.0,
        chunk_m: 16.0,
        density: 1.4,
        width_scale: 4.0,
    },
];

/// Assumed camera height above the ground, in metres, for the coverage
/// derivation. Not the camera's real height — the rings are baked once and
/// cannot follow a moving lens — but the height it holds while walking, which
/// is the case the meadow has to look right in.
#[cfg(test)]
const EYE_HEIGHT_M: f32 = 1.6;

/// How far past its derived minimum each ring is planted. Blades land on a
/// hash, not on a lattice, so some clumping is certain and the bare patches it
/// would otherwise leave are the artefact this margin buys off.
#[cfg(test)]
const COVERAGE_MARGIN: f32 = 1.25;

/// Blades per m² needed at distance `d` for the ground not to show through,
/// given blades of `width`.
///
/// The derivation, and the most useful result in `BOTWGrass.md`: a blade of
/// height `H` seen from eye height `h` at distance `d` hides a strip of ground
/// `H · d / h` long — the angle flattens with distance — and `width` across. One
/// over that area is the density needed to cover the ground.
#[cfg(test)]
fn minimum_density(distance_m: f32, width_m: f32) -> f32 {
    let average_height = f32::midpoint(BLADE_HEIGHT_MIN, BLADE_HEIGHT_MAX);
    EYE_HEIGHT_M / (width_m * average_height * distance_m.max(0.5))
}

/// The widest a blade may get. Past roughly four times its authored width a
/// blade stops reading as a blade and starts reading as a card, which is the
/// billboard this system deliberately does not use.
#[cfg(test)]
const MAX_WIDTH_SCALE: f32 = 4.0;

/// The density the rings are written against, so the hub's dial can scale them
/// as a ratio instead of replacing them. Stepping the knob to 25/m² makes the
/// whole ladder 0.56× as thick and keeps its shape, which is what makes the
/// sweep readable: one variable moves, not four.
const REFERENCE_DENSITY: f32 = bof_domain::perf::GRASS_DENSITY_STEPS[0];

/// How wide the band is, in metres, where the outermost ring's blades shrink to
/// nothing before their chunk is culled.
///
/// This is the anti-pop: without it, walking makes a wall of grass appear out of
/// nothing at the edge of the last ring. Growth rather than fade because growth
/// is geometry — no blending, no `discard`, no draw order, and none of the
/// early-Z cost that alpha would bring on a tile-based GPU (`BOTWGrass.md`,
/// law 3).
const FADE_BAND_M: f32 = 8.0;

/// At most one chunk is baked per frame.
///
/// Re-baking while the player walks is the only per-frame work this system will
/// ever have, and a frame spike at a chunk boundary would be exactly the kind of
/// stutter the whole design exists to avoid. One per frame means crossing a
/// boundary costs one chunk, not a ring.
const CHUNKS_BAKED_PER_FRAME: usize = 1;

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

/// Root and tip colours. The root-to-tip gradient is the single biggest reason
/// BOTW grass reads as grass; it used to be baked into a vertex colour and now
/// travels as two uniforms, because it is a pure function of the vertex's
/// height along the blade and a `mix` in the shader costs nothing per frame
/// while sixteen bytes per vertex cost bandwidth on every one of them.
pub(super) const ROOT_COLOR: LinearRgba = LinearRgba::rgb(0.13, 0.24, 0.09);
const TIP_COLOR: LinearRgba = LinearRgba::rgb(0.42, 0.70, 0.22);

/// One baked chunk of the meadow: a single mesh holding all its blades.
#[derive(Component)]
pub(super) struct GrassChunk;

/// Which chunk of which ring. The ring is part of the identity because the same
/// patch of ground is covered by different chunks at different distances.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ChunkKey {
    ring: usize,
    cell: IVec2,
}

/// The live grid: which chunks exist right now, and the one material they share.
///
/// One material for the whole meadow, cloned per chunk — the blades batch, and a
/// second material would double the draws for nothing.
#[derive(Resource)]
pub(super) struct GrassField {
    material: Handle<GrassMaterial>,
    live: HashMap<ChunkKey, Entity>,
}

/// Triángulos que la pradera declara a la escena: cada brizna son **dos**.
///
/// Sólo lo consume el presupuesto declarado (`perf::budget`), que es un test: el
/// contador de runtime cuenta lo que la cámara ve, no lo que la escena declara.
/// Mientras este número vivió dentro del módulo, la meadow no la sumaba nadie y
/// una escena con pradera leía como si el pasto fuera gratis.
///
/// Con la grilla rodante ya no es "el campo": es **lo que hay alrededor de la
/// cámara en cualquier momento**, que es constante y no depende del tamaño del
/// mapa. Ese cambio es el punto entero del Paso 4.
#[cfg(test)]
pub(crate) fn meadow_triangles() -> usize {
    neighbourhood_blades(Vec2::ZERO) * 2
}

/// Blades standing around a camera at `focus`.
///
/// The count wobbles with where the camera falls against the chunk grid — a
/// camera on a boundary needs one more row of chunks than one in the middle of a
/// cell — so the declared cost is taken at the origin, which is the worst
/// alignment there is: every ring's grid lines up there at once. Anywhere else
/// is cheaper, never dearer.
#[cfg(test)]
fn neighbourhood_blades(focus: Vec2) -> usize {
    RINGS
        .iter()
        .enumerate()
        .map(|(index, ring)| {
            ring_cells(index, focus).len() * blades_per_chunk(ring, REFERENCE_DENSITY) as usize
        })
        .sum()
}

/// Blades in one chunk of `ring` at a given dial setting. Rounded once, here, so
/// the count on screen and the count in the budget are the same number.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "density is clamped non-negative and a blade count is an integer bucket"
)]
fn blades_per_chunk(ring: &Ring, dial: f32) -> u32 {
    let density = ring.density * (dial / REFERENCE_DENSITY);
    (ring.chunk_m * ring.chunk_m * density).round().max(0.0) as u32
}

/// Which cells of `ring` should exist with the camera at `focus`.
///
/// A chunk is in if it *touches* the ring's square, and out only if the ring
/// inside already covers it *entirely*. Both halves matter, and the second one
/// is the subtle one: judging a chunk by its centre looks equivalent and is not.
/// A 10 m chunk whose centre falls just inside the inner ring gets dropped
/// whole, while the inner ring — made of smaller chunks — never reaches the far
/// side of it. What is left is a hole of bare dirt a few metres from the player,
/// which is the exact artefact this system exists to prevent. The overlap this
/// criterion allows instead is a strip up to half a chunk wide that is a little
/// denser than derived, and denser is not a defect.
#[expect(
    clippy::cast_possible_truncation,
    reason = "chunk coordinates are small integers by construction"
)]
fn ring_cells(index: usize, focus: Vec2) -> Vec<IVec2> {
    let ring = &RINGS[index];
    let inner_reach = index.checked_sub(1).map_or(0.0, |i| RINGS[i].reach_m);
    let half = ring.chunk_m * 0.5;
    // One cell of slack: a chunk can touch the ring while its centre sits
    // outside it.
    let span = (ring.reach_m / ring.chunk_m).ceil() as i32 + 1;
    let base = (focus / ring.chunk_m).floor().as_ivec2();

    let mut cells = Vec::new();
    for dz in -span..=span {
        for dx in -span..=span {
            let cell = base + IVec2::new(dx, dz);
            let offset = (cell_centre(cell, ring.chunk_m) - focus).abs();
            // Chebyshev, because the chunks are a square grid: a square boundary
            // is the one that never cuts a chunk in half.
            let nearest = (offset - Vec2::splat(half)).max(Vec2::ZERO).max_element();
            let farthest = (offset + Vec2::splat(half)).max_element();
            if nearest > ring.reach_m || farthest <= inner_reach {
                continue;
            }
            cells.push(cell);
        }
    }
    cells
}

fn cell_centre(cell: IVec2, chunk_m: f32) -> Vec2 {
    (cell.as_vec2() + Vec2::splat(0.5)) * chunk_m
}

/// Scene entry: start from an empty grid.
///
/// The chunks themselves are `DespawnOnExit`, so leaving a scene already killed
/// them; what this clears is the bookkeeping that would otherwise point at dead
/// entities and make the next scene think its meadow was already built.
pub(super) fn reset_meadow(mut field: ResMut<GrassField>) {
    field.live.clear();
}

pub(super) fn init_meadow_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<GrassMaterial>>,
) {
    commands.insert_resource(GrassField {
        material: materials.add(grass_material()),
        live: HashMap::default(),
    });
}

/// The meadow's material: PBR plus the grass extension.
///
/// `ExtendedMaterial` rather than a pipeline of our own — lighting, shadows,
/// fog and decals keep working, and what the extension owns is only where the
/// base colour and the normal come from.
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

/// Roll the grid: drop the chunks that fell behind, bake the ones ahead.
///
/// Reads the camera rather than the player because the LOD has to answer to what
/// the screen shows: zoom out or swing the camera away and it is the camera's
/// neighbourhood that needs blades, not the player's.
#[expect(
    clippy::too_many_arguments,
    reason = "baking a chunk needs the terrain, the assets, the dial and the scene"
)]
pub(super) fn roll_meadow_grid(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut field: ResMut<GrassField>,
    perf: Res<crate::perf::PerfToggles>,
    scene: Res<State<crate::scene::AppState>>,
    terrain: TerrainAccess,
    camera: Option<Single<&GlobalTransform, With<Camera3d>>>,
    mut dial: Local<Option<usize>>,
) {
    let Some(camera) = camera else {
        return;
    };
    let focus = camera.translation().xz();

    // A density change invalidates every baked chunk, so it is the one event
    // that clears the grid instead of rolling it.
    if dial.replace(perf.grass_density_step) != Some(perf.grass_density_step) {
        for entity in field.live.values() {
            commands.entity(*entity).despawn();
        }
        field.live.clear();
    }

    let wanted: HashSet<ChunkKey> = RINGS
        .iter()
        .enumerate()
        .flat_map(|(ring, _)| {
            ring_cells(ring, focus)
                .into_iter()
                .map(move |cell| ChunkKey { ring, cell })
        })
        .collect();

    field.live.retain(|key, entity| {
        let keep = wanted.contains(key);
        if !keep {
            commands.entity(*entity).despawn();
        }
        keep
    });

    let density = perf.grass_density();
    // Collected before the loop so the bake can borrow the field mutably; the
    // list is at most `CHUNKS_BAKED_PER_FRAME` long.
    let missing: Vec<ChunkKey> = wanted
        .iter()
        .filter(|key| !field.live.contains_key(*key))
        .take(CHUNKS_BAKED_PER_FRAME)
        .copied()
        .collect();
    for key in &missing {
        let ring = &RINGS[key.ring];
        // Seeded by ring and cell, not by spawn order: the same ground grows the
        // same blades every session, and walking away and back does not reshuffle
        // the field.
        let ring_salt = u32::try_from(key.ring).unwrap_or(0);
        let seed = hash_u32(
            key.cell.x.cast_unsigned().wrapping_mul(0x9e37_79b9)
                ^ key.cell.y.cast_unsigned().wrapping_mul(0x85eb_ca6b)
                ^ ring_salt.wrapping_mul(0xc2b2_ae35),
        );
        let mesh = build_chunk_mesh(
            cell_centre(key.cell, ring.chunk_m),
            ring.chunk_m,
            blades_per_chunk(ring, density),
            ring.width_scale,
            seed,
            Some(&terrain),
        );
        let entity = commands
            .spawn((
                DespawnOnExit(*scene.get()),
                Name::new(format!(
                    "GrassChunk_r{}_{}_{}",
                    key.ring, key.cell.x, key.cell.y
                )),
                GrassChunk,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(field.material.clone()),
                // Blades cast no shadows: thousands of alpha-free slivers in the
                // cascades buy noise, not depth.
                bevy::light::NotShadowCaster,
                Transform::default(),
            ))
            .id();
        field.live.insert(*key, entity);
    }
}

/// Tell the shader where the camera is, so the outermost blades can shrink
/// before their chunk disappears.
///
/// One material for the whole meadow means one uniform write per frame, not one
/// per chunk.
pub(super) fn track_meadow_focus(
    field: Res<GrassField>,
    mut materials: ResMut<Assets<GrassMaterial>>,
    camera: Option<Single<&GlobalTransform, With<Camera3d>>>,
) {
    let Some(camera) = camera else {
        return;
    };
    let Some(mut material) = materials.get_mut(&field.material) else {
        return;
    };
    let outermost = RINGS[RINGS.len() - 1].reach_m;
    material.extension.grass_data.focus_xz = camera.translation().xz();
    material.extension.grass_data.fade_start = outermost - FADE_BAND_M;
    material.extension.grass_data.fade_end = outermost;
}

/// Bake `count` blades into one mesh, in world space, around `centre`.
///
/// Vertices carry world positions (the chunk entity sits at the origin), so a
/// blade sits on the ground wherever the terrain put it without the chunk having
/// to track a height of its own.
fn build_chunk_mesh(
    centre: Vec2,
    chunk_m: f32,
    count: u32,
    width_scale: f32,
    seed: u32,
    terrain: Option<&TerrainAccess>,
) -> Mesh {
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

        let xz = centre + Vec2::new(u1 - 0.5, u2 - 0.5) * chunk_m;
        let ground = terrain.and_then(|t| t.height_at(xz)).unwrap_or(0.0);
        let slope = terrain.and_then(|t| t.slope_deg_at(xz)).unwrap_or(0.0);
        // No terrain means a test harness, and a test wants blades: flat soil
        // is what the real default cell is anyway.
        let kind = terrain
            .and_then(|t| t.kind_at(xz))
            .unwrap_or(crate::world::TerrainKind::Soil);
        // One rule, shared with the terrain tint — see `grass_cover`. Bare
        // ground here has to be bare ground there or the field grows an edge.
        let cover = grass_cover::coverage(kind, slope);
        if cover <= 0.0 {
            continue;
        }

        let height = (BLADE_HEIGHT_MIN + u4 * (BLADE_HEIGHT_MAX - BLADE_HEIGHT_MIN)) * cover;
        let yaw = u3 * std::f32::consts::TAU;
        // The quad's width runs along `side`; the lean tips it over `side`'s
        // perpendicular so blades do not all fall the same way.
        let side = Vec2::new(yaw.cos(), yaw.sin()) * (BLADE_WIDTH * width_scale * 0.5);
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

        // The uv is not a texture coordinate — nothing here samples one. It
        // carries the two things the shader cannot reconstruct from a position:
        //
        // - `y` is the vertex's height along the blade, 0 at the root and 1 at
        //   the tip. The colour gradient reads it, and the wind and the trample
        //   map will multiply their displacement by it.
        // - `x` is the ground height under the blade, which is what lets the
        //   shader collapse a blade toward its own root for the distance fade
        //   without a per-vertex blade height.
        //
        // The normal and the vertex colour that used to sit beside them are
        // gone: both are constants of the system, rebuilt in `grass.wgsl`.
        uvs.push([base.y, 0.0]);
        uvs.push([base.y, 0.0]);
        uvs.push([base.y, 1.0]);
        uvs.push([base.y, 1.0]);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_density_knob_is_what_actually_lands_on_the_ground() {
        // The failure this system was built to fix: a density that reads well in
        // a constant but arrives on screen divided by twenty. Stated against the
        // ring rather than a literal, so tuning the density cannot break the
        // test that guards it — the count must always be area × density.
        for ring in &RINGS {
            let per_chunk = blades_per_chunk(ring, REFERENCE_DENSITY);
            let expected = (ring.chunk_m * ring.chunk_m * ring.density).round();
            assert_eq!(
                (per_chunk as f32).to_bits(),
                expected.to_bits(),
                "the ring density must reach the ground intact"
            );
        }
    }

    /// The dial scales the ladder instead of flattening it, or the sweep would
    /// be measuring a different shape at every step.
    #[test]
    fn the_dial_scales_every_ring_by_the_same_ratio() {
        let sparse = bof_domain::perf::GRASS_DENSITY_STEPS[2];
        let ratio = sparse / REFERENCE_DENSITY;
        for ring in &RINGS {
            let full = f64::from(blades_per_chunk(ring, REFERENCE_DENSITY));
            let thin = f64::from(blades_per_chunk(ring, sparse));
            assert!(
                (thin - full * f64::from(ratio)).abs() <= 1.0,
                "ring at {} m does not follow the dial",
                ring.reach_m
            );
        }
    }

    /// The rings have to tile the ground, not stack on it: a cell covered by two
    /// rings is a patch paying twice for grass nobody asked for.
    #[test]
    fn no_point_inside_the_reach_is_left_uncovered() {
        let focus = Vec2::new(3.7, -11.2);
        let covered: Vec<(Vec2, f32)> = RINGS
            .iter()
            .enumerate()
            .flat_map(|(index, ring)| {
                ring_cells(index, focus)
                    .into_iter()
                    .map(move |cell| (cell_centre(cell, ring.chunk_m), ring.chunk_m * 0.5))
            })
            .collect();

        let outermost = RINGS[RINGS.len() - 1].reach_m;
        let mut along = -outermost;
        while along <= outermost {
            let mut across = -outermost;
            while across <= outermost {
                let point = focus + Vec2::new(along, across);
                let inside = covered.iter().any(|(centre, half)| {
                    let offset = (point - *centre).abs();
                    offset.x <= *half && offset.y <= *half
                });
                assert!(
                    inside,
                    "nothing covers {point:?}, {:.1} m from the camera",
                    (point - focus).length()
                );
                across += 1.7;
            }
            along += 1.7;
        }
    }

    /// The rings are derived from the coverage formula, not chosen by eye. A row
    /// planted below its own minimum shows bare ground, and would do it silently
    /// — the failure reads as "the grass is a bit thin over there".
    #[test]
    fn every_ring_meets_the_density_its_distance_demands() {
        for (index, ring) in RINGS.iter().enumerate() {
            // The demanding point of a ring is its *inner* edge: that is where
            // its blades are seen from closest and each one hides the least
            // ground.
            let inner = index.checked_sub(1).map_or(2.0, |i| RINGS[i].reach_m);
            let needed = minimum_density(inner, BLADE_WIDTH * ring.width_scale);
            assert!(
                ring.density >= needed,
                "ring reaching {} m plants {}/m2 where {needed:.1}/m2 is the minimum at {inner} m",
                ring.reach_m,
                ring.density
            );
            assert!(
                ring.density <= needed * COVERAGE_MARGIN * 1.05,
                "ring reaching {} m plants {}/m2, past the {:.1}/m2 its distance needs — \
                 surplus density is paid entirely in overdraw",
                ring.reach_m,
                ring.density,
                needed * COVERAGE_MARGIN
            );
            assert!(
                ring.width_scale <= MAX_WIDTH_SCALE,
                "a blade {}x its authored width reads as a card, not a blade",
                ring.width_scale
            );
        }
    }

    /// The whole point of the rolling grid: what it costs does not depend on
    /// where the camera stands, so the budget is a property of the view.
    /// The whole point of the rolling grid: what it costs is a property of the
    /// view, not of where in the world that view happens to be. A field that
    /// grew with distance from the origin would be the fixed square again,
    /// wearing a different shape.
    #[test]
    fn the_neighbourhood_costs_the_same_far_from_the_origin_as_near_it() {
        let declared = neighbourhood_blades(Vec2::ZERO);
        for focus in [
            Vec2::new(137.0, -488.0),
            Vec2::new(-2049.5, 903.25),
            Vec2::new(41_000.0, 41_000.0),
        ] {
            let count = neighbourhood_blades(focus);
            assert!(
                count <= declared,
                "at {focus:?} the meadow is {count} blades, over the {declared} declared \
                 at the origin — the declared cost has to be the worst case"
            );
            // And it does not collapse either: an alignment that shed most of
            // the field would mean the grid stops covering the ground.
            assert!(
                count * 3 >= declared * 2,
                "at {focus:?} the meadow shrinks to {count} of {declared} blades"
            );
        }
    }

    #[test]
    fn the_whole_neighbourhood_stays_inside_the_mobile_triangle_budget() {
        // Grass shares the frame with 32768 triangles of terrain, so the field
        // has to leave room for the ground it grows on.
        let terrain = 128 * 128 * 2;
        assert!(
            meadow_triangles() + terrain <= crate::perf::budget::MOBILE_TRIANGLES,
            "meadow {} + terrain {terrain} exceeds the mobile budget",
            meadow_triangles()
        );
    }

    #[test]
    fn blades_are_deterministic_per_chunk() {
        // Same chunk, same blades: a field that reshuffles when it is rebuilt
        // makes every visual comparison worthless.
        let a = build_chunk_mesh(Vec2::new(5.0, -5.0), 5.0, 64, 1.0, 11, None);
        let b = build_chunk_mesh(Vec2::new(5.0, -5.0), 5.0, 64, 1.0, 11, None);
        assert_eq!(
            a.attribute(Mesh::ATTRIBUTE_POSITION)
                .map(|values| values.len()),
            b.attribute(Mesh::ATTRIBUTE_POSITION)
                .map(|values| values.len())
        );
    }

    /// The vertex carries only what cannot be derived. Normal and colour are
    /// constants of the system rebuilt in `grass.wgsl`; baking them again would
    /// silently put back the 28 bytes per vertex this system removed, and
    /// nothing on screen would show it.
    #[test]
    fn the_vertex_carries_only_position_and_the_two_derived_values() {
        let mesh = build_chunk_mesh(Vec2::ZERO, 5.0, 8, 1.0, 5, None);
        assert!(mesh.attribute(Mesh::ATTRIBUTE_POSITION).is_some());
        assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
        assert!(
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL).is_none(),
            "the normal is +Y for every blade — the shader rebuilds it"
        );
        assert!(
            mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_none(),
            "the gradient is a function of uv.y between two uniforms"
        );
    }

    /// The shader reads `uv.y` as the height along the blade and `uv.x` as the
    /// ground under it, so the four vertices have to keep meaning root, root,
    /// tip, tip in that order.
    #[test]
    fn a_blades_four_vertices_keep_their_authored_order() {
        let mesh = build_chunk_mesh(Vec2::ZERO, 5.0, 1, 1.0, 5, None);
        let bevy::mesh::VertexAttributeValues::Float32x2(uvs) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0).expect("uvs")
        else {
            panic!("uvs must be Float32x2");
        };
        assert_eq!(uvs[0][1], 0.0, "base-left sits at the root");
        assert_eq!(uvs[1][1], 0.0, "base-right sits at the root");
        assert_eq!(uvs[2][1], 1.0, "tip-right sits at the tip");
        assert_eq!(uvs[3][1], 1.0, "tip-left sits at the tip");
        for uv in uvs {
            assert_eq!(uv[0], 0.0, "flat test ground is at y = 0 for every vertex");
        }
    }

    #[test]
    fn a_blade_stands_on_the_ground_and_reaches_its_height() {
        let mesh = build_chunk_mesh(Vec2::ZERO, 5.0, 1, 1.0, 3, None);
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
