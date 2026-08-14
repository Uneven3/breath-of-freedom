//! Pradera rodante de presentación: una brizna no es una entidad.

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{NoAutoAabb, ViewVisibility};
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

/// The renderer has three representations.  The count is structural (it is
/// the size of the shader uniform); every number that designs one of them is
/// in [`GrassRendererSettings`], so the Grass Lab can change the real renderer
/// rather than maintain a parallel set of test values.
pub(crate) const GRASS_RING_COUNT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GrassRingSettings {
    pub reach_m: f32,
    pub chunk_m: f32,
}

/// Runtime-owned grass authoring values.
///
/// `Default` is the shipped baseline, not a second source of truth. The Grass
/// Lab sends requests to this resource and all render, bake and measurement
/// paths consume it. Scene data decides *where* a cover grows (`TallGrass` or
/// `ShortGrass`); it does not override these renderer values by scene name.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub(crate) struct GrassRendererSettings {
    pub rings: [GrassRingSettings; GRASS_RING_COUNT],
    pub leaf_min_pixels: f32,
    pub spike_min_pixels: f32,
    /// Wide enough at the base to cover ground, tapered at the tip so it reads
    /// as a leaf rather than a strip of paper.
    ///
    /// **+2mm el 2026-08-09, experimento de overdraw**: `minimum_density` pide
    /// densidad inversamente proporcional a este ancho, así que una brizna más
    /// gorda ya pide menos briznas sola, por la misma ley — no hace falta bajar
    /// la densidad aparte. Menos briznas solapadas para la misma cobertura es
    /// menos overdraw, que es lo que cuesta en un frame fill-bound.
    pub blade_width_m: f32,
    pub blade_waist: f32,
    pub blade_root_sink_m: f32,
    pub blade_height_min_m: f32,
    pub blade_height_max_m: f32,
    pub blade_lean_m: f32,
    pub growth_sink_m: f32,
    pub card_width_m: f32,
    pub card_silhouette_area: f32,
    pub cards_enabled: bool,
    pub root_color: LinearRgba,
    pub tip_color: LinearRgba,
    pub grass_tint_strength: f32,
    pub hidden_by_distance: [(f32, f32); 7],
    pub hidden_per_width_per_metre_card: f32,
    pub target_coverage: f32,
    /// Técnica 2 del plan de anillos (2026-08-09): romper el círculo
    /// perfecto de la frontera **entre anillos** (0→1→2, la que cambia
    /// densidad y forma de golpe) con ruido determinista por chunk, en vez
    /// de una línea limpia. Apagado por default — no cambia nada hasta que
    /// F9 lo prenda.
    ///
    /// Un primer intento (2026-08-13, madrugada) aplicó este mismo ruido al
    /// límite interno de los tiers de buffer del anillo 0
    /// (`RING0_TIERS`) — un límite invisible por diseño (nunca cambia qué se
    /// ve, sólo cuántos índices se someten al shader), así que el ruido no
    /// tenía nada que romper ahí. Jugando se confirmó "no hace nada". Este
    /// campo ahora controla el lugar correcto — ver [`ring_boundary_jitter_m`].
    pub ragged_ring_boundary_enabled: bool,
    /// Cuánto puede correr el borde, en metros. Sólo empuja hacia **más**
    /// alcance — ver [`ring_boundary_jitter_m`] — así que un chunk nunca
    /// pierde cobertura que la asignación limpia le daría.
    pub ragged_ring_boundary_max_m: f32,
}

impl Default for GrassRendererSettings {
    fn default() -> Self {
        Self {
            rings: [
                GrassRingSettings {
                    reach_m: 24.0,
                    chunk_m: 12.0,
                },
                GrassRingSettings {
                    reach_m: 40.0,
                    chunk_m: 16.0,
                },
                GrassRingSettings {
                    reach_m: 128.0,
                    // Achicado de 64 a 48 el 2026-08-13: mitiga (no elimina) el pop
                    // de chunk documentado en `GRASS_PERF_DATA.md` ("Chunks
                    // apareciendo de golpe en el anillo lejano") — un chunk nuevo
                    // trae ~44% menos briznas de golpe. No se probó 32 (el valor
                    // que daría ~4× menos): rompe `the_neighbourhood_is_bounded`
                    // (120/100 chunks en el peor caso). 48 deja margen real
                    // (90/100).
                    chunk_m: 48.0,
                },
            ],
            leaf_min_pixels: 3.0,
            spike_min_pixels: 1.5,
            blade_width_m: 0.057,
            blade_waist: 0.30,
            blade_root_sink_m: 0.06,
            blade_height_min_m: 0.55,
            blade_height_max_m: 0.96,
            blade_lean_m: 0.27,
            growth_sink_m: 0.18,
            card_width_m: 0.30,
            card_silhouette_area: 0.583,
            cards_enabled: true,
            root_color: LinearRgba::rgb(0.093, 0.147, 0.031),
            tip_color: LinearRgba::rgb(0.340, 0.622, 0.089),
            grass_tint_strength: 0.25,
            hidden_by_distance: [
                (3.5, 0.082),
                (5.0, 0.085),
                (7.0, 0.093),
                (9.5, 0.109),
                (13.5, 0.112),
                (19.0, 0.109),
                (27.0, 0.114),
            ],
            hidden_per_width_per_metre_card: 0.185,
            target_coverage: 0.95,
            ragged_ring_boundary_enabled: false,
            ragged_ring_boundary_max_m: 4.0,
        }
    }
}

/// Requests cross the presentation/renderer boundary; only the renderer
/// writes its configuration.
#[derive(Message, Debug, Clone, Copy)]
pub(crate) enum GrassLabSettingRequest {
    AdjustFrontier {
        ring: usize,
        delta_m: f32,
    },
    AdjustSpikeThreshold {
        delta_pixels: f32,
    },
    AdjustCardWidth {
        delta_m: f32,
    },
    /// No toca `GrassRendererSettings` — ver [`GrowthRampOverride`]. El
    /// primer click arranca desde el valor **efectivo** de la perilla F1
    /// (`perf.grass_growth()`), no de un piso fijo, para que no salte al
    /// activarse.
    AdjustGrowthRamp {
        delta_m: f32,
    },
    /// Prende/apaga el ruido en la frontera entre anillos (Técnica 2, ver
    /// [`ring_boundary_jitter_m`]). Sí vive en `GrassRendererSettings` — a
    /// diferencia de la rampa de crecimiento, esto cambia qué chunks existen,
    /// así que el rehorneado completo que dispara es necesario, no un costo
    /// evitable.
    ToggleRaggedRingBoundary,
    Reset,
}

pub(super) fn apply_grass_lab_settings(
    mut requests: MessageReader<GrassLabSettingRequest>,
    mut settings: ResMut<GrassRendererSettings>,
    mut growth_override: ResMut<GrowthRampOverride>,
    perf: Res<crate::perf::PerfToggles>,
) {
    for request in requests.read() {
        match *request {
            GrassLabSettingRequest::AdjustFrontier { ring, delta_m }
                if ring + 1 < GRASS_RING_COUNT =>
            {
                let lower = if ring == 0 {
                    NEAREST_INTEREST_M + 4.0
                } else {
                    settings.rings[ring - 1].reach_m + 4.0
                };
                let upper = settings.rings[ring + 1].reach_m - 4.0;
                settings.rings[ring].reach_m = (settings.rings[ring].reach_m + delta_m)
                    .round()
                    .clamp(lower, upper);
            }
            GrassLabSettingRequest::AdjustSpikeThreshold { delta_pixels } => {
                settings.spike_min_pixels = (settings.spike_min_pixels + delta_pixels)
                    .clamp(0.5, settings.leaf_min_pixels - 0.25);
            }
            GrassLabSettingRequest::AdjustCardWidth { delta_m } => {
                settings.card_width_m = (settings.card_width_m + delta_m).clamp(0.10, 1.50);
            }
            GrassLabSettingRequest::AdjustGrowthRamp { delta_m } => {
                let base = growth_override.0.unwrap_or_else(|| perf.grass_growth());
                growth_override.0 = Some((base + delta_m).clamp(0.4, 80.0));
            }
            GrassLabSettingRequest::ToggleRaggedRingBoundary => {
                settings.ragged_ring_boundary_enabled = !settings.ragged_ring_boundary_enabled;
            }
            GrassLabSettingRequest::Reset => {
                *settings = GrassRendererSettings::default();
                growth_override.0 = None;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod grass_lab_settings_tests {
    use super::*;

    #[test]
    fn a_live_frontier_stays_ordered_and_the_baseline_is_recoverable() {
        let mut settings = GrassRendererSettings::default();
        let baseline = settings;
        settings.rings[0].reach_m = settings.rings[1].reach_m - 4.0;
        let mut app = App::new();
        app.insert_resource(settings);
        app.insert_resource(GrowthRampOverride::default());
        app.insert_resource(crate::perf::PerfToggles::default());
        app.add_message::<GrassLabSettingRequest>();
        app.add_systems(Update, apply_grass_lab_settings);
        app.world_mut()
            .resource_mut::<Messages<GrassLabSettingRequest>>()
            .write(GrassLabSettingRequest::AdjustFrontier {
                ring: 0,
                delta_m: 8.0,
            });
        app.update();
        assert!(
            app.world().resource::<GrassRendererSettings>().rings[0].reach_m
                <= baseline.rings[1].reach_m - 4.0
        );
        app.world_mut()
            .resource_mut::<Messages<GrassLabSettingRequest>>()
            .write(GrassLabSettingRequest::Reset);
        app.update();
        assert_eq!(*app.world().resource::<GrassRendererSettings>(), baseline);
    }

    /// El ajuste de rampa arranca desde el valor efectivo de la perilla F1
    /// (no de cero), se satura, y `Reset` lo devuelve a "sin ajuste" — no a
    /// un número, para que la perilla F1 vuelva a mandar sola.
    #[test]
    fn a_live_growth_ramp_starts_from_the_knob_and_resets_to_no_override() {
        let mut app = App::new();
        app.insert_resource(GrassRendererSettings::default());
        app.insert_resource(GrowthRampOverride::default());
        let mut perf = crate::perf::PerfToggles::default();
        perf.set_knob_step(bof_domain::perf::PerfKnob::GrassGrowth, 2);
        let knob_value = perf.grass_growth();
        app.insert_resource(perf);
        app.add_message::<GrassLabSettingRequest>();
        app.add_systems(Update, apply_grass_lab_settings);
        app.world_mut()
            .resource_mut::<Messages<GrassLabSettingRequest>>()
            .write(GrassLabSettingRequest::AdjustGrowthRamp { delta_m: 5.0 });
        app.update();
        assert_eq!(
            app.world().resource::<GrowthRampOverride>().0,
            Some(knob_value + 5.0),
            "el primer ajuste tiene que arrancar del valor efectivo de la perilla, no de cero",
        );
        app.world_mut()
            .resource_mut::<Messages<GrassLabSettingRequest>>()
            .write(GrassLabSettingRequest::Reset);
        app.update();
        assert_eq!(
            app.world().resource::<GrowthRampOverride>().0,
            None,
            "Reset tiene que devolver el mando a la perilla F1, no dejar un número fijo",
        );
    }
}

/// Selector de una máscara candidata exclusivamente para una corrida de
/// medición. La configuración normal nunca lo escribe: la pradera de juego
/// sigue usando el asset base hasta que el checkpoint apruebe una calibración.
const CARD_ALBEDO_BASE: &str = "textures/props/T_GrassMeadowCard_Albedo.png";
/// La primera carta que tuvo el proyecto. Hoy sólo la usa el prop suelto
/// `FoliageCard` (`prop_grass_card_a`) — el laboratorio `Card mesh` la
/// descartó para la pradera por alfa binaria y RGB oculto negro
/// (`BOTWGrass.md` → *Técnica 1*), pero sigue siendo una candidata medible.
const CARD_ALBEDO_LEGACY: &str = "textures/props/T_GrassCard_Albedo.png";
const CARD_ALBEDO_V3: &str = "textures/props/T_GrassMeadowCard_v3_Albedo.png";

fn card_albedo_path(candidate_label: &str) -> &'static str {
    match candidate_label {
        "legacy" => CARD_ALBEDO_LEGACY,
        "v3" => CARD_ALBEDO_V3,
        other => {
            if other != "base" {
                warn!(
                    "[grass] candidata de carta desconocida {other:?}; candidatos: {:?}",
                    bof_domain::perf::GRASS_CARD_CANDIDATE_STEPS
                );
            }
            CARD_ALBEDO_BASE
        }
    }
}

/// Umbral que ninguna distancia real cruza — el mismo sentido que ya usa
/// `GrassUniform::default` (`spike_from_m`/`card_from_m` en `1e9`) para decir
/// "nunca dispara".
const SHAPE_BENCH_NEVER_PIXELS: f32 = 1.0e6;
/// Umbral que cualquier distancia real cruza, incluida la más cercana.
const SHAPE_BENCH_ALWAYS_PIXELS: f32 = 1.0e-4;

/// Aplica el banco hoja/púa/carta (`PerfKnob::GrassShapeBench`) sobre una
/// copia de la configuración, sin tocar el recurso real.
///
/// **Mueve los dos umbrales de píxeles que ya deciden la forma
/// (`shape_at`), a pares que los saturan en vez de nudgearlos.** No agrega un
/// mecanismo nuevo ni un campo de uniform nuevo: `spike_from_m`/`card_from_m`
/// —que el shader ya deriva de estos mismos dos números— saturan igual, así
/// que Rust y WGSL siguen clasificando la misma brizna igual sin tocar
/// `grass.wgsl`. Cada forma conserva su propia densidad (`footprint_m`), así
/// que lo único que cambia entre pasos es el asset — ni densidad ni cámara,
/// que es lo que el corte de sesión del 2026-08-11 pedía separar. A
/// diferencia del primer intento (mover `spike_min_pixels` a sus extremos ya
/// legales en Grass Lab), esta versión es pura de punta a punta: "solo carta"
/// ya no deja una franja de púa en el borde de la hoja, y "solo hoja" —antes
/// imposible, `leaf_min_pixels` nunca alcanzaba a cubrir el anillo lejano
/// dentro de sus límites de F9— ahora es un tercer extremo más.
pub(crate) fn shape_bench_settings(
    perf: &crate::perf::PerfToggles,
    mut settings: GrassRendererSettings,
) -> GrassRendererSettings {
    match perf.grass_shape_bench_label() {
        "solo hoja" => {
            settings.leaf_min_pixels = SHAPE_BENCH_ALWAYS_PIXELS;
            settings.spike_min_pixels = SHAPE_BENCH_ALWAYS_PIXELS;
        }
        "solo púa" => {
            settings.leaf_min_pixels = SHAPE_BENCH_NEVER_PIXELS;
            settings.spike_min_pixels = SHAPE_BENCH_ALWAYS_PIXELS;
        }
        "solo carta" => {
            settings.leaf_min_pixels = SHAPE_BENCH_NEVER_PIXELS;
            settings.spike_min_pixels = SHAPE_BENCH_NEVER_PIXELS;
        }
        _ => {}
    }
    settings
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

/// Apagada y reencendida el 2026-08-09, detalle en `BOTWGrass.md`. La primera
/// vez el problema no era el tamaño ni el borde: era que la carta siempre
/// muestra su ancho completo por mirar a cámara, cosa que una brizna real no
/// hace. Vuelve con `AlphaMode::AlphaToCoverage` en vez de `Mask` — border
/// antialiaseado — más porque los árboles van a necesitar billboards de
/// todos modos que porque esto resuelva aquel diagnóstico.
/// Primitiva para esta distancia, elegida por tamaño en pantalla y no por un
/// radio atado a una resolución. Los umbrales visuales viven en `BOTWGrass.md`.
fn shape_at(distance_m: f32, scale: f32, settings: &GrassRendererSettings) -> BladeShape {
    let pixels = width_in_pixels(settings.blade_width_m, distance_m, scale);
    if pixels >= settings.leaf_min_pixels {
        BladeShape::Leaf
    } else if pixels >= settings.spike_min_pixels || !settings.cards_enabled {
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
fn density_at(distance_m: f32, shape: BladeShape, settings: &GrassRendererSettings) -> f32 {
    minimum_density(distance_m, shape, settings)
}

/// La brizna, en dos niveles de detalle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum BladeShape {
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
    fn footprint_m(self, settings: &GrassRendererSettings) -> f32 {
        match self {
            Self::Leaf | Self::Spike => settings.blade_width_m,
            Self::Card => settings.card_width_m * settings.card_silhouette_area,
        }
    }

    /// Si el vertex shader tiene que abrir la primitiva mirando a la cámara.
    const fn faces_camera(self) -> bool {
        matches!(self, Self::Card)
    }

    /// El número con que el shader la reconoce. Un test lo cobra contra las
    /// constantes `SHAPE_*` de `grass.wgsl`, que es lo único que las ata.
    pub(super) const fn shader_index(self) -> u32 {
        match self {
            Self::Leaf => 0,
            Self::Spike => 1,
            Self::Card => 2,
        }
    }

    /// Las tres formas, en el orden en que la leyenda y el shader las
    /// numeran. Única fuente para cualquier tabla que las enumere — antes
    /// había dos listas de nombres escritas a mano en `grass_debug.rs`,
    /// ninguna derivada de este enum.
    pub(super) const ALL: [Self; 3] = [Self::Leaf, Self::Spike, Self::Card];

    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Leaf => "hoja",
            Self::Spike => "púa",
            Self::Card => "carta",
        }
    }
}

/// Triángulos enviados por brizna. La púa degenera el segundo en el shader, pero
/// el presupuesto cuenta primitivas enviadas igual que el censo de la malla.
const SUBMITTED_TRIANGLES_PER_BLADE: usize = 2;

/// Cuántos triángulos por brizna manda la malla índice de un nivel: 2 salvo
/// que el banco de forma fuerce "solo púa". Ahí, y sólo ahí, todas las
/// briznas visibles de todos los niveles son púa a la vez —
/// `shape_bench_settings` satura `leaf_min_pixels`/`spike_min_pixels` juntos,
/// sin tabla por anillo, así que no hay forma de que sobreviva una hoja o una
/// carta mientras el banco está en ese paso—, así que el segundo triángulo
/// —que la púa ya degenera en el shader, `grass.wgsl::blade_vertex` caso
/// `SHAPE_SPIKE`— se puede directamente no enviar. En juego normal (`"auto"`)
/// un mismo nivel mezcla formas por brizna según distancia sobre la MISMA
/// malla índice compartida; bajarla ahí le cortaría la cintura real a
/// cualquier brizna que el shader clasificara como hoja.
fn submitted_triangles_per_blade(perf: &crate::perf::PerfToggles) -> usize {
    if perf.grass_shape_bench_label() == "solo púa" {
        1
    } else {
        SUBMITTED_TRIANGLES_PER_BLADE
    }
}

/// Cuántos píxeles de ancho tiene que medir una brizna para merecer cada forma.
///
/// En píxeles y no en metros, que es el punto entero de esta escalera. Con el
/// viewport de escritorio caen en ~24 m y ~36 m; a 900p, en ~20 y ~30. Nadie los
/// mueve: los mueve la pantalla.
/// La forma de un anillo sale de la distancia **media** de su banda: es lo que
/// se ve en la mayor parte de él.
fn shape_for_ring(
    index: usize,
    scale: f32,
    reach_scale: f32,
    settings: &GrassRendererSettings,
) -> BladeShape {
    shape_at(band_midpoint(index, reach_scale, settings), scale, settings)
}

/// Cuántas briznas por m² tienen que estar **vivas** a esta distancia.
///
/// La misma ley que `density_for_ring` evalúa en el borde de una banda, pero
/// como función continua de la distancia: es lo que `grass_tiles` invierte para
/// darle a cada brizna su propio alcance. La perilla la escala como razón, igual
/// que a la de los anillos, para que el barrido siga moviendo una sola variable.
pub(super) fn live_density_at(
    distance_m: f32,
    dial: f32,
    scale: f32,
    settings: &GrassRendererSettings,
) -> f32 {
    density_at(distance_m, shape_at(distance_m, scale, settings), settings)
        * (dial / REFERENCE_DENSITY)
}

/// Hasta dónde llega la pradera entera, que es donde termina el último nivel.
pub(super) fn farthest_reach(reach_scale: f32, settings: &GrassRendererSettings) -> f32 {
    ring_reach(GRASS_RING_COUNT - 1, reach_scale, settings)
}

/// Cuántas briznas por m² hay **realmente vivas** a esta distancia — no las que
/// la ley pide, que es `live_density_at`. La diferencia es lo que hace falta para
/// medir la huella real: despejarla con el número del dial da una huella que
/// absorbe el raleo, y así estuvo sobreestimada hasta el 2026-08-08.
pub(crate) fn live_blades_per_m2(
    distance_m: f32,
    dial: f32,
    reach_scale: f32,
    settings: &GrassRendererSettings,
) -> f32 {
    // La escalera de **referencia**, igual que `ring_facts`: el número acompaña a
    // una captura de cualquier tamaño, y uno que cambiara con la ventana no
    // compara dos corridas.
    let scale = reference_scale();
    let ladder = grass_tiles::reach_ladder(dial, scale, reach_scale, settings);
    // El tier 0 de cualquier anillo siempre pide el rango **entero** del
    // anillo — su borde interno es el del anillo mismo (`ring0_tier_bounds`
    // arranca en `band_inner`), y los tiers de más afuera son subconjuntos de
    // ése. Cuántas briznas están vivas a una distancia no depende de cómo se
    // partió el buffer del anillo en tiers, así que este cómputo usa siempre
    // el rango de su tier 0.
    let alive: usize = tile_ranges(dial, scale, reach_scale, settings)
        .iter()
        .enumerate()
        .filter(|(index, _)| ring_reach(*index, reach_scale, settings) >= distance_m)
        .map(|(_, tiers)| {
            let range = &tiers[0];
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
fn band_inner(index: usize, reach_scale: f32, settings: &GrassRendererSettings) -> f32 {
    index.checked_sub(1).map_or(NEAREST_INTEREST_M, |inner| {
        ring_reach(inner, reach_scale, settings)
    })
}

/// Cuántos tiers internos de buffer tiene un anillo. Sólo el anillo 0 —el
/// único donde se midió que un solo casillero por chunk desperdicia la
/// mayoría de lo que envía (`docs/GRASS_PERF_DATA.md`)— se parte en
/// [`RING0_TIERS`]; los demás siguen con uno, sin cambiar su comportamiento.
pub(super) const RING0_TIERS: usize = 6;
pub(super) const MAX_TIERS: usize = RING0_TIERS;

pub(super) fn tier_count(ring: usize) -> usize {
    if ring == 0 { RING0_TIERS } else { 1 }
}

/// Los bordes internos de los tiers del anillo 0, como **fracción de su
/// alcance vigente** — nunca metros fijos. El alcance del anillo 0 se mueve en
/// vivo (perilla `grass-reach`, o `AdjustFrontier` desde el hub F9); un borde
/// en metros fijos dejaría un tier sin territorio de un lado, o un chunk sin
/// tier del otro — la misma clase de agujero que ya costó el bug de
/// `vertex_index` (`BOTWGrass.md`). El primer borde coincide siempre con
/// `band_inner(0, ...)`, el último con `ring_reach(0, ...)`.
fn ring0_tier_bounds(reach_scale: f32, settings: &GrassRendererSettings) -> [f32; RING0_TIERS + 1] {
    let inner = band_inner(0, reach_scale, settings);
    let outer = ring_reach(0, reach_scale, settings);
    std::array::from_fn(|t| inner + (outer - inner) * t as f32 / RING0_TIERS as f32)
}

/// La distancia de un chunk al foco que importa para decidir su tier: su
/// **punto más cercano**, no su centro. Un chunk de varios metros de lado
/// cubre un rango de distancias, y el tier tiene que cubrir el peor caso
/// —el punto más cercano— o un chunk cerca de una frontera de tier queda con
/// menos casilleros de los que su propia esquina necesita. Misma fórmula que
/// ya usa `ring_cells_with_slack` para decidir si un chunk existe: una sola
/// función para las dos preguntas, para que nunca puedan divergir.
fn chunk_nearest_m(cell: IVec2, chunk_m: f32, focus: Vec2) -> f32 {
    let half = chunk_m * 0.5;
    let offset = (cell_centre(cell, chunk_m) - focus).abs();
    (offset - Vec2::splat(half)).max(Vec2::ZERO).length()
}

/// Tope del ruido de frontera para un anillo dado: cero si el ruido está
/// apagado, o si `ring` es el **último** anillo (`GRASS_RING_COUNT - 1`).
/// El último anillo no tiene territorio de sobra para prestar — más allá de
/// `farthest_reach()` no vive ninguna brizna de toda la pradera
/// (`grass_tiles::reach_ladder` corta ahí, un tope duro), así que un chunk
/// que sólo existiera por el ruido en ese borde saldría vacío: puro gasto de
/// draw call y memoria sin un triángulo que pintar. Una sola fuente de
/// verdad para "cuánto puede correr esta celda" — la usan tanto
/// [`ring_boundary_jitter_m`] como el radio de búsqueda de
/// `ring_cells_with_slack`, para que nunca puedan discrepar.
fn ring_boundary_jitter_cap_m(ring: usize, settings: &GrassRendererSettings) -> f32 {
    if !settings.ragged_ring_boundary_enabled || ring + 1 >= GRASS_RING_COUNT {
        0.0
    } else {
        settings.ragged_ring_boundary_max_m.max(0.0)
    }
}

/// Ruido determinista por celda para la frontera **entre anillos** — Técnica
/// 2 del plan de anillos (`BOTWGrass.md`, 2026-08-09): romper el círculo con
/// ruido, no con una forma nueva.
///
/// Un primer intento (2026-08-13, madrugada) aplicó este mismo ruido al
/// límite interno de los tiers de buffer del anillo 0 — invisible por
/// diseño, así que no rompía nada. Éste perturba `reach_m`, el radio real
/// que decide si un chunk del anillo existe (`ring_cells_with_slack`).
///
/// **Siempre `>= 0`, a propósito.** Sólo puede **extender** hacia afuera el
/// alcance de un anillo para ciertas celdas, nunca acortarlo — un chunk
/// nunca pierde cobertura que la asignación limpia le daría. El corte
/// *interior* de la corona siguiente (`handover` en `ring_cells_with_slack`)
/// sigue anclado al alcance limpio del anillo de adentro, no a éste: el
/// anillo de adentro nunca cubre menos que su valor limpio, con o sin ruido,
/// así que excluir al de afuera hasta ese punto sigue siendo seguro — en el
/// peor caso hay superposición extra, nunca un agujero. Empujar este valor
/// en la otra dirección dejaría un chunk con menos cobertura de la que su
/// punto más cercano real exige: el mismo bug que costó `vertex_index`
/// (2026-08-07), esta vez a propósito si alguien lo invirtiera sin leer este
/// comentario.
fn ring_boundary_jitter_m(cell: IVec2, ring: usize, settings: &GrassRendererSettings) -> f32 {
    let cap = ring_boundary_jitter_cap_m(ring, settings);
    if cap <= 0.0 {
        return 0.0;
    }
    let seed = crate::world::forest::hash_u32(
        cell.x.cast_unsigned().wrapping_mul(0x9e37_79b9)
            ^ cell.y.cast_unsigned().wrapping_mul(0x85eb_ca6b)
            ^ u32::try_from(ring)
                .unwrap_or(u32::MAX)
                .wrapping_mul(0xc2b2_ae35),
    );
    crate::world::forest::hash_unit(seed) * cap
}

/// A qué tier le toca un chunk del anillo 0, dado su punto más cercano.
/// Satura al último tier en vez de fallar: como `ring0_tier_bounds` deriva su
/// borde exterior del alcance **vigente**, y `ring_cells`/`ring_cells_with_slack`
/// filtran por ese mismo alcance salvo que el ruido de frontera
/// (`ragged_ring_boundary_enabled`) extienda la selección del propio anillo
/// más allá — en ese caso un chunk sí puede llegar con `nearest_m` más allá
/// del último borde, y saturar al último tier sigue siendo seguro: ese tier
/// ya reserva índices hasta `farthest_reach()` (ver `tile_ranges`), muy más
/// allá del borde limpio del anillo 0, así que necesitar menos de los que
/// tiene reservados nunca es un problema. Saturar en vez de entrar en pánico
/// es la misma disciplina que ya usa el resto del módulo ante un índice
/// fuera de rango.
fn ring0_tier_for(nearest_m: f32, reach_scale: f32, settings: &GrassRendererSettings) -> usize {
    let bounds = ring0_tier_bounds(reach_scale, settings);
    (0..RING0_TIERS)
        .rev()
        .find(|&tier| nearest_m >= bounds[tier])
        .unwrap_or(0)
}

/// El tier de un chunk del anillo 0, con histéresis: si el chunk ya existe en
/// algún tier y su distancia actual sigue dentro de la banda de ese tier
/// ensanchada por [`KEEP_SLACK_M`], se queda ahí — igual que un chunk en el
/// borde de un anillo no se rehornea cada cuadro. Sólo se reasigna cuando
/// cruza sólidamente a otra banda, o cuando todavía no existe.
///
/// **`wanted` y `keep_set` tienen que llamar a esta misma función**, no una
/// versión sin memoria cada uno: si acordaran un tier distinto para el mismo
/// chunk ya vivo, el de abajo se armaría como "falta" en el tier nuevo
/// mientras el de arriba mantiene vivo el tier viejo — la misma brizna
/// dibujada dos veces a la vez.
fn ring0_tier_with_hysteresis(
    field: &GrassField,
    cell: IVec2,
    nearest_m: f32,
    reach_scale: f32,
    settings: &GrassRendererSettings,
) -> usize {
    let bounds = ring0_tier_bounds(reach_scale, settings);
    for tier in 0..RING0_TIERS {
        if !field.live.contains_key(&ChunkKey {
            ring: 0,
            tier,
            cell,
        }) {
            continue;
        }
        let lo = if tier == 0 {
            f32::MIN
        } else {
            bounds[tier] - KEEP_SLACK_M
        };
        let hi = if tier + 1 == RING0_TIERS {
            f32::MAX
        } else {
            bounds[tier + 1] + KEEP_SLACK_M
        };
        if nearest_m >= lo && nearest_m < hi {
            return tier;
        }
    }
    ring0_tier_for(nearest_m, reach_scale, settings)
}

/// Cuántas briznas de un chunk producen geometría visible ahora, contra
/// cuántas paga su casillero de stride fijo.
///
/// Réplica en CPU de `blade_growth` (`grass.wgsl`): una brizna está muerta
/// —cero altura, cero píxeles— si su distancia al foco es menor al borde
/// interno de la corona (la dibuja el nivel de adentro) o mayor/igual a su
/// propio alcance horneado (`baked_reach`, la misma función que usó el
/// registro real, no la escalera cruda — sin el `floor`, una brizna con
/// `ladder[índice] = 30.7` se contaría viva un metro más allá de donde el
/// shader ya la mató). La zona de desvanecimiento (`growth_ramp` metros antes
/// del alcance) cuenta como viva: tiene altura fraccional, no cero.
fn chunk_vitality(
    centre: Vec2,
    chunk_m: f32,
    blades: std::ops::Range<u32>,
    ladder: &[f32],
    inner: f32,
    focus: Vec2,
) -> (usize, usize) {
    let mut resident = 0;
    let mut alive = 0;
    for (index, blade) in chunk_blade_positions(centre, chunk_m, blades) {
        resident += 1;
        let distance = (blade.xz - focus).length();
        if distance >= inner && distance < baked_reach(ladder, index) {
            alive += 1;
        }
    }
    (resident, alive)
}

/// Briznas vivas contra residentes, por anillo, para todo el campo que
/// `GrassField` tiene en memoria ahora mismo.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GrassVitality {
    pub resident_blades: [usize; GRASS_RING_COUNT],
    pub alive_blades: [usize; GRASS_RING_COUNT],
}

/// **A pedido, no continua.** Recorre cada brizna residente —hasta cientos de
/// miles en un campo típico—, así que su costo escala con el campo entero.
/// Vive del lado de `BOF_SHOT`, que ya paga una vuelta de GPU (una captura de
/// pantalla) más cara que este recorrido en CPU; llamarla cada frame junto al
/// resto de `GrassLabStats` competiría con el propio medidor de milisegundos
/// que esta herramienta existe para no contaminar.
///
/// `dial`/`reach_scale`/`settings` tienen que ser los mismos con que se
/// horneó `field` — la escala es siempre `reference_scale()`, nunca la del
/// viewport real, porque es la que usó `roll_meadow_grid` para fijar el
/// stride de cada nivel. Pasar la escala de cámara real haría que
/// `resident_blades` no coincidiera con `chunks × stride` sin que nada lo
/// avisara.
pub(crate) fn grass_vitality(
    field: &GrassField,
    focus: Vec2,
    dial: f32,
    reach_scale: f32,
    settings: &GrassRendererSettings,
) -> GrassVitality {
    let scale = reference_scale();
    let ladder = grass_tiles::reach_ladder(dial, scale, reach_scale, settings);
    let ranges = tile_ranges(dial, scale, reach_scale, settings);
    let mut out = GrassVitality::default();
    for key in field.live.keys() {
        let ring = key.ring;
        let Some(ring_settings) = settings.rings.get(ring) else {
            continue;
        };
        let blades = ranges
            .get(ring)
            .and_then(|tiers| tiers.get(key.tier))
            .cloned()
            .unwrap_or(0..0);
        // Mismo truncamiento que el uniform (`record_layout.w`,
        // `metres_as_u32`): el shader compara contra un entero, no contra el
        // metro exacto de `band_inner`.
        let inner = metres_as_u32(band_inner(ring, reach_scale, settings)) as f32;
        let (resident, alive) = chunk_vitality(
            cell_centre(key.cell, ring_settings.chunk_m),
            ring_settings.chunk_m,
            blades,
            &ladder,
            inner,
            focus,
        );
        out.resident_blades[ring] += resident;
        out.alive_blades[ring] += alive;
    }
    out
}

fn band_midpoint(index: usize, reach_scale: f32, settings: &GrassRendererSettings) -> f32 {
    f32::midpoint(
        band_inner(index, reach_scale, settings),
        ring_reach(index, reach_scale, settings),
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
fn ring_reaches(reach_scale: f32, settings: &GrassRendererSettings) -> (Vec4, Vec4) {
    slots(
        settings,
        |index, _| ring_reach(index, reach_scale, settings),
        0.0,
    )
}

/// Los tamaños de chunk, en el mismo orden: con ellos el fragment deduce de qué
/// celda salió una brizna. No los escala la perilla de alcance, que decide
/// cuántos chunks hay y no de qué tamaño son.
fn ring_chunks(settings: &GrassRendererSettings) -> (Vec4, Vec4) {
    slots(settings, |_, ring| ring.chunk_m, 1.0)
}

/// Qué anillos abren su primitiva mirando a la cámara.
fn ring_cards(scale: f32, reach_scale: f32, settings: &GrassRendererSettings) -> (Vec4, Vec4) {
    slots(
        settings,
        |index, _| {
            f32::from(u8::from(
                shape_for_ring(index, scale, reach_scale, settings).faces_camera(),
            ))
        },
        0.0,
    )
}

/// Un dato por anillo en los ocho casilleros del uniform. El tope se cobra en
/// compilación: uno de más desbordaría en silencio.
const _: () = assert!(
    GRASS_RING_COUNT <= grass_debug::PALETTE_SLOTS,
    "hay más anillos que casilleros en el uniform y en la paleta"
);

fn slots(
    settings: &GrassRendererSettings,
    of: impl Fn(usize, &GrassRingSettings) -> f32,
    empty: f32,
) -> (Vec4, Vec4) {
    let mut slots = [empty; 8];
    for (index, ring) in settings.rings.iter().enumerate() {
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
fn hidden_per_width_per_metre(
    distance_m: f32,
    shape: BladeShape,
    settings: &GrassRendererSettings,
) -> f32 {
    if matches!(shape, BladeShape::Card) {
        return settings.hidden_per_width_per_metre_card;
    }
    let first = settings.hidden_by_distance[0];
    let last = settings.hidden_by_distance[settings.hidden_by_distance.len() - 1];
    if distance_m <= first.0 {
        return first.1;
    }
    settings
        .hidden_by_distance
        .windows(2)
        .find(|pair| distance_m <= pair[1].0)
        .map_or(last.1, |pair| {
            let across = (distance_m - pair[0].0) / (pair[1].0 - pair[0].0);
            pair[0].1 + across * (pair[1].1 - pair[0].1)
        })
}

/// Blades per m² needed at distance `d` for the ground not to show through.
/// A floor, not a recipe — see the module header.
///
/// **Las briznas caen sobre un hash, no sobre una grilla**, así que la cobertura
/// es `1 − e^(−λ·a)` y no `λ·a`. Esa forma quedó verificada midiendo (Paso 0);
/// lo que estaba mal era `a`.
fn minimum_density(distance_m: f32, shape: BladeShape, settings: &GrassRendererSettings) -> f32 {
    let distance_m = distance_m.max(0.5);
    let hidden_per_blade = shape.footprint_m(settings)
        * distance_m
        * hidden_per_width_per_metre(distance_m, shape, settings);
    -(1.0 - settings.target_coverage).ln() / hidden_per_blade
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

/// La rampa del crecimiento, **con la perilla aplicada** — o con el ajuste
/// vivo de F9 si hay uno puesto, que gana. Ver [`GrowthRampOverride`] para
/// por qué ese ajuste no vive en `GrassRendererSettings`.
fn growth_band(perf: &crate::perf::PerfToggles, override_ramp: &GrowthRampOverride) -> f32 {
    override_ramp.0.unwrap_or_else(|| perf.grass_growth())
}

/// Ajuste vivo de F9 sobre la rampa de crecimiento, para medir sin depender
/// de la escalera fija de la perilla F1 (`GRASS_GROWTH_STEPS`, que sólo baja
/// de 6 m, nunca sube).
///
/// **Deliberadamente afuera de `GrassRendererSettings`.** Ese struct entero
/// se compara por valor en `MeadowRebuildDials` (`roll_meadow_grid`): *
/// cualquier* campo que cambie ahí tira la grilla entera y la rehornea. La
/// rampa de crecimiento ya viaja gratis por uniform cada cuadro
/// (`meadow_uniform`) — meterla en ese struct le costaría un rehorneado
/// completo por cada click de F9, para un número que nunca necesitó uno.
#[derive(Resource, Default, Clone, Copy, PartialEq)]
pub(crate) struct GrowthRampOverride(pub Option<f32>);

/// Desde qué distancia una brizna se abre como carta.
///
/// **Es el mismo umbral que elige la forma, no un número aparte:** la carta
/// representa la masa de un matojo y sólo tiene sentido donde una brizna ya no
/// se resuelve. Más cerca se construye angosta — un billboard de medio metro a
/// tres metros gira con la cámara, reportado jugando el 2026-08-08.
fn card_from_m(scale: f32, settings: &GrassRendererSettings) -> f32 {
    // Despejado de `width_in_pixels(BLADE_WIDTH, d, scale) = SPIKE_MIN_PIXELS`.
    settings.blade_width_m / (settings.spike_min_pixels * scale).max(1e-6)
}

/// Y desde dónde pierde la cintura. **Los dos umbrales son los mismos que
/// `shape_at` usa**: la forma la decide la pantalla, ahora también por brizna y
/// no sólo por nivel.
fn spike_from_m(scale: f32, settings: &GrassRendererSettings) -> f32 {
    settings.blade_width_m / (settings.leaf_min_pixels * scale).max(1e-6)
}

/// Como mucho un chunk se rehace por frame **mientras la grilla rueda**: cruzar
/// una frontera cuesta un chunk, no un anillo.
const CHUNKS_BAKED_PER_FRAME: usize = 1;

/// Filling an empty grid ignores the per-frame limit and bakes the lot in one
/// frame: a scene that starts with a hole in the meadow is worse than one hitch.
const FILL_IN_ONE_FRAME: bool = true;

/// Cuántos segundos tarda un chunk recién horneado en llegar al
/// desvanecimiento por distancia que ya tenía prometido (`blade_growth` en
/// `grass.wgsl`). Ataca el pop de chunk cuadrado en anillo 0/1 —confirmado
/// jugando el 2026-08-13, sólo caminando, con `growth_ramp` en su default—:
/// ese mecanismo desvanece una brizna cerca de **su propio** alcance, sin
/// noción de cuándo nació el chunk que la contiene, así que la mayoría de las
/// briznas de un chunk recién horneado no están cerca de su límite propio y
/// aparecen a altura completa de una. Fuera de `GrassRendererSettings` a
/// propósito, mismo motivo que [`GrowthRampOverride`]: sólo escala un número
/// de uniform, no cambia qué chunks existen, así que meterlo ahí costaría un
/// rehorneado completo por nada.
const CHUNK_FADE_IN_S: f32 = 0.35;

/// Centinela para "este chunk ya terminó de crecer, no arranques el fundido
/// de nuevo" — usado cuando una celda cambia de casillero sin ser brizna
/// nueva (reasignación de tier del anillo 0, ver `is_reassignment` en
/// `roll_meadow_grid`). Suficientemente negativo para que
/// `(reloj_actual - esto) / CHUNK_FADE_IN_S` sature en 1.0 sin desbordar a
/// `f32::INFINITY` ni `NaN` para ningún reloj real.
const ALREADY_GROWN_BORN_AT: f32 = -1.0e9;

/// Un chunk de la pradera: una entidad, un casillero del buffer de su nivel.
#[derive(Component)]
pub(super) struct GrassChunk;

/// Which chunk of which ring, and which internal buffer tier of that ring. The
/// ring is part of the identity because the same patch of ground is covered by
/// different chunks at different distances; the tier is part of it for the
/// same reason inside a single ring (`RING0_TIERS`) — always `0` for rings
/// that only have one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ChunkKey {
    ring: usize,
    tier: usize,
    cell: IVec2,
}

/// La grilla viva: qué chunks existen ahora y lo que cada nivel tiene en la GPU.
///
/// **Un material por nivel**: cada uno lleva su stride y su forma, y el de las
/// cartas además `AlphaMode::AlphaToCoverage`. No cuesta draws — es lo que los junta.
#[derive(Resource)]
pub(crate) struct GrassField {
    /// `[ring][tier]`. Sólo el anillo 0 usa más de un casillero de tier — los
    /// demás dejan `1..MAX_TIERS` sin usar (`tier_count`).
    materials: [[Handle<GrassMaterial>; MAX_TIERS]; GRASS_RING_COUNT],
    records: [[RingRecords; MAX_TIERS]; GRASS_RING_COUNT],
    live: HashMap<ChunkKey, Entity>,
    /// Qué candidata de carta llevan los materiales ahora mismo. Vive acá y no
    /// en un `Local` de sistema: el campo se recrea entera al entrar a
    /// escena, y un `Local` sobrevive a esa entrada — comparar contra un
    /// estado viejo dejaría los materiales frescos con la textura de la
    /// sesión anterior hasta que la perilla *cambiara* de nuevo.
    card_candidate_step: usize,
}

/// Facts for the controlled grass lab, not a second culling mechanism.
///
/// `resident` is what the rolling grid currently holds in the renderer;
/// `frustum` is Bevy's per-view visibility verdict for those entities. Neither
/// claims to be a pixel count or occlusion result: a mountain can still hide a
/// frustum-visible chunk in the depth test. Keeping those terms separate is how
/// the lab tells whether a later occlusion experiment is actually useful.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GrassLabStats {
    pub resident_chunks: [usize; GRASS_RING_COUNT],
    pub frustum_chunks: [usize; GRASS_RING_COUNT],
    pub resident_triangles: [usize; GRASS_RING_COUNT],
    pub frustum_triangles: [usize; GRASS_RING_COUNT],
    /// Desglose por tier **sólo del anillo 0** — el único que se parte en
    /// más de un buffer (`RING0_TIERS`). Los demás anillos siguen contados
    /// enteros arriba; esto es lo nuevo que el escalonado agrega a mirar.
    pub ring0_tier_chunks: [usize; RING0_TIERS],
    pub ring0_tier_triangles: [usize; RING0_TIERS],
}

/// Snapshot the field after rolling it. This deliberately reads `ViewVisibility`
/// rather than guessing from camera distance: the latter is precisely the
/// distinction the lab exists to expose.
pub(super) fn collect_grass_lab_stats(
    field: Res<GrassField>,
    visibility: Query<&ViewVisibility, With<GrassChunk>>,
    perf: Res<crate::perf::PerfToggles>,
    mut stats: ResMut<GrassLabStats>,
) {
    let mut next = GrassLabStats::default();
    let triangles_per_blade = submitted_triangles_per_blade(&perf);
    for (key, entity) in &field.live {
        let ring = key.ring;
        let triangles = field.records[ring][key.tier].stride as usize * triangles_per_blade;
        next.resident_chunks[ring] += 1;
        next.resident_triangles[ring] += triangles;
        if ring == 0 {
            next.ring0_tier_chunks[key.tier] += 1;
            next.ring0_tier_triangles[key.tier] += triangles;
        }
        if visibility.get(*entity).is_ok_and(|visible| visible.get()) {
            next.frustum_chunks[ring] += 1;
            next.frustum_triangles[ring] += triangles;
        }
    }
    if *stats != next {
        *stats = next;
    }
}

/// Cuántas briznas manda un anillo en vista, sumando chunk por chunk — no
/// `chunks × blades_per_chunk` como cuando cada chunk de un anillo llevaba el
/// mismo stride. Con tiers, cada chunk del anillo 0 paga el de **su propio**
/// tier, así que hay que sumarlos uno por uno.
#[cfg(test)]
fn ring_blades_in_view(
    ring: usize,
    focus: Vec2,
    dial: f32,
    scale: f32,
    reach_scale: f32,
    settings: &GrassRendererSettings,
) -> usize {
    ring_cells(ring, focus, reach_scale, settings)
        .into_iter()
        .map(|cell| {
            let tier = if ring == 0 {
                ring0_tier_for(
                    chunk_nearest_m(cell, settings.rings[ring].chunk_m, focus),
                    reach_scale,
                    settings,
                )
            } else {
                0
            };
            blades_per_chunk(ring, tier, dial, scale, reach_scale, settings) as usize
        })
        .sum()
}

/// Triángulos que la pradera declara a la escena, para `perf::budget`.
/// Declarados, no dibujados: el frustum descarta buena parte y cuánta es una
/// incógnita, no una medición.
#[cfg(test)]
pub(crate) fn meadow_triangles() -> usize {
    // La malla índice reserva dos para todas las formas. Que la púa degenere uno
    // después del vertex shader no lo borra de la geometría enviada.
    let settings = GrassRendererSettings::default();
    let period = settings
        .rings
        .iter()
        .map(|ring| ring.chunk_m)
        .fold(0.0_f32, f32::max);
    let mut worst = 0;
    for z in 0..8 {
        for x in 0..8 {
            let focus = Vec2::new(x as f32, z as f32) * (period / 8.0);
            let triangles: usize = (0..GRASS_RING_COUNT)
                .map(|ring| {
                    ring_blades_in_view(
                        ring,
                        focus,
                        REFERENCE_DENSITY,
                        reference_scale(),
                        REFERENCE_REACH,
                        &settings,
                    ) * SUBMITTED_TRIANGLES_PER_BLADE
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
    let settings = GrassRendererSettings::default();
    let period = settings
        .rings
        .iter()
        .map(|ring| ring.chunk_m)
        .fold(0.0_f32, f32::max);
    let steps = 8;
    let mut worst = 0;
    for z in 0..steps {
        for x in 0..steps {
            let offset = Vec2::new(x as f32, z as f32) * (period / steps as f32);
            worst = worst.max(neighbourhood_blades(offset, &settings));
        }
    }
    worst
}

/// Blades standing around a camera at `focus`.
#[cfg(test)]
fn neighbourhood_blades(focus: Vec2, settings: &GrassRendererSettings) -> usize {
    (0..GRASS_RING_COUNT)
        .map(|ring| {
            ring_blades_in_view(
                ring,
                focus,
                REFERENCE_DENSITY,
                reference_scale(),
                REFERENCE_REACH,
                settings,
            )
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
/// El rango de un tier corta en **su propio** borde interno — el del anillo
/// para el único tier de los anillos 1/2, el de su banda para cada uno de los
/// `RING0_TIERS` del anillo 0. Un tier lejano del anillo 0 pide un rango más
/// chico, porque su borde interno es mayor y la escalera ya bajó para cuando
/// llega ahí.
fn tile_ranges(
    dial: f32,
    scale: f32,
    reach_scale: f32,
    settings: &GrassRendererSettings,
) -> [[std::ops::Range<u32>; MAX_TIERS]; GRASS_RING_COUNT] {
    let ladder = grass_tiles::reach_ladder(dial, scale, reach_scale, settings);
    let total = u32::try_from(ladder.len()).unwrap_or(u32::MAX);
    let cutoff = |inner: f32| -> u32 {
        ladder
            .iter()
            .position(|blade_reach| blade_reach.floor() < inner)
            .and_then(|end| u32::try_from(end).ok())
            .unwrap_or(total)
            .min(total)
    };
    std::array::from_fn(|ring| {
        if ring == 0 {
            let bounds = ring0_tier_bounds(reach_scale, settings);
            std::array::from_fn(|tier| {
                if tier < RING0_TIERS {
                    // La histéresis (`ring0_tier_with_hysteresis`) retiene un chunk
                    // en este tier hasta que su punto más cercano baja a
                    // `bounds[tier] - KEEP_SLACK_M`, no sólo hasta `bounds[tier]`.
                    // Presupuestar para `bounds[tier]` a secas dejaba un chunk
                    // retenido sin las briznas que su borde real exige — un agujero
                    // de densidad que se veía al acercarse a una frontera de tier
                    // (encontrado auditando el 2026-08-13, nunca lo cubrió ningún
                    // test porque probaban la asignación limpia, no la retenida).
                    let budgeted_bound = if tier == 0 {
                        bounds[tier]
                    } else {
                        (bounds[tier] - KEEP_SLACK_M).max(bounds[0])
                    };
                    0..cutoff(budgeted_bound)
                } else {
                    0..0
                }
            })
        } else {
            let range = 0..cutoff(band_inner(ring, reach_scale, settings));
            std::array::from_fn(|tier| if tier == 0 { range.clone() } else { 0..0 })
        }
    })
}

/// Cuántas baldosas de mundo entran en un chunk de este nivel, por lado.
///
/// Entero por construcción: los lados de chunk (8, 16, 32) son múltiplos del
/// lado de la baldosa. Si dejaran de serlo, un chunk cubriría media baldosa y
/// dos chunks plantarían la misma brizna.
fn tiles_per_chunk_side(index: usize, settings: &GrassRendererSettings) -> u32 {
    tiles_per_side(settings.rings[index].chunk_m)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "chunk sides are small multiples of the tile side by construction"
)]
fn tiles_per_side(chunk_m: f32) -> u32 {
    u32::try_from((chunk_m / grass_tiles::TILE_M).round().max(1.0) as i64).unwrap_or(1)
}

/// Blades in one chunk of `(ring, tier)` at a given dial setting. Rounded
/// once, here, so the count on screen and the count in the budget are the
/// same number.
fn blades_per_chunk(
    index: usize,
    tier: usize,
    dial: f32,
    scale: f32,
    reach_scale: f32,
    settings: &GrassRendererSettings,
) -> u32 {
    let ranges = tile_ranges(dial, scale, reach_scale, settings);
    let per_tile = ranges
        .get(index)
        .and_then(|tiers| tiers.get(tier))
        .map_or(0, |range| range.end - range.start);
    let side = tiles_per_chunk_side(index, settings);
    per_tile.saturating_mul(side).saturating_mul(side)
}

/// Qué celdas de un nivel existen con la cámara en `focus`. **Decide qué se
/// tiene en memoria, no qué se ve**: desde que la brizna se descarta por su
/// propia distancia (`blade_growth` en `grass.wgsl`), esto es un test conservador
/// —se conserva el chunk que pueda tener *alguna* brizna viva—. Cuando decidía la
/// imagen, el campo aparecía y desaparecía en cuadrados de 32 m.
fn ring_cells(
    index: usize,
    focus: Vec2,
    reach_scale: f32,
    settings: &GrassRendererSettings,
) -> Vec<IVec2> {
    ring_cells_with_slack(index, focus, 0.0, reach_scale, settings)
}

/// A ring's reach with the dial applied, in **whole metres** — a hard
/// constraint, not tidiness: the reach travels in the integer part of `uv1.y`
/// with the blade's height in the fraction.
fn ring_reach(index: usize, reach_scale: f32, settings: &GrassRendererSettings) -> f32 {
    (settings.rings[index].reach_m * reach_scale)
        .round()
        .max(1.0)
}

/// How far past its reach a chunk is kept. Without it a camera on a grid line
/// rebakes the same chunk every frame, which on screen is a patch flickering.
const KEEP_SLACK_M: f32 = 3.0;

#[expect(
    clippy::cast_possible_truncation,
    reason = "chunk coordinates are small integers by construction"
)]
fn ring_cells_with_slack(
    index: usize,
    focus: Vec2,
    slack: f32,
    reach_scale: f32,
    settings: &GrassRendererSettings,
) -> Vec<IVec2> {
    let ring = &settings.rings[index];
    let base_reach_m = ring_reach(index, reach_scale, settings) + slack;
    let inner_reach = index
        .checked_sub(1)
        .map_or(0.0, |i| ring_reach(i, reach_scale, settings));
    let half = ring.chunk_m * 0.5;
    // Un cupo de holgura, más el tope del ruido de frontera si está prendido
    // — si no, una celda que sólo entra por el ruido nunca se visita en el
    // loop de abajo. Gateado por la misma función que decide el jitter en
    // sí, para que nunca puedan discrepar sobre cuánto puede correr esta
    // celda (y para no agrandar el radio de búsqueda cuando el ruido está
    // apagado, el caso por default).
    let jitter_cap = ring_boundary_jitter_cap_m(index, settings);
    let span = ((base_reach_m + jitter_cap) / ring.chunk_m).ceil() as i32 + 1;
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
            let nearest = chunk_nearest_m(cell, ring.chunk_m, focus);
            let farthest = (offset + Vec2::splat(half)).length();
            // El borde interno de la corona: desde que los niveles se anidan, el
            // de adentro dibuja *las mismas briznas* hasta su alcance, así que
            // este no tiene nada que hacer ahí. Sólo el `slack` de histéresis.
            // **Sin ruido**, a propósito: el anillo de adentro nunca cubre menos
            // que su alcance limpio (el ruido sólo suma), así que excluir hasta
            // ahí sigue siendo seguro sin importar si el de afuera tiene ruido.
            let handover = (inner_reach - slack).max(0.0);
            // El corte exterior sí lleva el ruido — es la frontera que se ve
            // como un círculo perfecto, y la que Técnica 2 rompe.
            let reach_m = base_reach_m + ring_boundary_jitter_m(cell, index, settings);
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

/// A grass renderer chunk only exists if authored grass reaches its footprint.
/// The cells remain the source of truth; this is merely a coarse culling query
/// over the chunk grid that cameras roll through.
fn grass_chunk_has_growth(
    cell: IVec2,
    ring: usize,
    terrain: Option<&crate::world::Terrain>,
    settings: &GrassRendererSettings,
) -> bool {
    let chunk_m = settings.rings[ring].chunk_m;
    let centre = cell_centre(cell, chunk_m);
    let half = Vec2::splat(chunk_m * 0.5);
    let Some(terrain) = terrain else {
        return false;
    };
    terrain.contains_kind_in_rect(
        centre - half,
        centre + half,
        crate::world::TerrainKind::ShortGrass,
    ) || terrain.contains_kind_in_rect(
        centre - half,
        centre + half,
        crate::world::TerrainKind::TallGrass,
    )
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
    asset_server: Res<AssetServer>,
    settings: Res<GrassRendererSettings>,
    perf: Res<crate::perf::PerfToggles>,
) {
    // Un buffer y un material **por tier** — un material sólo puede tener un
    // buffer bindeado a la vez, así que dos tiers no pueden compartir uno
    // aunque compartan anillo. Los anillos sin más de un tier (`tier_count`)
    // sólo usan el casillero 0; el resto queda creado pero sin chunk que lo
    // use nunca, un desperdicio de unos pocos handles vacíos, no de GPU.
    // El buffer arranca con un registro de relleno porque un `ShaderBuffer`
    // vacío no es un binding válido, y el material no puede declararlo
    // opcional: el macro de `AsBindGroup` no pasa por `Option`.
    let records: [[RingRecords; MAX_TIERS]; GRASS_RING_COUNT] = std::array::from_fn(|_ring| {
        std::array::from_fn(|_tier| {
            // `default()` y no `RENDER_WORLD`: con este último Bevy suelta el
            // dato de CPU en cuanto lo sube, y este buffer se reescribe cada
            // vez que la grilla rueda. El segundo buffer (nacimiento) es el
            // mismo trato, sólo que un `f32` por casillero en vez de por
            // brizna.
            RingRecords::new(
                buffers.add(ShaderBuffer::new(
                    &[0_u8; RECORD_BYTES],
                    RenderAssetUsages::default(),
                )),
                buffers.add(ShaderBuffer::new(
                    &0_f32.to_le_bytes(),
                    RenderAssetUsages::default(),
                )),
            )
        })
    });
    // Una sola textura compartida por todos los materiales: no agrega batches
    // y cubre las briznas del anillo lejano que, por distancia individual,
    // aún están construidas como hoja o púa.
    let card_albedo = asset_server.load(card_albedo_path(perf.grass_card_candidate_label()));
    let materials: [[Handle<GrassMaterial>; MAX_TIERS]; GRASS_RING_COUNT] =
        std::array::from_fn(|ring| {
            std::array::from_fn(|tier| {
                let mut material = grass_material(&settings);
                material.extension.blade_records = records[ring][tier].buffer.clone();
                material.extension.chunk_born_at = records[ring][tier].born_buffer.clone();
                material.extension.card_albedo = Some(card_albedo.clone());
                if shape_for_ring(ring, reference_scale(), REFERENCE_REACH, &settings)
                    .faces_camera()
                {
                    // Recomendado por Bevy para foliage: borde antialiaseado
                    // por cobertura de MSAA en vez de un corte binario, sin el
                    // costo de ordenar que tiene `Blend`. Necesita MSAA
                    // prendido — si no, Bevy la hace caer a comportarse como
                    // `Mask` sola.
                    material.base.alpha_mode = AlphaMode::AlphaToCoverage;
                }
                materials.add(material)
            })
        });
    commands.insert_resource(GrassField {
        materials,
        records,
        live: HashMap::default(),
        card_candidate_step: perf.grass_card_candidate_step,
    });
}

/// Recarga la textura de carta cuando `PerfKnob::GrassCardCandidate` cambia.
///
/// Separado de `track_meadow_focus`: eso corre cada frame porque el uniform
/// sigue a la cámara, esto sólo hace algo el frame en que la perilla se
/// movió. No rehornea la grilla — ninguna candidata cambia densidad ni
/// alcance, sólo qué PNG lee el fragment.
pub(super) fn apply_card_candidate(
    mut field: ResMut<GrassField>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<GrassMaterial>>,
    perf: Res<crate::perf::PerfToggles>,
) {
    if field.card_candidate_step == perf.grass_card_candidate_step {
        return;
    }
    field.card_candidate_step = perf.grass_card_candidate_step;
    let card_albedo = asset_server.load(card_albedo_path(perf.grass_card_candidate_label()));
    for handle in field.materials.iter().flatten() {
        if let Some(mut material) = materials.get_mut(handle) {
            material.extension.card_albedo = Some(card_albedo.clone());
        }
    }
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
    for tier in field.records.iter_mut().flatten() {
        tier.upload(&mut buffers);
    }
    memory.bytes = field
        .records
        .iter()
        .flatten()
        .map(RingRecords::buffer_bytes)
        .sum();
    memory.chunks = field
        .records
        .iter()
        .flatten()
        .map(RingRecords::chunks)
        .sum();
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
fn chunk_bounds(
    centre: Vec2,
    chunk_m: f32,
    ground: std::ops::RangeInclusive<f32>,
    settings: &GrassRendererSettings,
) -> Aabb {
    let margin = settings.card_width_m * 0.5;
    let half = chunk_m * 0.5 + margin;
    Aabb::from_min_max(
        Vec3::new(
            centre.x - half,
            ground.start() - settings.growth_sink_m - settings.blade_root_sink_m - margin,
            centre.y - half,
        ),
        Vec3::new(
            centre.x + half,
            ground.end() + settings.blade_height_max_m + settings.blade_lean_m + margin,
            centre.y + half,
        ),
    )
}

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
fn grass_material(settings: &GrassRendererSettings) -> GrassMaterial {
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
                root_color: settings.root_color,
                tip_color: settings.tip_color,
                ..default()
            },
            interaction_map: None,
            card_albedo: None,
            // Los dos los llena `init_meadow_material`, que es quien tiene el
            // `Assets<ShaderBuffer>`.
            blade_records: Handle::default(),
            chunk_born_at: Handle::default(),
        },
    }
}

/// Qué dispara un rehorneado completo de la grilla: los tres pasos de perilla
/// que cambian cuántas briznas tiene un chunk, más la configuración efectiva
/// (ya con el banco de forma aplicado) que decide qué forma tiene cada anillo.
type MeadowRebuildDials = (usize, usize, usize, usize, GrassRendererSettings);

/// Roll the grid: drop the chunks that fell behind, bake the ones ahead.
///
/// Reads the camera rather than the player because the LOD has to answer to what
/// the screen shows: zoom out or swing the camera away and it is the camera's
/// neighbourhood that needs blades, not the player's.
#[expect(
    clippy::too_many_arguments,
    reason = "rolling owns ECS entities, mesh assets, field state, terrain sampling and now the clock a freshly baked chunk stamps itself with"
)]
pub(super) fn roll_meadow_grid(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut field: ResMut<GrassField>,
    perf: Res<crate::perf::PerfToggles>,
    settings: Res<GrassRendererSettings>,
    scene: Res<State<crate::scene::AppState>>,
    terrain: TerrainAccess,
    camera: Option<Single<&GlobalTransform, With<Camera3d>>>,
    time: Res<Time>,
    mut dial: Local<Option<MeadowRebuildDials>>,
) {
    let Some(camera) = camera else {
        return;
    };
    let settings = shape_bench_settings(&perf, *settings);

    // Painting semantic terrain changes which chunks exist. The first vertical
    // slice deliberately re-bakes the *visible meadow*, not the entire map:
    // correctness is immediate in the World Lab and camera rolling keeps the
    // work bounded. A later dirty-rectangle event can narrow this to just the
    // stroke's chunks without changing the world-space ownership below.
    if terrain.changed().next().is_some() {
        for entity in field.live.values() {
            commands.entity(*entity).despawn();
        }
        field.live.clear();
        for tier in field.records.iter_mut().flatten() {
            tier.reset();
        }
    }

    let terrain = terrain.primary();

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
        perf.grass_shape_bench_step,
        settings,
    );
    if dial.replace(dials) != Some(dials) {
        for entity in field.live.values() {
            commands.entity(*entity).despawn();
        }
        field.live.clear();
        // La malla índice tiene tantos vértices como briznas lleva un chunk, así
        // que una perilla que cambia esa cuenta la invalida entera — igual que
        // invalida los chunks horneados.
        for tier in field.records.iter_mut().flatten() {
            tier.reset();
        }
    }

    // Una malla índice por (nivel, tier), creada una vez por configuración. Es
    // lo que hace que sus chunks batcheen entre sí: Bevy exige el mismo
    // `Handle<Mesh>`, y todos los chunks de un mismo tier llevan la misma
    // cantidad de briznas — la razón de ser de partir el buffer en tiers.
    for ring in 0..GRASS_RING_COUNT {
        for tier in 0..tier_count(ring) {
            let blades = blades_per_chunk(ring, tier, density, scale, reach_scale, &settings);
            if field.records[ring][tier].mesh.is_none() && planted_ring(&perf, ring) && blades > 0 {
                // **Dos triángulos para todos, no el de la forma del nivel** —
                // salvo que el banco fuerce "solo púa"
                // (`submitted_triangles_per_blade`): ahí no hay mezcla de
                // formas que proteger. Fuera de eso, la forma la decide la
                // distancia, y con un solo triángulo indexado una brizna
                // cercana de un nivel de púas salía **media hoja**. La púa no
                // paga el segundo: sus esquinas 2 y 3 caen en la punta y
                // degenera.
                field.records[ring][tier].mesh = Some(meshes.add(ring_index_mesh(
                    blades,
                    submitted_triangles_per_blade(&perf),
                )));
                field.records[ring][tier].stride = blades;
            }
        }
    }

    let planted = |ring: usize| planted_ring(&perf, ring);

    // El tier de un chunk del anillo 0 sale de su punto más cercano al foco,
    // con histéresis contra lo que ya está vivo (`ring0_tier_with_hysteresis`)
    // — `wanted` y `keep_set` tienen que llamar exactamente a la misma
    // función, o podrían acordar tiers distintos para el mismo chunk y
    // duplicar su geometría (ver el comentario de esa función).
    let settings_value = settings;
    let field_ref: &GrassField = &field;
    let chunk_key = move |ring: usize, cell: IVec2| -> ChunkKey {
        let tier = if ring == 0 {
            let nearest = chunk_nearest_m(cell, settings_value.rings[0].chunk_m, focus);
            ring0_tier_with_hysteresis(field_ref, cell, nearest, reach_scale, &settings_value)
        } else {
            0
        };
        ChunkKey { ring, tier, cell }
    };

    let wanted: HashSet<ChunkKey> = settings_value
        .rings
        .iter()
        .enumerate()
        .filter(|(ring, _)| planted(*ring))
        .flat_map(|(ring, _)| {
            ring_cells(ring, focus, reach_scale, &settings)
                .into_iter()
                .filter(move |cell| grass_chunk_has_growth(*cell, ring, terrain, &settings_value))
                .map(move |cell| chunk_key(ring, cell))
        })
        .collect();

    // Kept with slack, created without: a chunk on the boundary stays instead of
    // being re-baked every other frame, which is what the flicker was.
    let keep_set: HashSet<ChunkKey> = settings_value
        .rings
        .iter()
        .enumerate()
        .filter(|(ring, _)| planted(*ring))
        .flat_map(|(ring, _)| {
            ring_cells_with_slack(ring, focus, KEEP_SLACK_M, reach_scale, &settings)
                .into_iter()
                .filter(move |cell| grass_chunk_has_growth(*cell, ring, terrain, &settings_value))
                .map(move |cell| chunk_key(ring, cell))
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
    // Una celda del anillo 0 que se soltó por un cambio de tier —no porque
    // salió de la grilla— tiene que volver **este mismo cuadro**, no esperar
    // su turno. Es la misma celda migrando de buffer, y el jugador la ve como
    // parte del mismo campo; hacerla esperar el presupuesto de chunks nuevos
    // es lo que hacía desaparecer cuadrados enteros al cruzar una frontera de
    // tier con varios juntos — el bug que encontró jugando el 2026-08-13.
    let reassigned_cells: HashSet<IVec2> = dropped
        .iter()
        .filter(|key| key.ring == 0)
        .map(|key| key.cell)
        .collect();
    for key in &dropped {
        field.records[key.ring][key.tier].release(key.cell);
    }

    // An empty grid is being filled, not rolled: bake it whole rather than
    // letting the meadow grow in around the player over several seconds.
    let budget = if FILL_IN_ONE_FRAME && field.live.is_empty() {
        usize::MAX
    } else {
        CHUNKS_BAKED_PER_FRAME
    };
    let is_reassignment = |key: &ChunkKey| key.ring == 0 && reassigned_cells.contains(&key.cell);
    // Las reasignadas entran todas, sin presupuesto — hornear es "casi nulo"
    // (ver el log de abajo) y son pocas por cuadro salvo un salto de cámara
    // grande. Las genuinamente nuevas siguen esperando su turno.
    let mut missing: Vec<ChunkKey> = wanted
        .iter()
        .filter(|key| !field.live.contains_key(*key) && is_reassignment(key))
        .copied()
        .collect();
    missing.extend(
        wanted
            .iter()
            .filter(|key| !field.live.contains_key(*key) && !is_reassignment(key))
            .take(budget)
            .copied(),
    );
    // El único trabajo por frame que este sistema tiene. Desde el Paso 2 no
    // hornea geometría: sortea las briznas y escribe sus registros, que es lo que
    // convirtió "conviene instancing" de opinión en decisión.
    let bake_started = std::time::Instant::now();
    // Sin wrap — ver el comentario de `GrassUniform::chunk_clock` sobre por
    // qué el reloj del viento no sirve acá.
    let now = time.elapsed_secs();
    // La escalera de alcances y el reparto de índices son del **campo**, no del
    // chunk: se arman una vez por tanda en vez de una por chunk, que es lo único
    // que este rediseño le agrega al horneado.
    let ladder = std::sync::Arc::new(grass_tiles::reach_ladder(
        density,
        scale,
        reach_scale,
        &settings,
    ));
    let ranges = tile_ranges(density, scale, reach_scale, &settings);
    for key in &missing {
        let ring = &settings.rings[key.ring];
        let centre = cell_centre(key.cell, ring.chunk_m);
        let planting = build_chunk_records(
            &ChunkSpec {
                centre,
                chunk_m: ring.chunk_m,
                blades: ranges
                    .get(key.ring)
                    .and_then(|tiers| tiers.get(key.tier))
                    .cloned()
                    .unwrap_or(0..0),
                ladder: std::sync::Arc::clone(&ladder),
            },
            terrain,
            &settings,
        );
        let slot = field.records[key.ring][key.tier].slot_for(key.cell);
        // Una reasignación de tier (anillo 0, histéresis) no es una brizna
        // nueva: es la misma celda cambiando de casillero. Si arrancara el
        // fundido de nuevo ahí, reabriría exactamente el pop que la
        // histéresis ya tapa — el anillo 0 reasigna tier todo el tiempo al
        // caminar cerca de una frontera. `ALREADY_GROWN_BORN_AT` lo salta.
        let born_at = if is_reassignment(key) {
            ALREADY_GROWN_BORN_AT
        } else {
            now
        };
        field.records[key.ring][key.tier].write(slot, &planting.records, born_at);
        let entity = commands
            .spawn((
                DespawnOnExit(*scene.get()),
                Name::new(format!(
                    "GrassChunk_r{}t{}_{}_{}",
                    key.ring, key.tier, key.cell.x, key.cell.y
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
                Mesh3d(
                    field.records[key.ring][key.tier]
                        .mesh
                        .clone()
                        .unwrap_or_default(),
                ),
                MeshTag(slot),
                MeshMaterial3d(field.materials[key.ring][key.tier].clone()),
                // **El AABB va a mano, y con `NoAutoAabb`.** Bevy lo deriva de
                // las posiciones de la malla, que en una malla índice son todas
                // cero — un punto en el origen, y el nivel entero culleado. Y no
                // alcanza con insertarlo: `calculate_bounds` lo *sobrescribe*
                // cuando `Mesh3d` cambia, cosa que pasa en todo chunk recién
                // nacido, así que hace falta además marcarlo.
                chunk_bounds(centre, ring.chunk_m, planting.ground.clone(), &settings),
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
        for index in 0..GRASS_RING_COUNT {
            let live = field.live.keys().filter(|key| key.ring == index).count();
            // Suma el stride real de cada chunk vivo en vez de
            // `chunks × blades_per_chunk`: con tiers, los chunks de un mismo
            // anillo ya no llevan todos el mismo stride.
            let blades: usize = field
                .live
                .keys()
                .filter(|key| key.ring == index)
                .map(|key| field.records[key.ring][key.tier].stride as usize)
                .sum();
            debug!(
                "[grass]   anillo {index}: {live} chunks, {blades} primitivas, {} tris",
                blades * submitted_triangles_per_blade(&perf),
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
#[expect(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "a rendering-driving system reads camera, window, sun and ambient plus its own state"
)]
pub(super) fn track_meadow_focus(
    field: Res<GrassField>,
    mut materials: ResMut<Assets<GrassMaterial>>,
    camera: Option<Single<(&GlobalTransform, &Projection), With<Camera3d>>>,
    window: Option<Single<&Window, With<bevy::window::PrimaryWindow>>>,
    // Sol y luna llevan los dos `DirectionalLight` y coexisten en toda escena:
    // sin desambiguar, `Single` ve "más de una" y `Option` lo traga como
    // `None` en silencio. Por esto `sun_direction` quedó congelado siempre.
    sun: Option<
        Single<
            (&GlobalTransform, &DirectionalLight),
            (
                With<crate::world::day_night::Sun>,
                Without<crate::world::day_night::MoonLight>,
            ),
        >,
    >,
    ambient: Res<GlobalAmbientLight>,
    perf: Res<crate::perf::PerfToggles>,
    growth_override: Res<GrowthRampOverride>,
    settings: Res<GrassRendererSettings>,
    time: Res<Time>,
) {
    // **El reparto del buffer se escribe aunque no haya cámara** (2026-08-07).
    // Con `record_layout` en su default, todo chunk lee el casillero 0 y las
    // cartas se construyen como hojas: el nivel lejano desaparece, y no por un
    // frame sino hasta la siguiente escritura. El caso y su síntoma, en
    // `BOTWGrass.md`.
    let settings = shape_bench_settings(&perf, *settings);
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
    // Forma, índice de anillo y borde interno son del **anillo**, no del
    // tier — un tier es sólo cómo se parte el buffer, invisible para el
    // shader. El stride sí es de cada tier, y se pisa por separado abajo:
    // `layouts[ring].x` queda en 0 acá porque ningún tier lo comparte.
    let layouts: Vec<UVec4> = (0..GRASS_RING_COUNT)
        .map(|ring| {
            UVec4::new(
                0,
                shape_for_ring(ring, scale, reach_scale, &settings).shader_index(),
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
                metres_as_u32(band_inner(ring, reach_scale, &settings)),
            )
        })
        .collect();
    let data = camera.as_ref().map(|camera| {
        let (camera, _) = **camera;
        meadow_uniform(
            camera,
            sun.as_ref().map(|sun| **sun),
            &ambient,
            &perf,
            &growth_override,
            &time,
            scale,
            &settings,
        )
    });
    for (ring, layout) in layouts.iter().enumerate() {
        for tier in 0..tier_count(ring) {
            let handle = &field.materials[ring][tier];
            let Some(mut material) = materials.get_mut(handle) else {
                continue;
            };
            if let Some(data) = &data {
                material.extension.grass_data = GrassUniform { ..*data };
            }
            let mut layout = *layout;
            layout.x = field.records[ring][tier].stride;
            material.extension.grass_data.record_layout = layout;
            material.base.alpha_mode =
                if shape_for_ring(ring, scale, reach_scale, &settings).faces_camera() {
                    AlphaMode::AlphaToCoverage
                } else {
                    AlphaMode::Opaque
                };
        }
    }
}

/// El uniform de la pradera, armado una vez para los dos materiales: separado
/// del sistema para que no haya forma de escribir uno y olvidar el otro.
#[expect(
    clippy::too_many_arguments,
    reason = "arma el uniform entero a partir de todo lo que `track_meadow_focus` ya leyó; \
              partirlo no reduce nada, sólo mueve el conteo a un struct intermedio"
)]
fn meadow_uniform(
    camera: &GlobalTransform,
    sun: Option<(&GlobalTransform, &DirectionalLight)>,
    ambient: &GlobalAmbientLight,
    perf: &crate::perf::PerfToggles,
    growth_override: &GrowthRampOverride,
    time: &Time,
    screen_scale: f32,
    settings: &GrassRendererSettings,
) -> GrassUniform {
    let mut uniform = grass_material(settings).extension.grass_data;
    let data = &mut uniform;
    data.focus_xz = camera.translation().xz();
    data.growth_ramp = growth_band(perf, growth_override);
    // Sin wrap — el mismo reloj que estampó `chunk_born_at` al hornear. Ver
    // el comentario de `GrassUniform::chunk_clock`.
    data.chunk_clock = time.elapsed_secs();
    data.chunk_fade_in_s = CHUNK_FADE_IN_S;
    data.spike_from_m = spike_from_m(screen_scale, settings);
    data.card_from_m = card_from_m(screen_scale, settings);
    let (a, b) = ring_reaches(perf.grass_reach_scale(), settings);
    data.ring_reaches_a = a;
    data.ring_reaches_b = b;
    let (a, b) = ring_chunks(settings);
    data.ring_chunks_a = a;
    data.ring_chunks_b = b;
    let (a, b) = ring_cards(screen_scale, perf.grass_reach_scale(), settings);
    data.ring_cards_a = a;
    data.ring_cards_b = b;
    data.card_half_width = settings.card_width_m * 0.5;
    data.debug_view =
        grass_debug::GrassDebugView::from_step(perf.grass_debug_step()).shader_index();
    // Desde la constante, no repetido en el default del uniform: la vista
    // `subpixel` divide por esto para decir cuántos píxeles mide una brizna, y
    // un ancho desactualizado daría un veredicto con la precisión intacta.
    data.blade_width = settings.blade_width_m;
    for (slot, colour) in data.ring_colors.iter_mut().enumerate() {
        *colour = Vec4::from(grass_debug::slot_color(slot).to_f32_array());
    }
    data.growth_sink = settings.growth_sink_m;
    data.blade_root_sink = settings.blade_root_sink_m;
    data.blade_lean = settings.blade_lean_m;
    data.blade_waist = settings.blade_waist;
    // The wind is a function of world position and time — there is no per-blade
    // state anywhere, which is why a field of a hundred thousand blades costs
    // one uniform write a frame.
    data.time = time.elapsed_secs_wrapped();
    // Both the backlit transmission and the fragment's own diffuse (no
    // `apply_pbr_lighting`, ver `grass.wgsl`) need this; normalized against
    // `day_night`'s own noon reference so it fades at night, not just at dusk.
    if let Some((sun_transform, light)) = sun {
        data.sun_direction = sun_transform.back().as_vec3();
        let linear = LinearRgba::from(light.color);
        let strength = (light.illuminance / crate::world::day_night::SUN_NOON_LUX).clamp(0.0, 1.0);
        data.sun_color = Vec4::new(linear.red, linear.green, linear.blue, 1.0) * strength;
    }
    let ambient_linear = LinearRgba::from(ambient.color);
    let ambient_strength =
        (ambient.brightness / crate::world::day_night::AMBIENT_DAY).clamp(0.0, 1.0);
    data.ambient_color = Vec4::new(
        ambient_linear.red,
        ambient_linear.green,
        ambient_linear.blue,
        1.0,
    ) * ambient_strength;
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

pub(super) fn ring_facts(
    perf: &crate::perf::PerfToggles,
    settings: &GrassRendererSettings,
) -> Vec<RingFacts> {
    let dial = perf.grass_density();
    let reach_scale = perf.grass_reach_scale();
    // La escalera de **referencia**, no la del viewport de la corrida: acompaña a
    // una captura de cualquier tamaño, y un número que cambia con la ventana no
    // compara dos capturas.
    let scale = reference_scale();
    settings
        .rings
        .iter()
        .enumerate()
        .map(|(slot, ring)| RingFacts {
            reach_m: ring_reach(slot, reach_scale, settings),
            chunk_m: ring.chunk_m,
            // Lo que el chunk plantó dividido por su área: el redondeo a briznas
            // enteras la aparta un poco de la tabla. Tier 0 siempre pide el
            // rango **entero** del anillo (su borde interno es el del anillo),
            // así que describe la ley de densidad del anillo, no la partición
            // de su buffer en tiers.
            density: blades_per_chunk(slot, 0, dial, scale, reach_scale, settings) as f32
                / (ring.chunk_m * ring.chunk_m),
            triangles_per_blade: submitted_triangles_per_blade(perf),
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
/// Cada (índice de baldosa, brizna) que le toca a un chunk — sin altura de
/// terreno ni cobertura: sólo identidad y la brizna cruda de su baldosa. La
/// comparte el horneado real (`build_chunk_records`) y el censo de vitalidad
/// (`chunk_vitality`) para que los dos vean siempre el mismo conjunto de
/// briznas — nunca dos recorridos que puedan divergir en cuál brizna existe
/// en cuál chunk.
fn chunk_blade_positions(
    centre: Vec2,
    chunk_m: f32,
    blades: std::ops::Range<u32>,
) -> impl Iterator<Item = (u32, grass_tiles::TileBlade)> {
    let corner = centre - Vec2::splat(chunk_m * 0.5);
    let first_tile = grass_tiles::tile_at(corner + Vec2::splat(grass_tiles::TILE_M * 0.5));
    let side = i32::try_from(tiles_per_side(chunk_m)).unwrap_or(1);
    (0..side).flat_map(move |row| {
        let blades = blades.clone();
        (0..side).flat_map(move |column| {
            let tile = first_tile + IVec2::new(column, row);
            blades
                .clone()
                .map(move |index| (index, grass_tiles::blade_in_tile(tile, index)))
        })
    })
}

/// El alcance que de verdad viaja al registro de una brizna: la parte entera
/// del valor crudo de la escalera, nunca menor a un metro. **Única función que
/// lo calcula** — el censo de vitalidad lo reusa para no clasificar viva una
/// brizna que el shader ya mató por redondeo (`ladder[index]` sin `floor` se
/// queda hasta un metro más lejos de lo que el registro realmente dice).
fn baked_reach(ladder: &[f32], index: u32) -> f32 {
    ladder
        .get(index as usize)
        .copied()
        .unwrap_or(0.0)
        .floor()
        .max(1.0)
}

fn build_chunk_records(
    spec: &ChunkSpec,
    terrain: Option<&crate::world::Terrain>,
    settings: &GrassRendererSettings,
) -> ChunkPlanting {
    let ChunkSpec {
        centre,
        chunk_m,
        ref blades,
        ref ladder,
    } = *spec;
    let side = tiles_per_side(chunk_m);
    let per_tile = blades.end - blades.start;
    let mut records = Vec::with_capacity(
        usize::try_from(per_tile.saturating_mul(side).saturating_mul(side)).unwrap_or(0),
    );
    let mut lowest = f32::MAX;
    let mut highest = f32::MIN;
    for (index, blade) in chunk_blade_positions(centre, chunk_m, blades.clone()) {
        let xz = blade.xz;
        let ground = terrain.map(|t| t.height_at(xz)).unwrap_or(0.0);
        let slope = terrain.map(|t| t.slope_deg_at(xz)).unwrap_or(0.0);
        let kind = terrain
            .map(|t| t.kind_at(xz))
            .unwrap_or(crate::world::TerrainKind::Soil);
        let cover = grass_cover::coverage(kind, slope);
        let height = (settings.blade_height_min_m
            + blade.height_unit * (settings.blade_height_max_m - settings.blade_height_min_m))
            * cover;
        // **El alcance es de la brizna, no del anillo**, y viaja en la parte
        // entera igual que antes. Es lo que le saca al shader la ley `1/d` y
        // el hash: cada brizna muere donde su índice dice.
        let reach = baked_reach(ladder, index);
        records.push(blade_record(xz, ground, reach + height));
        lowest = lowest.min(ground);
        highest = highest.max(ground);
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

    fn settings() -> GrassRendererSettings {
        GrassRendererSettings::default()
    }

    /// Réplica en CPU de `chunk_time_fade` (`grass.wgsl`), para poder probar
    /// la matemática del fundido sin GPU.
    fn chunk_time_fade(now: f32, born_at: f32, fade_in_s: f32) -> f32 {
        ((now - born_at) / fade_in_s.max(0.001)).clamp(0.0, 1.0)
    }

    #[test]
    fn chunk_time_fade_rises_from_zero_to_one_over_the_fade_window() {
        let born_at = 100.0;
        assert_eq!(chunk_time_fade(born_at, born_at, CHUNK_FADE_IN_S), 0.0);
        let midpoint = chunk_time_fade(born_at + CHUNK_FADE_IN_S * 0.5, born_at, CHUNK_FADE_IN_S);
        assert!(
            midpoint > 0.0 && midpoint < 1.0,
            "a mitad de camino el fundido tiene que estar entre 0 y 1, no {midpoint}",
        );
        assert!(
            (chunk_time_fade(born_at + CHUNK_FADE_IN_S, born_at, CHUNK_FADE_IN_S) - 1.0).abs()
                < 1e-4,
            "al borde de la ventana el fundido tiene que estar completo",
        );
    }

    #[test]
    fn chunk_time_fade_saturates_past_its_window_and_never_goes_negative() {
        let born_at = 100.0;
        assert_eq!(
            chunk_time_fade(born_at + CHUNK_FADE_IN_S * 100.0, born_at, CHUNK_FADE_IN_S),
            1.0,
        );
        // Antes de nacer no debería poder pasar, pero si pasara, el fundido no
        // tiene que devolver un número negativo que después restara altura.
        assert_eq!(
            chunk_time_fade(born_at - 5.0, born_at, CHUNK_FADE_IN_S),
            0.0
        );
    }

    /// El centinela de reasignación de tier tiene que dar fundido completo
    /// **sin importar el reloj**, incluso a `now = 0.0` (el instante del
    /// arranque) — si dependiera de que `now` ya fuera grande, un chunk
    /// reasignado en los primeros milisegundos de la escena arrancaría
    /// desvanecido, reabriendo el mismo pop que este centinela existe para
    /// evitar.
    #[test]
    fn the_already_grown_sentinel_fades_in_fully_regardless_of_the_clock() {
        for now in [0.0, 1.0, 100.0, 1.0e6] {
            assert_eq!(
                chunk_time_fade(now, ALREADY_GROWN_BORN_AT, CHUNK_FADE_IN_S),
                1.0,
                "el centinela tiene que dar 1.0 en now={now}, no fundido a medias",
            );
        }
    }

    #[test]
    fn a_card_candidate_never_changes_the_shipped_asset_without_an_explicit_name() {
        assert_eq!(card_albedo_path("base"), CARD_ALBEDO_BASE);
        assert_eq!(card_albedo_path("legacy"), CARD_ALBEDO_LEGACY);
        assert_eq!(card_albedo_path("v3"), CARD_ALBEDO_V3);
        assert_eq!(card_albedo_path("nada-que-exista"), CARD_ALBEDO_BASE);
    }

    #[test]
    fn shape_bench_auto_leaves_the_configuration_untouched() {
        let perf = crate::perf::PerfToggles::default();
        assert_eq!(shape_bench_settings(&perf, settings()), settings());
    }

    /// Distancias que un experimento real recorre: desde casi encima de la
    /// cámara hasta el borde del anillo más lejano. Un banco puro tiene que
    /// dar la misma forma en las dos puntas, no sólo lejos.
    fn shape_bench_probe_distances() -> [f32; 2] {
        [0.5, farthest_reach(1.0, &GrassRendererSettings::default())]
    }

    #[test]
    fn shape_bench_solo_leaf_covers_the_whole_dome_leaf_shaped() {
        let mut perf = crate::perf::PerfToggles::default();
        perf.set_knob_step(crate::perf::PerfKnob::GrassShapeBench, 1);
        let bench = shape_bench_settings(&perf, settings());
        let scale = reference_scale();
        for distance in shape_bench_probe_distances() {
            assert_eq!(shape_at(distance, scale, &bench), BladeShape::Leaf);
        }
    }

    #[test]
    fn shape_bench_solo_spike_never_lets_a_blade_open_as_leaf_or_card() {
        let mut perf = crate::perf::PerfToggles::default();
        perf.set_knob_step(crate::perf::PerfKnob::GrassShapeBench, 2);
        let bench = shape_bench_settings(&perf, settings());
        let scale = reference_scale();
        for distance in shape_bench_probe_distances() {
            assert_eq!(shape_at(distance, scale, &bench), BladeShape::Spike);
        }
    }

    #[test]
    fn shape_bench_solo_card_never_leaves_a_sliver_of_another_shape() {
        let mut perf = crate::perf::PerfToggles::default();
        perf.set_knob_step(crate::perf::PerfKnob::GrassShapeBench, 3);
        let bench = shape_bench_settings(&perf, settings());
        let scale = reference_scale();
        for distance in shape_bench_probe_distances() {
            assert_eq!(shape_at(distance, scale, &bench), BladeShape::Card);
        }
    }

    /// Sólo "solo púa" (paso 2) baja a 1 triángulo — es el único paso donde
    /// ninguna brizna visible puede resolver a hoja, la forma que sí necesita
    /// el segundo triángulo (ver `submitted_triangles_per_blade`).
    #[test]
    fn only_solo_spike_drops_the_second_triangle() {
        let mut perf = crate::perf::PerfToggles::default();
        for step in 0..bof_domain::perf::GRASS_SHAPE_BENCH_STEPS.len() {
            perf.set_knob_step(crate::perf::PerfKnob::GrassShapeBench, step);
            let expected = if perf.grass_shape_bench_label() == "solo púa" {
                1
            } else {
                2
            };
            assert_eq!(submitted_triangles_per_blade(&perf), expected);
        }
    }

    /// El banco compara asset, no densidad: cada forma sigue calculando su
    /// propia huella con los mismos números que el juego real.
    #[test]
    fn shape_bench_never_touches_the_footprint_a_shape_declares() {
        let mut perf = crate::perf::PerfToggles::default();
        for step in 0..bof_domain::perf::GRASS_SHAPE_BENCH_STEPS.len() {
            perf.set_knob_step(crate::perf::PerfKnob::GrassShapeBench, step);
            let bench = shape_bench_settings(&perf, settings());
            assert_eq!(bench.card_width_m, settings().card_width_m);
            assert_eq!(bench.card_silhouette_area, settings().card_silhouette_area);
            assert_eq!(bench.blade_width_m, settings().blade_width_m);
        }
    }

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
        let settings = settings();
        let a = build_chunk_records(&spec(0..16, 8.0), None, &settings).records;
        let b = build_chunk_records(&spec(0..16, 8.0), None, &settings).records;
        assert!(!a.is_empty(), "un chunk vacío volvería esto vacuo");
        assert_eq!(a, b, "el mismo suelo creció otro campo");
        // Y la otra mitad, que sin ella lo pasaría un generador constante.
        let mut elsewhere = spec(0..16, 8.0);
        elsewhere.centre = Vec2::splat(400.0);
        assert_ne!(
            a,
            build_chunk_records(&elsewhere, None, &settings).records,
            "dos pedazos de suelo distintos dieron lo mismo"
        );
    }

    /// **La propiedad del rediseño, del lado del horneado**: dos niveles que
    /// pisan el mismo suelo tienen que hablar de las mismas briznas. El nivel
    /// lejano lleva un prefijo del tramo del cercano, así que sus registros
    /// aparecen **idénticos** entre los del otro — no parecidos, iguales.
    #[test]
    fn two_levels_over_the_same_ground_plant_the_same_blades() {
        let settings = settings();
        let far = build_chunk_records(&spec(0..4, 40.0), None, &settings).records;
        let near = build_chunk_records(&spec(0..16, 40.0), None, &settings).records;
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
        for record in build_chunk_records(&spec, None, &settings()).records {
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
        let settings = settings();
        for record in build_chunk_records(&spec, None, &settings).records {
            let packed = record[3];
            assert_eq!(packed.floor(), 13.0, "el alcance no vuelve entero");
            let height = packed.fract();
            assert!(
                (0.0..=settings.blade_height_max_m).contains(&height),
                "altura fuera de rango: {height}",
            );
        }
        assert!(settings.blade_height_max_m < 1.0);
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
            let settings = settings();
            let planting = build_chunk_records(&spec, None, &settings);
            // Sin terreno el sorteo planta todo a cero; correrlo entero es lo
            // que simula un chunk sobre una ladera.
            let raised: Vec<[f32; 4]> = planting
                .records
                .iter()
                .map(|r| [r[0], r[1], r[2] + ground, r[3]])
                .collect();
            let bounds = chunk_bounds(spec.centre, spec.chunk_m, ground..=ground, &settings);
            let (min, max) = (bounds.min(), bounds.max());
            for record in raised {
                let (x, z, base) = (record[0], record[1], record[2]);
                let tip = base + record[3].fract();
                assert!(
                    x >= min.x && x <= max.x && z >= min.z && z <= max.z,
                    "una brizna nace fuera de la caja de su chunk: {record:?}",
                );
                assert!(
                    base - settings.growth_sink_m - settings.blade_root_sink_m >= min.y
                        && tip <= max.y,
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

    /// La carta de pradera es una textura del material extendido, no una entidad
    /// aparte ni una silueta que color y prepass puedan interpretar distinto.
    #[test]
    fn textured_card_contract_is_shared_by_colour_and_prepass() {
        assert!(
            std::path::Path::new("assets/textures/props/T_GrassMeadowCard_Albedo.png").is_file(),
            "la carta de pradera tiene que tener su fuente PNG"
        );
        let wgsl = std::fs::read_to_string("assets/shaders/grass.wgsl")
            .expect("el shader de la pradera tiene que estar donde el material lo pide");
        for declaration in [
            "@binding(104)\nvar card_albedo: texture_2d<f32>;",
            "@binding(105)\nvar card_albedo_sampler: sampler;",
            "fn sample_card_albedo(",
        ] {
            assert!(
                wgsl.contains(declaration),
                "la carta texturada perdió `{declaration}`"
            );
        }
        assert_eq!(
            wgsl.matches("sample_card_albedo(card_texture_uv, card_texture_dx, card_texture_dy)")
                .count(),
            3,
            "color, detalle y prepass deben usar la misma muestra de carta"
        );
        assert!(
            wgsl.contains(".a < 0.5"),
            "el prepass tiene que respetar el cutoff que usa AlphaToCoverage cuando MSAA está apagado"
        );
        assert!(
            !wgsl.contains("CARD_SILHOUETTE_MIN_PIXELS"),
            "la carta texturada no puede volver a un fallback de rectángulo sólido"
        );
    }

    #[test]
    fn the_density_knob_is_what_actually_lands_on_the_ground() {
        // The failure this system was built to fix: a density that reads well in
        // a constant but arrives on screen divided by twenty. Lo que tiene que
        // llegar intacto es el tramo del nivel más cercano: es la baldosa
        // entera, porque su banda empieza donde la ley se evalúa.
        let scale = reference_scale();
        let settings = settings();
        for dial in [REFERENCE_DENSITY, bof_domain::perf::GRASS_DENSITY_STEPS[2]] {
            let ranges = tile_ranges(dial, scale, REFERENCE_REACH, &settings);
            let expected = grass_tiles::blades_in_tile(live_density_at(
                NEAREST_INTEREST_M,
                dial,
                scale,
                &settings,
            ));
            assert_eq!(
                ranges[0][0].end, expected,
                "con la perilla en {dial} la baldosa entera no es la que la ley pide"
            );
            // **Anidados, no partidos**: cada nivel es un prefijo del anterior, y
            // por eso la misma brizna pasa de uno a otro al cruzar la frontera en
            // vez de ser reemplazada. El tier 0 de un anillo es su rango entero
            // — el mismo prefijo que antes de que existieran los tiers.
            let ring_ranges: Vec<std::ops::Range<u32>> = (0..GRASS_RING_COUNT)
                .map(|ring| ranges[ring][0].clone())
                .collect();
            for pair in ring_ranges.windows(2) {
                assert!(
                    pair[1].end <= pair[0].end && pair[1].start == 0,
                    "los niveles dejaron de anidar: {pair:?}"
                );
            }
            // Y dentro del anillo 0, cada tier también anida contra el
            // anterior: el tier más lejano nunca pide más índices que uno más
            // cercano — es la propiedad que hace que escalonar el buffer no
            // pueda dejar una brizna sin casillero.
            for pair in ranges[0].windows(2) {
                assert!(
                    pair[1].end <= pair[0].end && pair[1].start == 0,
                    "los tiers del anillo 0 dejaron de anidar: {pair:?}"
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
        let settings = settings();
        // El tramo del nivel más cercano, que es la baldosa entera: sumar los
        // tres contaría dos veces a las que dos niveles comparten desde que se
        // anidan.
        let per_tile = |dial: f32| -> f64 {
            f64::from(tile_ranges(dial, scale, REFERENCE_REACH, &settings)[0][0].end)
        };
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
        let settings = settings();
        let covered: Vec<(Vec2, f32)> = settings
            .rings
            .iter()
            .enumerate()
            .flat_map(|(index, ring)| {
                ring_cells(index, focus, REFERENCE_REACH, &settings)
                    .into_iter()
                    .map(move |cell| (cell_centre(cell, ring.chunk_m), ring.chunk_m * 0.5))
            })
            .collect();

        let outermost = settings.rings[GRASS_RING_COUNT - 1].reach_m;
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
        let no_override = GrowthRampOverride::default();
        let first = growth_band(&perf, &no_override);
        perf.set_knob_step(bof_domain::perf::PerfKnob::GrassGrowth, 2);
        assert_ne!(growth_band(&perf, &no_override), first);
        assert_eq!(
            growth_band(&perf, &no_override),
            bof_domain::perf::GRASS_GROWTH_STEPS[2]
        );
    }

    /// El ajuste de F9 gana sobre la perilla F1, y no tener ninguno puesto
    /// devuelve exactamente el comportamiento de siempre.
    #[test]
    fn the_growth_override_wins_over_the_knob_when_set() {
        let perf = crate::perf::PerfToggles::default();
        assert_eq!(
            growth_band(&perf, &GrowthRampOverride::default()),
            perf.grass_growth(),
            "sin ajuste, F9 no debe cambiar nada",
        );
        let overridden = GrowthRampOverride(Some(42.0));
        assert_eq!(
            growth_band(&perf, &overridden),
            42.0,
            "con un ajuste puesto, F9 tiene que ganarle a la perilla",
        );
    }

    /// **Ningún nivel se queda sin chunk dentro de su corona.** Es el contrato
    /// con `blade_growth`: allá la brizna se apaga antes del borde interno de su
    /// nivel porque la dibuja el de adentro, así que acá el territorio tiene que
    /// llegar hasta esa misma línea. Si se recortara antes, lo que se ve es una
    /// franja pelada siguiendo al jugador.
    #[test]
    fn every_ring_has_chunks_across_its_own_band() {
        let focus = Vec2::new(3.7, -11.2);
        let settings = settings();
        for (index, ring) in settings.rings.iter().enumerate() {
            let cells = ring_cells(index, focus, REFERENCE_REACH, &settings);
            let reach = ring_reach(index, REFERENCE_REACH, &settings);
            let inner = band_inner(index, REFERENCE_REACH, &settings).max(NEAREST_INTEREST_M);
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
        let settings = settings();
        let ladder =
            grass_tiles::reach_ladder(REFERENCE_DENSITY, scale, REFERENCE_REACH, &settings);
        let ranges = tile_ranges(REFERENCE_DENSITY, scale, REFERENCE_REACH, &settings);
        let mut distance = NEAREST_INTEREST_M;
        while distance <= farthest_reach(REFERENCE_REACH, &settings) {
            let alive: usize = ranges
                .iter()
                .enumerate()
                // El nivel sólo planta hasta su propio alcance: más allá no tiene
                // chunks, y sus briznas no existen aunque su escalera llegue.
                .filter(|(index, _)| ring_reach(*index, REFERENCE_REACH, &settings) >= distance)
                .map(|(_, tiers)| {
                    let range = &tiers[0];
                    ladder[range.start as usize..range.end as usize]
                        .iter()
                        .filter(|reach| reach.floor() >= distance)
                        .count()
                })
                .sum();
            let planted = alive as f32 / grass_tiles::TILE_AREA_M2;
            let needed = live_density_at(distance, REFERENCE_DENSITY, scale, &settings);
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
    fn rings_covering(point: Vec2, focus: Vec2, settings: &GrassRendererSettings) -> Vec<usize> {
        (0..GRASS_RING_COUNT)
            .filter(|index| {
                let half = settings.rings[*index].chunk_m * 0.5;
                ring_cells(*index, focus, REFERENCE_REACH, settings)
                    .into_iter()
                    .any(|cell| {
                        let offset =
                            (point - cell_centre(cell, settings.rings[*index].chunk_m)).abs();
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
    fn worst_rings_over_ground(
        settings: &GrassRendererSettings,
    ) -> (usize, Vec2, Vec2, Vec<usize>) {
        let mut worst = (0usize, Vec2::ZERO, Vec2::ZERO, Vec::new());
        for focus in [Vec2::ZERO, Vec2::new(3.7, -11.2), Vec2::new(137.0, -488.0)] {
            let mut along = -40.0;
            while along <= 40.0 {
                let mut across = -40.0;
                while across <= 40.0 {
                    let point = focus + Vec2::new(along, across);
                    let rings = rings_covering(point, focus, settings);
                    if rings.len() > worst.0 {
                        worst = (rings.len(), focus, point, rings);
                    }
                    across += 3.1;
                }
                along += 3.1;
            }
        }
        worst
    }

    #[test]
    fn no_patch_of_ground_is_planted_by_more_than_two_rings() {
        let settings = settings();
        let worst = worst_rings_over_ground(&settings);
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

    /// El ruido de frontera (F9) extiende el borde exterior de un anillo hasta
    /// `ragged_ring_boundary_max_m`, que por default coincide con el gap mínimo
    /// que `AdjustFrontier` fuerza entre anillos vecinos. El `handover` que
    /// excluye al anillo de adentro sigue anclado a su alcance limpio (nunca al
    /// jitterado), así que el solape debería quedar confinado al par de anillos
    /// adyacentes — pero esa es justo la clase de invariante que el test de
    /// arriba sólo cubre con el ruido apagado. Auditoría del 2026-08-13.
    #[test]
    fn no_patch_of_ground_is_planted_by_more_than_two_rings_with_ragged_boundary_on() {
        let mut settings = settings();
        settings.ragged_ring_boundary_enabled = true;
        let worst = worst_rings_over_ground(&settings);
        assert!(
            worst.0 <= RINGS_OVER_THE_SAME_GROUND,
            "con el ruido de frontera prendido, la cámara en {:?}, el punto {:?} lo \
             plantan {} anillos ({:?}), por encima de los {RINGS_OVER_THE_SAME_GROUND} \
             que este archivo declara como deuda",
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
        let settings = settings();
        for focus in [
            Vec2::new(137.0, -488.0),
            Vec2::new(-2049.5, 903.25),
            Vec2::new(41_000.0, 41_000.0),
        ] {
            let count = neighbourhood_blades(focus, &settings);
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
        let settings = settings();
        let blades = neighbourhood_blades(Vec2::ZERO, &settings);
        assert!(blades > 0, "a meadow with no blades is not a meadow");
        // El peor caso, por lo mismo que el conteo de briznas: la alineación
        // cómoda no es la que hay que aguantar.
        let period = settings
            .rings
            .iter()
            .map(|ring| ring.chunk_m)
            .fold(0.0_f32, f32::max);
        let chunks: usize = (0..8)
            .flat_map(|z| (0..8).map(move |x| Vec2::new(x as f32, z as f32) * (period / 8.0)))
            .map(|focus| {
                (0..GRASS_RING_COUNT)
                    .map(|index| ring_cells(index, focus, REFERENCE_REACH, &settings).len())
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
        let settings = settings();
        for scale in bof_domain::perf::GRASS_REACH_STEPS {
            let (a, b) = ring_reaches(scale, &settings);
            let sent: Vec<f32> = a.to_array().into_iter().chain(b.to_array()).collect();
            for index in 0..GRASS_RING_COUNT {
                let baked = ring_reach(index, scale, &settings);
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
            GRASS_RING_COUNT + 1,
            "la perilla de anillos y la pradera no hablan del mismo campo"
        );
        let mut perf = crate::perf::PerfToggles::default();
        for step in 0..bof_domain::perf::GRASS_RINGS_STEPS.len() {
            perf.set_knob_step(crate::perf::PerfKnob::GrassRings, step);
            if let Some(only) = perf.grass_only_ring() {
                assert!(
                    only < GRASS_RING_COUNT,
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
        let settings = settings();
        perf.set_knob_step(bof_domain::perf::PerfKnob::GrassReach, 2);
        let scale = perf.grass_reach_scale();
        assert!(scale < 1.0, "este test necesita un paso que sí achique");
        for (slot, ring) in ring_facts(&perf, &settings).into_iter().enumerate() {
            assert_eq!(
                ring.reach_m,
                ring_reach(slot, scale, &settings),
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
        let settings = settings();
        let cells = |scale: f32| {
            (0..GRASS_RING_COUNT)
                .map(|index| ring_cells(index, focus, scale, &settings).len())
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

    #[test]
    fn chunk_blade_positions_yields_every_tile_times_index_pair() {
        let chunk_m = grass_tiles::TILE_M * 3.0;
        let blades = 0..5u32;
        let count = chunk_blade_positions(Vec2::new(10.0, -4.0), chunk_m, blades.clone()).count();
        assert_eq!(count, 3 * 3 * blades.len());
    }

    #[test]
    fn baked_reach_floors_and_never_drops_below_one_metre() {
        let ladder = [30.7, 0.4, 0.0];
        assert_eq!(baked_reach(&ladder, 0), 30.0);
        assert_eq!(
            baked_reach(&ladder, 1),
            1.0,
            "menos de un metro se satura a un metro"
        );
        assert_eq!(baked_reach(&ladder, 2), 1.0);
        assert_eq!(
            baked_reach(&ladder, 99),
            1.0,
            "un índice fuera de la escalera no revienta, se satura"
        );
    }

    /// Con un alcance enorme y el foco en el centro del chunk, ninguna brizna
    /// puede caer fuera de rango: prueba el caso "nada muere" sin depender de
    /// posiciones exactas del hash.
    #[test]
    fn chunk_vitality_counts_everyone_alive_when_the_reach_is_generous() {
        let centre = Vec2::new(50.0, 50.0);
        let chunk_m = grass_tiles::TILE_M;
        let ladder = vec![1000.0; 32];
        let (resident, alive) = chunk_vitality(centre, chunk_m, 0..8, &ladder, 0.0, centre);
        assert_eq!(resident, 8, "una sola baldosa con 8 índices son 8 briznas");
        assert_eq!(
            alive, resident,
            "con un alcance enorme y el foco en el centro nadie debería morir"
        );
    }

    /// El mismo chunk, visto desde 10 km — nadie puede seguir viva sin
    /// importar el hash.
    #[test]
    fn chunk_vitality_kills_everyone_far_beyond_reach() {
        let centre = Vec2::new(50.0, 50.0);
        let chunk_m = grass_tiles::TILE_M;
        let ladder = vec![5.0; 32];
        let far_focus = centre + Vec2::splat(10_000.0);
        let (resident, alive) = chunk_vitality(centre, chunk_m, 0..8, &ladder, 0.0, far_focus);
        assert_eq!(resident, 8);
        assert_eq!(alive, 0, "a 10 km del chunk nadie debería seguir viva");
    }

    /// Y un borde interno más lejano que cualquier punto del chunk mata a
    /// todos aunque el alcance individual sea generoso — el nivel de adentro
    /// se las está dibujando.
    #[test]
    fn chunk_vitality_kills_everyone_inside_the_inner_edge() {
        let centre = Vec2::new(50.0, 50.0);
        let chunk_m = grass_tiles::TILE_M;
        let ladder = vec![1000.0; 32];
        let (resident, alive) = chunk_vitality(centre, chunk_m, 0..8, &ladder, 1000.0, centre);
        assert_eq!(resident, 8);
        assert_eq!(
            alive, 0,
            "un borde interno más lejano que cualquier brizna real las mata a todas"
        );
    }

    /// El censo de un campo con un solo chunk residente tiene que pagar
    /// exactamente su stride — si `dial`/`scale`/`reach_scale` no coinciden
    /// con lo que hornea `roll_meadow_grid`, este número deja de cerrar
    /// contra `blades_per_chunk` sin que nada más lo avise.
    #[test]
    fn grass_vitality_resident_total_matches_the_baked_stride() {
        let settings = settings();
        let dial = REFERENCE_DENSITY;
        let reach_scale = REFERENCE_REACH;
        let scale = reference_scale();
        let ring = 0usize;
        let tier = 0usize;
        let key = ChunkKey {
            ring,
            tier,
            cell: IVec2::new(2, -1),
        };
        let field = GrassField {
            materials: std::array::from_fn(|_| std::array::from_fn(|_| Handle::default())),
            records: std::array::from_fn(|_| std::array::from_fn(|_| RingRecords::default())),
            live: HashMap::from_iter([(key, Entity::PLACEHOLDER)]),
            card_candidate_step: 0,
        };
        // Lejísimos: a esta prueba no le importa quién está viva, sólo cuánto
        // paga el buffer.
        let focus = Vec2::splat(1_000_000.0);
        let vitality = grass_vitality(&field, focus, dial, reach_scale, &settings);
        let expected = blades_per_chunk(ring, tier, dial, scale, reach_scale, &settings) as usize;
        assert_eq!(
            vitality.resident_blades[ring], expected,
            "un solo chunk residente tiene que pagar exactamente su stride"
        );
    }

    /// El invariante central del escalonado: ninguna brizna que debería
    /// seguir viva a una distancia queda fuera del rango de índices que su
    /// tier reservó. Si esto falla, aparece un parche de pasto faltante — la
    /// misma clase de bug que costó el `vertex_index` de 2026-08-07.
    #[test]
    fn no_tier_boundary_strands_a_blade_that_should_be_alive() {
        let settings = settings();
        let scale = reference_scale();
        let reach_scale = REFERENCE_REACH;
        let dial = REFERENCE_DENSITY;
        let ladder = grass_tiles::reach_ladder(dial, scale, reach_scale, &settings);
        let ranges = tile_ranges(dial, scale, reach_scale, &settings);
        let bounds = ring0_tier_bounds(reach_scale, &settings);
        for tier in 0..RING0_TIERS {
            let (lo, hi) = (bounds[tier], bounds[tier + 1]);
            for sample in [lo, f32::midpoint(lo, hi), (hi - 0.01).max(lo)] {
                let range = &ranges[0][tier];
                for (index, reach) in ladder.iter().enumerate() {
                    if reach.floor() >= sample {
                        assert!(
                            u32::try_from(index).unwrap_or(u32::MAX) < range.end,
                            "tier {tier} a {sample} m no incluye el índice {index} \
                             (alcance {reach}), que debería estar vivo",
                        );
                    }
                }
            }
        }
    }

    /// La histéresis retiene un chunk en su tier hasta que su punto más
    /// cercano baja a `bounds[tier] - KEEP_SLACK_M`, no sólo hasta
    /// `bounds[tier]` — el presupuesto de índices tiene que cubrir ese caso
    /// retenido, no sólo la asignación limpia que prueba el test de arriba.
    /// Si esto falla, un chunk acercándose a una frontera de tier pierde
    /// briznas que deberían seguir vivas mientras la histéresis lo retiene
    /// (encontrado auditando el 2026-08-13: el test de arriba sólo probaba
    /// `bounds[tier]`, nunca el peor caso que la histéresis realmente
    /// permite).
    #[test]
    fn no_tier_boundary_strands_a_blade_retained_by_hysteresis() {
        let settings = settings();
        let scale = reference_scale();
        let reach_scale = REFERENCE_REACH;
        let dial = REFERENCE_DENSITY;
        let ladder = grass_tiles::reach_ladder(dial, scale, reach_scale, &settings);
        let ranges = tile_ranges(dial, scale, reach_scale, &settings);
        let bounds = ring0_tier_bounds(reach_scale, &settings);
        for tier in 1..RING0_TIERS {
            let retained_worst_case = (bounds[tier] - KEEP_SLACK_M).max(bounds[0]);
            let range = &ranges[0][tier];
            for (index, reach) in ladder.iter().enumerate() {
                if reach.floor() >= retained_worst_case {
                    assert!(
                        u32::try_from(index).unwrap_or(u32::MAX) < range.end,
                        "tier {tier} retenido hasta {retained_worst_case} m (histéresis) no \
                         incluye el índice {index} (alcance {reach}), que debería estar vivo",
                    );
                }
            }
        }
    }

    /// Ningún tier del anillo 0 queda sin territorio a la densidad de
    /// referencia — un tier vacío dibujaría una banda entera de nada.
    #[test]
    fn no_ring0_tier_is_empty_at_reference_density() {
        let settings = settings();
        let ranges = tile_ranges(
            REFERENCE_DENSITY,
            reference_scale(),
            REFERENCE_REACH,
            &settings,
        );
        for (tier, range) in ranges[0].iter().enumerate() {
            assert!(
                range.end > 0,
                "el tier {tier} del anillo 0 no reserva ningún índice a la densidad de referencia",
            );
        }
    }

    /// El último borde de tier tiene que seguir al alcance **vigente** del
    /// anillo 0, no a un valor fijo en metros — si no, la perilla
    /// `grass-reach` (o `AdjustFrontier` desde F9) puede dejar un chunk sin
    /// tier válido.
    #[test]
    fn ring0_tier_bounds_track_a_scaled_reach() {
        let settings = settings();
        for reach_scale in [1.0, 0.75, 0.5] {
            let bounds = ring0_tier_bounds(reach_scale, &settings);
            assert_eq!(
                *bounds.last().unwrap(),
                ring_reach(0, reach_scale, &settings),
                "el último borde de tier tiene que seguir al alcance vigente, no a un valor fijo",
            );
        }
    }

    /// Lo mismo, pero por el otro camino de entrada: `AdjustFrontier` mueve
    /// `reach_m` del anillo 0 en vivo desde F9, sin pasar por ninguna perilla
    /// de perf. Ningún chunk que `ring_cells` seleccione puede quedar más
    /// allá del último borde de tier.
    #[test]
    fn moving_the_ring0_frontier_live_keeps_every_selected_chunk_inside_its_tiers() {
        let mut settings = settings();
        settings.rings[0].reach_m = 30.0;
        let bounds = ring0_tier_bounds(REFERENCE_REACH, &settings);
        let last_bound = *bounds.last().unwrap();
        assert_eq!(last_bound, ring_reach(0, REFERENCE_REACH, &settings));
        let focus = Vec2::ZERO;
        for cell in ring_cells(0, focus, REFERENCE_REACH, &settings) {
            let nearest = chunk_nearest_m(cell, settings.rings[0].chunk_m, focus);
            assert!(
                nearest <= last_bound + 0.01,
                "un chunk que ring_cells seleccionó ({cell}) quedó más allá \
                 del último borde de tier ({nearest} > {last_bound})",
            );
        }
    }

    /// Un cruce chico de frontera —dentro del margen de histéresis— no
    /// reasigna el tier de un chunk ya vivo; uno sólido sí. Sin esto, un
    /// chunk en el borde entre dos tiers se rehornearía cada cuadro.
    #[test]
    fn ring0_tier_hysteresis_does_not_flip_right_at_the_boundary() {
        let settings = settings();
        let reach_scale = REFERENCE_REACH;
        let bounds = ring0_tier_bounds(reach_scale, &settings);
        let cell = IVec2::new(3, -2);
        let field = GrassField {
            materials: std::array::from_fn(|_| std::array::from_fn(|_| Handle::default())),
            records: std::array::from_fn(|_| std::array::from_fn(|_| RingRecords::default())),
            live: HashMap::from_iter([(
                ChunkKey {
                    ring: 0,
                    tier: 0,
                    cell,
                },
                Entity::PLACEHOLDER,
            )]),
            card_candidate_step: 0,
        };
        let nearest_inside_slack = bounds[1] + KEEP_SLACK_M * 0.5;
        assert_eq!(
            ring0_tier_with_hysteresis(&field, cell, nearest_inside_slack, reach_scale, &settings),
            0,
            "un cruce chico dentro del margen no debería reasignar el tier",
        );
        // A mitad de camino entre "recién pasado el margen" y la frontera
        // siguiente — no un múltiplo fijo de `KEEP_SLACK_M`, que con más
        // tiers (bandas más angostas) podía saltar de largo el tier 1 y caer
        // en el 2, rompiendo el propio test en vez de probar la histéresis.
        assert!(
            bounds[1] + KEEP_SLACK_M < bounds[2],
            "con este RING0_TIERS la banda del tier 1 es más angosta que el margen de \
             histéresis; el test necesita revisarse, no sólo el valor de prueba",
        );
        let nearest_past_slack = f32::midpoint(bounds[1] + KEEP_SLACK_M, bounds[2]);
        assert_eq!(
            ring0_tier_with_hysteresis(&field, cell, nearest_past_slack, reach_scale, &settings),
            1,
            "un cruce sólido, más allá del margen, tiene que reasignar el tier",
        );
    }

    #[test]
    fn ring_boundary_jitter_is_zero_when_disabled_and_never_negative_when_on() {
        let mut settings = settings();
        for ring in 0..GRASS_RING_COUNT {
            for x in -20..20 {
                for z in -20..20 {
                    let cell = IVec2::new(x, z);
                    assert_eq!(
                        ring_boundary_jitter_m(cell, ring, &settings),
                        0.0,
                        "apagado, el ruido no puede mover nada",
                    );
                }
            }
        }
        settings.ragged_ring_boundary_enabled = true;
        for ring in 0..GRASS_RING_COUNT - 1 {
            for x in -20..20 {
                for z in -20..20 {
                    let cell = IVec2::new(x, z);
                    let jitter = ring_boundary_jitter_m(cell, ring, &settings);
                    assert!(
                        jitter >= 0.0,
                        "el ruido en {cell} (anillo {ring}) valió {jitter}, tiene que ser >= 0 siempre",
                    );
                    assert!(
                        jitter <= settings.ragged_ring_boundary_max_m,
                        "el ruido en {cell} (anillo {ring}) valió {jitter}, se pasó de la amplitud",
                    );
                }
            }
        }
    }

    /// El último anillo no tiene a quién prestarle territorio — más allá de
    /// `farthest_reach()` no vive ninguna brizna de toda la pradera, así que
    /// un chunk que sólo existiera por el ruido ahí saldría vacío. El tope
    /// tiene que ser cero para ese anillo sin importar la configuración.
    #[test]
    fn ring_boundary_jitter_never_touches_the_outermost_ring() {
        let mut settings = settings();
        settings.ragged_ring_boundary_enabled = true;
        settings.ragged_ring_boundary_max_m = 50.0;
        let last = GRASS_RING_COUNT - 1;
        assert_eq!(ring_boundary_jitter_cap_m(last, &settings), 0.0);
        for x in -20..20 {
            for z in -20..20 {
                let cell = IVec2::new(x, z);
                assert_eq!(ring_boundary_jitter_m(cell, last, &settings), 0.0);
            }
        }
    }

    /// **La propiedad de seguridad completa.** Prender el ruido nunca puede
    /// hacer que un anillo pierda una celda que la asignación limpia le
    /// daba — sólo puede sumar. Si esto fallara, un chunk quedaría con menos
    /// cobertura de la que su punto más cercano real exige: el mismo bug que
    /// costó `vertex_index` (2026-08-07).
    #[test]
    fn ring_boundary_jitter_never_shrinks_the_selected_cells() {
        let mut settings = settings();
        let reach_scale = REFERENCE_REACH;
        let focus = Vec2::new(37.0, -11.0);
        for ring in 0..GRASS_RING_COUNT - 1 {
            settings.ragged_ring_boundary_enabled = false;
            let clean: HashSet<IVec2> =
                ring_cells_with_slack(ring, focus, KEEP_SLACK_M, reach_scale, &settings)
                    .into_iter()
                    .collect();
            settings.ragged_ring_boundary_enabled = true;
            let ragged: HashSet<IVec2> =
                ring_cells_with_slack(ring, focus, KEEP_SLACK_M, reach_scale, &settings)
                    .into_iter()
                    .collect();
            for cell in &clean {
                assert!(
                    ragged.contains(cell),
                    "anillo {ring}: la celda {cell} existía sin ruido y desapareció con \
                     el ruido prendido",
                );
            }
        }
    }

    /// El ruido de frontera puede extender la selección del anillo 0 más
    /// allá de su propio borde limpio — `ring0_tier_for` tiene que seguir
    /// saturando de forma segura ahí, no sólo cuando nadie lo empuja.
    #[test]
    fn ring0_tier_saturates_safely_past_its_own_clean_edge_when_the_ring_boundary_is_jittered() {
        let settings = settings();
        let reach_scale = REFERENCE_REACH;
        let clean_edge = ring_reach(0, reach_scale, &settings);
        let past_edge = clean_edge + 3.0;
        assert_eq!(
            ring0_tier_for(past_edge, reach_scale, &settings),
            RING0_TIERS - 1,
            "más allá del borde limpio del anillo 0 tiene que saturar al último tier",
        );
        // Y ese último tier de verdad reserva territorio: sigue habiendo
        // índices vivos ahí (los de alcance largo, hasta `farthest_reach()`),
        // no un rango vacío que dibuje una banda de nada.
        let dial = REFERENCE_DENSITY;
        let scale = reference_scale();
        let ranges = tile_ranges(dial, scale, reach_scale, &settings);
        assert!(
            ranges[0][RING0_TIERS - 1].end > 0,
            "el último tier del anillo 0 no puede quedar vacío",
        );
    }
}
