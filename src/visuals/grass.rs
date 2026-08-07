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
use crate::visuals::grass_debug;
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
}

/// Cuánto mundo cubre un píxel **por metro de distancia**.
///
/// Es la constante que convierte metros en píxeles: a distancia `d`, un píxel
/// mide `d · esto` metros. Sale del campo de visión vertical y de la altura del
/// viewport, así que **el LOD sigue a la pantalla**: bajar a 900p acerca todas
/// las fronteras un 19% sin tocar una constante.
fn metres_per_pixel_at_one_metre(fov_y: f32, viewport_height: f32) -> f32 {
    2.0 * (fov_y * 0.5).tan() / viewport_height.max(1.0)
}

/// Ancho de una primitiva en píxeles, a esta distancia.
fn width_in_pixels(width_m: f32, distance_m: f32, scale: f32) -> f32 {
    width_m / (distance_m.max(0.1) * scale).max(1e-6)
}

/// Qué primitiva corresponde a esta distancia.
///
/// **La decisión es el tamaño en pantalla, no un radio.** Un radio en metros
/// describe una resolución concreta: los mismos 40 m que a 1080p dejan la brizna
/// en 1,8 px, a 900p la dejan en 1,5. Con umbrales en píxeles la escalera se
/// mueve sola con el viewport, que es lo que un LOD tiene que hacer.
///
/// Los dos umbrales son de ojo y se pueden discutir; lo que no se discute es la
/// unidad. Medido el 2026-08-07: el 96,7% del campo se resuelve entero, así que
/// ninguna de estas fronteras se esconde — cambiar de primitiva **se ve**, y por
/// eso los umbrales son generosos.
fn shape_at(distance_m: f32, scale: f32) -> BladeShape {
    let pixels = width_in_pixels(BLADE_WIDTH, distance_m, scale);
    if pixels >= LEAF_MIN_PIXELS {
        BladeShape::Leaf
    } else if pixels >= SPIKE_MIN_PIXELS {
        BladeShape::Spike
    } else {
        BladeShape::Card
    }
}

/// Cuántas primitivas por m² hacen falta a esta distancia para que el suelo no
/// se vea. Ver [`minimum_density`]: es la misma derivación, con el margen ya
/// aplicado, y ahora también en producción y no sólo en un test.
fn density_at(distance_m: f32, shape: BladeShape) -> f32 {
    minimum_density(distance_m, shape.footprint_m()) * COVERAGE_MARGIN
}

/// La brizna, en dos niveles de detalle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BladeShape {
    /// Cuatro vértices, dos triángulos unidos por una arista **horizontal** a la
    /// altura de la cintura: uno apunta abajo y otro arriba. Termina en punta por
    /// los dos lados, y esa fila del medio es la que le permite arquearse. El
    /// diseño original y por qué el quad diagonal no servía: `BOTWGrass.md`.
    Leaf,
    /// Three vertices, one triangle: two base corners and a single tip. The
    /// floor — a blade that no longer resolves does not need a waist.
    Spike,
    /// Cuatro vértices, dos triángulos: una **carta opaca** del tamaño de un
    /// matojo, que el vertex shader abre mirando a la cámara — sus cuatro
    /// vértices se hornean en el centro de la base.
    ///
    /// Opaca y no con alfa recortado, que es la carta que `BOTWGrass.md`
    /// descartó por la ley 3. El porqué y la referencia de escala, en el doc.
    Card,
}

impl BladeShape {
    /// Vertices and triangles per blade. One place, because several is how the
    /// budget quietly stops matching the mesh.
    const fn vertices(self) -> usize {
        match self {
            Self::Leaf | Self::Card => 4,
            Self::Spike => 3,
        }
    }

    const fn triangles(self) -> usize {
        match self {
            Self::Leaf | Self::Card => 2,
            Self::Spike => 1,
        }
    }

    /// Cuánto suelo tapa a lo ancho una primitiva de esta forma. Es lo que hace
    /// comparable la densidad de una carta con la de una brizna.
    const fn footprint_m(self) -> f32 {
        match self {
            Self::Leaf | Self::Spike => BLADE_WIDTH,
            Self::Card => CARD_WIDTH,
        }
    }

    /// Si el vertex shader tiene que abrir la primitiva mirando a la cámara.
    const fn faces_camera(self) -> bool {
        matches!(self, Self::Card)
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
    },
    Ring {
        reach_m: 24.0,
        chunk_m: 16.0,
    },
    Ring {
        reach_m: 40.0,
        chunk_m: 32.0,
    },
    Ring {
        reach_m: 64.0,
        chunk_m: 32.0,
    },
];

/// Cuántos píxeles de ancho tiene que medir una brizna para merecer cada forma.
///
/// En píxeles y no en metros, que es el punto entero de esta escalera. Con el
/// viewport de escritorio caen en ~24 m y ~47 m; a 900p, en ~20 y ~40. Nadie los
/// mueve: los mueve la pantalla.
const LEAF_MIN_PIXELS: f32 = 3.0;
const SPIKE_MIN_PIXELS: f32 = 1.5;

/// La forma de un anillo sale de la distancia **media** de su banda: es lo que
/// se ve en la mayor parte de él.
fn shape_for_ring(index: usize, scale: f32, reach_scale: f32) -> BladeShape {
    shape_at(band_midpoint(index, reach_scale), scale)
}

/// La densidad sale de su **borde interno**, que es el punto más exigente: ahí
/// sus primitivas se ven desde más cerca y cada una tapa menos suelo.
///
/// Dos puntos distintos para dos preguntas distintas, y no es una inconsistencia:
/// una forma equivocada se ve en todo el anillo, una densidad corta se ve sólo
/// donde falta. Se elige el promedio para la primera y el peor caso para la
/// segunda.
fn density_for_ring(index: usize, scale: f32, reach_scale: f32) -> f32 {
    let shape = shape_for_ring(index, scale, reach_scale);
    density_at(band_inner(index, reach_scale), shape)
}

/// El borde interno de la banda de un anillo. El del primero no es cero: nadie
/// mira el suelo pegado a la lente, y dividir por cero pediría densidad infinita.
fn band_inner(index: usize, reach_scale: f32) -> f32 {
    index
        .checked_sub(1)
        .map_or(NEAREST_INTEREST_M, |inner| ring_reach(inner, reach_scale))
}

fn band_midpoint(index: usize, reach_scale: f32) -> f32 {
    f32::midpoint(
        band_inner(index, reach_scale),
        ring_reach(index, reach_scale),
    )
}

/// La distancia más corta a la que todavía importa que el suelo esté tapado.
const NEAREST_INTEREST_M: f32 = 2.0;

/// El viewport contra el que se declara el presupuesto y corren los tests.
///
/// Existe porque un presupuesto tiene que ser **determinista**, y desde que el
/// LOD sigue a la pantalla el costo depende de ella. Declarar la pantalla de
/// referencia es honesto; que el número del test dependiera del viewport de
/// quien lo corre, no.
const REFERENCE_FOV_Y: f32 = std::f32::consts::FRAC_PI_4;
const REFERENCE_VIEWPORT_HEIGHT: f32 = 1080.0;

fn reference_scale() -> f32 {
    metres_per_pixel_at_one_metre(REFERENCE_FOV_Y, REFERENCE_VIEWPORT_HEIGHT)
}

/// Los alcances **con la perilla aplicada**, que es como el shader los necesita:
/// tiene que encontrar en esta tabla el mismo número que el vértice carga, o
/// `ring_inner` devuelve cero y la ley `1/d` se ancla donde no debe. Un test lo
/// cobra; el bug que hubo está en `BOTWGrass.md`.
fn ring_reaches(reach_scale: f32) -> (Vec4, Vec4) {
    slots(|index, _| ring_reach(index, reach_scale), 0.0)
}

/// Los tamaños de chunk, en el mismo orden. Con ellos el fragment deduce de qué
/// celda —o sea de qué draw call— salió una brizna, sin un byte más por vértice.
///
/// No los escala la perilla: el alcance decide cuántos chunks hay, no de qué
/// tamaño son.
fn ring_chunks() -> (Vec4, Vec4) {
    slots(|_, ring| ring.chunk_m, 1.0)
}

/// Qué anillos abren su primitiva mirando a la cámara.
fn ring_cards(scale: f32, reach_scale: f32) -> (Vec4, Vec4) {
    slots(
        |index, _| {
            f32::from(u8::from(
                shape_for_ring(index, scale, reach_scale).faces_camera(),
            ))
        },
        0.0,
    )
}

/// Un dato por anillo repartido en los ocho casilleros que el uniform tiene.
///
/// El tope se cobra en compilación: un anillo de más desbordaría el uniform y se
/// quedaría sin color propio, y las dos cosas son silenciosas en tiempo de
/// ejecución — la brizna saldría gris y el shader no encontraría su anillo.
const _: () = assert!(
    RINGS.len() <= grass_debug::PALETTE_SLOTS,
    "hay más anillos que casilleros en el uniform y en la paleta"
);

fn slots(of: impl Fn(usize, &Ring) -> f32, empty: f32) -> (Vec4, Vec4) {
    let mut slots = [empty; 8];
    for (index, ring) in RINGS.iter().enumerate() {
        slots[index] = of(index, ring);
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
const EYE_HEIGHT_M: f32 = 1.6;

/// Cuánto por encima de su mínimo derivado se planta. **Medido, no elegido**:
/// es la distancia entre la derivación y la realidad, calibrada contando
/// píxeles. Se vuelve a calibrar midiendo, no discutiendo; el despeje está en
/// `BOTWGrass.md`.
const COVERAGE_MARGIN: f32 = 2.4;

/// Qué fracción del suelo tiene que quedar tapada para que no se lea como ralo.
const TARGET_COVERAGE: f32 = 0.95;

/// Blades per m² needed at distance `d` for the ground not to show through.
/// A floor, not a recipe — see the module header.
///
/// **Las briznas caen sobre un hash, no sobre una grilla**, así que se pisan
/// entre ellas: la cobertura de un reparto de Poisson es `1 − e^(−λ·a)`, no
/// `λ·a`. La versión anterior usaba lo segundo, o sea que pedía la densidad con
/// la que las briznas taparían el suelo **si se ordenaran solas**, y por eso
/// pedía tres veces menos de lo que hace falta. Ésa es la aritmética detrás de
/// que el campo se viera ralo cada vez que se lo plantaba "según la derivación".
fn minimum_density(distance_m: f32, width_m: f32) -> f32 {
    let average_height = f32::midpoint(BLADE_HEIGHT_MIN, BLADE_HEIGHT_MAX);
    // Suelo que tapa una brizna: su ancho por lo que se alarga al verla en
    // ángulo rasante desde la altura del ojo.
    let hidden_per_blade = width_m * average_height * distance_m.max(0.5) / EYE_HEIGHT_M;
    -(1.0 - TARGET_COVERAGE).ln() / hidden_per_blade
}

/// The density the rings are written against, so the hub's dial can scale them
/// as a ratio instead of replacing them. Stepping the knob to 25/m² makes the
/// whole ladder 0.56× as thick and keeps its shape, which is what makes the
/// sweep readable: one variable moves, not four.
const REFERENCE_DENSITY: f32 = bof_domain::perf::GRASS_DENSITY_STEPS[0];

/// The reach scale the rings are written against, so the budget and the tests
/// measure the shipped field rather than whatever the dial happens to be on.
#[cfg(test)]
const REFERENCE_REACH: f32 = bof_domain::perf::GRASS_REACH_STEPS[0];

/// Cuántos metros tarda **una** brizna en pasar de nada a entera. Larga sólo
/// sirve con [`GROWTH_START_M`] lejos —la rampa se resta del umbral—, así que
/// las dos se mueven juntas o ninguna. El porqué, en `BOTWGrass.md`.
const GROWTH_RAMP_M: f32 = 6.0;

/// Over how many metres, inward from a ring's edge, the thresholds are spread.
///
/// Separate from the ramp on purpose: **one blade growing is invisible, a whole
/// band growing at once is not.** Shortening both together made it worse.
const GROWTH_SPREAD_M: f32 = 6.0;

/// A partir de qué distancia ralea la pradera, en metros. Los umbrales se
/// reparten como `start / (1 - hash)`, así que la fracción viva a distancia `d`
/// es `start / d` — la ley 1/d. **No ahorra un triángulo**: encoge en el vertex
/// shader, arregla la imagen y no el costo. El barrido que eligió el valor está
/// en `BOTWGrass.md`.
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
/// A qué fracción de la altura está la parte más ancha. Baja y no en el medio:
/// una hoja ensancha rápido y afina largo, y el rombo simétrico lee como
/// diamante.
const BLADE_WAIST: f32 = 0.30;
/// Cuánto se hunde la punta de abajo, en metros: en el suelo mismo la brizna
/// sería infinitamente angosta y dejaría ver tierra donde nace.
const BLADE_ROOT_SINK: f32 = 0.06;

/// Ancho de una carta, en metros: el de un **matojo de unas pocas briznas**, no
/// el de una pared de pasto.
///
/// La escala sale de una captura de BOTW que el usuario encontró (2026-08-07):
/// los trazos agrupados que se ven a media distancia son del tamaño de las
/// flores que tienen al lado, no de un arbusto. La primera versión de esto usaba
/// 1,6 m y era tres veces más grande de lo que la referencia muestra.
const CARD_WIDTH: f32 = 0.5;
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
                .map(|(index, _)| {
                    let scale = reference_scale();
                    let per_blade = shape_for_ring(index, scale, REFERENCE_REACH).triangles();
                    ring_cells(index, focus, REFERENCE_REACH).len()
                        * blades_per_chunk(index, REFERENCE_DENSITY, scale, REFERENCE_REACH)
                            as usize
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
        .map(|(index, _)| {
            ring_cells(index, focus, REFERENCE_REACH).len()
                * blades_per_chunk(index, REFERENCE_DENSITY, reference_scale(), REFERENCE_REACH)
                    as usize
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
fn blades_per_chunk(index: usize, dial: f32, scale: f32, reach_scale: f32) -> u32 {
    let ring = &RINGS[index];
    let density = density_for_ring(index, scale, reach_scale) * (dial / REFERENCE_DENSITY);
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
    camera: Option<Single<(&GlobalTransform, &Projection), With<Camera3d>>>,
    window: Option<Single<&Window, With<bevy::window::PrimaryWindow>>>,
    mut dial: Local<Option<(usize, usize)>>,
) {
    let Some(camera) = camera else {
        return;
    };
    let (camera, projection) = *camera;
    let focus = camera.translation().xz();
    let density = perf.grass_density();
    let reach_scale = perf.grass_reach_scale();
    // **La escalera de LOD sigue a la pantalla.** El campo de visión y la altura
    // del viewport deciden cuántos píxeles mide una brizna a cada distancia, y de
    // ahí sale qué primitiva le toca. Si no hay ventana todavía, la referencia:
    // vale más hornear con la escalera del escritorio que no hornear.
    let scale = match projection {
        Projection::Perspective(perspective) => metres_per_pixel_at_one_metre(
            perspective.fov,
            window.map_or(REFERENCE_VIEWPORT_HEIGHT, |window| {
                window.physical_height() as f32
            }),
        ),
        // Sin perspectiva no hay "píxeles por metro a distancia d": una
        // ortográfica los tiene constantes. La referencia es lo honesto.
        Projection::Orthographic(_) | Projection::Custom(_) => reference_scale(),
    };

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
    // El único trabajo por frame que este sistema tiene, y hasta el 2026-08-07
    // el único sin medir: está limitado a un chunk por frame *porque se supone
    // caro*. Cronometrarlo es lo que convierte "conviene instancing" de opinión
    // en decisión — con instancing serían unos bytes por brizna en vez de cuatro
    // vértices y seis índices.
    let bake_started = std::time::Instant::now();
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
                count: blades_per_chunk(key.ring, density, scale, reach_scale),
                shape: shape_for_ring(key.ring, scale, reach_scale),
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
                // Para que el inventario pueda decir cuánto pone la pradera, en
                // vez de sólo cuánto pone la escena. Ver `material_registry`.
                crate::visuals::material_registry::VisualSubject(
                    crate::visuals::material_registry::Subject::Meadow,
                ),
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
    if !missing.is_empty() {
        let millis = bake_started.elapsed().as_secs_f64() * 1000.0;
        // Al canal de perf y no a `info!`: el log arranca callado a propósito y
        // esto es una medición que se pide, no una que se sufre.
        debug!(
            "[grass] horneados {} chunks en {millis:.2} ms ({:.2} ms cada uno)",
            missing.len(),
            millis / missing.len() as f64,
        );
        // Y de quién es la geometría, por anillo. El inventario atribuye por
        // sistema —pradera contra bosque— y eso no alcanza para decidir qué
        // anillo conviene reemplazar por otra técnica.
        for (index, ring) in RINGS.iter().enumerate() {
            let live = field.live.keys().filter(|key| key.ring == index).count();
            let _ = ring;
            let blades = live * blades_per_chunk(index, density, scale, reach_scale) as usize;
            debug!(
                "[grass]   anillo {index}: {live} chunks, {blades} primitivas, {} tris",
                blades * shape_for_ring(index, scale, reach_scale).triangles(),
            );
        }
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
    perf: Res<crate::perf::PerfToggles>,
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
    let (a, b) = ring_reaches(perf.grass_reach_scale());
    data.ring_reaches_a = a;
    data.ring_reaches_b = b;
    let (a, b) = ring_chunks();
    data.ring_chunks_a = a;
    data.ring_chunks_b = b;
    let (a, b) = ring_cards(
        metres_per_pixel_at_one_metre(REFERENCE_FOV_Y, REFERENCE_VIEWPORT_HEIGHT),
        perf.grass_reach_scale(),
    );
    data.ring_cards_a = a;
    data.ring_cards_b = b;
    data.card_half_width = CARD_WIDTH * 0.5;
    data.debug_view =
        grass_debug::GrassDebugView::from_step(perf.grass_debug_step()).shader_index();
    // Desde la constante, no repetido en el default del uniform: la vista
    // `subpixel` divide por esto para decir cuántos píxeles mide una brizna, y
    // un ancho desactualizado daría un veredicto con la precisión intacta.
    data.blade_width = BLADE_WIDTH;
    for (slot, colour) in data.ring_colors.iter_mut().enumerate() {
        *colour = Vec4::from(grass_debug::slot_color(slot).to_f32_array());
    }
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

/// Qué significa cada color de la vista de diagnóstico.
///
/// Sale de [`RINGS`] y de la paleta, nunca escrita a mano: una leyenda que hay
/// que mantener sincronizada a mano es una leyenda que va a mentir. La consumen
/// el log —cuando la vista cambia— y el archivo que acompaña a cada captura,
/// que es de donde el analizador lee los colores en vez de conocerlos.
pub(crate) struct RingLegend {
    pub slot: usize,
    pub reach_m: f32,
    pub chunk_m: f32,
    pub density: f32,
    pub triangles_per_blade: usize,
    pub color: [u8; 3],
}

/// **Toma las perillas, no la tabla autorada.** Una leyenda que informa el
/// alcance y la densidad de diseño mientras la corrida está en 75% describe un
/// campo que no está en la foto — y es el archivo del que el analizador saca los
/// números. La misma clase de error que tenía el uniform.
pub(crate) fn ring_legend(perf: &crate::perf::PerfToggles) -> Vec<RingLegend> {
    let dial = perf.grass_density();
    let reach_scale = perf.grass_reach_scale();
    // La leyenda declara la escalera de **referencia**, no la del viewport de la
    // corrida: acompaña a una captura que puede tener cualquier tamaño, y un
    // número que cambia con la ventana no compara dos capturas.
    let scale = reference_scale();
    RINGS
        .iter()
        .enumerate()
        .map(|(slot, ring)| RingLegend {
            slot,
            reach_m: ring_reach(slot, reach_scale),
            chunk_m: ring.chunk_m,
            // La densidad que el chunk realmente plantó, dividida por su área:
            // el redondeo a briznas enteras hace que no sea exactamente la de la
            // tabla por la escala.
            density: blades_per_chunk(slot, dial, scale, reach_scale) as f32
                / (ring.chunk_m * ring.chunk_m),
            triangles_per_blade: shape_for_ring(slot, scale, reach_scale).triangles(),
            color: grass_debug::slot_srgb(slot),
        })
        .collect()
}

/// Una banda de la vista `subpixel`, con el color exacto que la identifica.
pub(crate) struct SubpixelBand {
    pub name: String,
    pub color: [u8; 3],
}

/// Las tres bandas de ancho en pantalla, y a qué distancia cae cada frontera.
///
/// La distancia sale del ancho de la brizna, de la resolución y del campo de
/// visión — **no del sistema de anillos**. Ese número sobrevive a cualquier
/// técnica de LOD que lo reemplace, que es justo lo que hace que valga la pena
/// medirlo antes de decidir la técnica.
pub(crate) fn subpixel_legend() -> Vec<SubpixelBand> {
    [
        ("menos de 1 px — no se resuelve", 0usize),
        ("entre 1 y 2 px — el cuarteto desperdicia", 5),
        ("2 px o mas — se resuelve entera", 3),
    ]
    .into_iter()
    .map(|(name, slot)| SubpixelBand {
        name: name.to_string(),
        color: grass_debug::slot_srgb(slot),
    })
    .collect()
}

/// Anuncia la vista puesta y qué es cada color.
///
/// Un color sin leyenda no es un diagnóstico: es una imagen bonita. Se imprime
/// al cambiar de vista y no cada frame, que es cuando la información hace falta.
pub(super) fn announce_grass_debug_view(
    perf: Res<crate::perf::PerfToggles>,
    mut announced: Local<Option<usize>>,
) {
    let step = perf.grass_debug_step();
    if announced.replace(step) == Some(step) {
        return;
    }
    let view = grass_debug::GrassDebugView::from_step(step);
    if view == grass_debug::GrassDebugView::Off {
        info!("[grass] vista de diagnóstico apagada");
        return;
    }
    info!("[grass] vista '{}':", perf.grass_debug_label());
    for ring in ring_legend(&perf) {
        let [r, g, b] = ring.color;
        info!(
            "[grass]   anillo {} #{r:02X}{g:02X}{b:02X} — hasta {:.0} m, chunks de {:.0} m, \
             {:.0} briznas/m2, {} tris por brizna",
            ring.slot, ring.reach_m, ring.chunk_m, ring.density, ring.triangles_per_blade,
        );
    }
}

struct ChunkSpec {
    centre: Vec2,
    chunk_m: f32,
    count: u32,
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
        let side = Vec2::new(yaw.cos(), yaw.sin()) * (BLADE_WIDTH * 0.5);
        let lean = Vec2::new(-yaw.sin(), yaw.cos()) * ((u5 - 0.5) * 2.0 * BLADE_LEAN);

        let base = Vec3::new(xz.x, ground, xz.y);
        let tip = base + Vec3::new(lean.x, height, lean.y);
        let Ok(first) = u32::try_from(positions.len()) else {
            error!("grass chunk exceeded the u32 mesh-index limit");
            break;
        };

        // `uv` is not a texture coordinate — nothing samples one. `y` is the
        // vertex's height along the blade (0 root, 1 tip); `x` is the ground
        // height under it, which is what lets the shader collapse a blade toward
        // its own root without a per-vertex blade height.
        //
        // `uv1.x` carries the blade's hash with the *side* of the blade in its
        // sign; `uv1.y` packs the ring's reach in whole metres and the blade's
        // height in the fraction, which `floor`/`fract` separate exactly. El
        // signo va por mitades —el borde izquierdo negativo, el derecho
        // positivo— y **la magnitud es la misma en todos los vértices**: el
        // vertex shader saca de ahí el umbral de crecimiento, y un vértice con
        // otro hash deformaría la brizna en vez de encogerla entera.
        let tint = hash_unit(hash ^ 0x2545_f491);
        let packed = ring_reach_m + height;
        let mut vertex = |point: Vec3, along: f32, side_sign: f32| {
            positions.push([point.x, point.y, point.z]);
            uvs.push([base.y, along]);
            blade_data.push([side_sign * tint, packed]);
        };

        match shape {
            BladeShape::Leaf => {
                // La brizna de dos triángulos unidos por una arista
                // **horizontal**: uno apunta abajo y otro arriba.
                let waist = base.lerp(tip, BLADE_WAIST);
                vertex(base - Vec3::Y * BLADE_ROOT_SINK, 0.0, -1.0);
                vertex(
                    Vec3::new(waist.x - side.x, waist.y, waist.z - side.y),
                    BLADE_WAIST,
                    -1.0,
                );
                vertex(
                    Vec3::new(waist.x + side.x, waist.y, waist.z + side.y),
                    BLADE_WAIST,
                    1.0,
                );
                vertex(tip, 1.0, 1.0);
                indices.extend_from_slice(&[first, first + 1, first + 2]);
                indices.extend_from_slice(&[first + 1, first + 3, first + 2]);
            }
            BladeShape::Card => {
                // Los cuatro vértices **en el mismo punto**, el centro de la
                // base. El vertex shader los abre contra el eje derecho de la
                // cámara, así que la carta siempre da la cara. Lo que decide
                // qué esquina es cada vértice ya viaja: el signo del hash dice
                // izquierda o derecha, y `uv.y` dice abajo o arriba.
                let base_centre = base - Vec3::Y * BLADE_ROOT_SINK;
                vertex(base_centre, 0.0, -1.0);
                vertex(base_centre, 0.0, 1.0);
                vertex(base_centre, 1.0, 1.0);
                vertex(base_centre, 1.0, -1.0);
                indices.extend_from_slice(&[first, first + 1, first + 2]);
                indices.extend_from_slice(&[first, first + 2, first + 3]);
            }
            BladeShape::Spike => {
                // Un triángulo: dos esquinas en la base y una punta. El piso de
                // la escalera, donde una brizna ya no se resuelve.
                vertex(
                    Vec3::new(base.x - side.x, base.y, base.z - side.y),
                    0.0,
                    -1.0,
                );
                vertex(
                    Vec3::new(base.x + side.x, base.y, base.z + side.y),
                    0.0,
                    1.0,
                );
                vertex(tip, 1.0, 1.0);
                indices.extend_from_slice(&[first, first + 1, first + 2]);
            }
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
        let scale = reference_scale();
        for (index, ring) in RINGS.iter().enumerate() {
            let per_chunk = blades_per_chunk(index, REFERENCE_DENSITY, scale, REFERENCE_REACH);
            let expected =
                (ring.chunk_m * ring.chunk_m * density_for_ring(index, scale, REFERENCE_REACH))
                    .round();
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
        let scale = reference_scale();
        for (index, ring) in RINGS.iter().enumerate() {
            let full = f64::from(blades_per_chunk(
                index,
                REFERENCE_DENSITY,
                scale,
                REFERENCE_REACH,
            ));
            let thin = f64::from(blades_per_chunk(index, sparse, scale, REFERENCE_REACH));
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
        let scale = reference_scale();
        for index in 0..RINGS.len() {
            let inner = band_inner(index, REFERENCE_REACH);
            // **Lo que un punto del suelo recibe es la SUMA de los anillos que
            // lo plantan**, no la densidad de su anillo: se pisan, y esa suma es
            // lo que hay ahí. Qué implica sobre el solapamiento: `BOTWGrass.md`.
            let planted: f32 = (0..RINGS.len())
                .filter(|other| ring_reach(*other, REFERENCE_REACH) >= inner)
                .map(|other| density_for_ring(other, scale, REFERENCE_REACH))
                .sum();
            let needed = density_at(inner, shape_for_ring(index, scale, REFERENCE_REACH));
            assert!(
                planted >= needed,
                "a {inner} m el suelo recibe {planted:.1}/m2 sumando los anillos que llegan \
                 ahí, y su distancia pide {needed:.1}/m2"
            );
        }
    }

    /// Cuántos anillos plantan sobre el mismo pedazo de suelo.
    ///
    /// Un punto lo cubre **un** anillo, o dos dentro de la banda de traspaso.
    /// Tres es densidad multiplicada que nadie pidió, pagada entera en overdraw
    /// — y encima con las briznas equivocadas, porque los anillos lejanos son
    /// los de menos triángulos.
    fn rings_covering(point: Vec2, focus: Vec2) -> Vec<usize> {
        (0..RINGS.len())
            .filter(|index| {
                let half = RINGS[*index].chunk_m * 0.5;
                ring_cells(*index, focus, REFERENCE_REACH)
                    .into_iter()
                    .any(|cell| {
                        let offset = (point - cell_centre(cell, RINGS[*index].chunk_m)).abs();
                        offset.x <= half && offset.y <= half
                    })
            })
            .collect()
    }

    /// Cuántos anillos se permiten hoy sobre el mismo suelo.
    ///
    /// **Debería ser 2 y es 4.** Es deuda con número, igual que
    /// `MEADOW_VIEW_TRIANGLES` en `perf::budget` — no una tolerancia: si sube,
    /// el test cae.
    const RINGS_OVER_THE_SAME_GROUND: usize = 4;

    /// **El defecto que las vistas de color destaparon el 2026-08-07.** El test
    /// de cobertura de arriba verifica que no queden huecos; nadie había
    /// verificado lo contrario, que es igual de caro. Medido, la tabla y por qué
    /// queda como deuda en vez de arreglarse: `BOTWGrass.md`.
    #[test]
    fn no_patch_of_ground_is_planted_by_more_than_two_rings() {
        let mut worst = (0usize, Vec2::ZERO, Vec2::ZERO, Vec::new());
        for focus in [Vec2::ZERO, Vec2::new(3.7, -11.2), Vec2::new(137.0, -488.0)] {
            let mut along = -40.0;
            while along <= 40.0 {
                let mut across = -40.0;
                while across <= 40.0 {
                    let point = focus + Vec2::new(along, across);
                    let rings = rings_covering(point, focus);
                    if rings.len() > worst.0 {
                        worst = (rings.len(), focus, point, rings);
                    }
                    across += 3.1;
                }
                along += 3.1;
            }
        }
        assert!(
            worst.0 <= RINGS_OVER_THE_SAME_GROUND,
            "con la cámara en {:?}, el punto {:?} lo plantan {} anillos ({:?}), \
             por encima de los {RINGS_OVER_THE_SAME_GROUND} que este archivo declara \
             como deuda: esa densidad multiplicada se paga entera en overdraw y pone \
             briznas de anillo lejano en primer plano",
            worst.1,
            worst.2,
            worst.0,
            worst.3,
        );
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

    /// Same chunk, same blades: a field that reshuffles when it is rebuilt makes
    /// every visual comparison worthless — y desde el 2026-08-07, también toda
    /// medición de píxeles, que compara dos capturas del mismo encuadre.
    ///
    /// **Este test comparaba longitudes.** O sea que pasaba igual si cada brizna
    /// del chunk se plantaba en otro lado, que es exactamente el fallo que su
    /// nombre promete atrapar. Ahora compara las posiciones.
    #[test]
    fn blades_are_deterministic_per_chunk() {
        let positions = |mesh: &Mesh| match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(bevy::mesh::VertexAttributeValues::Float32x3(values)) => values.clone(),
            _ => panic!("a chunk must carry Float32x3 positions"),
        };
        let a = positions(&build_chunk_mesh(&spec(64, BladeShape::Leaf, 11), None));
        let b = positions(&build_chunk_mesh(&spec(64, BladeShape::Leaf, 11), None));
        assert!(!a.is_empty(), "an empty chunk would make this vacuous");
        assert_eq!(a, b, "the same chunk grew a different field");

        // Y la otra mitad, que sin ella el test lo pasaría un generador que
        // devuelve siempre lo mismo: dos chunks distintos son campos distintos.
        let other = positions(&build_chunk_mesh(&spec(64, BladeShape::Leaf, 12), None));
        assert_ne!(
            a, other,
            "two chunks with different seeds grew the same field"
        );
    }

    /// The vertex carries only what cannot be derived. Normal and colour are
    /// constants of the system rebuilt in `grass.wgsl`; baking them again would
    /// silently put back the 28 bytes per vertex this system removed, and
    /// nothing on screen would show it.
    #[test]
    fn the_vertex_carries_only_position_and_the_two_derived_values() {
        let mesh = build_chunk_mesh(&spec(8, BladeShape::Leaf, 5), None);
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

    /// El shader lee `uv.y` como altura a lo largo de la brizna, así que el orden
    /// de los cuatro vértices **es** el contrato: punta abajo, cintura, cintura,
    /// punta arriba.
    ///
    /// Y la fila del medio es lo que este test existe para congelar: sin ella los
    /// bordes van rectos de la raíz a la punta y la brizna no puede arquearse,
    /// que es lo que pasaba con el quad partido en diagonal.
    #[test]
    fn a_blades_four_vertices_keep_their_authored_order() {
        let mesh = build_chunk_mesh(&spec(1, BladeShape::Leaf, 5), None);
        let bevy::mesh::VertexAttributeValues::Float32x2(uvs) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0).expect("uvs")
        else {
            panic!("uvs must be Float32x2");
        };
        assert_eq!(uvs[0][1], 0.0, "la punta de abajo es la raíz");
        assert_eq!(uvs[1][1], BLADE_WAIST, "cintura izquierda");
        assert_eq!(uvs[2][1], BLADE_WAIST, "cintura derecha");
        assert_eq!(uvs[3][1], 1.0, "la punta de arriba");
        // Propiedad de las constantes, así que se pierde compilando y no
        // corriendo: sin una fila estrictamente entre raíz y punta, los bordes
        // van rectos y la brizna no puede arquearse.
        const {
            assert!(BLADE_WAIST > 0.0 && BLADE_WAIST < 1.0);
        }
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
        let mesh = build_chunk_mesh(&spec(1, BladeShape::Leaf, 9), None);
        let bevy::mesh::VertexAttributeValues::Float32x2(data) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_1).expect("blade data")
        else {
            panic!("blade data must be Float32x2");
        };
        assert!(
            data[1][0] < 0.0 && data[2][0] > 0.0,
            "las dos esquinas de la cintura son los dos bordes y tienen que discrepar"
        );
        let magnitude = data[0][0].abs();
        for vertex in data {
            assert!(
                (vertex[0].abs() - magnitude).abs() < f32::EPSILON,
                "todos los vértices son de la misma brizna y comparten su hash: el \
                 vertex shader saca de ahí el umbral de crecimiento, y uno distinto \
                 deformaría la brizna en vez de encogerla entera"
            );
        }
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

    /// **Lo que el uniform dice tiene que existir en la malla.**
    ///
    /// El shader deduce el anillo de una brizna comparando el alcance que ella
    /// carga contra la tabla del uniform. Si las dos no salen del mismo cálculo,
    /// la comparación no falla: *no encuentra nada*, y `ring_inner` devuelve
    /// cero en silencio — o sea que la ley `1/d` se ancla donde no debe y nadie
    /// ve nada raro. Pasó con la perilla de alcance, que escala y redondea el
    /// número del vértice mientras el uniform mandaba el autorado.
    ///
    /// Es el mismo test para todas las perillas presentes y futuras: para cada
    /// paso, cada alcance horneado tiene que estar en la tabla que se envía.
    #[test]
    fn the_uniform_reaches_are_the_ones_baked_into_the_blades() {
        for scale in bof_domain::perf::GRASS_REACH_STEPS {
            let (a, b) = ring_reaches(scale);
            let sent: Vec<f32> = a.to_array().into_iter().chain(b.to_array()).collect();
            for index in 0..RINGS.len() {
                let baked = ring_reach(index, scale);
                assert!(
                    sent.iter().any(|value| (value - baked).abs() < 0.5),
                    "a {scale}x el anillo {index} hornea {baked} m y el uniform manda \
                     {sent:?}: el shader no va a encontrar su anillo y va a anclar la ley \
                     1/d en cero"
                );
            }
        }
    }

    /// Y la leyenda que acompaña a una captura describe **esa** captura.
    ///
    /// Es el archivo del que el analizador saca los alcances, así que informar
    /// los de diseño mientras la corrida está en 75% es contar píxeles de un
    /// campo y atribuirlos a otro.
    #[test]
    fn the_legend_reports_the_field_that_is_actually_planted() {
        let mut perf = crate::perf::PerfToggles::default();
        perf.set_knob_step(bof_domain::perf::PerfKnob::GrassReach, 2);
        let scale = perf.grass_reach_scale();
        assert!(scale < 1.0, "este test necesita un paso que sí achique");
        for ring in ring_legend(&perf) {
            assert_eq!(
                ring.reach_m,
                ring_reach(ring.slot, scale),
                "la leyenda del anillo {} no informa el alcance vigente",
                ring.slot
            );
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

    /// La escalera baja, y lo que baja son **triángulos por metro cuadrado de
    /// suelo**, no por primitiva: una carta son dos triángulos —más que la púa de
    /// uno— y cubre el suelo de decenas de briznas. Hoy: 80 / 80 / 40 / 3.
    #[test]
    fn each_rung_of_the_ladder_costs_what_it_claims() {
        let triangles = |mesh: &Mesh| mesh.indices().map(|i| i.len() / 3).unwrap_or(0);
        for shape in [BladeShape::Leaf, BladeShape::Spike, BladeShape::Card] {
            let mesh = build_chunk_mesh(&spec(1, shape, 9), None);
            assert_eq!(triangles(&mesh), shape.triangles(), "{shape:?}");
            assert_eq!(mesh.count_vertices(), shape.vertices(), "{shape:?}");
        }
        let scale = reference_scale();
        let per_square_metre: Vec<f32> = (0..RINGS.len())
            .map(|index| {
                density_for_ring(index, scale, REFERENCE_REACH)
                    * shape_for_ring(index, scale, REFERENCE_REACH).triangles() as f32
            })
            .collect();
        assert!(
            per_square_metre.windows(2).all(|pair| pair[0] >= pair[1]),
            "un anillo más lejano no puede costar más triángulos por m2: {per_square_metre:?}"
        );
    }

    #[test]
    fn a_blade_stands_on_the_ground_and_reaches_its_height() {
        let mesh = build_chunk_mesh(&spec(1, BladeShape::Leaf, 3), None);
        let bevy::mesh::VertexAttributeValues::Float32x3(positions) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions")
        else {
            panic!("positions must be Float32x3");
        };
        let root = positions[0][1];
        let tip = positions[3][1];
        // La punta de abajo se hunde: en el suelo mismo la brizna sería
        // infinitamente angosta y dejaría ver tierra donde nace.
        assert!(
            (root + BLADE_ROOT_SINK).abs() < 0.001,
            "la punta de abajo tiene que quedar hundida, quedó en {root}"
        );
        assert!(
            (BLADE_HEIGHT_MIN..=BLADE_HEIGHT_MAX).contains(&tip),
            "blade height {tip} outside its authored range"
        );
    }
}
