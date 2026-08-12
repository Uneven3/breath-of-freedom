//! One layered material for every semantic terrain kind.
//!
//! The source PNGs are ordinary, independently replaceable assets. At
//! runtime they are packed into one `texture_2d_array`, keeping the terrain a
//! single mesh/material/draw family while final art replaces today's solid
//! colours without changing code.

use bevy::asset::{AssetEvent, RenderAssetUsages};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;

use crate::asset_pipeline::terrain_textures::{TERRAIN_TEXTURE_EDGE, TERRAIN_TEXTURES};
use crate::visuals::grass::GrassRendererSettings;
use crate::visuals::material_registry::InstrumentedMaterialAppExt;
use crate::world::TerrainKind;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerrainDebugView {
    #[default]
    Off,
    Kind,
    Climbable,
    Flammable,
    Cuttable,
}

impl TerrainDebugView {
    pub const ALL: [Self; 5] = [
        Self::Off,
        Self::Kind,
        Self::Climbable,
        Self::Flammable,
        Self::Cuttable,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Arte",
            Self::Kind => "Tipo",
            Self::Climbable => "Escalable",
            Self::Flammable => "Inflamable",
            Self::Cuttable => "Cortable",
        }
    }

    pub(crate) fn legend(self) -> &'static [TerrainLegendEntry] {
        match self {
            Self::Off => &[],
            Self::Kind => &KIND_LEGEND,
            Self::Climbable => &CLIMBABLE_LEGEND,
            Self::Flammable => &FLAMMABLE_LEGEND,
            Self::Cuttable => &CUTTABLE_LEGEND,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct TerrainLegendEntry {
    pub label: &'static str,
    pub color: Color,
}

const KIND_LEGEND: [TerrainLegendEntry; 5] = [
    TerrainLegendEntry {
        label: "Tierra / camino",
        color: Color::srgb_u8(121, 81, 58),
    },
    TerrainLegendEntry {
        label: "Pasto corto",
        color: Color::srgb_u8(117, 150, 69),
    },
    TerrainLegendEntry {
        label: "Pasto largo",
        color: Color::srgb_u8(79, 155, 69),
    },
    TerrainLegendEntry {
        label: "Roca",
        color: Color::srgb_u8(125, 130, 140),
    },
    TerrainLegendEntry {
        label: "Arena",
        color: Color::srgb_u8(216, 194, 116),
    },
];
const CLIMBABLE_LEGEND: [TerrainLegendEntry; 2] = [
    TerrainLegendEntry {
        label: "Escalable",
        color: Color::srgb(1.0, 0.05, 0.03),
    },
    TerrainLegendEntry {
        label: "No escalable",
        color: Color::srgb(0.025, 0.025, 0.03),
    },
];
const FLAMMABLE_LEGEND: [TerrainLegendEntry; 2] = [
    TerrainLegendEntry {
        label: "Inflamable",
        color: Color::srgb(1.0, 0.3, 0.02),
    },
    TerrainLegendEntry {
        label: "No inflamable",
        color: Color::srgb(0.025, 0.025, 0.03),
    },
];
const CUTTABLE_LEGEND: [TerrainLegendEntry; 2] = [
    TerrainLegendEntry {
        label: "Cortable",
        color: Color::srgb(0.55, 1.0, 0.03),
    },
    TerrainLegendEntry {
        label: "No cortable",
        color: Color::srgb(0.025, 0.025, 0.03),
    },
];

#[derive(Resource, Default)]
pub struct TerrainDebugState {
    view: TerrainDebugView,
}

impl TerrainDebugState {
    pub fn view(&self) -> TerrainDebugView {
        self.view
    }
}

#[derive(Message, Clone, Copy, Debug)]
pub struct TerrainDebugViewRequest(pub TerrainDebugView);

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct TerrainExtension {
    #[texture(100, dimension = "2d_array")]
    #[sampler(101)]
    textures: Handle<Image>,
    /// `.x` stores [`TerrainDebugView`] as a number. A `Vec4` keeps the uniform
    /// layout portable without target-specific padding.
    #[uniform(102)]
    debug: Vec4,
    /// The colour a blade has at its root, `.a` being how far the ground is
    /// allowed to travel toward it.
    ///
    /// This is the whole of "the terrain is the furthest LOD": where grass
    /// grows, the ground underneath already wears its colour, so the field
    /// stops having an edge and the last blades fade into a floor that already
    /// matches. It is the cheapest step in `BOTWGrass.md` and it removes its
    /// worst artefact.
    #[uniform(103)]
    grass_tint: Vec4,
    /// The cover rule's parameters, as the shader needs them:
    /// `.x` cosine of the maximum slope, `.y` cosine of the steep slope,
    /// `.z` the grass-bearing texture layers as a bitmask, `.w` unused.
    ///
    /// Parameters rather than a copy of the rule — see `grass_cover`.
    #[uniform(104)]
    grass_rules: Vec4,
}

impl TerrainExtension {
    pub(crate) fn has_textures(&self) -> bool {
        self.textures != Handle::default()
    }
}

impl MaterialExtension for TerrainExtension {
    fn fragment_shader() -> ShaderRef {
        "shaders/terrain.wgsl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "shaders/terrain.wgsl".into()
    }
}

pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainExtension>;

#[derive(Resource)]
pub(super) struct TerrainMaterialAssets {
    pub material: Handle<TerrainMaterial>,
    array: Handle<Image>,
    sources: Vec<Handle<Image>>,
    dirty: bool,
}

pub(super) struct TerrainMaterialPlugin;

impl Plugin for TerrainMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_instrumented_material::<TerrainMaterial>()
            .init_resource::<TerrainDebugState>()
            .add_message::<TerrainDebugViewRequest>()
            .add_systems(Startup, setup_terrain_material)
            .add_systems(
                Update,
                (
                    mark_changed_source_textures,
                    rebuild_texture_array,
                    apply_debug_view_requests,
                    sync_grass_tint,
                )
                    .chain(),
            );
    }
}

fn setup_terrain_material(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    grass: Res<GrassRendererSettings>,
) {
    let array = images.add(fallback_array());
    let material = materials.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.9,
            metallic: 0.0,
            reflectance: 0.2,
            ..default()
        },
        extension: TerrainExtension {
            textures: array.clone(),
            debug: Vec4::ZERO,
            grass_tint: grass_tint(&grass),
            grass_rules: grass_rules(),
        },
    });
    let sources = TERRAIN_TEXTURES
        .iter()
        .map(|spec| asset_server.load(spec.path))
        .collect();
    commands.insert_resource(TerrainMaterialAssets {
        material,
        array,
        sources,
        dirty: false,
    });
}

/// How far the ground travels toward the blade's root colour where grass grows.
///
/// Not all the way: the tint has to make the field edgeless without erasing the
/// texture underneath, which is what still gives the ground its grain up close.
/// Judged by eye, and the one number here that is a look decision.
///
/// Started at 0.8 and came down after playing it: at that strength the ground
/// sat so close in value to the blades that the blades stopped reading as
/// separate objects and the whole field went flat. The floor has to *rhyme*
/// with the grass, not match it — what kills the field's edge is sharing a
/// colour family, and what keeps the grass legible is the ground staying darker
/// than it.
///
/// **0,55 → 0,25 el 2026-08-06, cuando el suelo dejó de ser un color plano.**
/// Esta constante era un sustituto: sin textura, teñir era lo único que impedía
/// que el campo terminara en una línea contra tierra marrón. Con una textura de
/// pradera authored en la capa `Soil` (#769746 de media, el mismo family que la
/// brizna), la rima ya la da el arte y lo que el tinte agrega es aplastarle el
/// grano. Se baja, no se borra: sigue siendo lo que distingue el suelo *donde
/// crece pasto* del mismo suelo en una pendiente donde no crece, que es una
/// diferencia que ninguna textura puede expresar porque depende de la geometría.
fn grass_tint(settings: &GrassRendererSettings) -> Vec4 {
    let root = settings.root_color;
    Vec4::new(
        root.red,
        root.green,
        root.blue,
        settings.grass_tint_strength,
    )
}

/// The terrain has to rhyme with the live blade configuration. Keeping this
/// update here means a future Grass Lab colour control cannot leave the ground
/// carrying the prior grass root colour.
fn sync_grass_tint(
    settings: Res<GrassRendererSettings>,
    terrain: Option<Res<TerrainMaterialAssets>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
) {
    if !settings.is_changed() {
        return;
    }
    let Some(terrain) = terrain else { return };
    if let Some(mut material) = materials.get_mut(&terrain.material) {
        material.extension.grass_tint = grass_tint(&settings);
    }
}

fn grass_rules() -> Vec4 {
    Vec4::new(
        super::grass_cover::MAX_SLOPE_DEG.to_radians().cos(),
        super::grass_cover::STEEP_SLOPE_DEG.to_radians().cos(),
        // A bitmask of four layers survives an f32 exactly; the shader rounds
        // it back to an integer.
        super::grass_cover::grass_layer_mask() as f32,
        0.0,
    )
}

fn fallback_array() -> Image {
    let data = TERRAIN_TEXTURES
        .iter()
        .flat_map(|spec| {
            let [r, g, b] = spec.placeholder_rgb;
            [r, g, b, 255]
        })
        .collect();
    array_image(1, data, TextureFormat::Rgba8UnormSrgb)
}

fn array_image(edge: u32, data: Vec<u8>, format: TextureFormat) -> Image {
    let Ok(layer_count) = u32::try_from(TERRAIN_TEXTURES.len()) else {
        panic!("terrain texture array exceeded the u32 layer limit");
    };
    let mut image = Image::new(
        Extent3d {
            width: edge,
            height: edge,
            depth_or_array_layers: layer_count,
        },
        TextureDimension::D2,
        data,
        format,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 16,
        ..default()
    });
    image
}

fn mark_changed_source_textures(
    mut events: MessageReader<AssetEvent<Image>>,
    mut terrain: Option<ResMut<TerrainMaterialAssets>>,
) {
    let Some(terrain) = terrain.as_mut() else {
        return;
    };
    if events.read().any(|event| {
        terrain
            .sources
            .iter()
            .any(|source| event.id() == source.id())
    }) {
        terrain.dirty = true;
    }
}

/// Arma el array desde los PNG y **vuelve a tocar el material**, que es la parte
/// que faltaba.
///
/// Reemplazar el contenido de la `Image` no alcanza. El array nace como un
/// fallback de 1×1 por capa, así que cuando los PNG llegan la imagen cambia de
/// tamaño; wgpu no puede redimensionar una textura, se crea una nueva, y el
/// bind group que el material armó apunta a la vieja. El material sigue
/// muestreando 1×1 para siempre — o sea los colores planos del fallback.
///
/// **Estuvo así desde que existe el sistema y nadie lo vio, por una coincidencia
/// que vale registrar:** los cuatro PNG canónicos se generaron a partir de los
/// mismos `placeholder_rgb` del fallback, así que su color medio coincide con él
/// exactamente (`#7D828C` la roca, `#D8C274` la arena, los cuatro). El fallback
/// y el arte real eran indistinguibles, el log decía "packed" y todo parecía
/// funcionar. Se destapó el 2026-08-06, al poner la primera textura que de
/// verdad no se parece a su placeholder: el suelo siguió café.
///
/// El array **entra como un asset nuevo**, no reescribiendo el que ya estaba, y
/// el material pasa a apuntar a ese handle.
///
/// Sobrescribir el contenido de la imagen anterior es lo que hacía antes, y no
/// funciona: el array nace como un fallback de 1×1 por capa, así que cuando los
/// PNG llegan la imagen cambia de tamaño. wgpu no redimensiona texturas —crea
/// una nueva— y el bind group que el material ya armó sigue apuntando a la
/// vieja. Tocar el material tampoco alcanza, porque el rearmado puede caer en un
/// frame en el que la textura nueva todavía no subió a la GPU. Con un handle
/// distinto no hay carrera posible: Bevy arma el bind group cuando esa
/// `GpuImage` existe, y hasta entonces el material sigue mostrando el fallback,
/// que es exactamente el comportamiento que se quiere.
///
/// **Estuvo roto desde que existe el sistema y nadie lo vio, por una
/// coincidencia que vale registrar:** los cuatro PNG canónicos se generaron a
/// partir de los mismos `placeholder_rgb` del fallback, así que su color medio
/// coincide con él exactamente (`#7D828C` la roca, `#D8C274` la arena, los
/// cuatro). El fallback y el arte real eran indistinguibles, el log decía
/// "packed" y todo parecía andar. Se destapó el 2026-08-06, con la primera
/// textura que de verdad no se parece a su placeholder: el suelo siguió café.
fn rebuild_texture_array(
    mut terrain: Option<ResMut<TerrainMaterialAssets>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
) {
    let Some(terrain) = terrain.as_mut() else {
        return;
    };
    if !terrain.dirty {
        return;
    }
    // One attempt per relevant AssetEvent. A missing or invalid source keeps
    // the colour fallback without retrying every frame; a later event opts in
    // to another attempt.
    terrain.dirty = false;
    let Some(array) = collect_source_layers(&terrain.sources, &images) else {
        return;
    };
    let edge = array.width();
    let layer_colours = describe_layers(&array);
    let handle = images.add(array);
    let Some(mut material) = materials.get_mut(&terrain.material) else {
        warn!("[terrain] texture array rebuilt but the material is gone; keeping the fallback");
        return;
    };
    material.extension.textures = handle.clone();
    terrain.array = handle;
    info!(
        "[terrain] packed {} canonical PNGs into one {edge}px texture array — {}",
        TERRAIN_TEXTURES.len(),
        layer_colours
    );
}

/// El color medio de cada capa, para el log.
///
/// Existe porque "packed 4 canonical PNGs" fue **verdadero y useless** durante
/// meses: se imprimía igual con el arte cargado que con el fallback, así que un
/// array que nunca llegaba al material se veía exactamente como uno que sí. Un
/// mensaje de éxito que no se puede distinguir de un fracaso no es un mensaje de
/// éxito. Con el color al lado, el log dice cuál de las dos cosas pasó.
fn describe_layers(array: &Image) -> String {
    let texels = (TERRAIN_TEXTURE_EDGE * TERRAIN_TEXTURE_EDGE) as usize;
    array
        .data
        .as_ref()
        .map(|data| {
            TERRAIN_TEXTURES
                .iter()
                .enumerate()
                .map(|(layer, spec)| {
                    let start = layer * texels * 4;
                    let Some(bytes) = data.get(start..start + texels * 4) else {
                        return format!("{}=?", spec.path);
                    };
                    let mut sums = [0u64; 3];
                    for texel in bytes.chunks_exact(4) {
                        for (sum, channel) in sums.iter_mut().zip(texel) {
                            *sum += u64::from(*channel);
                        }
                    }
                    let avg = sums.map(|sum| u8::try_from(sum / texels as u64).unwrap_or(u8::MAX));
                    format!("L{layer}=#{:02X}{:02X}{:02X}", avg[0], avg[1], avg[2])
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_else(|| "sin datos".to_string())
}

fn collect_source_layers(sources: &[Handle<Image>], images: &Assets<Image>) -> Option<Image> {
    let mut format = None;
    let mut data = Vec::new();
    for (source, spec) in sources.iter().zip(TERRAIN_TEXTURES) {
        let image = images.get(source)?;
        if image.width() != TERRAIN_TEXTURE_EDGE
            || image.height() != TERRAIN_TEXTURE_EDGE
            || image.texture_descriptor.size.depth_or_array_layers != 1
        {
            warn!(
                "[terrain] {} loaded with invalid dimensions {}x{}x{}; keeping colour fallback",
                spec.path,
                image.width(),
                image.height(),
                image.texture_descriptor.size.depth_or_array_layers
            );
            return None;
        }
        let layer_format = image.texture_descriptor.format;
        if let Some(expected) = format
            && expected != layer_format
        {
            warn!(
                "[terrain] {} has format {layer_format:?}, expected {expected:?}; keeping colour fallback",
                spec.path
            );
            return None;
        }
        format = Some(layer_format);
        data.extend_from_slice(image.data.as_deref()?);
    }
    Some(array_image(TERRAIN_TEXTURE_EDGE, data, format?))
}

fn apply_debug_view_requests(
    mut requests: MessageReader<TerrainDebugViewRequest>,
    benchmark: Res<crate::perf::Benchmark>,
    flythrough: Res<crate::perf::Flythrough>,
    mut state: ResMut<TerrainDebugState>,
    terrain: Option<Res<TerrainMaterialAssets>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
) {
    let Some(view) = requests.read().last().map(|request| request.0) else {
        return;
    };
    if benchmark.is_running() || flythrough.is_running() {
        warn!("[terrain] ignoring diagnostic view change while a measurement runs");
        return;
    }
    state.view = view;
    let Some(terrain) = terrain else {
        return;
    };
    let Some(mut material) = materials.get_mut(&terrain.material) else {
        warn!("[terrain] debug view changed but terrain material is unavailable");
        return;
    };
    material.extension.debug.x = view as u8 as f32;
    info!("[terrain] semantic view: {}", view.label());
}

pub(super) const fn texture_layer(kind: TerrainKind) -> u8 {
    match kind {
        TerrainKind::Soil => 0,
        TerrainKind::Rock => 1,
        TerrainKind::TallGrass => 2,
        TerrainKind::Sand => 3,
        // Appended to preserve the four layers every existing terrain mesh and
        // material already encode. Palette order belongs to `TerrainKind::ALL`.
        TerrainKind::ShortGrass => 4,
    }
}

pub(super) fn semantic_vertex_data(kind: TerrainKind, climbable: bool) -> [f32; 4] {
    [
        f32::from(texture_layer(kind)) / 255.0,
        f32::from(climbable),
        f32::from(kind.flammable()),
        f32::from(kind.cuttable()),
    ]
}

trait AssetEventId {
    fn id(&self) -> bevy::asset::AssetId<Image>;
}

impl AssetEventId for AssetEvent<Image> {
    fn id(&self) -> bevy::asset::AssetId<Image> {
        match *self {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::Unused { id }
            | AssetEvent::LoadedWithDependencies { id } => id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_maps_to_one_stable_array_layer() {
        assert_eq!(texture_layer(TerrainKind::Soil), 0);
        assert_eq!(texture_layer(TerrainKind::Rock), 1);
        assert_eq!(texture_layer(TerrainKind::TallGrass), 2);
        assert_eq!(texture_layer(TerrainKind::Sand), 3);
        assert_eq!(texture_layer(TerrainKind::ShortGrass), 4);
        assert_eq!(TerrainKind::ALL.len(), TERRAIN_TEXTURES.len());
    }

    #[test]
    fn semantic_payload_is_data_not_display_colour() {
        let grass = semantic_vertex_data(TerrainKind::TallGrass, true);
        assert_eq!(grass[0], 2.0 / 255.0);
        assert_eq!(&grass[1..], &[1.0, 1.0, 1.0]);

        let rock = semantic_vertex_data(TerrainKind::Rock, false);
        assert_eq!(&rock[1..], &[0.0, 0.0, 0.0]);
    }

    #[test]
    fn fallback_is_a_five_layer_rgba_array() {
        let image = fallback_array();
        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
        assert_eq!(image.texture_descriptor.size.depth_or_array_layers, 5);
        assert_eq!(image.data.as_ref().map(Vec::len), Some(20));
    }
}
