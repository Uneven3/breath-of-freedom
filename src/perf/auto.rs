//! Correr una suite **sin tocar el juego**: `BOF_BENCH=grass cargo run`.
//!
//! Medir era un ritual de seis pasos que terminaba en no moverse durante
//! cuarenta segundos, con tres consecuencias que se pagaron: nadie mide lo que
//! no piensa medir, dos corridas no se comparan, y **yo no puedo jugar**, así
//! que cada número dependía de que el usuario dejara lo suyo.
//!
//! No corre sin ventana: la medición es de GPU y una corrida headless mediría un
//! renderer que no existe. Lo que se automatiza es el ritual, no el hardware. Y
//! no toca ninguna perilla por su cuenta — qué se mide lo decide la suite.

use bevy::prelude::*;

use super::sequence::{Benchmark, BenchmarkRequest, VantageMode};
use super::suite::BenchSuite;
use crate::scene::AppState;

const BENCH_ENV: &str = "BOF_BENCH";

/// Cuánto se espera desde que la escena entra hasta pedir la corrida.
///
/// La escena tarda en asentarse: el terreno se hornea, la pradera llena su
/// grilla, los assets terminan de cargar y el player cae al suelo. Pedir la
/// corrida en el primer frame mediría esa carga. Dos segundos alcanzan porque
/// el precalentamiento de la propia secuencia hace el resto.
/// En qué punto del ritual está la corrida automática.
#[derive(Default, PartialEq, Eq, Debug, Clone, Copy)]
enum Stage {
    /// Falta entrar a la escena de la suite.
    #[default]
    EnteringScene,
    /// En la escena, esperando que se asiente.
    Settling,
    /// Pedida; esperando que el runner la termine.
    Running,
}

#[derive(Resource)]
pub struct AutoBench {
    suite: BenchSuite,
    stage: Stage,
    elapsed: f32,
}

/// Lee `BOF_BENCH`. Un nombre que no existe **aborta el arranque con un error
/// nombrando las suites válidas**, en vez de arrancar el juego normal: pedir una
/// medición y recibir un juego es la clase de silencio que hace perder una
/// tarde.
pub(super) fn configured_auto_bench() -> Result<Option<AutoBench>, &'static str> {
    let raw = match std::env::var(BENCH_ENV) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            error!(
                "[bench] {BENCH_ENV} no es texto válido; suites: {}",
                BenchSuite::labels()
            );
            return Err("BOF_BENCH is not valid Unicode");
        }
    };
    parse_auto_bench(&raw).map(Some)
}

fn parse_auto_bench(raw: &str) -> Result<AutoBench, &'static str> {
    match BenchSuite::from_label(raw) {
        Some(suite) => Ok(AutoBench {
            suite,
            stage: Stage::default(),
            elapsed: 0.0,
        }),
        None => {
            error!(
                "[bench] {BENCH_ENV}={raw} no nombra ninguna suite; hay: {}",
                BenchSuite::labels()
            );
            Err("BOF_BENCH does not name a suite")
        }
    }
}

fn completion_exit(finished: &super::sequence::FinishedRun) -> AppExit {
    if finished.aborted.is_some() || finished.valid != finished.total {
        AppExit::error()
    } else {
        AppExit::Success
    }
}

/// Lleva la corrida automática de punta a punta.
///
/// Un solo sistema con una máquina de tres estados en vez de tres sistemas con
/// condiciones de corrida entre ellos: el orden de las etapas *es* la lógica, y
/// partirlo lo escondería.
pub fn drive_auto_bench(
    mut auto: ResMut<AutoBench>,
    mut next_scene: ResMut<NextState<AppState>>,
    scene: Res<State<AppState>>,
    mut requests: MessageWriter<BenchmarkRequest>,
    benchmark: Res<Benchmark>,
    mut exit: MessageWriter<AppExit>,
    time: Res<Time<Real>>,
) {
    match auto.stage {
        Stage::EnteringScene => {
            let wanted = AppState::Scene(auto.suite.scene());
            if *scene.get() == wanted {
                auto.stage = Stage::Settling;
                auto.elapsed = 0.0;
            } else {
                next_scene.set(wanted);
            }
        }
        Stage::Settling => {
            auto.elapsed += time.delta_secs();
            if auto.elapsed >= super::sequence::SCENE_SETTLE_SECS {
                requests.write(BenchmarkRequest::new(auto.suite, VantageMode::Canonical));
                auto.stage = Stage::Running;
            }
        }
        Stage::Running => {
            // `finished` sólo aparece cuando la corrida terminó o se abortó, y
            // el reporte ya se escribió para entonces — así que salir acá no
            // corta el log.
            if let Some(finished) = benchmark.finished.as_ref() {
                match finished.aborted {
                    Some(reason) => error!("[bench] corrida automática abortada: {reason}"),
                    None => info!(
                        "[bench] corrida automática terminada: {}/{} pasos válidos",
                        finished.valid, finished.total
                    ),
                }
                exit.write(completion_exit(finished));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct ObservedExit(Option<AppExit>);

    fn observe_exit(mut exits: MessageReader<AppExit>, mut observed: ResMut<ObservedExit>) {
        observed.0 = exits.read().last().cloned();
    }

    fn run_finished_automation(finished: super::super::sequence::FinishedRun) -> AppExit {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<AppState>()
            .init_resource::<ObservedExit>()
            .init_resource::<Time<Real>>()
            .add_message::<BenchmarkRequest>()
            .insert_resource(AutoBench {
                suite: BenchSuite::Grass,
                stage: Stage::Running,
                elapsed: 0.0,
            })
            .insert_resource(Benchmark {
                pending: None,
                run: None,
                finished: Some(finished),
            })
            .add_systems(Update, (drive_auto_bench, observe_exit).chain());

        app.update();

        app.world()
            .resource::<ObservedExit>()
            .0
            .clone()
            .expect("the automatic runner must exit")
    }

    /// El contrato de `BOF_BENCH`: los nombres que acepta son exactamente los
    /// que las suites declaran, y cualquier otra cosa no arranca una corrida.
    #[test]
    fn only_declared_suites_start_a_run() {
        for suite in BenchSuite::ALL {
            assert!(parse_auto_bench(suite.label()).is_ok());
        }
        for wrong in ["", " ", "grasss", "pasto", "todo"] {
            assert!(parse_auto_bench(wrong).is_err(), "{wrong} fue aceptado");
        }
    }

    /// Espacios y mayúsculas al escribir la variable no deberían costar una
    /// corrida perdida.
    #[test]
    fn the_label_survives_the_usual_typing() {
        assert_eq!(
            parse_auto_bench("  GRASS  ").map(|auto| auto.suite),
            Ok(BenchSuite::Grass)
        );
    }

    #[test]
    fn aborted_or_incomplete_automation_returns_an_error_exit() {
        let aborted = super::super::sequence::FinishedRun {
            at: 0.0,
            valid: 0,
            total: 2,
            aborted: Some("camera missing"),
        };
        let incomplete = super::super::sequence::FinishedRun {
            at: 0.0,
            valid: 1,
            total: 2,
            aborted: None,
        };
        let complete = super::super::sequence::FinishedRun {
            at: 0.0,
            valid: 2,
            total: 2,
            aborted: None,
        };

        assert!(run_finished_automation(aborted).is_error());
        assert!(run_finished_automation(incomplete).is_error());
        assert!(run_finished_automation(complete).is_success());
    }
}
