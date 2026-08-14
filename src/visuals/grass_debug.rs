//! Las vistas de diagnóstico de la pradera: qué significa cada una y de qué
//! color se pinta. El porqué, en `docs/BOTWGrass.md` → *Ver el pasto*.
//!
//! Dos familias. **Ver** tiñe el color real hacia un pastel y deja la luz
//! puesta, para que el campo siga leyéndose como campo. **Medir** pinta plano y
//! exacto, para que contar píxeles sea aritmética y no detección de bordes.
//!
//! La paleta tiene un solo dueño: vive acá, viaja al shader por uniform y se
//! escribe al lado de cada captura (`shot.rs`), así que el analizador no conoce
//! ningún color — los lee del archivo que escribió la corrida.

use bevy::prelude::*;

/// Cuántos anillos puede pintar la paleta. Ocho, que es lo que entra en los dos
/// `Vec4` de alcances que el shader ya recibe.
pub(crate) const PALETTE_SLOTS: usize = 8;

/// Un color por anillo, en sRGB de 8 bits — no en flotante, para que el viaje de
/// ida y vuelta sea exacto y el analizador encuentre el mismo entero.
const PALETTE: [[u8; 3]; PALETTE_SLOTS] = [
    [0xF5, 0x8C, 0xA0], // rosa
    [0x8C, 0xC7, 0xF5], // celeste
    [0xF5, 0xDE, 0x8C], // amarillo
    [0x9E, 0xE8, 0xA8], // menta
    [0xC9, 0xA0, 0xF5], // lila
    [0xF5, 0xB9, 0x8C], // durazno
    [0x8C, 0xF5, 0xE4], // aguamarina
    [0xD5, 0xD5, 0xD5], // gris — el casillero de "ninguno de los anteriores"
];

/// El color de un anillo, como lo ve el shader.
pub(crate) fn slot_color(slot: usize) -> LinearRgba {
    let [r, g, b] = PALETTE[slot.min(PALETTE_SLOTS - 1)];
    LinearRgba::from(Color::srgb_u8(r, g, b))
}

/// El color de un anillo, como lo va a encontrar el analizador en el PNG.
pub(crate) fn slot_srgb(slot: usize) -> [u8; 3] {
    PALETTE[slot.min(PALETTE_SLOTS - 1)]
}

/// Qué significa cada color de la vista de diagnóstico.
///
/// Sale de los anillos y de la paleta, nunca escrita a mano: una leyenda que hay
/// que mantener sincronizada a mano es una leyenda que va a mentir. La consumen
/// el log —cuando la vista cambia— y el registro RON que acompaña a cada
/// captura, para que la evidencia conserve qué colores significaban qué.
pub(crate) struct RingLegend {
    pub slot: usize,
    pub reach_m: f32,
    pub chunk_m: f32,
    pub density: f32,
    pub triangles_per_blade: usize,
    pub color: [u8; 3],
    /// Si esta corrida lo plantó: sin esto, cero píxeles no distingue un anillo
    /// apagado de uno que se plantó y no se ve.
    pub planted: bool,
}

/// La leyenda del campo que esta corrida tiene puesto.
///
/// Los hechos los da la pradera —que es la única que sabe qué plantó— y el color
/// lo pone la paleta de acá.
pub(crate) fn ring_legend(
    perf: &crate::perf::PerfToggles,
    settings: &super::grass::GrassRendererSettings,
) -> Vec<RingLegend> {
    super::grass::ring_facts(perf, settings)
        .into_iter()
        .enumerate()
        .map(|(slot, facts)| RingLegend {
            slot,
            reach_m: facts.reach_m,
            chunk_m: facts.chunk_m,
            density: facts.density,
            triangles_per_blade: facts.triangles_per_blade,
            color: slot_srgb(slot),
            planted: facts.planted,
        })
        .collect()
}

/// Una banda de la vista `subpixel`, con el color exacto que la identifica.
pub(crate) struct SubpixelBand {
    pub name: String,
    pub color: [u8; 3],
}

/// Las tres bandas de ancho en pantalla. La frontera sale del ancho de la
/// brizna, la resolución y el campo de visión — **no del sistema de anillos**,
/// así que el número sobrevive a cualquier técnica de LOD que lo reemplace.
pub(crate) fn subpixel_legend() -> Vec<SubpixelBand> {
    [
        ("menos de 1 px — no se resuelve", 0usize),
        ("entre 1 y 2 px — el cuarteto desperdicia", 5),
        ("2 px o mas — se resuelve entera", 3),
    ]
    .into_iter()
    .map(|(name, slot)| SubpixelBand {
        name: name.to_string(),
        color: slot_srgb(slot),
    })
    .collect()
}

/// Categorías planas para comparar las representaciones que coexisten en una
/// misma franja de la imagen. Los colores son slots existentes de la paleta:
/// no agrega otra tabla que el analizador pueda desincronizar.
pub(crate) fn shape_measure_legend() -> Vec<SubpixelBand> {
    super::grass::BladeShape::ALL
        .into_iter()
        .map(|shape| SubpixelBand {
            name: shape.name().to_string(),
            color: slot_srgb(shape.shader_index() as usize),
        })
        .collect()
}

/// Anuncia la vista puesta y qué es cada color. Un color sin leyenda no es un
/// diagnóstico: es una imagen bonita.
pub(crate) fn announce_grass_debug_view(
    perf: Res<crate::perf::PerfToggles>,
    settings: Res<super::grass::GrassRendererSettings>,
    mut announced: Local<Option<usize>>,
) {
    let step = perf.grass_debug_step();
    if announced.replace(step) == Some(step) {
        return;
    }
    if GrassDebugView::from_step(step) == GrassDebugView::Off {
        info!("[grass] vista de diagnóstico apagada");
        return;
    }
    info!("[grass] vista '{}':", perf.grass_debug_label());
    for ring in ring_legend(&perf, &settings) {
        let [r, g, b] = ring.color;
        info!(
            "[grass]   anillo {} #{r:02X}{g:02X}{b:02X} — hasta {:.0} m, chunks de {:.0} m, \
             {:.0} briznas/m2, {} tris por brizna",
            ring.slot, ring.reach_m, ring.chunk_m, ring.density, ring.triangles_per_blade,
        );
    }
}

/// Qué está mirando la vista puesta. El orden es el de
/// `bof_domain::perf::GRASS_DEBUG_STEPS` y un test lo cobra.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum GrassDebugView {
    /// El juego.
    Off,
    /// Un pastel por anillo: dónde cambia el LOD, y cuánto se solapan.
    Ring,
    /// Un pastel por celda de chunk — que es una malla y un draw, así que esta
    /// vista es también el mapa de draw calls de la pradera.
    Chunk,
    /// Un pastel por brizna: contesta si al acercarse las briznas se **suman** o
    /// se **reemplazan**, la causa raíz que `BOTWGrass.md` todavía no atacó.
    Blade,
    /// Cuánto de su altura tiene cada brizna: la banda que crece, visible.
    Growth,
    /// Cuántos píxeles de ancho mide una brizna en pantalla. Rojo la que ya no
    /// se resuelve: el rasterizador trabaja en cuartetos de 2×2, así que una
    /// brizna sub-píxel paga cuatro fragmentos por el uno que aporta. Es la ley
    /// 2 de `BOTWGrass.md`, que hasta ahora no se podía mirar.
    Subpixel,
    /// Plano y exacto, un color por anillo. Para contar píxeles.
    Measure,
    /// Plano y exacto, un color por forma. Para comparar púa y carta donde
    /// coexisten, sin que el anillo o la iluminación confundan la medición.
    ShapeMeasure,
}

impl GrassDebugView {
    pub(crate) const ALL: [GrassDebugView; 8] = [
        GrassDebugView::Off,
        GrassDebugView::Ring,
        GrassDebugView::Chunk,
        GrassDebugView::Blade,
        GrassDebugView::Growth,
        GrassDebugView::Subpixel,
        GrassDebugView::Measure,
        GrassDebugView::ShapeMeasure,
    ];

    pub(crate) fn from_step(step: usize) -> Self {
        Self::ALL[step.min(Self::ALL.len() - 1)]
    }

    /// El índice que viaja al shader.
    pub(crate) fn shader_index(self) -> u32 {
        match self {
            GrassDebugView::Off => 0,
            GrassDebugView::Ring => 1,
            GrassDebugView::Chunk => 2,
            GrassDebugView::Blade => 3,
            GrassDebugView::Growth => 4,
            GrassDebugView::Subpixel => 5,
            GrassDebugView::Measure => 6,
            GrassDebugView::ShapeMeasure => 7,
        }
    }

    /// Si el color que sale del shader es un valor exacto y no una imagen. Lo
    /// consume `camera::apply_flat_measure_view`, que con esto puesto apaga el
    /// tonemapping y el dithering.
    pub(crate) fn is_flat(self) -> bool {
        matches!(
            self,
            GrassDebugView::Measure | GrassDebugView::ShapeMeasure | GrassDebugView::Subpixel
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La perilla y el shader tienen que hablar de la misma vista: una lista de
    /// nombres en `bof_domain` y un enum acá, y nada más que esto los ata.
    #[test]
    fn the_knob_ladder_and_the_views_are_the_same_list() {
        assert_eq!(
            bof_domain::perf::GRASS_DEBUG_STEPS.len(),
            GrassDebugView::ALL.len()
        );
        for (step, label) in bof_domain::perf::GRASS_DEBUG_STEPS.iter().enumerate() {
            let view = GrassDebugView::from_step(step);
            assert_eq!(
                view.shader_index() as usize,
                step,
                "el paso '{label}' no manda su propio índice al shader"
            );
        }
        assert_eq!(GrassDebugView::from_step(0), GrassDebugView::Off);
    }

    /// Si el viaje de ida y vuelta por sRGB no es exacto, el analizador busca un
    /// color que el PNG no contiene. Es la propiedad entera de la vista.
    #[test]
    fn every_palette_colour_survives_the_round_trip_to_srgb_bytes() {
        for slot in 0..PALETTE_SLOTS {
            let linear = slot_color(slot);
            let back = Color::from(linear).to_srgba().to_u8_array_no_alpha();
            assert_eq!(
                back,
                slot_srgb(slot),
                "el color del anillo {slot} no vuelve entero de lineal a sRGB"
            );
        }
    }

    #[test]
    fn shape_measurement_has_exact_categories_and_a_shader_path() {
        let shapes = shape_measure_legend();
        assert_eq!(
            shapes
                .iter()
                .map(|shape| shape.name.as_str())
                .collect::<Vec<_>>(),
            super::super::grass::BladeShape::ALL
                .map(super::super::grass::BladeShape::name)
                .to_vec()
        );
        for shape in super::super::grass::BladeShape::ALL {
            assert_eq!(
                shapes[shape.shader_index() as usize].color,
                slot_srgb(shape.shader_index() as usize)
            );
        }
        let wgsl = include_str!("../../assets/shaders/grass.wgsl");
        assert!(wgsl.contains("const DEBUG_SHAPE_MEASURE: u32 = 7u;"));
        assert!(wgsl.contains("shape == SHAPE_CARD"));
    }

    /// Dos anillos del mismo color son indistinguibles en la foto y en el conteo.
    #[test]
    fn no_two_rings_share_a_colour() {
        for a in 0..PALETTE_SLOTS {
            for b in (a + 1)..PALETTE_SLOTS {
                assert_ne!(slot_srgb(a), slot_srgb(b), "anillos {a} y {b}");
            }
        }
    }
}
