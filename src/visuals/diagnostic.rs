//! Reversible render-only views for inspecting geometry density and overdraw.
//!
//! # Una sola copia, genérica
//!
//! Este archivo tenía **el mismo bucle escrito tres veces**, una por tipo de
//! material, con tres componentes gemelos y seis tipos de query. El costo no
//! era la repetición: era que agregar un material cuarto no rompía nada y la
//! vista simplemente dejaba de mostrarlo. Pasó — la pradera se mudó a su
//! `ExtendedMaterial` y desapareció durante una tarde de la única vista que
//! existe para mirarla.
//!
//! Ahora el swap es genérico sobre [`InstrumentedMaterial`] y se instala desde
//! [`super::material_registry`], junto con el recuento del inventario. Un
//! material se registra una vez y las dos herramientas lo ven.

use bevy::pbr::wireframe::{WireframeConfig, WireframePlugin};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MeshMaterial3d};
use bevy::prelude::*;
use bevy::render::mesh::Mesh3d;
use bevy::render::render_resource::{AsBindGroup, Face};
use bevy::shader::ShaderRef;

use crate::perf::PerfToggles;
use crate::visuals::DiagnosticViewState;
use crate::visuals::material_registry::InstrumentedMaterial;

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

/// El material verdadero, guardado mientras la vista de overdraw ocupa su lugar.
///
/// Genérico sobre el tipo de material: es el mismo dato para todos, y tenerlo
/// tres veces con tres nombres es lo que hacía que olvidarse de uno fuera
/// invisible.
#[derive(Component)]
struct OverdrawOriginal<M: InstrumentedMaterial> {
    original: Handle<M>,
    diagnostic: Handle<AdditiveOverdrawMaterial>,
}

/// Si algún material sigue guardado, o sea si la restauración de dos frames
/// todavía está en curso. Lo escriben todos los tipos y lo lee la publicación.
#[derive(Resource, Default)]
struct OverdrawResidue(bool);

/// Las etapas del swap, para que el orden valga entre tipos registrados desde
/// plugins distintos.
#[derive(SystemSet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum OverdrawSet {
    Reset,
    Swap,
    Publish,
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
        .init_resource::<OverdrawResidue>()
        .add_systems(Startup, create_overdraw_material)
        // Scene instances can add mesh entities late in the frame. Last
        // catches those before render extraction and keeps the view global.
        .configure_sets(
            Last,
            (OverdrawSet::Reset, OverdrawSet::Swap, OverdrawSet::Publish).chain(),
        )
        .add_systems(
            Last,
            (
                (reset_overdraw_residue, apply_wireframe).in_set(OverdrawSet::Reset),
                publish_diagnostic_state.in_set(OverdrawSet::Publish),
            ),
        );
    }
}

/// Engancha un tipo de material al swap de overdraw. Lo llama
/// [`super::material_registry::InstrumentedMaterialAppExt`] — no se llama suelto,
/// justamente para que registrar el material y registrar sus herramientas sean
/// el mismo acto.
pub(crate) fn register_overdraw<M: InstrumentedMaterial>(app: &mut App) {
    app.add_systems(
        Last,
        swap_material_for_overdraw::<M>.in_set(OverdrawSet::Swap),
    );
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

fn reset_overdraw_residue(mut residue: ResMut<OverdrawResidue>) {
    residue.0 = false;
}

fn apply_wireframe(perf: Res<PerfToggles>, mut wireframe: ResMut<WireframeConfig>) {
    let wanted = perf.wireframe && !perf.overdraw;
    if wireframe.global != wanted {
        wireframe.global = wanted;
    }
}

type LiveMesh<'a, M> = (Entity, &'a MeshMaterial3d<M>);
type SavedMesh<'a, M> = (
    Entity,
    &'a OverdrawOriginal<M>,
    Has<Mesh3d>,
    Has<MeshMaterial3d<AdditiveOverdrawMaterial>>,
);

/// Reemplaza —o repone— el material de un tipo, en dos tiempos.
///
/// Los dos tiempos no son prolijidad: las fases de render son retenidas, así
/// que la entidad tiene que pasar **una extracción sin material** entre un
/// pipeline y el otro. Por eso quitar e insertar nunca ocurren en el mismo
/// frame, y por eso los dos bucles usan queries distintas: lo insertado por el
/// primero no lo ve el segundo hasta el frame siguiente.
fn swap_material_for_overdraw<M: InstrumentedMaterial>(
    mut commands: Commands,
    perf: Res<PerfToggles>,
    overdraw: Res<OverdrawMaterials>,
    materials: Res<Assets<M>>,
    live: Query<LiveMesh<M>, With<Mesh3d>>,
    saved: Query<SavedMesh<M>>,
    mut residue: ResMut<OverdrawResidue>,
) {
    // The shipped path pays no full-scene scan. While active we do scan each
    // frame so newly instantiated scenes join before render extraction; the
    // diagnostic mode already distorts cost by design.
    if !perf.overdraw && !perf.is_changed() && saved.is_empty() {
        return;
    }

    if perf.overdraw {
        for (entity, material) in &live {
            let cull_mode = materials
                .get(&material.0)
                .map_or(Some(Face::Back), InstrumentedMaterial::diagnostic_cull_mode);
            commands
                .entity(entity)
                .try_remove::<MeshMaterial3d<M>>()
                .try_insert(OverdrawOriginal::<M> {
                    original: material.0.clone(),
                    diagnostic: overdraw.matching(cull_mode),
                });
        }
        for (entity, saved, has_mesh, has_overdraw) in &saved {
            if !has_mesh {
                // La malla se fue mientras la vista estaba puesta: no hay nada
                // que restaurar y el guardado quedaría colgando para siempre.
                commands
                    .entity(entity)
                    .try_remove::<MeshMaterial3d<AdditiveOverdrawMaterial>>()
                    .try_remove::<OverdrawOriginal<M>>();
            } else if !has_overdraw {
                commands
                    .entity(entity)
                    .try_insert(MeshMaterial3d(saved.diagnostic.clone()));
            }
        }
        return;
    }

    for (entity, saved, has_mesh, has_overdraw) in &saved {
        let mut entity = commands.entity(entity);
        if has_overdraw {
            // Éste sigue a medio restaurar **después** de este frame: recién el
            // que viene recupera su material. El residuo se declara acá y no
            // con `saved.is_empty()` porque esa pregunta se contesta antes de
            // que los `Commands` se apliquen, y daría "todavía hay" un frame
            // entero de más — con el inventario congelado sin motivo.
            residue.0 = true;
            entity.try_remove::<MeshMaterial3d<AdditiveOverdrawMaterial>>();
        } else {
            entity.try_remove::<OverdrawOriginal<M>>();
            if has_mesh {
                entity.try_insert(MeshMaterial3d(saved.original.clone()));
            }
        }
    }
}

/// Mientras el swap esté a medio camino, lo que se dibuja no es el juego: el
/// inventario tiene que saberlo para no publicar un presupuesto de mentira.
fn publish_diagnostic_state(
    perf: Res<PerfToggles>,
    residue: Res<OverdrawResidue>,
    mut state: ResMut<DiagnosticViewState>,
) {
    state.overdraw_material_override = perf.overdraw || residue.0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visuals::grass_material::GrassMaterial;
    use crate::visuals::terrain_material::TerrainMaterial;

    fn test_app() -> App {
        let mut app = App::new();
        app.init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Assets<TerrainMaterial>>()
            .init_resource::<Assets<GrassMaterial>>()
            .init_resource::<Assets<AdditiveOverdrawMaterial>>()
            .init_resource::<PerfToggles>()
            .init_resource::<WireframeConfig>()
            .init_resource::<OverdrawResidue>()
            .init_resource::<DiagnosticViewState>()
            .add_systems(Startup, create_overdraw_material)
            .configure_sets(
                Last,
                (OverdrawSet::Reset, OverdrawSet::Swap, OverdrawSet::Publish).chain(),
            )
            .add_systems(
                Last,
                (
                    (reset_overdraw_residue, apply_wireframe).in_set(OverdrawSet::Reset),
                    swap_material_for_overdraw::<StandardMaterial>.in_set(OverdrawSet::Swap),
                    publish_diagnostic_state.in_set(OverdrawSet::Publish),
                ),
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
        assert!(entity.contains::<OverdrawOriginal<StandardMaterial>>());
        assert!(
            app.world()
                .resource::<DiagnosticViewState>()
                .overdraw_material_override
        );

        app.update();

        let entity = app.world().entity(mesh);
        assert!(!entity.contains::<MeshMaterial3d<StandardMaterial>>());
        assert!(entity.contains::<MeshMaterial3d<AdditiveOverdrawMaterial>>());
        assert!(entity.contains::<OverdrawOriginal<StandardMaterial>>());

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
                .get::<OverdrawOriginal<StandardMaterial>>()
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
        assert!(!orphan.contains::<OverdrawOriginal<StandardMaterial>>());
        assert!(!orphan.contains::<MeshMaterial3d<AdditiveOverdrawMaterial>>());

        app.world_mut().resource_mut::<PerfToggles>().overdraw = false;
        app.update();

        let entity = app.world().entity(mesh);
        assert!(!entity.contains::<MeshMaterial3d<StandardMaterial>>());
        assert!(!entity.contains::<MeshMaterial3d<AdditiveOverdrawMaterial>>());
        assert!(entity.contains::<OverdrawOriginal<StandardMaterial>>());

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
        assert!(!entity.contains::<OverdrawOriginal<StandardMaterial>>());
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

    /// La pradera es doble cara (`cull_mode: None`) y el terreno no. Que el swap
    /// lea el modo **por el trait** y no por un `match` sobre tipos es lo que
    /// hace que un material nuevo no pueda entrar mal: el compilador le exige la
    /// respuesta.
    #[test]
    fn every_instrumented_material_answers_for_its_own_culling() {
        let grass = crate::visuals::grass_material::GrassMaterial {
            base: StandardMaterial {
                cull_mode: None,
                ..default()
            },
            extension: crate::visuals::grass_material::GrassExtension {
                grass_data: default(),
                interaction_map: None,
                blade_records: Handle::default(),
            },
        };
        assert_eq!(grass.diagnostic_cull_mode(), None);
        assert_eq!(
            StandardMaterial {
                cull_mode: Some(Face::Back),
                ..default()
            }
            .diagnostic_cull_mode(),
            Some(Face::Back)
        );
    }
}
