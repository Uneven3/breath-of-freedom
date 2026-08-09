//! Pradera rodante de presentación: una brizna no es una entidad.

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::mesh::MeshTag;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;

use crate::visuals::grass_cover;
use crate::visuals::grass_debug;
use crate::visuals::grass_material::{GrassExtension, GrassMaterial, GrassUniform};
use crate::visuals::grass_records::{RECORD_BYTES, RingRecords, blade_record, ring_index_mesh};
use crate::visuals::grass_tiles;
use crate::world::TerrainAccess;

/// One LOD ring. `reach_m` is a Chebyshev radius; seams read as sparse grass
/// over the terrain tint rather than as holes.
struct Ring {
    reach_m: f32,
    chunk_m: f32,
}

/// Metros cubiertos por un píxel y por metro de distancia, derivados de FOV y
/// viewport para que el LOD siga a la pantalla.
fn metres_per_pixel_at_one_metre(fov_y: f32, viewport_height: f32) -> f32 {
    2.0 * (fov_y * 0.5).tan() / viewport_height.max(1.0)
}

/// Ancho de una primitiva en píxeles, a esta distancia.
fn width_in_pixels(width_m: f32, distance_m: f32, scale: f32) -> f32 {
    width_m / (distance_m.max(0.1) * scale).max(1e-6)
}

/// Experimento 2026-08-09, detalle en `BOTWGrass.md` → *Por dónde retomar*:
/// saca la carta de la simulación sin sacarla del código. `true` vuelve a lo
/// de antes.
const CARDS_ENABLED: bool = false;

/// Primitiva para esta distancia, elegida por tamaño en pantalla y no por un
/// radio atado a una resolución. Los umbrales visuales viven en `BOTWGrass.md`.
fn shape_at(distance_m: f32, scale: f32) -> BladeShape {
    let pixels = width_in_pixels(BLADE_WIDTH, distance_m, scale);
    if pixels >= LEAF_MIN_PIXELS {
        BladeShape::Leaf
    } else if pixels >= SPIKE_MIN_PIXELS || !CARDS_ENABLED {
        BladeShape::Spike
    } else {
        BladeShape::Card
    }
}

/// Cuántas primitivas por m² hacen falta a esta distancia para que el suelo no
/// se vea.
///
/// **Sin margen desde el 2026-08-07**: era 2,4, y era el parche de un error de
/// 2,83× en la huella de la brizna. Con la huella medida, la derivación pide
/// directamente lo que la imagen entrega — ver [`minimum_density`].
fn density_at(distance_m: f32, shape: BladeShape) -> f32 {
    minimum_density(distance_m, shape)
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
    /// Dos triángulos del tamaño de un matojo que el vertex shader abre mirando
    /// a la cámara, y que el fragment recorta en una silueta de puntas.
    Card,
}

impl BladeShape {
    /// Cuánto suelo tapa a lo ancho una primitiva, que es lo que hace comparable
    /// la densidad de una carta con la de una brizna. La carta declara **lo que
    /// su silueta conserva** y no su ancho a secas: desde que recorta puntas no
    /// es un rectángulo lleno, y ignorarlo planta la mitad de lo que hace falta.
    const fn footprint_m(self) -> f32 {
        match self {
            Self::Leaf | Self::Spike => BLADE_WIDTH,
            Self::Card => CARD_WIDTH * CARD_SILHOUETTE_AREA,
        }
    }

    /// Si el vertex shader tiene que abrir la primitiva mirando a la cámara.
    const fn faces_camera(self) -> bool {
        matches!(self, Self::Card)
    }

    /// El número con que el shader la reconoce. Un test lo cobra contra las
    /// constantes `SHAPE_*` de `grass.wgsl`, que es lo único que las ata.
    const fn shader_index(self) -> u32 {
        match self {
            Self::Leaf => 0,
            Self::Spike => 1,
            Self::Card => 2,
        }
    }
}

/// Los tres niveles, de la cámara hacia afuera: **uno por forma de brizna.**
///
/// Eran cuatro y los dos primeros tenían la misma forma, así que el segundo sólo
/// aportaba una frontera — *"hay muchos anillos"*, jugando el 2026-08-08. Un
/// nivel es un **tamaño de chunk y una forma**, y llega hasta donde su forma
/// llega; el costo de eso quedó declarado como deuda en `BOTWGrass.md`.
const RINGS: [Ring; 3] = [
    Ring {
        reach_m: 24.0,
        chunk_m: 12.0,
    },
    Ring {
        reach_m: 40.0,
        chunk_m: 16.0,
    },
    Ring {
        reach_m: 64.0,
        chunk_m: 32.0,
    },
];

/// Triángulos enviados por brizna. La púa degenera el segundo en el shader, pero
/// el presupuesto cuenta primitivas enviadas igual que el censo de la malla.
const SUBMITTED_TRIANGLES_PER_BLADE: usize = 2;

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

/// Cuántas briznas por m² tienen que estar **vivas** a esta distancia.
///
/// La misma ley que `density_for_ring` evalúa en el borde de una banda, pero
/// como función continua de la distancia: es lo que `grass_tiles` invierte para
/// darle a cada brizna su propio alcance. La perilla la escala como razón, igual
/// que a la de los anillos, para que el barrido siga moviendo una sola variable.
pub(super) fn live_density_at(distance_m: f32, dial: f32, scale: f32) -> f32 {
    density_at(distance_m, shape_at(distance_m, scale)) * (dial / REFERENCE_DENSITY)
}

/// Hasta dónde llega la pradera entera, que es donde termina el último nivel.
pub(super) fn farthest_reach(reach_scale: f32) -> f32 {
    ring_reach(RINGS.len() - 1, reach_scale)
}

/// Cuántas briznas por m² hay **realmente vivas** a esta distancia — no las que
/// la ley pide, que es `live_density_at`. La diferencia es lo que hace falta para
/// medir la huella real: despejarla con el número del dial da una huella que
/// absorbe el raleo, y así estuvo sobreestimada hasta el 2026-08-08.
pub(crate) fn live_blades_per_m2(distance_m: f32, dial: f32, reach_scale: f32) -> f32 {
    // La escalera de **referencia**, igual que `ring_facts`: el número acompaña a
    // una captura de cualquier tamaño, y uno que cambiara con la ventana no
    // compara dos corridas.
    let scale = reference_scale();
    let ladder = grass_tiles::reach_ladder(dial, scale, reach_scale);
    let alive: usize = tile_ranges(dial, scale, reach_scale)
        .iter()
        .enumerate()
        .filter(|(index, _)| ring_reach(*index, reach_scale) >= distance_m)
        .map(|(_, range)| {
            ladder
                .get(range.start as usize..range.end as usize)
                .map_or(0, |tramo| {
                    tramo
                        .iter()
                        .filter(|reach| reach.floor() >= distance_m)
                        .count()
                })
        })
        .sum();
    alive as f32 / grass_tiles::TILE_AREA_M2
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
pub(super) const NEAREST_INTEREST_M: f32 = 2.0;

/// El viewport contra el que se declara el presupuesto y corren los tests.
///
/// Existe porque un presupuesto tiene que ser **determinista**, y desde que el
/// LOD sigue a la pantalla el costo depende de ella. Declarar la pantalla de
/// referencia es honesto; que el número del test dependiera del viewport de
/// quien lo corre, no.
const REFERENCE_FOV_Y: f32 = std::f32::consts::FRAC_PI_4;
const REFERENCE_VIEWPORT_HEIGHT: f32 = 1080.0;

pub(super) fn reference_scale() -> f32 {
    metres_per_pixel_at_one_metre(REFERENCE_FOV_Y, REFERENCE_VIEWPORT_HEIGHT)
}

/// Los alcances **con la perilla aplicada**. Desde que la brizna lleva el suyo,
/// el shader ya no los busca: quedan como declaración de la corrida.
fn ring_reaches(reach_scale: f32) -> (Vec4, Vec4) {
    slots(|index, _| ring_reach(index, reach_scale), 0.0)
}

/// Los tamaños de chunk, en el mismo orden: con ellos el fragment deduce de qué
/// celda salió una brizna. No los escala la perilla de alcance, que decide
/// cuántos chunks hay y no de qué tamaño son.
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

/// Un dato por anillo en los ocho casilleros del uniform. El tope se cobra en
/// compilación: uno de más desbordaría en silencio.
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

/// Cuánto suelo tapa una primitiva, **por metro de ancho y metro de distancia**,
/// medido a cada distancia y no supuesto constante.
///
/// *(a, 2026-08-08)* Despejado de `C = 1 − e^(−λ·a)` con la densidad **viva** de
/// cada banda. Un solo número —0,149— pedía 1,8× menos de lo necesario cerca y
/// de más lejos. Se interpola, y los puntos son los centros de las bandas del
/// medidor: `BOF_SHOT_SWEEP=grass-density` vuelve a sacarla entera.
const HIDDEN_BY_DISTANCE: [(f32, f32); 7] = [
    (3.5, 0.082),
    (5.0, 0.085),
    (7.0, 0.093),
    (9.5, 0.109),
    (13.5, 0.112),
    (19.0, 0.109),
    (27.0, 0.114),
];

/// Y el de la carta, que es otra primitiva: medido 0,185 en 45-64 m contra su
/// propia huella. **Mayor que el de una brizna**, así que con el número viejo la
/// pradera plantaba cartas de más justo donde menos se notan.
const HIDDEN_PER_WIDTH_PER_METRE_CARD: f32 = 0.185;

fn hidden_per_width_per_metre(distance_m: f32, shape: BladeShape) -> f32 {
    if matches!(shape, BladeShape::Card) {
        return HIDDEN_PER_WIDTH_PER_METRE_CARD;
    }
    let first = HIDDEN_BY_DISTANCE[0];
    let last = HIDDEN_BY_DISTANCE[HIDDEN_BY_DISTANCE.len() - 1];
    if distance_m <= first.0 {
        return first.1;
    }
    HIDDEN_BY_DISTANCE
        .windows(2)
        .find(|pair| distance_m <= pair[1].0)
        .map_or(last.1, |pair| {
            let across = (distance_m - pair[0].0) / (pair[1].0 - pair[0].0);
            pair[0].1 + across * (pair[1].1 - pair[0].1)
        })
}

/// Qué fracción del suelo tiene que quedar tapada para que no se lea como ralo.
const TARGET_COVERAGE: f32 = 0.95;

/// Blades per m² needed at distance `d` for the ground not to show through.
/// A floor, not a recipe — see the module header.
///
/// **Las briznas caen sobre un hash, no sobre una grilla**, así que la cobertura
/// es `1 − e^(−λ·a)` y no `λ·a`. Esa forma quedó verificada midiendo (Paso 0);
/// lo que estaba mal era `a`.
fn minimum_density(distance_m: f32, shape: BladeShape) -> f32 {
    let distance_m = distance_m.max(0.5);
    let hidden_per_blade =
        shape.footprint_m() * distance_m * hidden_per_width_per_metre(distance_m, shape);
    -(1.0 - TARGET_COVERAGE).ln() / hidden_per_blade
}

/// The density the rings are written against, so the hub's dial can scale them
/// as a ratio instead of replacing them. Stepping the knob to 25/m² makes the
/// whole ladder 0.56× as thick and keeps its shape, which is what makes the
/// sweep readable: one variable moves, not four.
const REFERENCE_DENSITY: f32 = bof_domain::perf::GRASS_DENSITY_STEPS[0];

/// The reach scale the rings are written against, so the budget and the tests
/// measure the shipped field rather than whatever the dial happens to be on.
///
/// Lo usa además el armado de materiales, que corre una vez al entrar a la
/// escena: qué forma tiene un nivel decide si su material recorta alfa, y esa
/// pregunta no puede depender de dónde quedó una perilla.
const REFERENCE_REACH: f32 = bof_domain::perf::GRASS_REACH_STEPS[0];

/// Una distancia en metros, como la lleva el uniform: entera y no negativa.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "reaches are small positive metres, clamped before the cast"
)]
fn metres_as_u32(metres: f32) -> u32 {
    metres.clamp(0.0, 1_000.0) as u32
}

/// La rampa del crecimiento, **con la perilla aplicada**. Vive en `bof_domain`
/// porque lo que gobierna sólo se ve caminando: lo elige el usuario barriéndolo,
/// no una captura (`BOTWGrass.md`).
fn growth_band(perf: &crate::perf::PerfToggles) -> f32 {
    perf.grass_growth()
}

/// Desde qué distancia una brizna se abre como carta.
///
/// **Es el mismo umbral que elige la forma, no un número aparte:** la carta
/// representa la masa de un matojo y sólo tiene sentido donde una brizna ya no
/// se resuelve. Más cerca se construye angosta — un billboard de medio metro a
/// tres metros gira con la cámara, reportado jugando el 2026-08-08.
fn card_from_m(scale: f32) -> f32 {
    // Despejado de `width_in_pixels(BLADE_WIDTH, d, scale) = SPIKE_MIN_PIXELS`.
    BLADE_WIDTH / (SPIKE_MIN_PIXELS * scale).max(1e-6)
}

/// Y desde dónde pierde la cintura. **Los dos umbrales son los mismos que
/// `shape_at` usa**: la forma la decide la pantalla, ahora también por brizna y
/// no sólo por nivel.
fn spike_from_m(scale: f32) -> f32 {
    BLADE_WIDTH / (LEAF_MIN_PIXELS * scale).max(1e-6)
}

/// Hasta cuánto **bajo** el suelo colapsa una brizna. No cero, y ahí está todo:
/// al ras queda coplanar con el terreno y hace z-fighting, que en pantalla es el
/// parpadeo que costó tres diagnósticos equivocados.
const GROWTH_SINK_M: f32 = 0.18;

/// Como mucho un chunk se rehace por frame **mientras la grilla rueda**: cruzar
/// una frontera cuesta un chunk, no un anillo.
const CHUNKS_BAKED_PER_FRAME: usize = 1;

/// Filling an empty grid ignores the per-frame limit and bakes the lot in one
/// frame: a scene that starts with a hole in the meadow is worse than one hitch.
const FILL_IN_ONE_FRAME: bool = true;

/// Blade shape. Wide enough at the base to cover ground, tapered at the tip so
/// it reads as a leaf rather than a strip of paper.
pub(crate) const BLADE_WIDTH: f32 = 0.055;
/// A qué fracción de la altura está la parte más ancha. Baja y no en el medio:
/// una hoja ensancha rápido y afina largo, y el rombo simétrico lee como
/// diamante.
const BLADE_WAIST: f32 = 0.30;
/// Cuánto se hunde la punta de abajo, en metros: en el suelo mismo la brizna
/// sería infinitamente angosta y dejaría ver tierra donde nace.
const BLADE_ROOT_SINK: f32 = 0.06;

/// Ancho de una carta, en metros: el de un **matojo de unas pocas briznas**, no
/// el de una pared. La escala salió de una captura de BOTW; ver `BOTWGrass.md`.
///
/// **Bajado de 0,5 el 2026-08-08 y no cuesta un triángulo:** la ley pide el doble
/// de cartas de la mitad de huella. Lo que cambia es el grano.
const CARD_WIDTH: f32 = 0.25;

/// Qué fracción de su rectángulo conserva la carta al recortar su silueta:
/// la integral de `card_silhouette` en `grass.wgsl`.
///
/// **Vive en dos lados, y es deuda declarada**: cambiar los dientes allá sin
/// tocar esto deja las cartas ralas. La red es medir — con la fracción mal, la
/// banda de 45-64 m no llega al 99% en `grass-view=medir`, que es como se
/// encontró que hacía falta.
const CARD_SILHOUETTE_AREA: f32 = 0.583;
/// Blade height range in metres. Knee to hip on a 1,8 m capsule.
///
/// **The ceiling is one metre and it is hard**: the height travels in the
/// fraction of `uv1.y` with the reach in the whole part. A test pins it. Subidas
/// el 2026-08-08 pidiendo *"el pasto un poquito más largo"* jugando.
const BLADE_HEIGHT_MIN: f32 = 0.55;
const BLADE_HEIGHT_MAX: f32 = 0.96;
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

/// Un chunk de la pradera: una entidad, un casillero del buffer de su nivel.
#[derive(Component)]
pub(super) struct GrassChunk;

/// Which chunk of which ring. The ring is part of the identity because the same
/// patch of ground is covered by different chunks at different distances.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ChunkKey {
    ring: usize,
    cell: IVec2,
}

/// La grilla viva: qué chunks existen ahora y lo que cada nivel tiene en la GPU.
///
/// **Un material por nivel**: cada uno lleva su stride y su forma, y el de las
/// cartas además `AlphaMode::Mask`. No cuesta draws — es lo que los junta.
#[derive(Resource)]
pub(super) struct GrassField {
    materials: [Handle<GrassMaterial>; RINGS.len()],
    records: [RingRecords; RINGS.len()],
    live: HashMap<ChunkKey, Entity>,
}

/// Triángulos que la pradera declara a la escena, para `perf::budget`.
/// Declarados, no dibujados: el frustum descarta buena parte y cuánta es una
/// incógnita, no una medición.
#[cfg(test)]
pub(crate) fn meadow_triangles() -> usize {
    // La malla índice reserva dos para todas las formas. Que la púa degenere uno
    // después del vertex shader no lo borra de la geometría enviada.
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
                    ring_cells(index, focus, REFERENCE_REACH).len()
                        * blades_per_chunk(index, REFERENCE_DENSITY, scale, REFERENCE_REACH)
                            as usize
                        * SUBMITTED_TRIANGLES_PER_BLADE
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

/// Qué tramo de la secuencia de una baldosa lleva cada nivel.
///
/// **Cada nivel lleva las briznas vivas en su banda, y eso es un prefijo:** la
/// escalera baja, así que las que llegan a su borde interno son las primeras. Un
/// nivel es un **superconjunto** del que sigue, así que al cruzar una frontera la
/// brizna no se reemplaza: la dibuja el otro, en el mismo lugar. De ahí que los
/// niveles puedan ser **coronas** y no discos.
fn tile_ranges(dial: f32, scale: f32, reach_scale: f32) -> Vec<std::ops::Range<u32>> {
    let ladder = grass_tiles::reach_ladder(dial, scale, reach_scale);
    let total = u32::try_from(ladder.len()).unwrap_or(u32::MAX);
    (0..RINGS.len())
        .map(|index| {
            let inner = band_inner(index, reach_scale);
            let last = ladder
                .iter()
                .position(|blade_reach| blade_reach.floor() < inner)
                .and_then(|end| u32::try_from(end).ok())
                .unwrap_or(total);
            0..last.min(total)
        })
        .collect()
}

/// Cuántas baldosas de mundo entran en un chunk de este nivel, por lado.
///
/// Entero por construcción: los lados de chunk (8, 16, 32) son múltiplos del
/// lado de la baldosa. Si dejaran de serlo, un chunk cubriría media baldosa y
/// dos chunks plantarían la misma brizna.
fn tiles_per_chunk_side(index: usize) -> u32 {
    tiles_per_side(RINGS[index].chunk_m)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "chunk sides are small multiples of the tile side by construction"
)]
fn tiles_per_side(chunk_m: f32) -> u32 {
    u32::try_from((chunk_m / grass_tiles::TILE_M).round().max(1.0) as i64).unwrap_or(1)
}

/// Blades in one chunk of `ring` at a given dial setting. Rounded once, here, so
/// the count on screen and the count in the budget are the same number.
fn blades_per_chunk(index: usize, dial: f32, scale: f32, reach_scale: f32) -> u32 {
    let per_tile = tile_ranges(dial, scale, reach_scale)
        .get(index)
        .map_or(0, |range| range.end - range.start);
    let side = tiles_per_chunk_side(index);
    per_tile.saturating_mul(side).saturating_mul(side)
}

/// Qué celdas de un nivel existen con la cámara en `focus`. **Decide qué se
/// tiene en memoria, no qué se ve**: desde que la brizna se descarta por su
/// propia distancia (`blade_growth` en `grass.wgsl`), esto es un test conservador
/// —se conserva el chunk que pueda tener *alguna* brizna viva—. Cuando decidía la
/// imagen, el campo aparecía y desaparecía en cuadrados de 32 m.
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
            // **Euclídeas, no Chebyshev**: el shader mide con `length()`, y esto
            // sólo puede descartar un chunk cuyas briznas ya estén *todas*
            // muertas para él. La esquina de un cuadrado está a √2 de su lado, y
            // de ahí salían chunks que se iban con briznas vivas adentro y un
            // borde de anillo que se veía cuadrado.
            let nearest = (offset - Vec2::splat(half)).max(Vec2::ZERO).length();
            let farthest = (offset + Vec2::splat(half)).length();
            // El borde interno de la corona: desde que los niveles se anidan, el
            // de adentro dibuja *las mismas briznas* hasta su alcance, así que
            // este no tiene nada que hacer ahí. Sólo el `slack` de histéresis.
            let handover = (inner_reach - slack).max(0.0);
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
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    // Un buffer y un material por nivel. El buffer arranca con un registro de
    // relleno porque un `ShaderBuffer` vacío no es un binding válido, y el
    // material no puede declararlo opcional: el macro de `AsBindGroup` no pasa
    // por `Option`.
    let records = std::array::from_fn(|_| {
        // `default()` y no `RENDER_WORLD`: con este último Bevy suelta el dato de
        // CPU en cuanto lo sube, y este buffer se reescribe cada vez que la
        // grilla rueda.
        RingRecords::new(buffers.add(ShaderBuffer::new(
            &[0_u8; RECORD_BYTES],
            RenderAssetUsages::default(),
        )))
    });
    let materials = std::array::from_fn(|ring| {
        let mut material = grass_material();
        material.extension.blade_records = records[ring].buffer.clone();
        if shape_for_ring(ring, reference_scale(), REFERENCE_REACH).faces_camera() {
            // El umbral no importa —el shader ya descartó con `discard` y lo que
            // queda sale opaco—; lo que compra es que Bevy no trate el draw como
            // opaco. Sólo las cartas recortan silueta, y el `discard` cuesta el
            // early-Z del draw que lo usa.
            material.base.alpha_mode = AlphaMode::Mask(0.5);
        }
        materials.add(material)
    });
    commands.insert_resource(GrassField {
        materials,
        records,
        live: HashMap::default(),
    });
}

/// Sube al GPU lo que el rodado dejó escrito, un buffer por nivel.
///
/// Separado del rodado porque `Assets<ShaderBuffer>` es otro recurso, y porque
/// una escritura por chunk sería una subida por chunk: acá es una por nivel y
/// sólo en los frames en que algo se movió.
pub(super) fn upload_meadow_records(
    mut field: ResMut<GrassField>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut memory: ResMut<MeadowRecordMemory>,
) {
    if !field.is_changed() {
        return;
    }
    for ring in &mut field.records {
        ring.upload(&mut buffers);
    }
    memory.bytes = field.records.iter().map(RingRecords::buffer_bytes).sum();
    memory.chunks = field.records.iter().map(RingRecords::chunks).sum();
}

/// Lo que la pradera tiene en buffers de registros. **El inventario de la escena
/// cuenta mallas y no `ShaderBuffer`s**, así que sin esto una corrida declararía
/// como caída lo que el Paso 2 sólo mudó de una cosa a la otra.
#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct MeadowRecordMemory {
    pub bytes: usize,
    pub chunks: usize,
}

/// La caja de un chunk, en mundo. El `Transform` es identidad, así que su
/// espacio local es el del mundo y este AABB vale tal cual.
/// **La altura sale del terreno que se muestreó, no de cero**: fija entre `−1` y
/// `1,9` sólo valía sobre suelo plano, y con relieve el chunk se descartaba con
/// el jugador mirándolo. El margen cubre lo que el vertex shader agrega después
/// —raíz hundida, punta inclinada, carta abriéndose contra la cámara—.
fn chunk_bounds(centre: Vec2, chunk_m: f32, ground: std::ops::RangeInclusive<f32>) -> Aabb {
    let half = chunk_m * 0.5 + CHUNK_BOUNDS_MARGIN_M;
    Aabb::from_min_max(
        Vec3::new(
            centre.x - half,
            ground.start() - GROWTH_SINK_M - BLADE_ROOT_SINK - CHUNK_BOUNDS_MARGIN_M,
            centre.y - half,
        ),
        Vec3::new(
            centre.x + half,
            ground.end() + BLADE_HEIGHT_MAX + BLADE_LEAN + CHUNK_BOUNDS_MARGIN_M,
            centre.y + half,
        ),
    )
}

/// Cuánto se agranda la caja de un chunk sobre lo que su contenido pide.
///
/// Media carta, que es lo que más se aparta de su base al abrirse contra la
/// cámara — y de paso deja **holgura**, que una caja de culling ajustada al
/// último bit descarta geometría por un error de redondeo.
const CHUNK_BOUNDS_MARGIN_M: f32 = CARD_WIDTH * 0.5;

/// Si esta corrida planta este anillo. Con uno aislado la foto mide cuánta
/// cobertura **aporta** ese nivel solo — lo que `medir` sobre el campo entero no
/// puede decir, porque ahí cada píxel lo gana uno y el de atrás tapaba igual.
fn planted_ring(perf: &crate::perf::PerfToggles, ring: usize) -> bool {
    perf.grass_only_ring().is_none_or(|only| only == ring)
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
            // El campo lo llena `init_meadow_material`, que es quien tiene el
            // `Assets<ShaderBuffer>`.
            blade_records: Handle::default(),
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
    reason = "rolling owns ECS entities, mesh assets, field state and terrain sampling"
)]
pub(super) fn roll_meadow_grid(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut field: ResMut<GrassField>,
    perf: Res<crate::perf::PerfToggles>,
    scene: Res<State<crate::scene::AppState>>,
    terrain: TerrainAccess,
    camera: Option<Single<&GlobalTransform, With<Camera3d>>>,
    mut dial: Local<Option<(usize, usize, usize)>>,
) {
    let Some(camera) = camera else {
        return;
    };
    let focus = camera.translation().xz();
    let density = perf.grass_density();
    let reach_scale = perf.grass_reach_scale();
    // El layout de registros queda en la referencia: cambiarlo con cada paso del
    // zoom invalidaba el stride sin tirar los chunks ya horneados. La forma sí
    // sigue al viewport, pero lo hace enteramente en el shader; ver
    // `track_meadow_focus`. Así el FOV no mezcla casilleros de dos layouts.
    let scale = reference_scale();

    // Densidad, alcance y anillos cambian **cuántas briznas** tiene un chunk, y
    // de eso depende la malla índice del nivel entero. Son el único evento que
    // tira la grilla en vez de rodarla, y se miran juntos porque una corrida que
    // cambiara dos y reconstruyera por uno dejaría medio campo describiendo la
    // perilla vieja.
    let dials = (
        perf.grass_density_step,
        perf.grass_reach_step,
        perf.grass_rings_step,
    );
    if dial.replace(dials) != Some(dials) {
        for entity in field.live.values() {
            commands.entity(*entity).despawn();
        }
        field.live.clear();
        // La malla índice tiene tantos vértices como briznas lleva un chunk, así
        // que una perilla que cambia esa cuenta la invalida entera — igual que
        // invalida los chunks horneados.
        for ring in &mut field.records {
            ring.reset();
        }
    }

    // Una malla índice por nivel, creada una vez por configuración. Es lo que
    // hace que sus chunks batcheen: Bevy exige el mismo `Handle<Mesh>`, y todos
    // los chunks de un nivel llevan la misma cantidad de briznas.
    for ring in 0..RINGS.len() {
        let blades = blades_per_chunk(ring, density, scale, reach_scale);
        if field.records[ring].mesh.is_none() && planted_ring(&perf, ring) && blades > 0 {
            // **Dos triángulos para todos, no el de la forma del nivel.** La
            // forma la decide la distancia, y con un solo triángulo indexado una
            // brizna cercana de un nivel de púas salía **media hoja**. La púa no
            // paga el segundo: sus esquinas 2 y 3 caen en la punta y degenera.
            field.records[ring].mesh =
                Some(meshes.add(ring_index_mesh(blades, SUBMITTED_TRIANGLES_PER_BLADE)));
            field.records[ring].stride = blades;
        }
    }

    let planted = |ring: usize| planted_ring(&perf, ring);

    let wanted: HashSet<ChunkKey> = RINGS
        .iter()
        .enumerate()
        .filter(|(ring, _)| planted(*ring))
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
        .filter(|(ring, _)| planted(*ring))
        .flat_map(|(ring, _)| {
            ring_cells_with_slack(ring, focus, KEEP_SLACK_M, reach_scale)
                .into_iter()
                .map(move |cell| ChunkKey { ring, cell })
        })
        .collect();

    let mut dropped: Vec<ChunkKey> = Vec::new();
    field.live.retain(|key, entity| {
        let keep = keep_set.contains(key);
        if !keep {
            commands.entity(*entity).despawn();
            dropped.push(*key);
        }
        keep
    });
    for key in dropped {
        field.records[key.ring].release(key.cell);
    }

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
    // El único trabajo por frame que este sistema tiene. Desde el Paso 2 no
    // hornea geometría: sortea las briznas y escribe sus registros, que es lo que
    // convirtió "conviene instancing" de opinión en decisión.
    let bake_started = std::time::Instant::now();
    // La escalera de alcances y el reparto de índices son del **campo**, no del
    // chunk: se arman una vez por tanda en vez de una por chunk, que es lo único
    // que este rediseño le agrega al horneado.
    let ladder = std::sync::Arc::new(grass_tiles::reach_ladder(density, scale, reach_scale));
    let ranges = tile_ranges(density, scale, reach_scale);
    for key in &missing {
        let ring = &RINGS[key.ring];
        let centre = cell_centre(key.cell, ring.chunk_m);
        let planting = build_chunk_records(
            &ChunkSpec {
                centre,
                chunk_m: ring.chunk_m,
                blades: ranges.get(key.ring).cloned().unwrap_or(0..0),
                ladder: std::sync::Arc::clone(&ladder),
            },
            Some(&terrain),
        );
        let slot = field.records[key.ring].slot_for(key.cell);
        field.records[key.ring].write(slot, &planting.records);
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
                Mesh3d(field.records[key.ring].mesh.clone().unwrap_or_default()),
                MeshTag(slot),
                MeshMaterial3d(field.materials[key.ring].clone()),
                // **El AABB va a mano, y con `NoAutoAabb`.** Bevy lo deriva de
                // las posiciones de la malla, que en una malla índice son todas
                // cero — un punto en el origen, y el nivel entero culleado. Y no
                // alcanza con insertarlo: `calculate_bounds` lo *sobrescribe*
                // cuando `Mesh3d` cambia, cosa que pasa en todo chunk recién
                // nacido, así que hace falta además marcarlo.
                chunk_bounds(centre, ring.chunk_m, planting.ground.clone()),
                NoAutoAabb,
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
                blades * SUBMITTED_TRIANGLES_PER_BLADE,
            );
        }
    }
}

/// Tell the shader where the camera is, so the outermost blades can shrink
/// before their chunk disappears.
///
/// Un material por nivel significa **tres** escrituras de uniform por frame, no
/// una por chunk. Casi el mismo valor: lo que los separa es el modo de alfa y el
/// `record_stride`.
pub(super) fn track_meadow_focus(
    field: Res<GrassField>,
    mut materials: ResMut<Assets<GrassMaterial>>,
    camera: Option<Single<(&GlobalTransform, &Projection), With<Camera3d>>>,
    window: Option<Single<&Window, With<bevy::window::PrimaryWindow>>>,
    sun: Option<Single<&GlobalTransform, With<DirectionalLight>>>,
    perf: Res<crate::perf::PerfToggles>,
    time: Res<Time>,
) {
    // **El reparto del buffer se escribe aunque no haya cámara** (2026-08-07).
    // Con `record_layout` en su default, todo chunk lee el casillero 0 y las
    // cartas se construyen como hojas: el nivel lejano desaparece, y no por un
    // frame sino hasta la siguiente escritura. El caso y su síntoma, en
    // `BOTWGrass.md`.
    let viewport_height = window.map_or(REFERENCE_VIEWPORT_HEIGHT, |window| {
        window.physical_height() as f32
    });
    let scale = camera.as_ref().map_or(reference_scale(), |camera| {
        let (_, projection) = **camera;
        match projection {
            Projection::Perspective(perspective) => {
                metres_per_pixel_at_one_metre(perspective.fov, viewport_height)
            }
            Projection::Orthographic(_) | Projection::Custom(_) => reference_scale(),
        }
    });
    let reach_scale = perf.grass_reach_scale();
    let layouts: Vec<UVec4> = (0..RINGS.len())
        .map(|ring| {
            UVec4::new(
                field.records[ring].stride,
                shape_for_ring(ring, scale, reach_scale).shader_index(),
                // **Qué nivel es, del material y no de una tabla.** Desde que
                // cada brizna lleva su propio alcance, buscar el nivel entre los
                // alcances devuelve "ninguno" y las vistas de diagnóstico pintan
                // todo gris — o sea el medidor deja de contar por nivel. El draw
                // ya sabía cuál es; es el mismo error que `ring_is_card` cerró.
                u32::try_from(ring).unwrap_or(0),
                // Y desde dónde empieza su corona: más cerca que esto, la misma
                // brizna la dibuja el nivel de adentro, así que ésta no. En
                // metros enteros, como el alcance, porque el shader los compara
                // contra una distancia y no necesita más resolución.
                metres_as_u32(band_inner(ring, reach_scale)),
            )
        })
        .collect();
    let data = camera.as_ref().map(|camera| {
        let (camera, _) = **camera;
        meadow_uniform(
            camera,
            sun.as_deref().map(|sun| &**sun),
            &perf,
            &time,
            scale,
        )
    });
    for (ring, handle) in field.materials.iter().enumerate() {
        if let Some(mut material) = materials.get_mut(handle) {
            if let Some(data) = &data {
                material.extension.grass_data = GrassUniform { ..*data };
            }
            material.extension.grass_data.record_layout = layouts[ring];
            material.base.alpha_mode = if shape_for_ring(ring, scale, reach_scale).faces_camera() {
                AlphaMode::Mask(0.5)
            } else {
                AlphaMode::Opaque
            };
        }
    }
}

/// El uniform de la pradera, armado una vez para los dos materiales: separado
/// del sistema para que no haya forma de escribir uno y olvidar el otro.
fn meadow_uniform(
    camera: &GlobalTransform,
    sun: Option<&GlobalTransform>,
    perf: &crate::perf::PerfToggles,
    time: &Time,
    screen_scale: f32,
) -> GrassUniform {
    let mut uniform = grass_material().extension.grass_data;
    let data = &mut uniform;
    data.focus_xz = camera.translation().xz();
    data.growth_ramp = growth_band(perf);
    data.spike_from_m = spike_from_m(screen_scale);
    data.card_from_m = card_from_m(screen_scale);
    let (a, b) = ring_reaches(perf.grass_reach_scale());
    data.ring_reaches_a = a;
    data.ring_reaches_b = b;
    let (a, b) = ring_chunks();
    data.ring_chunks_a = a;
    data.ring_chunks_b = b;
    let (a, b) = ring_cards(screen_scale, perf.grass_reach_scale());
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
    data.blade_root_sink = BLADE_ROOT_SINK;
    data.blade_lean = BLADE_LEAN;
    data.blade_waist = BLADE_WAIST;
    // The wind is a function of world position and time — there is no per-blade
    // state anywhere, which is why a field of a hundred thousand blades costs
    // one uniform write a frame.
    data.time = time.elapsed_secs_wrapped();
    // Backlit transmission needs to know where the sun is; reading the light's
    // own transform rather than a copy keeps day/night driving it for free.
    if let Some(sun) = sun {
        data.sun_direction = sun.back().as_vec3();
    }
    uniform
}

/// Lo que la pradera **plantó**, por anillo. Sin colores ni formato: el color lo
/// pone `grass_debug`, que es de quien es la paleta.
///
/// **Toma las perillas, no la tabla autorada.** Informar el alcance y la densidad
/// de diseño mientras la corrida está en otra cosa describe un campo que no está
/// en la foto — y de acá salen los números que el analizador lee.
pub(super) struct RingFacts {
    pub reach_m: f32,
    pub chunk_m: f32,
    pub density: f32,
    pub triangles_per_blade: usize,
    pub planted: bool,
}

pub(super) fn ring_facts(perf: &crate::perf::PerfToggles) -> Vec<RingFacts> {
    let dial = perf.grass_density();
    let reach_scale = perf.grass_reach_scale();
    // La escalera de **referencia**, no la del viewport de la corrida: acompaña a
    // una captura de cualquier tamaño, y un número que cambia con la ventana no
    // compara dos capturas.
    let scale = reference_scale();
    RINGS
        .iter()
        .enumerate()
        .map(|(slot, ring)| RingFacts {
            reach_m: ring_reach(slot, reach_scale),
            chunk_m: ring.chunk_m,
            // Lo que el chunk plantó dividido por su área: el redondeo a briznas
            // enteras la aparta un poco de la tabla.
            density: blades_per_chunk(slot, dial, scale, reach_scale) as f32
                / (ring.chunk_m * ring.chunk_m),
            triangles_per_blade: SUBMITTED_TRIANGLES_PER_BLADE,
            planted: perf.grass_only_ring().is_none_or(|only| only == slot),
        })
        .collect()
}

/// Lo que hace falta para sortear las briznas de un chunk. **Sin forma**: la
/// forma la construye el vertex shader desde el registro, y acá sólo se decide
/// dónde nace cada brizna y cuánto mide.
struct ChunkSpec {
    centre: Vec2,
    chunk_m: f32,
    /// Qué tramo de la secuencia de cada baldosa le toca a este nivel.
    blades: std::ops::Range<u32>,
    /// Hasta dónde llega cada índice de la secuencia. La comparte todo el campo:
    /// es la ley de densidad invertida, no una propiedad del chunk.
    ladder: std::sync::Arc<Vec<f32>>,
}

/// Los registros de un chunk, en el orden en que el shader los indexa.
///
/// **Las briznas ya no se sortean dentro del chunk: se leen del mundo.** El
/// chunk recorre sus baldosas y pide de cada una las briznas de su tramo, así
/// que la misma brizna sale igual la plante quien la plante (`grass_tiles`).
///
/// **Las filtradas no se saltan: se emiten con altura cero.** El casillero es un
/// rango de stride fijo, así que saltear una correría de lugar a las siguientes.
fn build_chunk_records(spec: &ChunkSpec, terrain: Option<&TerrainAccess>) -> ChunkPlanting {
    let ChunkSpec {
        centre,
        chunk_m,
        ref blades,
        ref ladder,
    } = *spec;
    let corner = centre - Vec2::splat(chunk_m * 0.5);
    let first_tile = grass_tiles::tile_at(corner + Vec2::splat(grass_tiles::TILE_M * 0.5));
    let side = tiles_per_side(chunk_m);
    let per_tile = blades.end - blades.start;
    let mut records = Vec::with_capacity(
        usize::try_from(per_tile.saturating_mul(side).saturating_mul(side)).unwrap_or(0),
    );
    let side = i32::try_from(side).unwrap_or(1);
    let mut lowest = f32::MAX;
    let mut highest = f32::MIN;
    for row in 0..side {
        for column in 0..side {
            let tile = first_tile + IVec2::new(column, row);
            for index in blades.clone() {
                let blade = grass_tiles::blade_in_tile(tile, index);
                let xz = blade.xz;
                let ground = terrain.and_then(|t| t.height_at(xz)).unwrap_or(0.0);
                let slope = terrain.and_then(|t| t.slope_deg_at(xz)).unwrap_or(0.0);
                let kind = terrain
                    .and_then(|t| t.kind_at(xz))
                    .unwrap_or(crate::world::TerrainKind::Soil);
                let cover = grass_cover::coverage(kind, slope);
                let height = (BLADE_HEIGHT_MIN
                    + blade.height_unit * (BLADE_HEIGHT_MAX - BLADE_HEIGHT_MIN))
                    * cover;
                // **El alcance es de la brizna, no del anillo**, y viaja en la
                // parte entera igual que antes. Es lo que le saca al shader la
                // ley `1/d` y el hash: cada brizna muere donde su índice dice.
                let reach = ladder
                    .get(index as usize)
                    .copied()
                    .unwrap_or(0.0)
                    .floor()
                    .max(1.0);
                records.push(blade_record(xz, ground, reach + height));
                lowest = lowest.min(ground);
                highest = highest.max(ground);
            }
        }
    }
    // Un chunk sin briznas —todo roca, o densidad cero— no tiene rango que
    // informar; su caja vale lo que valga, porque no va a dibujar nada.
    if records.is_empty() {
        lowest = 0.0;
        highest = 0.0;
    }
    ChunkPlanting {
        records,
        ground: lowest..=highest,
    }
}

/// Lo que sale de sortear un chunk: sus registros y **hasta dónde llega el suelo
/// bajo ellos**, que es lo que su caja de culling necesita saber.
struct ChunkPlanting {
    records: Vec<[f32; 4]>,
    ground: std::ops::RangeInclusive<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un chunk de prueba: dos baldosas por lado y un tramo de la secuencia,
    /// para que cada test nombre sólo lo que le importa.
    fn spec(blades: std::ops::Range<u32>, reach_m: f32) -> ChunkSpec {
        ChunkSpec {
            centre: Vec2::ZERO,
            chunk_m: grass_tiles::TILE_M * 2.0,
            ladder: std::sync::Arc::new(vec![reach_m; blades.end as usize]),
            blades,
        }
    }

    /// El campo tiene que ser **determinista**: el mismo suelo crece las mismas
    /// briznas cada sesión, o caminar y volver reordena la pradera y comparar
    /// dos capturas del mismo encuadre deja de valer. Compara los registros y no
    /// sus longitudes: contar pasaba igual con cada brizna en otro lado.
    #[test]
    fn blades_are_deterministic_per_patch_of_ground() {
        let a = build_chunk_records(&spec(0..16, 8.0), None).records;
        let b = build_chunk_records(&spec(0..16, 8.0), None).records;
        assert!(!a.is_empty(), "un chunk vacío volvería esto vacuo");
        assert_eq!(a, b, "el mismo suelo creció otro campo");
        // Y la otra mitad, que sin ella lo pasaría un generador constante.
        let mut elsewhere = spec(0..16, 8.0);
        elsewhere.centre = Vec2::splat(400.0);
        assert_ne!(
            a,
            build_chunk_records(&elsewhere, None).records,
            "dos pedazos de suelo distintos dieron lo mismo"
        );
    }

    /// **La propiedad del rediseño, del lado del horneado**: dos niveles que
    /// pisan el mismo suelo tienen que hablar de las mismas briznas. El nivel
    /// lejano lleva un prefijo del tramo del cercano, así que sus registros
    /// aparecen **idénticos** entre los del otro — no parecidos, iguales.
    #[test]
    fn two_levels_over_the_same_ground_plant_the_same_blades() {
        let far = build_chunk_records(&spec(0..4, 40.0), None).records;
        let near = build_chunk_records(&spec(0..16, 40.0), None).records;
        for record in &far {
            assert!(
                near.contains(record),
                "el nivel lejano plantó una brizna que el cercano no tiene: {record:?}",
            );
        }
    }

    /// Un registro son **cuatro números y nada más**: base en XZ, suelo, y el
    /// alcance con la altura empaquetada. Cualquier cosa que se agregue acá
    /// multiplica por la cantidad de briznas del campo, que es el costo que este
    /// paso vino a bajar.
    #[test]
    fn a_record_is_four_numbers_and_lands_inside_its_chunk() {
        let spec = spec(0..16, 8.0);
        for record in build_chunk_records(&spec, None).records {
            assert_eq!(record.len() * 4, RECORD_BYTES);
            let half = spec.chunk_m * 0.5;
            assert!(
                (record[0] - spec.centre.x).abs() <= half
                    && (record[1] - spec.centre.y).abs() <= half,
                "una brizna nació fuera de su chunk: {record:?}",
            );
            assert_eq!(record[2], 0.0, "el terreno de prueba es plano en y = 0");
        }
    }

    /// **El techo de un metro es duro**: la altura viaja en la fracción de un
    /// número que lleva el alcance de la brizna en su parte entera, así que una
    /// brizna de 1,2 m se leería como otro alcance. `floor`/`fract` tienen que
    /// separarlos exactos.
    #[test]
    fn the_packed_reach_and_height_survive_floor_and_fract() {
        let spec = spec(0..64, 13.0);
        for record in build_chunk_records(&spec, None).records {
            let packed = record[3];
            assert_eq!(packed.floor(), 13.0, "el alcance no vuelve entero");
            let height = packed.fract();
            assert!(
                (0.0..=BLADE_HEIGHT_MAX).contains(&height),
                "altura fuera de rango: {height}",
            );
        }
        const {
            assert!(BLADE_HEIGHT_MAX < 1.0);
        }
    }

    /// **La caja de un chunk tiene que contener lo que ese chunk planta.**
    ///
    /// Lo único que sostiene el culling desde que la malla índice no tiene
    /// posiciones. Falla en silencio de la peor manera —el chunk desaparece con
    /// el jugador mirándolo— y su primera versión era correcta sólo sobre suelo
    /// plano, que es el de la caja Pasto: ninguna captura lo mostró.
    #[test]
    fn a_chunks_bounds_contain_every_blade_it_plants() {
        for ground in [0.0_f32, 7.5, -12.25] {
            let spec = spec(0..64, 13.0);
            let planting = build_chunk_records(&spec, None);
            // Sin terreno el sorteo planta todo a cero; correrlo entero es lo
            // que simula un chunk sobre una ladera.
            let raised: Vec<[f32; 4]> = planting
                .records
                .iter()
                .map(|r| [r[0], r[1], r[2] + ground, r[3]])
                .collect();
            let bounds = chunk_bounds(spec.centre, spec.chunk_m, ground..=ground);
            let (min, max) = (bounds.min(), bounds.max());
            for record in raised {
                let (x, z, base) = (record[0], record[1], record[2]);
                let tip = base + record[3].fract();
                assert!(
                    x >= min.x && x <= max.x && z >= min.z && z <= max.z,
                    "una brizna nace fuera de la caja de su chunk: {record:?}",
                );
                assert!(
                    base - GROWTH_SINK_M - BLADE_ROOT_SINK >= min.y && tip <= max.y,
                    "la caja no cubre la altura de la brizna: base {base}, punta {tip}",
                );
            }
        }
    }

    /// **El índice de cada forma está escrito en los dos lados**, y nada en el
    /// tipo los ata: desincronizarlos no da error, da una brizna con otra forma.
    /// Esto lee el WGSL y los compara.
    #[test]
    fn the_shader_agrees_on_the_numbers_it_shares_with_this_module() {
        let wgsl = std::fs::read_to_string("assets/shaders/grass.wgsl")
            .expect("el shader de la pradera tiene que estar donde el material lo pide");
        for (shape, name) in [
            (BladeShape::Leaf, "SHAPE_LEAF"),
            (BladeShape::Spike, "SHAPE_SPIKE"),
            (BladeShape::Card, "SHAPE_CARD"),
        ] {
            let declared = format!("const {name}: u32 = {}u;", shape.shader_index());
            assert!(
                wgsl.contains(&declared),
                "el shader no declara `{declared}` — su `{name}` y el de este \
                 módulo dejaron de ser el mismo número",
            );
        }
        // La cintura y la inclinación no se comparan porque **no están
        // duplicadas**: viajan por uniform desde este módulo, que es la única
        // forma de que no puedan discrepar.
        assert!(
            !wgsl.contains("const BLADE_WAIST"),
            "la cintura volvió a estar escrita en el shader: mandala por uniform",
        );
    }

    #[test]
    fn the_density_knob_is_what_actually_lands_on_the_ground() {
        // The failure this system was built to fix: a density that reads well in
        // a constant but arrives on screen divided by twenty. Lo que tiene que
        // llegar intacto es el tramo del nivel más cercano: es la baldosa
        // entera, porque su banda empieza donde la ley se evalúa.
        let scale = reference_scale();
        for dial in [REFERENCE_DENSITY, bof_domain::perf::GRASS_DENSITY_STEPS[2]] {
            let ranges = tile_ranges(dial, scale, REFERENCE_REACH);
            let expected =
                grass_tiles::blades_in_tile(live_density_at(NEAREST_INTEREST_M, dial, scale));
            assert_eq!(
                ranges[0].end, expected,
                "con la perilla en {dial} la baldosa entera no es la que la ley pide"
            );
            // **Anidados, no partidos**: cada nivel es un prefijo del anterior, y
            // por eso la misma brizna pasa de uno a otro al cruzar la frontera en
            // vez de ser reemplazada.
            for pair in ranges.windows(2) {
                assert!(
                    pair[1].end <= pair[0].end && pair[1].start == 0,
                    "los niveles dejaron de anidar: {pair:?}"
                );
            }
        }
    }

    /// The dial scales the ladder instead of flattening it, or the sweep would
    /// be measuring a different shape at every step. **Sobre el campo y no sobre
    /// cada nivel**: al ralear, el alcance de cada índice se estira y las briznas
    /// migran de nivel. Lo que la perilla conserva es cuántas hay.
    #[test]
    fn the_dial_scales_the_whole_field_by_the_same_ratio() {
        let scale = reference_scale();
        // El tramo del nivel más cercano, que es la baldosa entera: sumar los
        // tres contaría dos veces a las que dos niveles comparten desde que se
        // anidan.
        let per_tile =
            |dial: f32| -> f64 { f64::from(tile_ranges(dial, scale, REFERENCE_REACH)[0].end) };
        let full = per_tile(REFERENCE_DENSITY);
        for sparse in [
            bof_domain::perf::GRASS_DENSITY_STEPS[2],
            bof_domain::perf::GRASS_DENSITY_STEPS[6],
        ] {
            let ratio = f64::from(sparse / REFERENCE_DENSITY);
            // Media brizna de cada lado: las dos cuentas redondean a entero, y la
            // del baseline arrastra su medio error multiplicado por la razón.
            let slack = 0.5 * (1.0 + ratio);
            assert!(
                (per_tile(sparse) - full * ratio).abs() <= slack,
                "con la perilla en {sparse} el campo no sigue la razón"
            );
        }
    }

    /// Todo lo que está **dentro del alcance** cae en algún chunk — y alcance
    /// quiere decir **círculo**, desde que el selector mide en euclídeas. Las
    /// esquinas del cuadrado de 64 m están a 89 m y ningún nivel las prometió.
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
                if Vec2::new(along, across).length() > outermost {
                    across += 1.7;
                    continue;
                }
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

    /// **La pradera lee la perilla, no una constante.** Sin esto el paso se puede
    /// mover en el hub y el campo no cambiar, que es la clase de silencio que
    /// obliga a repetir una sesión de juego entera para descubrirlo.
    #[test]
    fn the_growth_band_follows_the_knob() {
        let mut perf = crate::perf::PerfToggles::default();
        let first = growth_band(&perf);
        perf.set_knob_step(bof_domain::perf::PerfKnob::GrassGrowth, 2);
        assert_ne!(growth_band(&perf), first);
        assert_eq!(growth_band(&perf), bof_domain::perf::GRASS_GROWTH_STEPS[2]);
    }

    /// **Ningún nivel se queda sin chunk dentro de su corona.** Es el contrato
    /// con `blade_growth`: allá la brizna se apaga antes del borde interno de su
    /// nivel porque la dibuja el de adentro, así que acá el territorio tiene que
    /// llegar hasta esa misma línea. Si se recortara antes, lo que se ve es una
    /// franja pelada siguiendo al jugador.
    #[test]
    fn every_ring_has_chunks_across_its_own_band() {
        let focus = Vec2::new(3.7, -11.2);
        for (index, ring) in RINGS.iter().enumerate() {
            let cells = ring_cells(index, focus, REFERENCE_REACH);
            let reach = ring_reach(index, REFERENCE_REACH);
            let inner = band_inner(index, REFERENCE_REACH).max(NEAREST_INTEREST_M);
            for step in 0_u8..48 {
                let angle = f32::from(step) * std::f32::consts::TAU / 48.0;
                let direction = Vec2::new(angle.cos(), angle.sin());
                for distance in [inner, f32::midpoint(inner, reach), reach * 0.999] {
                    let point = focus + direction * distance;
                    let half = ring.chunk_m * 0.5;
                    assert!(
                        cells.iter().any(|cell| {
                            let offset = (point - cell_centre(*cell, ring.chunk_m)).abs();
                            offset.x <= half && offset.y <= half
                        }),
                        "el anillo {index} tiene briznas vivas a {distance:.1} m y ningún chunk ahí",
                    );
                }
            }
        }
    }

    /// El campo se deriva de la fórmula de cobertura, no se elige a ojo. Un tramo
    /// que raleara por debajo de su mínimo mostraría suelo, y lo haría en
    /// silencio — la falla se lee como "el pasto está un poco ralo por allá".
    ///
    /// **Y ahora la cuenta cruza el reparto por niveles**, que es donde el
    /// rediseño puede fallar sin que nada avise: una brizna cuyo alcance llega a
    /// una distancia pero cuyo nivel no tiene chunks ahí, no se dibuja.
    #[test]
    fn every_distance_gets_the_density_it_demands() {
        let scale = reference_scale();
        let ladder = grass_tiles::reach_ladder(REFERENCE_DENSITY, scale, REFERENCE_REACH);
        let ranges = tile_ranges(REFERENCE_DENSITY, scale, REFERENCE_REACH);
        let mut distance = NEAREST_INTEREST_M;
        while distance <= farthest_reach(REFERENCE_REACH) {
            let alive: usize = ranges
                .iter()
                .enumerate()
                // El nivel sólo planta hasta su propio alcance: más allá no tiene
                // chunks, y sus briznas no existen aunque su escalera llegue.
                .filter(|(index, _)| ring_reach(*index, REFERENCE_REACH) >= distance)
                .map(|(_, range)| {
                    ladder[range.start as usize..range.end as usize]
                        .iter()
                        .filter(|reach| reach.floor() >= distance)
                        .count()
                })
                .sum();
            let planted = alive as f32 / grass_tiles::TILE_AREA_M2;
            let needed = live_density_at(distance, REFERENCE_DENSITY, scale);
            assert!(
                planted >= needed - 1.0 / grass_tiles::TILE_AREA_M2,
                "a {distance} m el suelo recibe {planted:.1}/m2 y su distancia pide {needed:.1}/m2"
            );
            distance += 1.0;
        }
    }

    /// Cuántos anillos plantan sobre el mismo pedazo de suelo. Uno, o dos dentro
    /// de la banda de traspaso: tres es densidad que nadie pidió, pagada entera
    /// en overdraw y con las briznas equivocadas.
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

    /// Cuántos niveles se permiten hoy sobre el mismo suelo: **todos**, desde que
    /// se reparten índices y no suelo. Pisarse dejó de ser densidad multiplicada
    /// —una brizna la dibuja un solo nivel— y pasó a ser sólo territorio
    /// compartido. Lo que el test sigue cobrando es que no aparezca un cuarto.
    const RINGS_OVER_THE_SAME_GROUND: usize = 3;

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

    /// **Lo que el uniform dice tiene que existir en la malla.** El shader busca
    /// el alcance de la brizna en la tabla del uniform; si no coincide no falla,
    /// *no encuentra nada*, y `ring_inner` ancla la ley `1/d` en cero sin que se
    /// vea. Pasó con la perilla de alcance (`BOTWGrass.md`), y este test vale
    /// para toda perilla presente y futura.
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

    /// **Las herramientas de diagnóstico tienen que conocer los niveles que hay.**
    ///
    /// El 2026-08-08 la pradera bajó de cuatro niveles a tres y la perilla siguió
    /// ofreciendo "solo 3": ese paso deja el campo **vacío**, y como una escena
    /// vacía es un resultado creíble, la herramienta no falla — miente. Lo
    /// encontró el usuario jugando, que es exactamente a quien no le tiene que
    /// pasar. Un paso por nivel, más el "todos".
    #[test]
    fn the_ring_knob_offers_exactly_the_levels_that_exist() {
        assert_eq!(
            bof_domain::perf::GRASS_RINGS_STEPS.len(),
            RINGS.len() + 1,
            "la perilla de anillos y la pradera no hablan del mismo campo"
        );
        let mut perf = crate::perf::PerfToggles::default();
        for step in 0..bof_domain::perf::GRASS_RINGS_STEPS.len() {
            perf.set_knob_step(crate::perf::PerfKnob::GrassRings, step);
            if let Some(only) = perf.grass_only_ring() {
                assert!(
                    only < RINGS.len(),
                    "el paso '{}' aísla un nivel que no existe",
                    bof_domain::perf::GRASS_RINGS_STEPS[step],
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
        for (slot, ring) in ring_facts(&perf).into_iter().enumerate() {
            assert_eq!(
                ring.reach_m,
                ring_reach(slot, scale),
                "la leyenda del anillo {slot} no informa el alcance vigente",
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
}
