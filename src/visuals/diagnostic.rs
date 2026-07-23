//! Reversible render-only views for inspecting geometry density and overdraw.

use bevy::pbr::wireframe::{WireframeConfig, WireframePlugin};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Face};
use bevy::shader::ShaderRef;

use crate::perf::PerfToggles;
use crate::visuals::DiagnosticViewState;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
struct OverdrawExtension {
    #[uniform(100)]
    color: LinearRgba,
}

impl MaterialExtension for OverdrawExtension {
    fn fragment_shader() -> ShaderRef {
        "shaders/overdraw.wgsl".into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

type AdditiveOverdrawMaterial = ExtendedMaterial<StandardMaterial, OverdrawExtension>;

#[derive(Resource)]
struct OverdrawMaterials {
    back: Handle<AdditiveOverdrawMaterial>,
    front: Handle<AdditiveOverdrawMaterial>,
    double_sided: Handle<AdditiveOverdrawMaterial>,
}

impl OverdrawMaterials {
    fn matching(&self, cull_mode: Option<Face>) -> Handle<AdditiveOverdrawMaterial> {
        match cull_mode {
            Some(Face::Front) => self.front.clone(),
            Some(Face::Back) => self.back.clone(),
            None => self.double_sided.clone(),
        }
    }
}

/// The authoritative material temporarily removed from the render entity.
#[derive(Component)]
struct OverdrawOriginalMaterial {
    original: Handle<StandardMaterial>,
    diagnostic: Handle<AdditiveOverdrawMaterial>,
}

pub(super) struct DiagnosticViewsPlugin;

impl Plugin for DiagnosticViewsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            WireframePlugin::default(),
            MaterialPlugin::<AdditiveOverdrawMaterial>::default(),
        ))
        .insert_resource(WireframeConfig {
            global: false,
            default_color: Color::srgb(0.2, 1.0, 0.35),
            ..default()
        })
        .init_resource::<DiagnosticViewState>()
        .add_systems(Startup, create_overdraw_material)
        // Scene instances can add mesh entities late in the frame. Last
        // catches those before render extraction and keeps the view global.
        .add_systems(
            Last,
            (apply_diagnostic_views, publish_diagnostic_state).chain(),
        );
    }
}

fn create_overdraw_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<AdditiveOverdrawMaterial>>,
) {
    let mut add = |cull_mode| {
        materials.add(ExtendedMaterial {
            base: StandardMaterial {
                base_color: Color::linear_rgba(1.0, 0.03, 0.0, 0.06),
                unlit: true,
                fog_enabled: false,
                alpha_mode: AlphaMode::Add,
                cull_mode,
                ..default()
            },
            extension: OverdrawExtension {
                color: LinearRgba::new(1.0, 0.03, 0.0, 0.06),
            },
        })
    };
    commands.insert_resource(OverdrawMaterials {
        back: add(Some(Face::Back)),
        front: add(Some(Face::Front)),
        double_sided: add(None),
    });
}

type StandardMeshQuery<'a> = (
    Entity,
    &'a MeshMaterial3d<StandardMaterial>,
    Option<&'a OverdrawOriginalMaterial>,
    Has<Mesh3d>,
);

type SavedMeshQuery<'a> = (
    Entity,
    &'a OverdrawOriginalMaterial,
    Has<Mesh3d>,
    Has<MeshMaterial3d<AdditiveOverdrawMaterial>>,
);

fn apply_diagnostic_views(
    mut commands: Commands,
    perf: Res<PerfToggles>,
    overdraw: Res<OverdrawMaterials>,
    standard_materials: Res<Assets<StandardMaterial>>,
    mut wireframe: ResMut<WireframeConfig>,
    standard_meshes: Query<StandardMeshQuery>,
    saved_meshes: Query<SavedMeshQuery>,
) {
    let wanted_wireframe = perf.wireframe && !perf.overdraw;
    if wireframe.global != wanted_wireframe {
        wireframe.global = wanted_wireframe;
    }

    // The shipped path pays no full-scene scan. While active we do scan each
    // frame so newly instantiated scenes join before render extraction; the
    // diagnostic mode already distorts cost by design.
    if !perf.overdraw && !perf.is_changed() && saved_meshes.is_empty() {
        return;
    }

    if perf.overdraw {
        for (entity, material, _saved, has_mesh) in &standard_meshes {
            if !has_mesh {
                continue;
            }
            let cull_mode = standard_materials
                .get(&material.0)
                .map(|material| material.cull_mode)
                .unwrap_or(Some(Face::Back));
            commands
                .entity(entity)
                .try_remove::<MeshMaterial3d<StandardMaterial>>()
                .try_insert(OverdrawOriginalMaterial {
                    original: material.0.clone(),
                    diagnostic: overdraw.matching(cull_mode),
                });
        }
        for (entity, saved, has_mesh, has_overdraw) in &saved_meshes {
            if !has_mesh {
                commands
                    .entity(entity)
                    .try_remove::<MeshMaterial3d<AdditiveOverdrawMaterial>>()
                    .try_remove::<OverdrawOriginalMaterial>();
            } else if !has_overdraw {
                commands
                    .entity(entity)
                    .try_insert(MeshMaterial3d(saved.diagnostic.clone()));
            }
        }
    } else {
        for (entity, original, has_mesh, has_overdraw) in &saved_meshes {
            let mut entity = commands.entity(entity);
            if has_overdraw {
                // Give retained render phases one extraction with no material
                // before the entity returns to the StandardMaterial pipeline.
                entity.try_remove::<MeshMaterial3d<AdditiveOverdrawMaterial>>();
            } else {
                entity.try_remove::<OverdrawOriginalMaterial>();
                if has_mesh {
                    entity.try_insert(MeshMaterial3d(original.original.clone()));
                }
            }
        }
    }
}

fn publish_diagnostic_state(
    perf: Res<PerfToggles>,
    saved_meshes: Query<(), With<OverdrawOriginalMaterial>>,
    mut state: ResMut<DiagnosticViewState>,
) {
    state.overdraw_material_override = perf.overdraw || !saved_meshes.is_empty();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let mut app = App::new();
        app.init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Assets<AdditiveOverdrawMaterial>>()
            .init_resource::<PerfToggles>()
            .init_resource::<WireframeConfig>()
            .init_resource::<DiagnosticViewState>()
            .add_systems(Startup, create_overdraw_material)
            .add_systems(
                Last,
                (apply_diagnostic_views, publish_diagnostic_state).chain(),
            );
        app
    }

    #[test]
    fn overdraw_handles_late_spawn_replacement_restore_and_orphan_cleanup() {
        let mut app = test_app();
        {
            let mut perf = app.world_mut().resource_mut::<PerfToggles>();
            perf.wireframe = true;
            perf.overdraw = true;
        }
        app.update();
        assert!(
            !app.world().resource::<WireframeConfig>().global,
            "overdraw wins even if invalid external state enables both views"
        );
        assert!(
            app.world()
                .resource::<DiagnosticViewState>()
                .overdraw_material_override
        );

        let original = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(Color::WHITE);
        let mesh = app
            .world_mut()
            .spawn((Mesh3d::default(), MeshMaterial3d(original)))
            .id();
        app.update();

        let entity = app.world().entity(mesh);
        assert!(!entity.contains::<MeshMaterial3d<StandardMaterial>>());
        assert!(!entity.contains::<MeshMaterial3d<AdditiveOverdrawMaterial>>());
        assert!(entity.contains::<OverdrawOriginalMaterial>());
        assert!(
            app.world()
                .resource::<DiagnosticViewState>()
                .overdraw_material_override
        );

        app.update();

        let entity = app.world().entity(mesh);
        assert!(!entity.contains::<MeshMaterial3d<StandardMaterial>>());
        assert!(entity.contains::<MeshMaterial3d<AdditiveOverdrawMaterial>>());
        assert!(entity.contains::<OverdrawOriginalMaterial>());

        let replacement = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(Color::BLACK);
        app.world_mut()
            .entity_mut(mesh)
            .insert(MeshMaterial3d(replacement.clone()));
        app.update();

        let entity = app.world().entity(mesh);
        assert!(!entity.contains::<MeshMaterial3d<StandardMaterial>>());
        assert_eq!(
            &entity
                .get::<OverdrawOriginalMaterial>()
                .expect("replacement becomes authoritative")
                .original,
            &replacement
        );

        let orphan = app
            .world_mut()
            .spawn((Mesh3d::default(), MeshMaterial3d(replacement.clone())))
            .id();
        app.update();
        app.update();
        app.world_mut().entity_mut(orphan).remove::<Mesh3d>();
        app.update();
        let orphan = app.world().entity(orphan);
        assert!(!orphan.contains::<OverdrawOriginalMaterial>());
        assert!(!orphan.contains::<MeshMaterial3d<AdditiveOverdrawMaterial>>());

        app.world_mut().resource_mut::<PerfToggles>().overdraw = false;
        app.update();

        let entity = app.world().entity(mesh);
        assert!(!entity.contains::<MeshMaterial3d<StandardMaterial>>());
        assert!(!entity.contains::<MeshMaterial3d<AdditiveOverdrawMaterial>>());
        assert!(entity.contains::<OverdrawOriginalMaterial>());

        app.update();

        let entity = app.world().entity(mesh);
        assert_eq!(
            &entity
                .get::<MeshMaterial3d<StandardMaterial>>()
                .expect("latest authoritative handle is restored")
                .0,
            &replacement
        );
        assert!(!entity.contains::<MeshMaterial3d<AdditiveOverdrawMaterial>>());
        assert!(!entity.contains::<OverdrawOriginalMaterial>());
        assert!(
            !app.world()
                .resource::<DiagnosticViewState>()
                .overdraw_material_override,
            "collection resumes only after the chained restoration is observable"
        );
    }

    #[test]
    fn overdraw_variants_preserve_the_source_cull_mode() {
        let mut app = test_app();
        app.update();

        let (back, front, double_sided) = {
            let variants = app.world().resource::<OverdrawMaterials>();
            (
                variants.matching(Some(Face::Back)),
                variants.matching(Some(Face::Front)),
                variants.matching(None),
            )
        };
        let materials = app.world().resource::<Assets<AdditiveOverdrawMaterial>>();
        assert_eq!(
            materials.get(&back).expect("back variant").base.cull_mode,
            Some(Face::Back)
        );
        assert_eq!(
            materials.get(&front).expect("front variant").base.cull_mode,
            Some(Face::Front)
        );
        assert_eq!(
            materials
                .get(&double_sided)
                .expect("double-sided variant")
                .base
                .cull_mode,
            None
        );
    }
}
