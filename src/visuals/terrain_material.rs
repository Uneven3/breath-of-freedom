//! One layered material for every semantic terrain kind.
//!
//! The four source PNGs are ordinary, independently replaceable assets. At
//! runtime they are packed into one `texture_2d_array`, keeping the terrain a
//! single mesh/material/draw family while final art replaces today's solid
//! colours without changing code.

use bevy::asset::{AssetEvent, RenderAssetUsages};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;

use crate::asset_pipeline::terrain_textures::{TERRAIN_TEXTURE_EDGE, TERRAIN_TEXTURES};
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

const KIND_LEGEND: [TerrainLegendEntry; 4] = [
    TerrainLegendEntry {
        label: "Tierra / camino",
        color: Color::srgb_u8(121, 81, 58),
    },
    TerrainLegendEntry {
        label: "Roca",
        color: Color::srgb_u8(125, 130, 140),
    },
    TerrainLegendEntry {
        label: "Pasto largo",
        color: Color::srgb_u8(79, 155, 69),
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
        app.add_plugins(MaterialPlugin::<TerrainMaterial>::default())
            .init_resource::<TerrainDebugState>()
            .add_message::<TerrainDebugViewRequest>()
            .add_systems(Startup, setup_terrain_material)
            .add_systems(
                Update,
                (
                    mark_changed_source_textures,
                    rebuild_texture_array,
                    apply_debug_view_requests,
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
            grass_tint: grass_tint(),
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
const GRASS_TINT_STRENGTH: f32 = 0.25;

fn grass_tint() -> Vec4 {
    let root = super::grass::ROOT_COLOR;
    Vec4::new(root.red, root.green, root.blue, GRASS_TINT_STRENGTH)
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

fn rebuild_texture_array(
    mut terrain: Option<ResMut<TerrainMaterialAssets>>,
    mut images: ResMut<Assets<Image>>,
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
    let Some(mut target) = images.get_mut(&terrain.array) else {
        warn!("[terrain] texture array asset disappeared; keeping current material");
        return;
    };
    *target = array;
    info!(
        "[terrain] packed {} canonical PNGs into one texture array",
        TERRAIN_TEXTURES.len()
    );
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
    mut state: ResMut<TerrainDebugState>,
    terrain: Option<Res<TerrainMaterialAssets>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
) {
    let Some(view) = requests.read().last().map(|request| request.0) else {
        return;
    };
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
        for (expected, kind) in TerrainKind::ALL.into_iter().enumerate() {
            assert_eq!(usize::from(texture_layer(kind)), expected);
        }
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
    fn fallback_is_a_four_layer_rgba_array() {
        let image = fallback_array();
        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
        assert_eq!(image.texture_descriptor.size.depth_or_array_layers, 4);
        assert_eq!(image.data.as_ref().map(Vec::len), Some(16));
    }
}
