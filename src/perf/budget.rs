//! Mobile scene-budget guardrails and their transition-only warnings.

use bevy::prelude::*;

pub(crate) const MOBILE_TRIANGLES: usize = 100_000;
pub(crate) const MOBILE_DRAWS: usize = 100;
pub(crate) const MOBILE_MATERIALS: usize = 64;

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SceneInventory {
    pub visible_meshes: u32,
    pub triangles: usize,
    pub draws: usize,
    pub materials: usize,
    pub ranged_culled: u32,
    pub ranged_total: u32,
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
            "[budget/mobile] scene {}: tris={}/{} draws={}/{} mats={}/{} — reduce visible detail, lots, or material variants",
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
                "[budget/mobile] scene recovered to {}: tris={} draws={} mats={}",
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

    /// Lo que el Mundo se pasa hoy, medido el 2026-08-04: 106.918 contra
    /// 100.000. **No es una tolerancia, es una deuda con número.** Sale casi
    /// entera de la pradera (56.250 tris, 52% del total, sobre 625 m² del mundo
    /// de 320×320) y se paga con LOD o densidad por distancia, no bajando el
    /// listón. Mientras exista, el test la deja pasar sólo a esta escena y
    /// falla si crece — que es lo que el hueco anterior no podía hacer, porque
    /// la pradera no la sumaba nadie.
    const WORLD_SCENE_OVERSHOOT: usize = 6_918;

    #[test]
    fn every_scene_fits_the_mobile_triangle_budget() {
        // The guardrail the runtime counter cannot be: it grades what the camera
        // happens to see, so a scene can be over budget and still read "bien"
        // from a corner where most of it is culled. This sums what the scene
        // *declares*, so passing means it fits from anywhere in it.
        let forest =
            crate::world::forest::tree_count() * static_cost::asset_triangles("tree_pine_a");
        let meadow = crate::visuals::grass::meadow_triangles();
        for scene in crate::scene::SCENES {
            let mut triangles = terrain_cost();
            if scene.contents.forest {
                triangles += forest;
            }
            if scene.contents.meadow {
                triangles += meadow;
            }
            let ceiling = match scene.id {
                crate::scene::SceneId::World => MOBILE_TRIANGLES + WORLD_SCENE_OVERSHOOT,
                _ => MOBILE_TRIANGLES,
            };
            assert!(
                triangles <= ceiling,
                "scene {} declares {triangles} triangles, over its {ceiling} ceiling",
                scene.label
            );
        }
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
