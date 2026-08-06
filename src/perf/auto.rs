//! Correr una suite **sin tocar el juego**: `BOF_BENCH=grass cargo run`.
//!
//! # Por qué existe
//!
//! Hasta hoy, medir era un ritual de seis pasos: lanzar, elegir la escena,
//! caminar hasta el lugar, abrir F1, encontrar el botón, y después **no
//! moverse** durante cuarenta segundos — porque el runner invalida cualquier
//! paso en el que la cámara se corra más de 75 cm. Eso tiene tres consecuencias
//! que se pagaron de verdad:
//!
//! - **Nadie mide lo que no piensa medir.** El barrido del pasto existía desde
//!   el 2026-08-04 y se corrió por primera vez el 2026-08-06.
//! - **Las corridas no se comparan.** Media hora de juego antes cambia la
//!   temperatura del chip, y el mirador a mano era reproducible a medio metro,
//!   que en el bosque valía ~1 ms.
//! - **Yo no puedo medir.** Un agente no puede jugar, así que cada número
//!   dependía de que el usuario dejara lo que estaba haciendo.
//!
//! Con esto, una corrida es un comando: arranca en la escena de la suite, se
//! para en su mirador, mide, escribe la tabla y **cierra el proceso**. Se puede
//! poner en un script, correr dos veces y comparar, o dejarla mientras uno hace
//! otra cosa.
//!
//! # Lo que deliberadamente NO hace
//!
//! No corre sin ventana. La medición es de GPU y una corrida headless mediría
//! un renderer que no existe; lo que se automatiza es el *ritual*, no el
//! hardware. Y no toca ninguna perilla por su cuenta: el que decide qué se mide
//! sigue siendo la suite.

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
const SETTLE_SECS: f32 = 2.0;

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
pub fn configured_auto_bench() -> Option<AutoBench> {
    let raw = match std::env::var(BENCH_ENV) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => return None,
        Err(std::env::VarError::NotUnicode(_)) => {
            error!(
                "[bench] {BENCH_ENV} no es texto válido; suites: {}",
                BenchSuite::labels()
            );
            return None;
        }
    };
    match BenchSuite::from_label(&raw) {
        Some(suite) => Some(AutoBench {
            suite,
            stage: Stage::default(),
            elapsed: 0.0,
        }),
        None => {
            error!(
                "[bench] {BENCH_ENV}={raw} no nombra ninguna suite; hay: {}",
                BenchSuite::labels()
            );
            None
        }
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
            if auto.elapsed >= SETTLE_SECS {
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
                exit.write(AppExit::Success);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El contrato de `BOF_BENCH`: los nombres que acepta son exactamente los
    /// que las suites declaran, y cualquier otra cosa no arranca una corrida.
    #[test]
    fn only_declared_suites_start_a_run() {
        for suite in BenchSuite::ALL {
            assert_eq!(BenchSuite::from_label(suite.label()), Some(suite));
        }
        for wrong in ["", " ", "grasss", "pasto", "todo"] {
            assert_eq!(BenchSuite::from_label(wrong), None, "{wrong} fue aceptado");
        }
    }

    /// Espacios y mayúsculas al escribir la variable no deberían costar una
    /// corrida perdida.
    #[test]
    fn the_label_survives_the_usual_typing() {
        assert_eq!(BenchSuite::from_label("  GRASS  "), Some(BenchSuite::Grass));
    }
}
