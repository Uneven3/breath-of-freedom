//! Ver el juego **sin jugarlo**: `BOF_SHOT=grass cargo run` deja un PNG y sale.
//!
//! # Por qué existe
//!
//! `auto.rs` resolvió que yo pudiera *medir* sin el usuario. Quedó afuera la
//! otra mitad, y en un problema visual es la que importa: yo no puedo **ver**.
//! Todo cambio de aspecto viajaba hasta el usuario para su veredicto, así que
//! una idea equivocada costaba una sesión de juego suya en vez de treinta
//! segundos míos. El 2026-08-06 se gastaron cuatro corridas en cuatro intentos
//! que un vistazo habría descartado.
//!
//! Con esto un experimento visual se descarta solo: se cambia una constante, se
//! saca la foto, se mira. Al usuario le llega lo que ya sobrevivió a eso, que es
//! el único uso honesto de su tiempo.
//!
//! # Por qué reusa las suites
//!
//! La escena y el mirador salen de [`BenchSuite`], los mismos que usa la
//! medición. No es ahorro de código: es que **la foto y el número tienen que
//! mirar lo mismo**. Un mirador propio acá volvería incomparables las dos
//! mitades del diagnóstico justo cuando se necesitan juntas — "se ve mejor" y
//! "cuesta 3 ms más" sólo forman una decisión si son del mismo lugar.
//!
//! # Lo que deliberadamente NO hace
//!
//! No juzga. Guarda el frame y sale; qué significa lo decide quien lo mira. Y no
//! corre sin ventana por la misma razón que la medición: no hay frame que sacar
//! de un renderer que no existe.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use super::suite::BenchSuite;
use crate::scene::AppState;
use crate::visuals::material_registry::Subject;

const SHOT_ENV: &str = "BOF_SHOT";
const OUT_ENV: &str = "BOF_SHOT_OUT";
const POSE_ENV: &str = "BOF_SHOT_POSE";

/// Un mirador escrito a mano, `x,y,z:dx,dy,dz`.
///
/// Rompe a propósito la regla de que la foto mira desde donde mide: es para
/// **reproducir una queja**. Cuando el usuario dice "desde arriba se ve ralo", el
/// mirador canónico no sirve para verificarlo, y verificar antes de opinar es
/// justo lo que esta herramienta existe para permitir.
fn configured_pose() -> Option<(Vec3, Vec3)> {
    let raw = std::env::var(POSE_ENV).ok()?;
    let (from, towards) = raw.split_once(':')?;
    let triple = |text: &str| -> Option<Vec3> {
        let mut parts = text.split(',').map(|n| n.trim().parse::<f32>());
        Some(Vec3::new(
            parts.next()?.ok()?,
            parts.next()?.ok()?,
            parts.next()?.ok()?,
        ))
    };
    let facing = triple(towards)?.normalize_or_zero();
    // Una dirección nula dejaría a `look_to` sin base y la cámara en cualquier
    // parte; mejor ignorar la variable que fotografiar un lugar al azar.
    (facing != Vec3::ZERO).then(|| Some((triple(from)?, facing)))?
}

/// Dónde caen las fotos si nadie dice otra cosa.
///
/// Fuera de `assets/` a propósito: son diagnóstico, no material del juego, y no
/// tienen por qué entrar al repositorio ni al pipeline de assets.
const DEFAULT_OUT: &str = "target/shots";

/// Cuánto se espera desde que la escena entra hasta apretar el obturador.
///
/// Más que los dos segundos de la medición, y no por prolijidad: la pradera
/// llena su grilla por anillos y las texturas del terreno entran cuando el
/// asset server las termina. Una foto sacada temprano muestra un suelo con el
/// array de respaldo — que es exactamente el bug que costó meses encontrar el
/// 2026-08-06, ahora en forma de evidencia falsa.
const SETTLE_SECS: f32 = 4.0;

/// Cuántos frames se dejan pasar entre pedir la foto y salir.
///
/// La captura es asíncrona: viaja al mundo de render, espera que la GPU termine
/// y recién ahí el observador escribe el archivo. Salir en cuanto se pide deja
/// un PNG de cero bytes.
const FRAMES_AFTER_SHUTTER: u32 = 12;

#[derive(Default, PartialEq, Eq, Debug, Clone, Copy)]
enum Stage {
    #[default]
    EnteringScene,
    Settling,
    /// Foto pedida; contando frames para que alcance a escribirse.
    Draining(u32),
}

#[derive(Resource)]
pub struct AutoShot {
    suite: BenchSuite,
    out: PathBuf,
    stage: Stage,
    elapsed: f32,
}

impl AutoShot {
    /// El mirador escrito a mano si lo hay, y si no el de la suite.
    fn pose(&self) -> (Vec3, Vec3) {
        configured_pose().unwrap_or_else(|| self.suite.vantage())
    }

    fn path(&self) -> PathBuf {
        self.out.join(format!("{}.png", self.suite.label()))
    }
}

/// Lee `BOF_SHOT`. Igual que `BOF_BENCH`, un nombre inválido **no arranca el
/// juego**: pedir una foto y recibir una sesión jugable es la clase de silencio
/// que hace perder una tarde.
pub fn configured_auto_shot() -> Option<AutoShot> {
    let raw = match std::env::var(SHOT_ENV) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => return None,
        Err(std::env::VarError::NotUnicode(_)) => {
            error!(
                "[shot] {SHOT_ENV} no es texto válido; suites: {}",
                BenchSuite::labels()
            );
            return None;
        }
    };
    let Some(suite) = BenchSuite::from_label(&raw) else {
        error!(
            "[shot] {SHOT_ENV}={raw} no nombra ninguna suite; hay: {}",
            BenchSuite::labels()
        );
        return None;
    };
    let out = std::env::var(OUT_ENV).map_or_else(|_| PathBuf::from(DEFAULT_OUT), PathBuf::from);
    Some(AutoShot {
        suite,
        out,
        stage: Stage::default(),
        elapsed: 0.0,
    })
}

/// El mirador que la foto tiene que sostener.
///
/// Se publica igual que el de una medición para que la cámara siga leyendo un
/// solo seam. Ver [`super::reconcile_scripted_camera_pose`].
pub fn shot_pose(shot: &AutoShot) -> Option<(Vec3, Vec3)> {
    // Antes de entrar a la escena no hay nada que encuadrar, y parquear la
    // cámara mientras la escena anterior sigue viva la saca de su lugar.
    (shot.stage != Stage::EnteringScene).then(|| shot.pose())
}

/// F7 durante el juego: una captura de lo que el usuario está viendo.
///
/// Numeradas y no pisadas, al revés que las de `BOF_SHOT`: una corrida de
/// captura reemplaza su archivo porque compara dos versiones del mismo
/// encuadre, y esto documenta una sesión donde cada disparo es un momento
/// distinto.
pub fn capture_on_request(
    mut commands: Commands,
    mut requests: MessageReader<crate::input::ScreenshotRequest>,
    inventory: Res<super::budget::SceneInventory>,
    camera: Option<Single<&GlobalTransform, With<Camera3d>>>,
    mut taken: Local<u32>,
) {
    if requests.read().count() == 0 {
        return;
    }
    let out = std::env::var(OUT_ENV).map_or_else(|_| PathBuf::from(DEFAULT_OUT), PathBuf::from);
    if let Err(error) = std::fs::create_dir_all(&out) {
        error!("[shot] no se pudo crear {}: {error}", out.display());
        return;
    }
    *taken += 1;
    let path = out.join(format!("captura_{:03}.png", *taken));
    // El mirador va al log porque una queja sobre la cámara sin la pose de la
    // cámara no se puede reproducir — que es exactamente el caso que esta tecla
    // existe para cubrir.
    if let Some(camera) = camera {
        let (p, f) = (camera.translation(), camera.forward().as_vec3());
        info!(
            "[shot] F7 → {} — pos=({:.1},{:.1},{:.1}) facing=({:.2},{:.2},{:.2}) · BOF_SHOT_POSE=\"{:.1},{:.1},{:.1}:{:.2},{:.2},{:.2}\"",
            path.display(),
            p.x,
            p.y,
            p.z,
            f.x,
            f.y,
            f.z,
            p.x,
            p.y,
            p.z,
            f.x,
            f.y,
            f.z,
        );
    }
    log_framing(&inventory);
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

/// Qué hay en cuadro, repartido por sistema.
///
/// Una foto sola no dice si lo que cambió es lo que se estaba mirando. Con el
/// reparto al lado, "se ve distinto" y "hay 40% menos pradera en cuadro" dejan
/// de ser la misma frase.
fn log_framing(inventory: &super::budget::SceneInventory) {
    let mut parts: Vec<String> = Vec::new();
    for subject in Subject::ALL {
        let tally = inventory.subject(subject);
        if tally.meshes == 0 {
            continue;
        }
        parts.push(format!(
            "{}={} tris/{} draws ({:.0}% del cuadro)",
            subject.label(),
            tally.triangles,
            tally.draws,
            inventory.triangle_share_of(subject) * 100.0,
        ));
    }
    info!(
        "[shot] escena: {} mallas visibles, {} triángulos, {} draws · {}",
        inventory.visible_meshes,
        inventory.triangles,
        inventory.draws,
        parts.join(" · "),
    );
}

/// Lleva la foto de punta a punta, con la misma máquina de estados que la
/// medición y por la misma razón: el orden de las etapas *es* la lógica.
pub fn drive_auto_shot(
    mut commands: Commands,
    mut shot: ResMut<AutoShot>,
    mut next_scene: ResMut<NextState<AppState>>,
    scene: Res<State<AppState>>,
    mut exit: MessageWriter<AppExit>,
    time: Res<Time<Real>>,
    inventory: Res<super::budget::SceneInventory>,
) {
    match shot.stage {
        Stage::EnteringScene => {
            let wanted = AppState::Scene(shot.suite.scene());
            if *scene.get() == wanted {
                shot.stage = Stage::Settling;
                shot.elapsed = 0.0;
            } else {
                next_scene.set(wanted);
            }
        }
        Stage::Settling => {
            shot.elapsed += time.delta_secs();
            if shot.elapsed < SETTLE_SECS {
                return;
            }
            let path = shot.path();
            if let Some(parent) = path.parent()
                && let Err(error) = std::fs::create_dir_all(parent)
            {
                error!("[shot] no se pudo crear {}: {error}", parent.display());
                exit.write(AppExit::error());
                return;
            }
            let (position, facing) = shot.pose();
            info!(
                "[shot] {} — escena {:?}, mirador pos=({:.1},{:.1},{:.1}) facing=({:.2},{:.2},{:.2}) → {}",
                shot.suite.label(),
                shot.suite.scene(),
                position.x,
                position.y,
                position.z,
                facing.x,
                facing.y,
                facing.z,
                path.display(),
            );
            // Lo que la foto no puede mostrar: si dos encuadres discrepan, hace
            // falta saber si cambió lo que se dibuja o sólo cómo se proyecta.
            log_framing(&inventory);
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
            shot.stage = Stage::Draining(FRAMES_AFTER_SHUTTER);
        }
        Stage::Draining(left) => {
            if let Some(left) = left.checked_sub(1) {
                shot.stage = Stage::Draining(left);
            } else {
                exit.write(AppExit::Success);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot(suite: BenchSuite, stage: Stage) -> AutoShot {
        AutoShot {
            suite,
            out: PathBuf::from(DEFAULT_OUT),
            stage,
            elapsed: 0.0,
        }
    }

    /// La foto y el número tienen que mirar lo mismo, o dejan de formar una
    /// decisión. Este test es la única cosa que lo sostiene: nada en el tipo
    /// impide que alguien le escriba a la captura un mirador propio.
    #[test]
    fn the_shot_stands_exactly_where_the_measurement_stands() {
        for suite in BenchSuite::ALL {
            let shot = shot(suite, Stage::Settling);
            assert_eq!(
                shot_pose(&shot),
                Some(suite.vantage()),
                "{} fotografía desde otro lado del que mide",
                suite.label(),
            );
            assert_eq!(shot.suite.scene(), suite.scene());
        }
    }

    /// Parquear la cámara antes de que la escena cambie la saca de su lugar en
    /// la escena que todavía está viva.
    #[test]
    fn nothing_is_framed_before_the_scene_is_entered() {
        assert_eq!(
            shot_pose(&shot(BenchSuite::Grass, Stage::EnteringScene)),
            None
        );
    }

    /// Una foto por suite, con el nombre de la suite: dos corridas seguidas se
    /// pisan a propósito, así que comparar "antes y después" es copiar el
    /// archivo, no acordarse de cuál de doce timestamps era cuál.
    #[test]
    fn the_file_is_named_after_the_suite() {
        let shot = shot(BenchSuite::Grass, Stage::Settling);
        assert_eq!(shot.path(), PathBuf::from("target/shots/grass.png"));
    }
}
