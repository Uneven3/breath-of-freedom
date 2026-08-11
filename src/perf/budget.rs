//! Mobile scene-budget guardrails and their transition-only warnings.

use bevy::prelude::*;

use crate::visuals::material_registry::{BUCKETS, Subject, SubjectTally};

pub(crate) const MOBILE_TRIANGLES: usize = 100_000;
pub(crate) const MOBILE_DRAWS: usize = 100;
pub(crate) const MOBILE_MATERIALS: usize = 64;

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SceneInventory {
    pub visible_meshes: u32,
    pub triangles: usize,
    /// Lower-bound estimate of visible `(mesh, material)` batches, not render
    /// world draw calls. Kept as a guardrail; every presentation labels it `~`.
    pub draws: usize,
    pub materials: usize,
    pub ranged_culled: u32,
    pub ranged_total: u32,
    /// De quién es cada cosa. Un total no contesta *"cuánto está poniendo la
    /// pradera ahora mismo"*, que es la pregunta de cada ajuste del pasto — y
    /// tampoco contesta si el mirador de una medición ve el sistema que dice
    /// medir. Ver `visuals::material_registry`.
    pub subjects: [SubjectTally; BUCKETS],
}

impl SceneInventory {
    pub(crate) fn subject(&self, subject: Subject) -> SubjectTally {
        self.subjects[subject.index()]
    }

    /// Qué fracción de las mallas visibles pone un sujeto. Es lo que decide si
    /// un mirador **ve lo que dice medir**: el del bosque no lo veía, y eso se
    /// dedujo semanas después por una resta rara en vez de leerlo acá.
    pub(crate) fn share_of(&self, subject: Subject) -> f32 {
        if self.visible_meshes == 0 {
            return 0.0;
        }
        self.subject(subject).meshes as f32 / self.visible_meshes as f32
    }

    /// Y la misma pregunta en triángulos, que es la que se compara contra el
    /// presupuesto.
    pub(crate) fn triangle_share_of(&self, subject: Subject) -> f32 {
        if self.triangles == 0 {
            return 0.0;
        }
        self.subject(subject).triangles as f32 / self.triangles as f32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SceneBudgetGrade {
    Good,
    Medium,
    Bad,
    Critical,
}

impl SceneBudgetGrade {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Good => "bien",
            Self::Medium => "medio",
            Self::Bad => "malo",
            Self::Critical => "crítico",
        }
    }
}

pub(crate) fn scene_budget_grade(scene: &SceneInventory) -> SceneBudgetGrade {
    let load = (scene.triangles as f64 / MOBILE_TRIANGLES as f64)
        .max(scene.draws as f64 / MOBILE_DRAWS as f64)
        .max(scene.materials as f64 / MOBILE_MATERIALS as f64);
    if load > 1.5 {
        SceneBudgetGrade::Critical
    } else if load > 1.0 {
        SceneBudgetGrade::Bad
    } else if load > 0.7 {
        SceneBudgetGrade::Medium
    } else {
        SceneBudgetGrade::Good
    }
}

#[derive(Resource, Default)]
pub(crate) struct SceneBudgetWarningState(bool);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BudgetTransition {
    None,
    Exceeded(SceneBudgetGrade),
    Recovered(SceneBudgetGrade),
}

fn budget_transition(was_over: bool, grade: SceneBudgetGrade) -> BudgetTransition {
    let is_over = matches!(grade, SceneBudgetGrade::Bad | SceneBudgetGrade::Critical);
    match (was_over, is_over) {
        (false, true) => BudgetTransition::Exceeded(grade),
        (true, false) => BudgetTransition::Recovered(grade),
        _ => BudgetTransition::None,
    }
}

pub(crate) fn warn_scene_budget(
    scene: Res<SceneInventory>,
    mut warning: ResMut<SceneBudgetWarningState>,
) {
    if !scene.is_changed() {
        return;
    }
    let grade = scene_budget_grade(&scene);
    match budget_transition(warning.0, grade) {
        BudgetTransition::Exceeded(grade) => warn!(
            "[budget/mobile] scene {}: tris={}/{} draws~={}/{} mats={}/{} — reduce visible detail, lots, or material variants",
            grade.label(),
            scene.triangles,
            MOBILE_TRIANGLES,
            scene.draws,
            MOBILE_DRAWS,
            scene.materials,
            MOBILE_MATERIALS,
        ),
        BudgetTransition::Recovered(grade) => {
            info!(
                "[budget/mobile] scene recovered to {}: tris={} draws~={} mats={}",
                grade.label(),
                scene.triangles,
                scene.draws,
                scene.materials,
            );
        }
        BudgetTransition::None => {}
    }
    warning.0 = matches!(grade, SceneBudgetGrade::Bad | SceneBudgetGrade::Critical);
}

/// What a scene costs in triangles **before it is ever rendered**, summed from
/// the data that decides it: the terrain grid, the authored assets' LOD0 counts
/// and how many instances of each the scene declares.
///
/// This is deliberately a *static* count, not a measurement. It ignores frustum
/// culling and LOD distance, so it answers "what did we sign up for", which is
/// the question a budget is about — a number that does not move when the camera
/// does, and that a test can therefore hold to.
#[cfg(test)]
pub(crate) mod static_cost {
    use crate::asset_pipeline::authored_assets;

    /// LOD0 triangles of an authored asset, by key. Panics rather than
    /// defaulting to zero: a silently missing asset would make a scene look
    /// free, which is the one failure a budget must never have.
    pub(crate) fn asset_triangles(key: &str) -> usize {
        authored_assets()
            .iter()
            .find(|asset| asset.key == key)
            .unwrap_or_else(|| panic!("no authored asset named {key}"))
            .triangles
            .first()
            .copied()
            .unwrap_or(0) as usize
    }

    /// The ground: two triangles per grid cell, always on screen, in every
    /// scene. At 128 cells that is a third of the whole mobile budget spent
    /// before anything is placed on it — which is why raising `CELLS` is a
    /// budget decision, not a quality knob.
    pub(crate) fn terrain_triangles(points: usize) -> usize {
        let cells = points - 1;
        cells * cells * 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_percent(load: usize) -> SceneInventory {
        SceneInventory {
            triangles: MOBILE_TRIANGLES * load / 100,
            ..default()
        }
    }

    /// The ground every scene pays for, whatever else it declares.
    fn terrain_cost() -> usize {
        static_cost::terrain_triangles(crate::world::Terrain::flat_for_test().points())
    }

    #[test]
    fn the_terrain_alone_leaves_room_for_a_scene_on_top_of_it() {
        // The grid resolution is a performance decision disguised as a quality
        // knob: it is fixed cost, always in frame, in every scene. 128 cells
        // spend a third of the mobile budget before a single prop is placed —
        // raising it has to be a deliberate diff, not a tuning afternoon.
        let terrain = terrain_cost();
        assert!(
            terrain * 2 <= MOBILE_TRIANGLES,
            "the terrain alone is {terrain} triangles of a {MOBILE_TRIANGLES} budget; \
             at over half, no scene can be built on it"
        );
    }

    #[test]
    fn every_scene_fits_the_mobile_triangle_budget() {
        // The guardrail the runtime counter cannot be: it grades what the camera
        // happens to see, so a scene can be over budget and still read "bien"
        // from a corner where most of it is culled. This sums what the scene
        // *declares*, so passing means it fits from anywhere in it.
        //
        // La pradera **ya no entra en esta suma**, y no por indulgencia: desde
        // que es una grilla rodante centrada en la cámara dejó de ser contenido
        // de escena. No escala con el tamaño del mapa ni con lo que la escena
        // contenga — es la misma vecindad en un patio de 25 m que en un mundo
        // de 4 km, así que sumarla al declarado de la escena mide una cosa que
        // no existe. Tiene su propio techo, por vista, en el test de abajo. Con
        // eso el Mundo dejó de estar excedido: la deuda de 6.918 triángulos que
        // este archivo declaraba el 2026-08-04 salía entera de los 56.250 de
        // pradera fija que ya no están.
        let forest =
            crate::world::forest::tree_count() * static_cost::asset_triangles("tree_pine_a");
        for scene in crate::scene::SCENES {
            let mut triangles = terrain_cost();
            if scene.contents.forest {
                triangles += forest;
            }
            assert!(
                triangles <= MOBILE_TRIANGLES,
                "scene {} declares {triangles} triangles, over its {MOBILE_TRIANGLES} ceiling",
                scene.label
            );
        }
    }

    /// Lo que la pradera puede costar **por vista** — la única unidad con sentido
    /// para algo que existe alrededor de la cámara y no en un lugar del mapa.
    ///
    /// **No es el presupuesto móvil, y desde el 2026-08-07 tampoco pretende
    /// serlo:** el target dejó de ser un veto previo (`NORTE.md`). Es un techo
    /// de cordura, para que la pradera no crezca sin que nadie lo note, y se
    /// baja cuando el feeling esté logrado y toque adaptar.
    ///
    /// Es el **peor caso barrido** sobre todas las alineaciones de la cámara
    /// contra la grilla, no una cómoda — una versión anterior declaraba el origen
    /// y lo llamaba el peor caso, y no lo era.
    ///
    /// Y el número que de verdad manda hoy no es éste: la pradera es
    /// *fill-bound*, y lo que se paga son los píxeles. El conteo es guardrail.
    /// **Subido de 2 a 4 millones el 2026-08-08**, y de **4 a 5 millones el
    /// 2026-08-09 al vestir el terreno hasta los 128 m** (era 64 m, el borde
    /// donde el pasto se cortaba y dejaba ver tierra pelada) — misma deuda
    /// declarada: *"olvidémonos del techo por ahora, optimizamos cuando
    /// logremos el feeling correcto"*. Optimizar es lo que sigue.
    const MEADOW_VIEW_TRIANGLES: usize = 5_000_000;

    #[test]
    fn the_meadow_neighbourhood_fits_its_own_per_view_budget() {
        let meadow = crate::visuals::grass::meadow_triangles();
        assert!(
            meadow <= MEADOW_VIEW_TRIANGLES,
            "the meadow neighbourhood declares {meadow} triangles, over its \
             {MEADOW_VIEW_TRIANGLES} per-view ceiling"
        );
    }

    #[test]
    fn an_authored_asset_costs_what_the_build_counted() {
        // Guards the seam rather than a number: if `build.rs` ever stops filling
        // `triangles`, every budget test above would pass by measuring zero.
        let pine = static_cost::asset_triangles("tree_pine_a");
        assert!(
            pine > 0,
            "the manifest reports no triangles for tree_pine_a"
        );
        assert!(
            pine <= crate::asset_pipeline::schema::lod0_triangle_budget("tree") as usize,
            "tree_pine_a is over its category budget at {pine}"
        );
    }

    #[test]
    fn grades_have_stable_and_ordered_boundaries() {
        assert_eq!(scene_budget_grade(&at_percent(70)), SceneBudgetGrade::Good);
        assert_eq!(
            scene_budget_grade(&at_percent(71)),
            SceneBudgetGrade::Medium
        );
        assert_eq!(
            scene_budget_grade(&at_percent(100)),
            SceneBudgetGrade::Medium
        );
        assert_eq!(scene_budget_grade(&at_percent(101)), SceneBudgetGrade::Bad);
        assert_eq!(
            scene_budget_grade(&at_percent(151)),
            SceneBudgetGrade::Critical
        );
    }

    #[test]
    fn worst_axis_sets_the_whole_scene_grade() {
        let scene = SceneInventory {
            triangles: 1,
            draws: 1,
            materials: MOBILE_MATERIALS + 1,
            ..default()
        };
        assert_eq!(scene_budget_grade(&scene), SceneBudgetGrade::Bad);
    }

    #[test]
    fn bad_and_critical_are_one_over_budget_episode() {
        let grades = [
            SceneBudgetGrade::Good,
            SceneBudgetGrade::Bad,
            SceneBudgetGrade::Critical,
            SceneBudgetGrade::Bad,
            SceneBudgetGrade::Medium,
        ];
        let mut was_over = false;
        let transitions: Vec<BudgetTransition> = grades
            .into_iter()
            .map(|grade| {
                let transition = budget_transition(was_over, grade);
                was_over = matches!(grade, SceneBudgetGrade::Bad | SceneBudgetGrade::Critical);
                transition
            })
            .filter(|transition| *transition != BudgetTransition::None)
            .collect();

        assert_eq!(
            transitions,
            vec![
                BudgetTransition::Exceeded(SceneBudgetGrade::Bad),
                BudgetTransition::Recovered(SceneBudgetGrade::Medium),
            ]
        );
    }
}
