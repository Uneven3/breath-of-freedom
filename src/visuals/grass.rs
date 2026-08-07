//! The grass meadow: a **rolling grid of baked chunks, centred on the camera**.
//!
//! Pure presentation — no collider, no simulation meaning; the ground beneath
//! already reports `Surface(Grass)` for footstep audio.
//!
//! Two decisions carry the module, both argued in `docs/BOTWGrass.md`: a chunk
//! bakes its blades into **one mesh** (a blade is not an entity), and the field
//! is a **neighbourhood, not a place** — rings around wherever the camera is, so
//! the budget is *per view*. The unit is the two-triangle blade
//! ([`BladeShape`]) and density falls as `1/d`, a floor rather than a recipe.

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
/// `reach_m` is a Chebyshev radius. The seam that leaves between two rings is a
/// strip up to half a chunk wide; after the terrain tint it reads as sparser
/// grass over green ground rather than as a hole.
struct Ring {
    reach_m: f32,
    chunk_m: f32,
    /// Blades per m², at the shipped density. The knob scales every ring.
    density: f32,
    /// How much wider than [`BLADE_WIDTH`] this ring's blades are.
    ///
    /// **1.0 everywhere, and that is a finding.** ×2/×4 was rejected on sight as
    /// a bed of fat spikes: widening only pays once a blade is sub-pixel.
    width_scale: f32,
    /// How many triangles this ring's blades are worth spending.
    shape: BladeShape,
}

/// The blade, at three levels of detail.
///
/// **The LOD the system did not have.** Until 2026-08-06 only density fell with
/// distance — the blade kept its shape all the way out, and thinning the field
/// is what opens the emptiness the meadow is judged on. Law 1's two-triangle
/// blade is now a statement about the *near* ring; past it a blade may be less.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BladeShape {
    /// Five vertices, three triangles: the top edge dips, so the blade ends in
    /// two points. The one shape cue that says "leaf" at arm's length.
    Notched,
    /// Four vertices, two triangles: the plain tapered quad.
    Quad,
    /// Three vertices, one triangle: two base corners and a single tip. The
    /// floor — the taper already converges, so the fourth vertex is a corner
    /// nobody resolves out here.
    Spike,
}

impl BladeShape {
    /// Vertices and triangles per blade. One place, because several is how the
    /// budget quietly stops matching the mesh.
    const fn vertices(self) -> usize {
        match self {
            Self::Notched => 5,
            Self::Quad => 4,
            Self::Spike => 3,
        }
    }

    const fn triangles(self) -> usize {
        match self {
            Self::Notched => 3,
            Self::Quad => 2,
            Self::Spike => 1,
        }
    }
}

/// The three rings, from the camera outward. Each row is **floored** by
/// [`minimum_density`] — a test enforces it — and chosen above it by eye, because
/// covering the ground and looking like a meadow are different bars.
///
/// Derivation and cost in `docs/BOTWGrass.md`. The fact that governs decisions
/// here: the meadow is **fill-bound**, so the triangle count is a guardrail.
/// Seis anillos hasta 64 m. **Un anillo ya no es un escalón de densidad: es un
/// tamaño de chunk.**
///
/// Cada uno se planta a la densidad que la derivación pide en su **borde
/// interno**, y el shader ancla ahí su ley `1/d` (ver `ring_inner` en
/// `grass.wgsl`), así que dentro del anillo la densidad viva es `C/d` exacta.
/// Sobreplantan a lo sumo 1,6× en su borde externo, que es lo que la ley se
/// come. Antes se plantaba plano por anillo y el escalón era inevitable por más
/// que se afinaran los números — el artefacto que el usuario identificó como
/// *el* problema de la sesión.
const RINGS: [Ring; 4] = [
    Ring {
        reach_m: 13.0,
        chunk_m: 8.0,
        // Densidad plena hasta [`GROWTH_START_M`]: adentro de ese radio no ralea
        // nada, así que los tres primeros anillos plantan lo mismo y sólo
        // cambian de forma de brizna y de tamaño de chunk.
        density: 40.0,
        width_scale: 1.0,
        shape: BladeShape::Notched,
    },
    Ring {
        reach_m: 24.0,
        chunk_m: 16.0,
        density: 40.0,
        width_scale: 1.0,
        shape: BladeShape::Quad,
    },
    Ring {
        reach_m: 40.0,
        chunk_m: 32.0,
        density: 40.0,
        width_scale: 1.0,
        shape: BladeShape::Spike,
    },
    Ring {
        // El único con ancla propia: `C / 40`, con `C = 40 · 24`.
        reach_m: 64.0,
        chunk_m: 32.0,
        density: 24.0,
        width_scale: 1.0,
        shape: BladeShape::Spike,
    },
];

/// Los alcances, como el shader los necesita para deducir el borde interno.
fn ring_reaches() -> (Vec4, Vec4) {
    let mut slots = [0.0_f32; 8];
    for (slot, ring) in slots.iter_mut().zip(RINGS.iter()) {
        *slot = ring.reach_m;
    }
    (Vec4::from_slice(&slots[..4]), Vec4::from_slice(&slots[4..]))
}

// Los tamaños de chunk se dividen entre sí (8 | 16 | 32) a propósito: el barrido
// del peor caso recorre un período igual al chunk más grande, y con tamaños
// primos entre sí ese período no cubre todas las alineaciones — el test del peor
// caso pasaba a ciegas.

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
const COVERAGE_MARGIN: f32 = 1.2;

/// Blades per m² needed at distance `d` for the ground not to show through.
/// A floor, not a recipe — see the module header.
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

/// The reach scale the rings are written against, so the budget and the tests
/// measure the shipped field rather than whatever the dial happens to be on.
#[cfg(test)]
const REFERENCE_REACH: f32 = bof_domain::perf::GRASS_REACH_STEPS[0];

/// Cuántos metros tarda **una** brizna en pasar de nada a entera.
///
/// **La constante que gobierna el artefacto, y la que nunca se tocó.** Una
/// brizna cambia `1/rampa` de su altura por metro caminado; con 1 m era el 100%,
/// y a la distancia donde ocurría medía 117 px de alto — o sea 147 px de cambio
/// por metro, en algo que el ojo está mirando. Todo el trabajo previo atacaba el
/// perfil *espacial* de densidad, que no es lo que se percibe caminando.
///
/// Larga sólo sirve junto con [`GROWTH_START_M`] lejos: la rampa se resta del
/// umbral, así que con umbrales cerca deja briznas a media altura a los pies del
/// jugador. Las dos se mueven juntas o ninguna.
const GROWTH_RAMP_M: f32 = 6.0;

/// Over how many metres, inward from a ring's edge, the thresholds are spread.
///
/// Separate from the ramp on purpose: **one blade growing is invisible, a whole
/// band growing at once is not.** Shortening both together made it worse.
const GROWTH_SPREAD_M: f32 = 6.0;

/// A partir de qué distancia la pradera empieza a ralear, en metros.
///
/// Los umbrales se reparten como `start / (1 - hash)`, así que la fracción viva a
/// distancia `d` es `start / d`: **la ley 1/d que `BOTWGrass.md` deriva y que
/// hasta el 2026-08-06 no se aplicaba**. Se plantaba plano y se recortaba en una
/// banda al borde del anillo — una escalera donde correspondía una rampa, y esa
/// escalera viajando con la cámara *es* el artefacto de "veo crecer el pasto".
///
/// Ocho metros salieron de un barrido cenital midiendo qué tan pareja queda la
/// pendiente; cuatro da la rampa más lisa pero deja el campo en el look que este
/// proyecto ya jugó y rechazó, y doce es peor que no hacer nada. La tabla está en
/// `BOTWGrass.md`.
///
/// **No ahorra un triángulo:** la geometría sigue horneada y esto sólo la encoge
/// en el vertex shader. Arregla la imagen, no el costo.
const GROWTH_START_M: f32 = 24.0;

/// How far **below** the ground a blade collapses to, in metres.
///
/// Not zero, and that is the whole point: collapsed at ground level the quad
/// lies coplanar with the terrain and z-fights, which on screen is the flicker
/// that took three wrong diagnoses to name. Sunk, the blade is simply behind the
/// ground and the depth test settles it.
const GROWTH_SINK_M: f32 = 0.18;

/// At most one chunk is baked per frame **while rolling**.
///
/// Re-baking while the player walks is the only per-frame work this system will
/// ever have, and a frame spike at a chunk boundary would be exactly the kind of
/// stutter the whole design exists to avoid. One per frame means crossing a
/// boundary costs one chunk, not a ring.
const CHUNKS_BAKED_PER_FRAME: usize = 1;

/// Filling an empty grid ignores the per-frame limit and bakes the lot in one
/// frame: a scene that starts with a hole in the meadow is worse than one hitch.
const FILL_IN_ONE_FRAME: bool = true;

/// Blade shape. Wide enough at the base to cover ground, tapered at the tip so
/// it reads as a leaf rather than a strip of paper.
const BLADE_WIDTH: f32 = 0.055;
/// Fraction of the base width kept at the tip.
///
/// Was 0,35 until 2026-08-06 and the tip read as **cut square** rather than
/// pointed — reported playing. 0,18 leaves a point without the two halves of the
/// notch degenerating into slivers.
const BLADE_TIP_TAPER: f32 = 0.18;
/// How far down from the tip the notch sits, as a fraction of the height. Deep
/// enough to read as two points, shallow enough not to be a fork.
const TIP_NOTCH_DEPTH: f32 = 0.72;
/// Blade height range in metres. Knee to hip on a 1,8 m capsule.
///
/// **The ceiling is one metre and it is hard**: the height travels in the
/// fraction of `uv1.y` with the ring's reach in the whole part. A test pins it.
const BLADE_HEIGHT_MIN: f32 = 0.45;
const BLADE_HEIGHT_MAX: f32 = 0.90;
/// How far a tip may lean off vertical, in metres, so the field is not a bed of
/// nails. Deterministic per blade — this is authored variety, not animation.
///
/// Scaled with the height above: kept at 0,16 the taller blades stood
/// noticeably straighter than the short ones used to, which is the uniformity
/// this constant exists to break.
const BLADE_LEAN: f32 = 0.27;

/// Root and tip colours, as uniforms because the gradient is a pure function of
/// the vertex's height.
///
/// **The criterion is the soil the blades stand in** (hue 84°, sat 37%), not
/// taste: where the field thins, blade and ground are seen together. The root was
/// 16° off that hue and half its saturation — see `docs/BOTWGrass.md`.
pub(super) const ROOT_COLOR: LinearRgba = LinearRgba::rgb(0.093, 0.147, 0.031);
const TIP_COLOR: LinearRgba = LinearRgba::rgb(0.340, 0.622, 0.089);

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

/// Triángulos que la pradera declara a la escena, para `perf::budget`.
/// Declarados, no dibujados: el frustum descarta buena parte y cuánta es una
/// incógnita, no una medición.
#[cfg(test)]
pub(crate) fn meadow_triangles() -> usize {
    // Not `blades * 2`: a notched tip is three triangles, and the two inner
    // rings have one. A budget that assumed two everywhere would under-declare
    // the meadow by a third of its near geometry — the kind of quiet error a
    // budget exists to make loud.
    let period = RINGS
        .iter()
        .map(|ring| ring.chunk_m)
        .fold(0.0_f32, f32::max);
    let mut worst = 0;
    for z in 0..8 {
        for x in 0..8 {
            let focus = Vec2::new(x as f32, z as f32) * (period / 8.0);
            let triangles: usize = RINGS
                .iter()
                .enumerate()
                .map(|(index, ring)| {
                    let per_blade = ring.shape.triangles();
                    ring_cells(index, focus, REFERENCE_REACH).len()
                        * blades_per_chunk(ring, REFERENCE_DENSITY) as usize
                        * per_blade
                })
                .sum();
            worst = worst.max(triangles);
        }
    }
    worst
}

/// The most blades the meadow can ever have standing at once. A guardrail on
/// the baker, not a budget: `perf::budget` owns what the scene may cost.
///
/// Swept over every alignment inside one cell of the largest chunk, which is the
/// period after which the pattern repeats.
#[cfg(test)]
fn worst_case_blades() -> usize {
    let period = RINGS
        .iter()
        .map(|ring| ring.chunk_m)
        .fold(0.0_f32, f32::max);
    let steps = 8;
    let mut worst = 0;
    for z in 0..steps {
        for x in 0..steps {
            let offset = Vec2::new(x as f32, z as f32) * (period / steps as f32);
            worst = worst.max(neighbourhood_blades(offset));
        }
    }
    worst
}

/// Blades standing around a camera at `focus`.
#[cfg(test)]
fn neighbourhood_blades(focus: Vec2) -> usize {
    RINGS
        .iter()
        .enumerate()
        .map(|(index, ring)| {
            ring_cells(index, focus, REFERENCE_REACH).len()
                * blades_per_chunk(ring, REFERENCE_DENSITY) as usize
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
/// Chebyshev square, not a circle: the chunks are a square grid, so a square
/// boundary keeps a chunk wholly in a ring or wholly out.
fn ring_cells(index: usize, focus: Vec2, reach_scale: f32) -> Vec<IVec2> {
    ring_cells_with_slack(index, focus, 0.0, reach_scale)
}

/// A ring's reach with the dial applied, in **whole metres** — a hard
/// constraint, not tidiness: the reach travels in the integer part of `uv1.y`
/// with the blade's height in the fraction.
fn ring_reach(index: usize, reach_scale: f32) -> f32 {
    (RINGS[index].reach_m * reach_scale).round().max(1.0)
}

/// How far past its reach a chunk is kept. Without it a camera on a grid line
/// rebakes the same chunk every frame, which on screen is a patch flickering.
const KEEP_SLACK_M: f32 = 3.0;

#[expect(
    clippy::cast_possible_truncation,
    reason = "chunk coordinates are small integers by construction"
)]
fn ring_cells_with_slack(index: usize, focus: Vec2, slack: f32, reach_scale: f32) -> Vec<IVec2> {
    let ring = &RINGS[index];
    let reach_m = ring_reach(index, reach_scale) + slack;
    let inner_reach = index
        .checked_sub(1)
        .map_or(0.0, |i| ring_reach(i, reach_scale));
    let half = ring.chunk_m * 0.5;
    // One cell of slack: a chunk can touch the ring while its centre sits
    // outside it.
    let span = (reach_m / ring.chunk_m).ceil() as i32 + 1;
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
            // El anillo de afuera empieza *antes* de donde termina el de
            // adentro, por el ancho de la dispersión: en esa franja el interior
            // ralea y el exterior ya está entero, así que la densidad cruza sin
            // escalón.
            let handover = (inner_reach - GROWTH_SPREAD_M).max(0.0);
            if nearest > reach_m || farthest <= handover {
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
    mut dial: Local<Option<(usize, usize)>>,
) {
    let Some(camera) = camera else {
        return;
    };
    let focus = camera.translation().xz();
    let density = perf.grass_density();
    let reach_scale = perf.grass_reach_scale();

    // Density and reach are both **baked into the mesh** — the blade count in the
    // geometry, the ring's reach in a vertex attribute — so neither can be
    // rolled into. They are the one event that clears the grid instead of
    // rolling it, and they are checked together because a run that changed both
    // and rebuilt for one would leave half the field describing the old dial.
    let dials = (perf.grass_density_step, perf.grass_reach_step);
    if dial.replace(dials) != Some(dials) {
        for entity in field.live.values() {
            commands.entity(*entity).despawn();
        }
        field.live.clear();
    }

    let wanted: HashSet<ChunkKey> = RINGS
        .iter()
        .enumerate()
        .flat_map(|(ring, _)| {
            ring_cells(ring, focus, reach_scale)
                .into_iter()
                .map(move |cell| ChunkKey { ring, cell })
        })
        .collect();

    // Kept with slack, created without: a chunk on the boundary stays instead of
    // being re-baked every other frame, which is what the flicker was.
    let keep_set: HashSet<ChunkKey> = RINGS
        .iter()
        .enumerate()
        .flat_map(|(ring, _)| {
            ring_cells_with_slack(ring, focus, KEEP_SLACK_M, reach_scale)
                .into_iter()
                .map(move |cell| ChunkKey { ring, cell })
        })
        .collect();

    field.live.retain(|key, entity| {
        let keep = keep_set.contains(key);
        if !keep {
            commands.entity(*entity).despawn();
        }
        keep
    });

    // An empty grid is being filled, not rolled: bake it whole rather than
    // letting the meadow grow in around the player over several seconds.
    let budget = if FILL_IN_ONE_FRAME && field.live.is_empty() {
        usize::MAX
    } else {
        CHUNKS_BAKED_PER_FRAME
    };
    // Collected before the loop so the bake can borrow the field mutably.
    let missing: Vec<ChunkKey> = wanted
        .iter()
        .filter(|key| !field.live.contains_key(*key))
        .take(budget)
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
            &ChunkSpec {
                centre: cell_centre(key.cell, ring.chunk_m),
                chunk_m: ring.chunk_m,
                count: blades_per_chunk(ring, density),
                width_scale: ring.width_scale,
                shape: ring.shape,
                ring_reach_m: ring_reach(key.ring, reach_scale),
                seed,
            },
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
                // Su cuenta de triángulos es una decisión, no un descuido: el
                // watchdog de mallas pesadas es para assets, y el presupuesto de
                // la pradera se cobra en `perf::budget`.
                crate::visuals::budget::BakedByDesign,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(field.material.clone()),
                // Blades cast no shadows: thousands of alpha-free slivers in the
                // cascades buy noise, not depth.
                bevy::light::NotShadowCaster,
                // And they receive none either. Shadows were the only lever
                // outside the noise floor (−0,66 ms), and receiving is the
                // expensive half: a sample per fragment over the geometry with
                // the most overdraw in the scene. The cost is real and worth
                // naming — grass under a tree is lit as if the tree were not
                // there.
                bevy::light::NotShadowReceiver,
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
    sun: Option<Single<&GlobalTransform, With<DirectionalLight>>>,
    time: Res<Time>,
) {
    let Some(camera) = camera else {
        return;
    };
    let Some(mut material) = materials.get_mut(&field.material) else {
        return;
    };
    let data = &mut material.extension.grass_data;
    data.focus_xz = camera.translation().xz();
    data.growth_ramp = GROWTH_RAMP_M;
    data.growth_spread = GROWTH_SPREAD_M;
    data.growth_start = GROWTH_START_M;
    let (a, b) = ring_reaches();
    data.ring_reaches_a = a;
    data.ring_reaches_b = b;
    data.growth_sink = GROWTH_SINK_M;
    // The wind is a function of world position and time — there is no per-blade
    // state anywhere, which is why a field of a hundred thousand blades costs
    // one uniform write a frame.
    data.time = time.elapsed_secs_wrapped();
    // Backlit transmission needs to know where the sun is; reading the light's
    // own transform rather than a copy keeps day/night driving it for free.
    if let Some(sun) = sun {
        data.sun_direction = sun.back().as_vec3();
    }
}

struct ChunkSpec {
    centre: Vec2,
    chunk_m: f32,
    count: u32,
    width_scale: f32,
    shape: BladeShape,
    ring_reach_m: f32,
    seed: u32,
}

/// Bake `count` blades into one mesh, in world space, around `centre`.
fn build_chunk_mesh(spec: &ChunkSpec, terrain: Option<&TerrainAccess>) -> Mesh {
    let ChunkSpec {
        centre,
        chunk_m,
        count,
        width_scale,
        shape,
        ring_reach_m,
        seed,
    } = *spec;
    // Reserved from the shape rather than from the worst shape: a `Spike` chunk
    // asked to hold five vertices a blade allocates nearly twice what it fills,
    // and chunks are rebaked continuously as the grid rolls.
    let capacity = count as usize;
    let vertices = capacity * shape.vertices();
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(vertices);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(vertices);
    let mut blade_data: Vec<[f32; 2]> = Vec::with_capacity(vertices);
    let mut indices: Vec<u32> = Vec::with_capacity(capacity * shape.triangles() * 3);

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
        if shape == BladeShape::Spike {
            // One tip vertex on the blade's centre line instead of two corners.
            // The taper already pulls the top nearly to a point, so this removes
            // a corner rather than a feature — and it is the difference between
            // two triangles and one.
            positions.push([tip.x, tip.y, tip.z]);
        } else {
            positions.push([tip.x + side.x * taper, tip.y, tip.z + side.y * taper]);
            positions.push([tip.x - side.x * taper, tip.y, tip.z - side.y * taper]);
        }

        // `uv` is not a texture coordinate — nothing samples one. `y` is the
        // vertex's height along the blade (0 root, 1 tip); `x` is the ground
        // height under it, which is what lets the shader collapse a blade toward
        // its own root without a per-vertex blade height.
        uvs.push([base.y, 0.0]);
        uvs.push([base.y, 0.0]);
        uvs.push([base.y, 1.0]);
        if shape != BladeShape::Spike {
            uvs.push([base.y, 1.0]);
        }

        // `x` carries the blade's hash with the quad's *side* in its sign;
        // `y` packs the ring's reach (whole part) and the blade's height
        // (fraction), which `floor`/`fract` separate exactly.
        let tint = hash_unit(hash ^ 0x2545_f491);
        // `y` packs two numbers into one channel: the ring's reach in whole
        // metres and the blade's height in the fraction. A blade is never a
        // metre tall and a reach is always a whole number, so `floor` and
        // `fract` separate them exactly — which is cheaper than an attribute
        // nobody would otherwise need.
        let packed = ring_reach_m + height;
        blade_data.push([-tint, packed]);
        blade_data.push([tint, packed]);
        blade_data.push([tint, packed]);
        if shape != BladeShape::Spike {
            blade_data.push([-tint, packed]);
        }

        if shape == BladeShape::Notched {
            // The notched tip: a fifth vertex dipping between the two corners,
            // so the blade ends in two points instead of a flat cut. Up close a
            // straight-topped quad reads as a strip of paper — this is the one
            // shape cue that says "leaf" at arm's length, and it costs one
            // triangle on the only ring where anyone can see it.
            let notch = base.lerp(tip, TIP_NOTCH_DEPTH);
            positions.push([notch.x, notch.y, notch.z]);
            uvs.push([base.y, TIP_NOTCH_DEPTH]);
            blade_data.push([tint, packed]);
            let notch_index = first + 4;
            indices.extend_from_slice(&[first, first + 1, first + 2]);
            indices.extend_from_slice(&[first, first + 2, notch_index]);
            indices.extend_from_slice(&[first, notch_index, first + 3]);
        } else if shape == BladeShape::Quad {
            indices.extend_from_slice(&[first, first + 1, first + 2]);
            indices.extend_from_slice(&[first, first + 2, first + 3]);
        } else {
            indices.extend_from_slice(&[first, first + 1, first + 2]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, blade_data);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-ring chunk for the shape tests, so each of them names only what it
    /// actually cares about.
    fn spec(count: u32, shape: BladeShape, seed: u32) -> ChunkSpec {
        ChunkSpec {
            centre: Vec2::ZERO,
            chunk_m: 5.0,
            count,
            width_scale: 1.0,
            shape,
            ring_reach_m: 8.0,
            seed,
        }
    }

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
                ring_cells(index, focus, REFERENCE_REACH)
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
                ring.density >= needed * COVERAGE_MARGIN,
                "ring reaching {} m plants {}/m2 where {:.1}/m2 is the minimum at {inner} m",
                ring.reach_m,
                ring.density,
                needed * COVERAGE_MARGIN
            );
            // Deliberately no upper bound: one turned a density change into a
            // failing test that said nothing.
            // Deliberately no upper bound. There used to be one, pinning every
            // ring to its minimum plus a margin, and it did exactly what it was
            // told: the field covered the ground and looked like sparse spikes.
            // Surplus density is paid in overdraw and that is a real cost, but
            // it is one the measurement decides, not a test — this file's job is
            // to keep a ring from falling *below* what its distance demands.
            assert!(
                ring.width_scale <= MAX_WIDTH_SCALE,
                "a blade {}x its authored width reads as a card, not a blade",
                ring.width_scale
            );
        }
    }

    /// The whole point of the rolling grid: cost does not grow with the map.
    #[test]
    fn the_neighbourhood_costs_the_same_far_from_the_origin_as_near_it() {
        let declared = worst_case_blades();
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

    /// The declared cost is checked against its per-view ceiling — and its debt
    /// — in `perf::budget`, which is where the mobile budget lives. What this
    /// one keeps is the property that makes that number mean anything: the
    /// neighbourhood is bounded at all, so no camera position can make the
    /// meadow arbitrarily expensive.
    #[test]
    fn the_neighbourhood_is_bounded() {
        let blades = neighbourhood_blades(Vec2::ZERO);
        assert!(blades > 0, "a meadow with no blades is not a meadow");
        // El peor caso, por lo mismo que el conteo de briznas: la alineación
        // cómoda no es la que hay que aguantar.
        let period = RINGS
            .iter()
            .map(|ring| ring.chunk_m)
            .fold(0.0_f32, f32::max);
        let chunks: usize = (0..8)
            .flat_map(|z| (0..8).map(move |x| Vec2::new(x as f32, z as f32) * (period / 8.0)))
            .map(|focus| {
                (0..RINGS.len())
                    .map(|index| ring_cells(index, focus, REFERENCE_REACH).len())
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(0);
        assert!(
            chunks <= crate::perf::budget::MOBILE_DRAWS,
            "{chunks} chunks is over the {} draw budget before anything else draws",
            crate::perf::budget::MOBILE_DRAWS
        );
    }

    #[test]
    fn blades_are_deterministic_per_chunk() {
        // Same chunk, same blades: a field that reshuffles when it is rebuilt
        // makes every visual comparison worthless.
        let a = build_chunk_mesh(&spec(64, BladeShape::Quad, 11), None);
        let b = build_chunk_mesh(&spec(64, BladeShape::Quad, 11), None);
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
        let mesh = build_chunk_mesh(&spec(8, BladeShape::Quad, 5), None);
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
        let mesh = build_chunk_mesh(&spec(1, BladeShape::Quad, 5), None);
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

    /// The shader reads the blade hash out of the *sign* of `uv1.x`, so the two
    /// edges of a quad have to disagree in sign and agree in magnitude. Get this
    /// wrong and the normal bows the same way on both sides — which looks like
    /// nothing at all, and would be found by nobody.
    #[test]
    fn the_two_edges_of_a_blade_carry_the_same_hash_with_opposite_signs() {
        let mesh = build_chunk_mesh(&spec(1, BladeShape::Quad, 9), None);
        let bevy::mesh::VertexAttributeValues::Float32x2(data) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_1).expect("blade data")
        else {
            panic!("blade data must be Float32x2");
        };
        assert!(data[0][0] < 0.0 && data[1][0] > 0.0, "the edges disagree");
        assert!(
            (data[0][0].abs() - data[1][0].abs()).abs() < f32::EPSILON,
            "both edges belong to the same blade and share its hash"
        );
        for vertex in data {
            // `y` packs the ring's reach in the whole part and the blade's
            // height in the fraction — the shader splits them with floor/fract.
            let (reach, height) = (vertex[1].floor(), vertex[1].fract());
            assert_eq!(reach, 8.0, "the blade carries its own ring's reach");
            assert!(
                (BLADE_HEIGHT_MIN..=BLADE_HEIGHT_MAX).contains(&height),
                "every vertex carries its blade's height for the wind to scale"
            );
        }
    }

    /// The packing in `uv1.y` only works while a blade is under a metre.
    #[test]
    fn a_blade_stays_under_the_metre_its_packing_allows() {
        // Const block: the packing is a compile-time property of the constants,
        // so the build is the right place to lose, not the test run.
        const {
            assert!(
                BLADE_HEIGHT_MAX < 1.0,
                "blade height rides in the fraction of uv1.y, next to the ring reach"
            );
            assert!(BLADE_HEIGHT_MIN > 0.0 && BLADE_HEIGHT_MIN < BLADE_HEIGHT_MAX);
        }
        for ring in &RINGS {
            assert!(
                (ring.reach_m - ring.reach_m.round()).abs() < f32::EPSILON,
                "a ring's reach is the whole part of the same number, so it must \
                 be a whole number of metres — {} is not",
                ring.reach_m
            );
        }
        // And the same has to hold for every position of the reach dial, which
        // is why `ring_reach` rounds instead of multiplying straight through.
        for scale in bof_domain::perf::GRASS_REACH_STEPS {
            for index in 0..RINGS.len() {
                let reach = ring_reach(index, scale);
                assert!(
                    (reach - reach.round()).abs() < f32::EPSILON && reach >= 1.0,
                    "ring {index} at {scale}x reaches {reach} m, which does not pack"
                );
            }
        }
    }

    /// Shrinking the reach has to shrink the *field*, not just the number: if
    /// the dial moved the reach but the cells were chosen against the authored
    /// value, the sweep would report that reach costs nothing at all.
    #[test]
    fn the_reach_dial_actually_removes_chunks() {
        let focus = Vec2::new(3.7, -12.1);
        let cells = |scale: f32| {
            (0..RINGS.len())
                .map(|index| ring_cells(index, focus, scale).len())
                .sum::<usize>()
        };
        let full = cells(REFERENCE_REACH);
        for scale in bof_domain::perf::GRASS_REACH_STEPS {
            if scale < REFERENCE_REACH {
                assert!(
                    cells(scale) < full,
                    "the dial at {scale}x kept every chunk the full reach did"
                );
            }
        }
    }

    /// The near ring's blades end in two points; the outer ones do not, because
    /// out there the extra triangle buys a silhouette nobody can resolve.
    #[test]
    fn each_rung_of_the_ladder_costs_what_it_claims() {
        let triangles = |mesh: &Mesh| mesh.indices().map(|i| i.len() / 3).unwrap_or(0);
        for shape in [BladeShape::Notched, BladeShape::Quad, BladeShape::Spike] {
            let mesh = build_chunk_mesh(&spec(1, shape, 9), None);
            assert_eq!(triangles(&mesh), shape.triangles(), "{shape:?}");
            assert_eq!(mesh.count_vertices(), shape.vertices(), "{shape:?}");
        }
        // The ladder has to descend, or the ring order means nothing.
        let costs: Vec<usize> = RINGS.iter().map(|ring| ring.shape.triangles()).collect();
        assert!(
            costs.windows(2).all(|pair| pair[0] >= pair[1]),
            "a further ring may never cost more per blade: {costs:?}"
        );
        assert_eq!(
            RINGS[RINGS.len() - 1].shape,
            BladeShape::Spike,
            "the outermost ring sits on the floor of one triangle"
        );
    }

    #[test]
    fn a_blade_stands_on_the_ground_and_reaches_its_height() {
        let mesh = build_chunk_mesh(&spec(1, BladeShape::Quad, 3), None);
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
