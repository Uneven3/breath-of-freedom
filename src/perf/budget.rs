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

#[cfg(test)]
mod tests {
    use super::*;

    fn at_load(load: f64) -> SceneInventory {
        SceneInventory {
            triangles: (MOBILE_TRIANGLES as f64 * load) as usize,
            ..default()
        }
    }

    #[test]
    fn grades_have_stable_and_ordered_boundaries() {
        assert_eq!(scene_budget_grade(&at_load(0.7)), SceneBudgetGrade::Good);
        assert_eq!(scene_budget_grade(&at_load(0.71)), SceneBudgetGrade::Medium);
        assert_eq!(scene_budget_grade(&at_load(1.0)), SceneBudgetGrade::Medium);
        assert_eq!(scene_budget_grade(&at_load(1.01)), SceneBudgetGrade::Bad);
        assert_eq!(
            scene_budget_grade(&at_load(1.51)),
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
