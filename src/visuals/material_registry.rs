//! El registro de materiales: qué tiene que declarar un tipo de material para
//! que las herramientas lo vean.
//!
//! # El bug que este módulo existe para hacer imposible
//!
//! Hasta el 2026-08-06 dos herramientas distintas —el inventario de escena
//! (`debug::collect`) y la vista de overdraw (`visuals::diagnostic`)— sabían de
//! los materiales por una **lista de tipos escrita a mano**, cada una la suya.
//! Un material nuevo no daba error: daba una herramienta que seguía funcionando
//! y contestando de menos. Pasó dos veces, con el mismo material y el mismo
//! síntoma: `GrassMaterial` estuvo fuera del presupuesto desde que existe (el
//! grader calificaba la escena sin lo más caro que hay en ella) y fuera de la
//! vista de overdraw una tarde entera — justo la geometría que esa vista se
//! abre para mirar.
//!
//! Acá hay **una** llamada, [`InstrumentedMaterialAppExt::add_instrumented_material`],
//! y engancha las dos cosas a la vez. Registrar el material y registrarlo en las
//! herramientas dejan de ser dos actos, así que no se pueden desincronizar.
//!
//! # Y lo que sigue sin poder garantizarse
//!
//! Nada obliga a llamarla. Lo que sí hay es un guardia que hace ruido: el censo
//! cuenta cuántas mallas visibles vio con material registrado, `collect_scene`
//! lo compara contra las que hay dibujadas —esa cuenta no mira el material, así
//! que no puede quedarse corta— y avisa por la diferencia. Una omisión se
//! delata en la primera corrida en vez de en un número que no cierra tres
//! semanas después.
//!
//! # Atribución: de quién es cada triángulo
//!
//! Un total de escena no contesta *"cuánto está poniendo la pradera ahora
//! mismo"*, que es la pregunta que cada ajuste del pasto necesita. El censo
//! reparte lo que cuenta entre [`Subject`]s, y un sujeto se declara con un
//! componente en la entidad que dibuja ([`VisualSubject`]) — no por tipo de
//! material, porque el bosque y las cápsulas comparten `StandardMaterial` y son
//! cosas distintas.

use bevy::asset::{AssetId, UntypedAssetId};
use bevy::camera::visibility::ViewVisibility;
use bevy::pbr::{ExtendedMaterial, Material, MaterialExtension, MaterialPlugin, MeshMaterial3d};
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::render::mesh::Mesh3d;
use bevy::render::render_resource::{AsBindGroup, Face};

/// Un tipo de material que las herramientas de diagnóstico saben tratar.
///
/// Lo único que pide de más sobre [`Material`] es el modo de culling, y no por
/// gusto: la vista de overdraw reemplaza el material por uno aditivo y **tiene
/// que conservarlo**, o una malla de una sola cara empieza a pintar dos y el
/// número que la vista existe para dar sale inflado.
///
/// La implementación para `ExtendedMaterial<StandardMaterial, _>` cubre de una
/// vez todos los materiales extendidos, presentes y futuros: no hay lista que
/// extender.
///
/// El bound sobre `Data` sale de Bevy, no de nosotros: es lo que
/// `MaterialPlugin` pide para ser un plugin. Puesto acá como supertrait, cada
/// uso genérico lo hereda y ninguno tiene que repetirlo — que es lo que permite
/// que el registro instale el plugin **y** las herramientas de una sola llamada.
pub(crate) trait InstrumentedMaterial:
    Material + AsBindGroup<Data: PartialEq + Eq + std::hash::Hash + Clone>
{
    fn diagnostic_cull_mode(&self) -> Option<Face>;
}

impl InstrumentedMaterial for StandardMaterial {
    fn diagnostic_cull_mode(&self) -> Option<Face> {
        self.cull_mode
    }
}

impl<E> InstrumentedMaterial for ExtendedMaterial<StandardMaterial, E>
where
    E: MaterialExtension<Data: Copy + Eq + std::hash::Hash>,
{
    fn diagnostic_cull_mode(&self) -> Option<Face> {
        self.base.cull_mode
    }
}

/// De qué sistema es una malla, para el reparto del inventario.
///
/// Deliberadamente pocos y gruesos: son los sistemas que compiten por el frame
/// y sobre los que se toman decisiones, no una taxonomía de la escena.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Subject {
    Meadow,
    Forest,
    Terrain,
}

impl Subject {
    pub(crate) const ALL: [Subject; 3] = [Subject::Meadow, Subject::Forest, Subject::Terrain];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Subject::Meadow => "pradera",
            Subject::Forest => "bosque",
            Subject::Terrain => "terreno",
        }
    }

    pub(crate) const fn index(self) -> usize {
        match self {
            Subject::Meadow => 0,
            Subject::Forest => 1,
            Subject::Terrain => 2,
        }
    }
}

/// Cuántos casilleros tiene el reparto: uno por sujeto más el resto.
pub(crate) const BUCKETS: usize = Subject::ALL.len() + 1;
const OTHER: usize = BUCKETS - 1;

/// A qué sistema pertenece esta malla. Sin el componente, la malla cae en "el
/// resto", que es una respuesta honesta y no un agujero.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct VisualSubject(pub Subject);

/// Lo que un sujeto pone en el frame.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct SubjectTally {
    pub meshes: u32,
    pub triangles: usize,
    pub draws: usize,
}

/// El recuento del frame, mientras se arma.
///
/// Vive separado de [`crate::perf::budget::SceneInventory`] a propósito: esto
/// es el andamio (conjuntos que se llenan y se tiran), aquello es el resultado
/// publicado que el resto del juego lee.
#[derive(Resource, Default)]
pub(crate) struct SceneCensus {
    /// Mallas visibles cuyo material está registrado. Es la mitad que puede
    /// quedarse corta, y por eso se compara contra el conteo sin material.
    accounted_meshes: u32,
    subjects: [SubjectTally; BUCKETS],
    /// Bevy batchea por `(malla, material)`, así que un lote distinto es un
    /// draw. Untyped para que todos los tipos sumen en el mismo conjunto.
    batches: HashSet<(AssetId<Mesh>, UntypedAssetId)>,
    subject_batches: [HashSet<(AssetId<Mesh>, UntypedAssetId)>; BUCKETS],
    materials: HashSet<UntypedAssetId>,
}

impl SceneCensus {
    fn clear(&mut self) {
        self.accounted_meshes = 0;
        self.subjects = [SubjectTally::default(); BUCKETS];
        self.batches.clear();
        for set in &mut self.subject_batches {
            set.clear();
        }
        self.materials.clear();
    }

    fn record(
        &mut self,
        subject: Option<Subject>,
        mesh: AssetId<Mesh>,
        material: UntypedAssetId,
        triangles: usize,
    ) {
        let bucket = subject.map_or(OTHER, Subject::index);
        self.accounted_meshes += 1;
        self.batches.insert((mesh, material));
        self.materials.insert(material);
        self.subject_batches[bucket].insert((mesh, material));
        let tally = &mut self.subjects[bucket];
        tally.meshes += 1;
        tally.triangles += triangles;
    }

    /// Los draws no se pueden sumar mientras se cuenta —son la cardinalidad de
    /// un conjunto, no un acumulador— así que se cierran acá.
    pub(crate) fn tallies(&self) -> [SubjectTally; BUCKETS] {
        let mut out = self.subjects;
        for (bucket, tally) in out.iter_mut().enumerate() {
            tally.draws = self.subject_batches[bucket].len();
        }
        out
    }

    pub(crate) fn accounted_meshes(&self) -> u32 {
        self.accounted_meshes
    }

    pub(crate) fn draws(&self) -> usize {
        self.batches.len()
    }

    pub(crate) fn materials(&self) -> usize {
        self.materials.len()
    }
}

/// Cada cuánto se hace el recuento. Un barrido de escena entera es más pesado
/// que cualquier otro colector del hub, y una herramienta que aparece en el
/// tiempo que mide no sirve.
const CENSUS_PERIOD_SECS: f32 = 0.25;

/// Abre y cierra la ventana del recuento.
///
/// Existe en vez de un `on_timer` por sistema porque son varios sistemas que
/// tienen que contar **el mismo frame**: cada `on_timer` lleva su propio reloj y
/// dos relojes con el mismo período no disparan juntos. Con un solo recurso, la
/// condición es la misma lectura para todos.
#[derive(Resource)]
pub(crate) struct CensusWindow {
    timer: Timer,
    open: bool,
}

impl Default for CensusWindow {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(CENSUS_PERIOD_SECS, TimerMode::Repeating),
            open: false,
        }
    }
}

/// Las tres etapas del recuento, como conjuntos para que el orden cruce los
/// límites de plugin: quien cuenta vive en `visuals`, quien publica en `debug`.
#[derive(SystemSet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum CensusSet {
    Open,
    Count,
    Read,
}

/// El reloj corre siempre; lo que se saltea es el barrido.
fn open_census_window(
    time: Res<Time<Real>>,
    mut window: ResMut<CensusWindow>,
    mut census: ResMut<SceneCensus>,
) {
    window.open = window.timer.tick(time.delta()).just_finished();
    if window.open {
        census.clear();
    }
}

pub(crate) fn census_is_open(window: Res<CensusWindow>) -> bool {
    window.open
}

type CensusMesh<'a, M> = (
    &'a ViewVisibility,
    &'a Mesh3d,
    &'a MeshMaterial3d<M>,
    Option<&'a VisualSubject>,
);

/// Cuenta un tipo de material. Una instancia por tipo registrado, y ninguna
/// escrita a mano.
fn count_material<M: InstrumentedMaterial>(
    meshes: Query<CensusMesh<M>>,
    assets: Res<Assets<Mesh>>,
    mut census: ResMut<SceneCensus>,
) {
    for (visibility, mesh3d, material, subject) in &meshes {
        if !visibility.get() {
            continue; // Descartada por frustum, jerarquía o rango: no se envía.
        }
        let triangles = assets.get(&mesh3d.0).map_or(0, mesh_triangles);
        census.record(
            subject.map(|subject| subject.0),
            mesh3d.0.id(),
            material.0.id().untyped(),
            triangles,
        );
    }
}

/// Triángulos de una malla, con la única sutileza que tiene: una malla sin
/// índices lista cada vértice de cada triángulo.
pub(crate) fn mesh_triangles(mesh: &Mesh) -> usize {
    match mesh.indices() {
        Some(indices) => indices.len() / 3,
        None => mesh.count_vertices() / 3,
    }
}

pub(crate) trait InstrumentedMaterialAppExt {
    /// Registra un tipo de material y, con él, todo lo que las herramientas
    /// necesitan para verlo: el recuento del inventario y la vista de overdraw.
    ///
    /// Idempotente sobre el `MaterialPlugin`: `StandardMaterial` ya viene con
    /// `DefaultPlugins`, y agregarlo de nuevo haría panickear el arranque.
    fn add_instrumented_material<M: InstrumentedMaterial>(&mut self) -> &mut Self;
}

impl InstrumentedMaterialAppExt for App {
    fn add_instrumented_material<M: InstrumentedMaterial>(&mut self) -> &mut Self {
        if !self.is_plugin_added::<MaterialPlugin<M>>() {
            self.add_plugins(MaterialPlugin::<M>::default());
        }
        self.add_systems(
            Update,
            count_material::<M>
                .in_set(CensusSet::Count)
                .run_if(census_is_open),
        );
        super::diagnostic::register_overdraw::<M>(self);
        self
    }
}

pub(super) struct MaterialRegistryPlugin;

impl Plugin for MaterialRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneCensus>()
            .init_resource::<CensusWindow>()
            .configure_sets(
                Update,
                (CensusSet::Open, CensusSet::Count, CensusSet::Read).chain(),
            )
            .add_systems(Update, open_census_window.in_set(CensusSet::Open));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn census_with(entries: &[(Option<Subject>, u64, u64, usize)]) -> SceneCensus {
        let mut census = SceneCensus::default();
        for (subject, mesh, material, triangles) in entries {
            census.record(
                *subject,
                AssetId::<Mesh>::Uuid {
                    uuid: bevy::asset::uuid::Uuid::from_u128(u128::from(*mesh)),
                },
                AssetId::<StandardMaterial>::Uuid {
                    uuid: bevy::asset::uuid::Uuid::from_u128(u128::from(*material)),
                }
                .untyped(),
                *triangles,
            );
        }
        census
    }

    /// Lo que el inventario no sabía hacer y por lo que se escribió esto: decir
    /// de quién es cada triángulo. Un total no contesta "cuánto está poniendo la
    /// pradera", que es la pregunta de cada ajuste del pasto.
    #[test]
    fn the_census_attributes_what_it_counts() {
        let census = census_with(&[
            (Some(Subject::Meadow), 1, 10, 300),
            (Some(Subject::Meadow), 2, 10, 500),
            (Some(Subject::Terrain), 3, 20, 32_768),
            (None, 4, 30, 12),
        ]);
        let tallies = census.tallies();
        assert_eq!(tallies[Subject::Meadow.index()].triangles, 800);
        assert_eq!(tallies[Subject::Meadow.index()].meshes, 2);
        assert_eq!(tallies[Subject::Terrain.index()].triangles, 32_768);
        assert_eq!(tallies[OTHER].triangles, 12);
        assert_eq!(census.accounted_meshes(), 4);
    }

    /// Un draw es un `(malla, material)` distinto, así que dos entidades que
    /// comparten los dos batchean juntas y valen **uno**. Contarlo como
    /// acumulador —que es lo natural cuando se cuenta por sujeto— daría el
    /// número de entidades disfrazado de draws.
    #[test]
    fn sharing_a_mesh_and_a_material_is_one_draw_not_two() {
        let census = census_with(&[
            (Some(Subject::Forest), 1, 10, 100),
            (Some(Subject::Forest), 1, 10, 100),
            (Some(Subject::Forest), 1, 11, 100),
        ]);
        assert_eq!(census.tallies()[Subject::Forest.index()].draws, 2);
        assert_eq!(census.draws(), 2);
        assert_eq!(census.materials(), 2);
    }

    /// Los sujetos reparten el total sin perder ni duplicar: si el reparto no
    /// suma, el número que se mira para decidir está mal en una dirección que
    /// nadie puede ver.
    #[test]
    fn the_buckets_add_up_to_what_was_counted() {
        let census = census_with(&[
            (Some(Subject::Meadow), 1, 10, 300),
            (Some(Subject::Forest), 2, 20, 6_265),
            (Some(Subject::Terrain), 3, 30, 32_768),
            (None, 4, 40, 12),
        ]);
        let tallies = census.tallies();
        let meshes: u32 = tallies.iter().map(|tally| tally.meshes).sum();
        let triangles: usize = tallies.iter().map(|tally| tally.triangles).sum();
        assert_eq!(meshes, census.accounted_meshes());
        assert_eq!(triangles, 300 + 6_265 + 32_768 + 12);
    }

    /// Limpiar tiene que dejarlo como recién nacido, o el recuento de un frame
    /// arrastra el anterior y todo queda el doble.
    #[test]
    fn clearing_leaves_nothing_behind() {
        let mut census = census_with(&[(Some(Subject::Meadow), 1, 10, 300)]);
        census.clear();
        assert_eq!(census.accounted_meshes(), 0);
        assert_eq!(census.draws(), 0);
        assert_eq!(census.materials(), 0);
        assert_eq!(census.tallies(), [SubjectTally::default(); BUCKETS]);
    }
}
