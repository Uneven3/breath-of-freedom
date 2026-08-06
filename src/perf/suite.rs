//! **Qué se mide** — las matrices de medición, como dato (§6).
//!
//! El motor que las corre vive en [`super::sequence`]; acá sólo están las
//! tablas. La separación no es estética: una matriz es una lista de
//! configuraciones y un motor es un reloj con reglas, y mientras vivieron
//! juntos había exactamente **una** matriz, la general, porque agregar otra
//! quería decir tocar el motor.
//!
//! # Por qué hay más de una matriz
//!
//! La general mide el juego entero desde un mirador del bosque, y contesta
//! "¿qué me está costando el frame?". Es la pregunta correcta cuando no sabés
//! dónde estás perdiendo tiempo, y la equivocada cuando ya sabés: el 2026-08-06
//! el pasto necesitaba cuatro pasos que la general no tiene, y meterlos ahí
//! habría alargado a un minuto y medio una corrida que ya dura cuarenta y dos
//! segundos, para gente que quería medir sombras.
//!
//! Entonces una matriz por **contexto**: cada una nombra su pregunta, elige la
//! caja donde esa pregunta se puede contestar, y se para donde hay que pararse.
//! Agregar una es una variante y una tabla — nunca código nuevo en el motor.
//!
//! # Las tres reglas que toda matriz cumple, y que hay tests que las cobran
//!
//! 1. **Empieza y termina en el baseline.** El último paso repite el primero, y
//!    su diferencia es la deriva: cuánto se movió la máquina sola durante la
//!    corrida. Sin eso no hay forma de distinguir una mejora de un ventilador
//!    que arrancó.
//! 2. **Cada paso mueve un eje y sólo uno.** Un paso que cambia dos cosas
//!    produce un delta que nadie puede atribuir.
//! 3. **El baseline es la configuración que se envía**, no una cómoda. Si la
//!    primera fila no es el juego real, todas las diferencias se miden contra
//!    algo que no existe.

use bevy::prelude::*;

use crate::scene::SceneId;

/// Una configuración bajo prueba. Sólo se listan los campos que un paso varía;
/// todo lo demás se queda en el baseline de esa corrida.
#[derive(Clone, Copy)]
pub struct BenchmarkStep {
    pub name: &'static str,
    pub forest_visible: bool,
    pub sun_shadows: bool,
    pub moon_shadows: bool,
    pub cull_step: usize,
    pub shadow_range_step: usize,
    pub shadow_map_step: usize,
    pub leaf_shadows: bool,
    pub grass_density_step: usize,
    pub grass_reach_step: usize,
    pub render_scale_step: usize,
    pub msaa_step: usize,
}

impl BenchmarkStep {
    /// El baseline **no es todo en cero**: es lo que el perfil envía. Los dos
    /// campos que no son neutros (sombras de hoja apagadas, mapa de 1024) están
    /// así en el juego real, y medir contra la configuración cara habría hecho
    /// que el sol pareciera algo que hay que sacar en vez de algo que hay que
    /// presupuestar.
    const fn baseline(name: &'static str) -> Self {
        Self {
            name,
            forest_visible: true,
            sun_shadows: true,
            moon_shadows: true,
            cull_step: 0,
            shadow_range_step: 0,
            shadow_map_step: 1,
            leaf_shadows: false,
            grass_density_step: 0,
            grass_reach_step: 0,
            render_scale_step: 0,
            msaa_step: 0,
        }
    }
}

/// Índices con nombre, porque `grass_density_step: 4` en una tabla no dice nada
/// y `GRASS_OFF` sí. Cada uno apunta a su entrada en las tablas de
/// `bof_domain::perf`, y hay un test que verifica que sigan apuntando a lo que
/// su nombre dice — un paso insertado en el medio los correría en silencio.
mod step {
    /// `GRASS_DENSITY_STEPS`: 80, 30, 12 y 0 briznas/m².
    pub const GRASS_DENSE: usize = 1;
    pub const GRASS_SPARSE: usize = 2;
    pub const GRASS_SPARSEST: usize = 3;
    pub const GRASS_OFF: usize = 4;
    /// `GRASS_REACH_STEPS`: 75% y 50% del alcance de los anillos.
    pub const REACH_75: usize = 1;
    pub const REACH_50: usize = 2;
    /// `MSAA_STEPS`: 4x y 2x.
    pub const MSAA_4X: usize = 1;
    pub const MSAA_2X: usize = 2;
    /// `RENDER_SCALE_STEPS`: 75% y 50% de los píxeles.
    pub const SCALE_75: usize = 1;
    pub const SCALE_50: usize = 2;
    /// `CULL_STEPS`: 70 m.
    pub const CULL_MID: usize = 2;
    /// `SHADOW_CASTER_STEPS`: sin límite, o sea el comportamiento viejo contra
    /// el que se mide el presupuesto.
    pub const SHADOW_BUDGET_OFF: usize = 3;
    /// `SHADOW_MAP_STEPS`: 2048 y 512.
    pub const SHADOW_MAP_2048: usize = 0;
    pub const SHADOW_MAP_512: usize = 2;
}

/// Qué contexto mide una corrida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BenchSuite {
    /// El frame entero, desde el mirador del bosque.
    #[default]
    General,
    /// La pradera, en la caja donde no hay nada más que la tape.
    Grass,
    /// Lo que cuesta el sol, que es el ítem suelto más caro que se ha medido.
    Shadows,
}

impl BenchSuite {
    pub const ALL: [BenchSuite; 3] = [BenchSuite::General, BenchSuite::Grass, BenchSuite::Shadows];

    /// El nombre que la acepta desde `BOF_BENCH` y la nombra en el log.
    pub const fn label(self) -> &'static str {
        match self {
            BenchSuite::General => "general",
            BenchSuite::Grass => "grass",
            BenchSuite::Shadows => "shadows",
        }
    }

    /// La pregunta que esta matriz existe para contestar. Va en el encabezado
    /// del reporte: una tabla de milisegundos sin la pregunta que la motivó es
    /// un número que dentro de un mes nadie va a poder interpretar.
    pub const fn question(self) -> &'static str {
        match self {
            BenchSuite::General => "que me esta costando el frame",
            BenchSuite::Grass => "que parte de la pradera cuesta: densidad, alcance o pixeles",
            BenchSuite::Shadows => "cuanto del frame es el sol, y que palanca lo baja",
        }
    }

    /// Dónde se contesta. Una suite mide en la caja donde su sistema está
    /// aislado: medir el pasto en el Mundo es medirlo con un bosque encima.
    pub const fn scene(self) -> SceneId {
        match self {
            BenchSuite::General | BenchSuite::Shadows => SceneId::World,
            BenchSuite::Grass => SceneId::Grass,
        }
    }

    /// Dónde se para la cámara, y mirando a dónde.
    ///
    /// Una por suite y no una global, porque el mirador del bosque no sirve
    /// para medir pasto: desde ahí la pradera es un cuarto de la pantalla y
    /// cualquier delta suyo queda enterrado. Ver [`VANTAGES`].
    pub const fn vantage(self) -> (Vec3, Vec3) {
        match self {
            BenchSuite::General | BenchSuite::Shadows => (FOREST_VANTAGE, FOREST_FACING),
            BenchSuite::Grass => (MEADOW_VANTAGE, MEADOW_FACING),
        }
    }

    pub const fn steps(self) -> &'static [BenchmarkStep] {
        match self {
            BenchSuite::General => &GENERAL_STEPS,
            BenchSuite::Grass => &GRASS_STEPS,
            BenchSuite::Shadows => &SHADOW_STEPS,
        }
    }

    /// Para `BOF_BENCH`. Case-insensitive porque nadie se acuerda.
    pub fn from_label(raw: &str) -> Option<Self> {
        let raw = raw.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|suite| suite.label() == raw)
    }

    /// Los nombres aceptados, para el mensaje de error de `BOF_BENCH`.
    pub fn labels() -> String {
        Self::ALL
            .iter()
            .map(|suite| suite.label())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// # VANTAGES
///
/// Un punto de vista es parte de la medición, no una comodidad. Pararse a mano
/// era reproducible a medio metro, y medio metro valía ~1 ms en el bosque:
/// suficiente para tapar las diferencias que la corrida iba a resolver.
///
/// El del bosque salió del ritual del propio operador —caminar hasta las
/// escaleras y mirar hacia los árboles— promediado sobre dos corridas puestas a
/// mano.
const FOREST_VANTAGE: Vec3 = Vec3::new(-6.2, 4.7, -3.2);
const FOREST_FACING: Vec3 = Vec3::new(-0.78, -0.02, 0.63);

/// El de la pradera es otra cosa y por eso es otro punto: **altura de ojo y
/// mirada casi horizontal**. Es el caso que el pasto tiene que sobrevivir —
/// mirando al horizonte se ven los tres anillos a la vez, el traspaso entre
/// ellos y el overdraw en el peor ángulo, que es el rasante. Mirando hacia
/// abajo se ve un anillo y la mitad del sistema queda fuera del cuadro.
const MEADOW_VANTAGE: Vec3 = Vec3::new(0.0, 1.6, 0.0);
const MEADOW_FACING: Vec3 = Vec3::new(0.0, -0.08, 1.0);

/// La matriz general: qué le cuesta el frame al juego entero.
const GENERAL_STEPS: [BenchmarkStep; 11] = [
    BenchmarkStep::baseline("baseline"),
    BenchmarkStep {
        moon_shadows: false,
        ..BenchmarkStep::baseline("moon-shadow off")
    },
    BenchmarkStep {
        sun_shadows: false,
        moon_shadows: false,
        ..BenchmarkStep::baseline("all shadows off")
    },
    BenchmarkStep {
        shadow_range_step: step::SHADOW_BUDGET_OFF,
        ..BenchmarkStep::baseline("shadow budget off")
    },
    BenchmarkStep {
        cull_step: step::CULL_MID,
        ..BenchmarkStep::baseline("cull 70m")
    },
    BenchmarkStep {
        forest_visible: false,
        ..BenchmarkStep::baseline("forest hidden")
    },
    BenchmarkStep {
        grass_density_step: step::GRASS_OFF,
        ..BenchmarkStep::baseline("grass off")
    },
    BenchmarkStep {
        grass_density_step: step::GRASS_SPARSE,
        ..BenchmarkStep::baseline("grass 30/m2")
    },
    BenchmarkStep {
        render_scale_step: step::SCALE_75,
        ..BenchmarkStep::baseline("render 75%")
    },
    BenchmarkStep {
        render_scale_step: step::SCALE_50,
        ..BenchmarkStep::baseline("render 50%")
    },
    BenchmarkStep::baseline("baseline repeat"),
];

/// La matriz del pasto. Ordenada para que se lea como un argumento:
///
/// 1. **Cuánto cuesta entero** — `grass off` contra el baseline es una resta, no
///    una extrapolación desde el paso más ralo, que es lo único que se podía
///    hacer antes de que existiera un paso en cero.
/// 2. **Con qué escala** — la escalera de densidad completa. Si el tiempo sigue
///    a la densidad, la palanca es cuántas briznas hay.
/// 3. **Qué mitad** — el alcance contra la densidad. Las dos mueven el conteo de
///    briznas y no son intercambiables: las cercanas son pocas y cubren muchos
///    píxeles cada una, las lejanas son muchas y casi no cubren ninguno.
/// 4. **Píxeles contra geometría** — la escala de render, misma geometría y
///    menos fragmentos. Contra los pasos de densidad decide fill-bound o
///    vertex-bound, que es la pregunta que `BOTWGrass.md` viene arrastrando.
/// 5. **El parpadeo** — MSAA 4x y 2x. Este paso no se juzga por el número: se
///    juzga mirando. El número dice lo que costaría el arreglo.
const GRASS_STEPS: [BenchmarkStep; 11] = [
    BenchmarkStep::baseline("baseline"),
    BenchmarkStep {
        grass_density_step: step::GRASS_OFF,
        ..BenchmarkStep::baseline("grass off")
    },
    BenchmarkStep {
        grass_density_step: step::GRASS_DENSE,
        ..BenchmarkStep::baseline("grass 80/m2")
    },
    BenchmarkStep {
        grass_density_step: step::GRASS_SPARSE,
        ..BenchmarkStep::baseline("grass 30/m2")
    },
    BenchmarkStep {
        grass_density_step: step::GRASS_SPARSEST,
        ..BenchmarkStep::baseline("grass 12/m2")
    },
    BenchmarkStep {
        grass_reach_step: step::REACH_75,
        ..BenchmarkStep::baseline("reach 75%")
    },
    BenchmarkStep {
        grass_reach_step: step::REACH_50,
        ..BenchmarkStep::baseline("reach 50%")
    },
    BenchmarkStep {
        render_scale_step: step::SCALE_50,
        ..BenchmarkStep::baseline("render 50%")
    },
    BenchmarkStep {
        msaa_step: step::MSAA_4X,
        ..BenchmarkStep::baseline("msaa 4x")
    },
    BenchmarkStep {
        msaa_step: step::MSAA_2X,
        ..BenchmarkStep::baseline("msaa 2x")
    },
    BenchmarkStep::baseline("baseline repeat"),
];

/// La matriz de sombras. El sol es el ítem suelto más caro medido hasta hoy
/// —1,96 ms el 2026-08-06— y las palancas que lo bajan son varias y no
/// equivalentes: el tamaño del mapa es fill, el rango de caster es geometría, y
/// las sombras de hoja son alpha-test que además rompe el early-Z.
const SHADOW_STEPS: [BenchmarkStep; 8] = [
    BenchmarkStep::baseline("baseline"),
    BenchmarkStep {
        sun_shadows: false,
        moon_shadows: false,
        ..BenchmarkStep::baseline("all shadows off")
    },
    BenchmarkStep {
        moon_shadows: false,
        ..BenchmarkStep::baseline("moon-shadow off")
    },
    BenchmarkStep {
        shadow_map_step: step::SHADOW_MAP_2048,
        ..BenchmarkStep::baseline("shadow map 2048")
    },
    BenchmarkStep {
        shadow_map_step: step::SHADOW_MAP_512,
        ..BenchmarkStep::baseline("shadow map 512")
    },
    BenchmarkStep {
        shadow_range_step: step::SHADOW_BUDGET_OFF,
        ..BenchmarkStep::baseline("shadow budget off")
    },
    BenchmarkStep {
        leaf_shadows: true,
        ..BenchmarkStep::baseline("leaf shadows on")
    },
    BenchmarkStep::baseline("baseline repeat"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use bof_domain::perf::{
        GRASS_DENSITY_STEPS, GRASS_REACH_STEPS, MSAA_STEPS, RENDER_SCALE_STEPS, SHADOW_MAP_STEPS,
    };

    /// Los índices con nombre son la parte frágil de todo esto: insertar una
    /// entrada en el medio de una tabla de `bof_domain` los corre en silencio y
    /// la matriz sigue compilando midiendo otra cosa. Esto lo vuelve ruidoso.
    #[test]
    fn the_named_indices_still_point_at_what_they_are_named_after() {
        assert_eq!(GRASS_DENSITY_STEPS[step::GRASS_OFF], 0.0);
        assert_eq!(GRASS_DENSITY_STEPS[step::GRASS_DENSE], 80.0);
        assert_eq!(GRASS_DENSITY_STEPS[step::GRASS_SPARSE], 30.0);
        assert_eq!(GRASS_DENSITY_STEPS[step::GRASS_SPARSEST], 12.0);
        assert_eq!(GRASS_REACH_STEPS[step::REACH_75], 0.75);
        assert_eq!(GRASS_REACH_STEPS[step::REACH_50], 0.5);
        assert_eq!(MSAA_STEPS[step::MSAA_4X], 4);
        assert_eq!(MSAA_STEPS[step::MSAA_2X], 2);
        assert_eq!(RENDER_SCALE_STEPS[step::SCALE_75], 0.75);
        assert_eq!(RENDER_SCALE_STEPS[step::SCALE_50], 0.5);
        assert_eq!(SHADOW_MAP_STEPS[step::SHADOW_MAP_2048], 2048);
        assert_eq!(SHADOW_MAP_STEPS[step::SHADOW_MAP_512], 512);
    }

    /// Regla 2: un paso que cambia dos cosas produce un delta que nadie puede
    /// atribuir. Vale para toda suite, presente y futura.
    #[test]
    fn every_step_of_every_suite_varies_exactly_one_axis() {
        for suite in BenchSuite::ALL {
            let steps = suite.steps();
            let base = &steps[0];
            for step in &steps[1..steps.len() - 1] {
                let changed = [
                    step.forest_visible != base.forest_visible,
                    step.cull_step != base.cull_step,
                    step.shadow_range_step != base.shadow_range_step,
                    step.shadow_map_step != base.shadow_map_step,
                    step.leaf_shadows != base.leaf_shadows,
                    step.grass_density_step != base.grass_density_step,
                    step.grass_reach_step != base.grass_reach_step,
                    step.render_scale_step != base.render_scale_step,
                    step.msaa_step != base.msaa_step,
                    // Las sombras cuentan como un eje: "todas las sombras
                    // apagadas" mueve las dos luces a propósito.
                    step.sun_shadows != base.sun_shadows || step.moon_shadows != base.moon_shadows,
                ]
                .iter()
                .filter(|changed| **changed)
                .count();
                assert_eq!(
                    changed,
                    1,
                    "{}/{} varia {changed} ejes",
                    suite.label(),
                    step.name
                );
            }
        }
    }

    /// Regla 1: sin baseline repetido no hay forma de distinguir una mejora de
    /// la máquina calentándose durante la corrida.
    #[test]
    fn every_suite_starts_and_ends_on_its_baseline() {
        for suite in BenchSuite::ALL {
            let steps = suite.steps();
            assert!(steps.len() >= 3, "{} es demasiado corta", suite.label());
            let (first, last) = (&steps[0], &steps[steps.len() - 1]);
            assert_eq!(first.name, "baseline", "{}", suite.label());
            assert_eq!(last.name, "baseline repeat", "{}", suite.label());
            assert_eq!(first.grass_density_step, last.grass_density_step);
            assert_eq!(first.grass_reach_step, last.grass_reach_step);
            assert_eq!(first.render_scale_step, last.render_scale_step);
            assert_eq!(first.msaa_step, last.msaa_step);
            assert_eq!(first.sun_shadows, last.sun_shadows);
            assert_eq!(first.shadow_map_step, last.shadow_map_step);
        }
    }

    /// Regla 3: la primera fila tiene que ser el juego que se envía, o todas
    /// las diferencias se miden contra algo que no existe.
    #[test]
    fn the_baseline_is_the_shipped_configuration() {
        let shipped = crate::perf::PerfToggles::default();
        let base = BenchmarkStep::baseline("baseline");
        assert_eq!(base.grass_density_step, shipped.grass_density_step);
        assert_eq!(base.grass_reach_step, shipped.grass_reach_step);
        assert_eq!(base.render_scale_step, shipped.render_scale_step);
        assert_eq!(base.msaa_step, shipped.msaa_step);
        assert_eq!(base.shadow_map_step, shipped.shadow_map_step);
        assert_eq!(base.leaf_shadows, shipped.leaf_shadows);
        assert_eq!(base.cull_step, shipped.cull_step);
        assert_eq!(base.shadow_range_step, shipped.shadow_range_step);
        assert_eq!(base.forest_visible, shipped.forest_visible);
    }

    /// Un nombre repetido dentro de una suite haría dos filas indistinguibles
    /// en el reporte, salvo el baseline repetido que es a propósito.
    #[test]
    fn step_names_are_unique_within_a_suite() {
        for suite in BenchSuite::ALL {
            let mut names: Vec<&str> = suite.steps().iter().map(|step| step.name).collect();
            names.sort_unstable();
            let before = names.len();
            names.dedup();
            assert_eq!(before, names.len(), "{} repite un nombre", suite.label());
        }
    }

    /// `BOF_BENCH` acepta exactamente los nombres que las suites declaran, y
    /// ninguno más.
    #[test]
    fn every_suite_is_reachable_by_its_own_label() {
        for suite in BenchSuite::ALL {
            assert_eq!(BenchSuite::from_label(suite.label()), Some(suite));
            assert_eq!(
                BenchSuite::from_label(&suite.label().to_uppercase()),
                Some(suite)
            );
        }
        assert_eq!(BenchSuite::from_label("no-existe"), None);
    }

    /// Una suite que mide en una caja que no contiene su sistema mediría el
    /// vacío. El pasto es el caso vivo: su escena tiene que tener pradera.
    #[test]
    fn every_suite_measures_in_a_scene_that_has_what_it_measures() {
        let contents = |id: SceneId| {
            crate::scene::SCENES
                .iter()
                .find(|scene| scene.id == id)
                .expect("every suite names a scene in the table")
                .contents
        };
        assert!(contents(BenchSuite::Grass.scene()).meadow);
        assert!(contents(BenchSuite::Shadows.scene()).forest);
        assert!(contents(BenchSuite::General.scene()).forest);
    }
}
