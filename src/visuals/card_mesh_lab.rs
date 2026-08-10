//! A small visual lab for judging grass-card silhouettes before touching the meadow.

use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::asset_pipeline::MaterialPalette;
use crate::scene::{AppState, SceneId};

#[derive(Component)]
pub(crate) struct CardMeshLab;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CardMeshVariant {
    Meadow,
}

impl CardMeshVariant {
    const fn triangles(self) -> usize {
        match self {
            Self::Meadow => 2,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GrassReference {
    Leaf,
    Spike,
}

#[derive(Component)]
pub(super) struct FaceCamera {
    yaw_offset: f32,
}

#[derive(Resource)]
pub(super) struct CardMeshLabAssets {
    leaf: Handle<Mesh>,
    spike: Handle<Mesh>,
    meadow_card: Handle<Mesh>,
    leaf_material: Handle<StandardMaterial>,
    card_material: Handle<StandardMaterial>,
}

pub(super) fn build_card_mesh_lab_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    palette: Res<MaterialPalette>,
    asset_server: Res<AssetServer>,
) {
    // This is a silhouette lab, not a PBR approval gate. Keep an independent
    // copy so normal/light/exposure cannot turn a valid card black, while the
    // production foliage material remains lit everywhere else.
    let lab_material = palette.instance("FoliageCommon", &mut materials);
    let Some(mut material) = materials.get_mut(&lab_material) else {
        error!("[card mesh] could not configure its isolated lab material");
        return;
    };
    configure_lab_material(&mut material);
    drop(material);
    let card_material = materials.add(meadow_card_material(&asset_server));

    commands.insert_resource(CardMeshLabAssets {
        leaf: meshes.add(blade_mesh(0.08, 0.72)),
        spike: meshes.add(spike_mesh(0.15, 0.56)),
        meadow_card: meshes.add(card_mesh(4.8, 4.8)),
        leaf_material: lab_material.clone(),
        card_material,
    });
}

fn configure_lab_material(material: &mut StandardMaterial) {
    material.unlit = true;
    // The lab deliberately compares geometry, not the source card texture:
    // that PNG has an opaque black background and masks the silhouette.
    material.base_color_texture = None;
    material.alpha_mode = AlphaMode::Opaque;
    material.cull_mode = None;
    material.double_sided = true;
}

fn meadow_card_material(asset_server: &AssetServer) -> StandardMaterial {
    let mut material = StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(asset_server.load("textures/props/T_GrassMeadowCard_Albedo.png")),
        ..default()
    };
    configure_meadow_card_material(&mut material);
    material
}

fn configure_meadow_card_material(material: &mut StandardMaterial) {
    // The card is the one deliberate alpha-cost experiment in this lab. Its
    // unlit colour keeps the checkpoint about texture, coverage and silhouette
    // instead of the current sun angle.
    material.unlit = true;
    material.alpha_mode = AlphaMode::Mask(0.4);
    material.cull_mode = None;
    material.double_sided = true;
    material.perceptual_roughness = 0.95;
    material.reflectance = 0.05;
}

pub(super) fn spawn_card_mesh_lab(mut commands: Commands, assets: Res<CardMeshLabAssets>) {
    for (index, offset) in [
        Vec3::new(-0.24, 0.0, 0.0),
        Vec3::ZERO,
        Vec3::new(0.24, 0.0, 0.08),
    ]
    .into_iter()
    .enumerate()
    {
        spawn_reference(
            &mut commands,
            &assets,
            GrassReference::Leaf,
            assets.leaf.clone(),
            Vec3::new(-3.4, 0.0, 5.5) + offset,
            index,
        );
        spawn_reference(
            &mut commands,
            &assets,
            GrassReference::Spike,
            assets.spike.clone(),
            Vec3::new(-1.1, 0.0, 5.5) + offset,
            index,
        );
    }

    let variant = CardMeshVariant::Meadow;
    commands.spawn((
        Name::new(format!("CardMesh_{variant:?}_{}tris", variant.triangles())),
        CardMeshLab,
        variant,
        FaceCamera { yaw_offset: 0.0 },
        Mesh3d(assets.meadow_card.clone()),
        MeshMaterial3d(assets.card_material.clone()),
        NotShadowCaster,
        Transform::from_translation(Vec3::new(2.7, 0.0, 6.8)),
        DespawnOnExit(AppState::Scene(SceneId::CardMesh)),
    ));
}

fn spawn_reference(
    commands: &mut Commands,
    assets: &CardMeshLabAssets,
    reference: GrassReference,
    mesh: Handle<Mesh>,
    position: Vec3,
    index: usize,
) {
    commands.spawn((
        Name::new(format!("CardMeshReference_{reference:?}_{index}")),
        CardMeshLab,
        reference,
        Mesh3d(mesh),
        MeshMaterial3d(assets.leaf_material.clone()),
        NotShadowCaster,
        Transform::from_translation(position),
        DespawnOnExit(AppState::Scene(SceneId::CardMesh)),
    ));
}

pub(super) fn face_card_meshes(
    camera: Option<Single<&GlobalTransform, With<Camera3d>>>,
    mut cards: Query<(&mut Transform, &FaceCamera), With<CardMeshLab>>,
) {
    let Some(camera) = camera else {
        return;
    };
    for (mut transform, card) in &mut cards {
        let to_camera = camera.translation() - transform.translation;
        if to_camera.xz().length_squared() > 1.0e-6 {
            transform.rotation =
                Quat::from_rotation_y(to_camera.x.atan2(to_camera.z) + card.yaw_offset);
        }
    }
}

fn blade_mesh(width: f32, height: f32) -> Mesh {
    let mut mesh = mesh_with_vertices(
        vec![
            [0.0, 0.0, 0.0],
            [-width * 0.5, height * 0.58, 0.0],
            [width * 0.5, height * 0.58, 0.0],
            [0.0, height, 0.0],
        ],
        vec![[0.0, 0.0], [0.0, 0.58], [1.0, 0.58], [0.5, 1.0]],
        vec![[0.0, 0.0, 1.0]; 4],
    );
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 1, 3, 2]));
    mesh
}

fn spike_mesh(width: f32, height: f32) -> Mesh {
    let mut mesh = mesh_with_vertices(
        vec![
            [-width * 0.5, 0.0, 0.0],
            [width * 0.5, 0.0, 0.0],
            [0.0, height, 0.0],
        ],
        vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
        vec![[0.0, 0.0, 1.0]; 3],
    );
    mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
    mesh
}

fn card_mesh(width: f32, height: f32) -> Mesh {
    let mut mesh = mesh_with_vertices(
        vec![
            [-width * 0.5, 0.0, 0.0],
            [width * 0.5, 0.0, 0.0],
            [-width * 0.5, height, 0.0],
            [width * 0.5, height, 0.0],
        ],
        // PNG rows start at the top; a base at world y=0 needs texture v=1.
        vec![[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]],
        vec![[0.0, 0.0, 1.0]; 4],
    );
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 1, 3, 2]));
    mesh
}

fn mesh_with_vertices(
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    normals: Vec<[f32; 3]>,
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::mesh::VertexAttributeValues;

    fn triangles(mesh: &Mesh) -> usize {
        mesh.indices().map_or(0, |indices| indices.len() / 3)
    }

    #[test]
    fn the_meadow_card_is_a_two_triangle_full_uv_quad() {
        let mesh = card_mesh(4.8, 4.8);
        assert_eq!(triangles(&mesh), CardMeshVariant::Meadow.triangles());
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("a card mesh must carry positions");
        };
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("a card mesh must carry UV_0");
        };
        assert_eq!(positions.len(), 4, "the detailed drawing must cost a quad");
        assert_eq!(uvs, &vec![[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]]);
    }

    #[test]
    fn references_keep_their_declared_geometry() {
        assert_eq!(triangles(&blade_mesh(0.08, 0.72)), 2);
        assert_eq!(triangles(&spike_mesh(0.15, 0.56)), 1);
    }

    #[test]
    fn cards_are_complete_non_degenerate_meshes() {
        let mesh = card_mesh(4.8, 4.8);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("a card mesh must carry positions");
        };
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("a card mesh must carry UV_0");
        };
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("a card mesh must carry normals");
        };
        assert_eq!(positions.len(), uvs.len());
        assert_eq!(positions.len(), normals.len());
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!("a card mesh must use u32 indices");
        };
        for triangle in indices.chunks_exact(3) {
            let [first, second, third] = triangle else {
                unreachable!("chunks_exact(3) only yields triplets");
            };
            let (a, b, c) = (
                positions[*first as usize],
                positions[*second as usize],
                positions[*third as usize],
            );
            let twice_area = (Vec3::from(b) - Vec3::from(a)).cross(Vec3::from(c) - Vec3::from(a));
            assert!(
                twice_area.length_squared() > 1.0e-6,
                "a card triangle must not degenerate"
            );
            let face_normal = twice_area.normalize();
            let vertex_normal = Vec3::from(normals[*first as usize]).normalize();
            assert!(
                face_normal.dot(vertex_normal) > 0.999,
                "a card face must wind in the direction of its vertex normal"
            );
        }
    }

    #[test]
    fn lab_material_is_unlit_opaque_and_keeps_the_foliage_color() {
        let color = Color::srgb(0.32, 0.58, 0.22);
        let mut material = StandardMaterial {
            base_color: color,
            base_color_texture: Some(Handle::default()),
            alpha_mode: AlphaMode::Mask(0.4),
            cull_mode: Some(bevy::render::render_resource::Face::Back),
            ..default()
        };
        configure_lab_material(&mut material);

        assert!(material.unlit);
        assert_eq!(material.base_color.to_srgba(), color.to_srgba());
        assert!(material.base_color_texture.is_none());
        assert_eq!(material.alpha_mode, AlphaMode::Opaque);
        assert_eq!(material.cull_mode, None);
        assert!(material.double_sided);
    }

    #[test]
    fn meadow_card_material_keeps_its_texture_and_pays_only_the_mask_cost() {
        let mut material = StandardMaterial {
            base_color_texture: Some(Handle::default()),
            alpha_mode: AlphaMode::Opaque,
            cull_mode: Some(bevy::render::render_resource::Face::Back),
            ..default()
        };
        configure_meadow_card_material(&mut material);

        assert!(material.unlit);
        assert!(material.base_color_texture.is_some());
        assert_eq!(material.alpha_mode, AlphaMode::Mask(0.4));
        assert_eq!(material.cull_mode, None);
        assert!(material.double_sided);
    }

    #[test]
    fn every_lab_specimen_is_not_a_shadow_caster() {
        let mut world = World::new();
        let mut materials = Assets::<StandardMaterial>::default();
        let reference_material = materials.add(StandardMaterial::default());
        let card_material = materials.add(StandardMaterial::default());
        world.insert_resource(materials);
        world.insert_resource(CardMeshLabAssets {
            leaf: Handle::default(),
            spike: Handle::default(),
            meadow_card: Handle::default(),
            leaf_material: reference_material.clone(),
            card_material: card_material.clone(),
        });
        let _ = world.run_system_once(spawn_card_mesh_lab);
        let total = world
            .query_filtered::<Entity, With<CardMeshLab>>()
            .iter(&world)
            .count();
        let casting = world
            .query_filtered::<Entity, (With<CardMeshLab>, Without<NotShadowCaster>)>()
            .iter(&world)
            .count();
        assert_eq!(
            total, 7,
            "the lab has three references of each shape plus one meadow card"
        );
        assert_eq!(casting, 0, "a lab specimen must never cast a shadow");
        let references_with_wrong_material = world
            .query_filtered::<&MeshMaterial3d<StandardMaterial>, With<GrassReference>>()
            .iter(&world)
            .filter(|material| material.0 != reference_material)
            .count();
        assert_eq!(
            references_with_wrong_material, 0,
            "references must retain their opaque lab material"
        );
        let cards_with_wrong_material = world
            .query_filtered::<&MeshMaterial3d<StandardMaterial>, With<CardMeshVariant>>()
            .iter(&world)
            .filter(|material| material.0 != card_material)
            .count();
        assert_eq!(
            cards_with_wrong_material, 0,
            "the meadow card must retain its alpha material"
        );
    }
}
