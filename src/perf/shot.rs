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

use super::budget::SceneInventory;
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

/// Cuánto puede alejarse la cámara del mirador antes de que la foto deje de ser
/// de donde el reporte dice. Medio metro es lo que ya se midió que vale ~1 ms en
/// el bosque; para una imagen es todavía menos tolerable.
const POSE_TOLERANCE_M: f32 = 0.5;
/// Y cuánto puede desviarse la mirada: 0,999 son unos 2,5°.
const POSE_TOLERANCE_DOT: f32 = 0.999;

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
#[expect(
    clippy::too_many_arguments,
    reason = "una captura declara de dónde es y con qué se sacó: escena, cámara, \
              ventana, terreno, inventario, perillas y si algún asset falló"
)]
pub fn capture_on_request(
    mut commands: Commands,
    mut requests: MessageReader<crate::input::ScreenshotRequest>,
    inventory: Res<SceneInventory>,
    perf: Res<crate::perf::PerfToggles>,
    camera: Option<Single<(&GlobalTransform, &Projection), With<Camera3d>>>,
    window: Option<Single<&Window, With<bevy::window::PrimaryWindow>>>,
    terrain: crate::world::TerrainAccess,
    broken: Res<BrokenAssets>,
    records: Res<crate::visuals::grass::MeadowRecordMemory>,
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
    let projection = camera.as_ref().map(|camera| camera.1);
    let camera_pose = camera.as_ref().map_or((Vec3::ZERO, Vec3::Z), |camera| {
        (camera.0.translation(), camera.0.forward().as_vec3())
    });
    let (p, f) = camera_pose;
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
    warn_on_broken_assets(&broken);
    log_framing(&inventory, &records);
    let geometry = shot_geometry(
        camera_pose,
        projection,
        window.as_deref().map(|w| &**w),
        &terrain,
    );
    write_legend(&path, &perf, &inventory, camera_pose, &geometry);
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

/// Avisa si la foto se sacó con assets rotos.
///
/// **Encontrado el 2026-08-07, corriendo estas mismas herramientas.** El binario
/// se lanzó desde una ruta donde `assets/` no existe; Bevy no encontró ni un
/// shader, el pasto y el terreno no se dibujaron, y la captura salió siendo
/// cielo liso. Todo lo demás siguió funcionando y reportando: el inventario
/// contó 691.200 triángulos de pradera y el log declaró "95% del cuadro", porque
/// la malla existe, es visible y tiene triángulos — sólo que su material nunca
/// llegó. Noventa segundos de corrida, un reporte entero, y ninguna línea
/// distinguible de una corrida buena.
///
/// Es literalmente la lección que `AHORA.md` ya tenía escrita —*"un mensaje de
/// éxito que no se distingue de un fracaso no es un mensaje de éxito"*— y esta
/// herramienta la incumplía. Un shader que no carga invalida la foto entera, así
/// que la foto tiene que decirlo.
/// Sin tipo a propósito: `UntypedAssetLoadFailedEvent` cubre **todo** lo que el
/// asset server carga, así que este guardia no puede quedarse corto por un tipo
/// nuevo — que es exactamente la falla que el registro de materiales acaba de
/// cerrar del otro lado.
#[derive(Resource, Default)]
pub struct BrokenAssets {
    count: usize,
    first: Option<String>,
}

impl BrokenAssets {
    pub fn count(&self) -> usize {
        self.count
    }
}

pub fn note_failed_assets(
    mut failures: MessageReader<bevy::asset::UntypedAssetLoadFailedEvent>,
    mut broken: ResMut<BrokenAssets>,
) {
    for failure in failures.read() {
        broken.count += 1;
        if broken.first.is_none() {
            broken.first = Some(failure.path.to_string());
        }
    }
}

fn warn_on_broken_assets(broken: &BrokenAssets) {
    if broken.count == 0 {
        return;
    }
    warn!(
        "[shot] ESTA FOTO NO ES EVIDENCIA: {} assets no cargaron (el primero: {}). \
         Lo que no cargó no se dibuja, pero sí se cuenta — su malla existe, es \
         visible y tiene triángulos, así que el inventario de esta corrida \
         describe una escena que la imagen no contiene.",
        broken.count,
        broken.first.as_deref().unwrap_or("?"),
    );
}

/// Escribe, al lado del PNG, qué significa cada color de la foto.
///
/// **El analizador no conoce ningún color.** Los lee de acá, y por eso no puede
/// quedar desincronizado de la corrida que sacó la foto: la paleta y la leyenda
/// salen de `visuals::grass_debug` y de la tabla de anillos, no de una constante
/// repetida en un script. Un color escrito a mano en la herramienta de análisis
/// sería exactamente el bug que este trabajo vino a cerrar, con otro disfraz.
///
/// Se escribe siempre que haya una vista puesta, incluso en las de *ver*: saber
/// qué configuración produjo una imagen es la mitad de poder compararla con
/// otra.
/// Lo que hace falta para convertir una fila de pantalla en una distancia.
///
/// **Existe para el Paso 0 de `BOTWGrass.md`**, que necesita la curva de
/// cobertura contra densidad *a varias distancias*. El perfil por bandas del
/// analizador siempre supo que la fila de arriba está más lejos; lo que no podía
/// decir es *cuánto*, y una curva sin eje x no es una curva. Con el campo de
/// visión, el alto del viewport y la altura del ojo sobre el suelo, la fila `y`
/// sale por trigonometría — y la aritmética es exacta **sólo si el suelo es
/// plano**, así que la corrida no lo supone: muestrea el terreno a lo largo de
/// la línea de vista y deja el perfil escrito para que el analizador lo
/// verifique en vez de creerle.
struct ShotGeometry {
    fov_y: f32,
    viewport: (u32, u32),
    eye_above_ground_m: Option<f32>,
    ground_profile: Vec<(f32, f32)>,
}

/// Hasta dónde se muestrea el suelo, y cada cuánto. Cubre el anillo más lejano
/// (64 m) con holgura.
const GROUND_PROFILE_M: f32 = 80.0;
const GROUND_PROFILE_STEP_M: f32 = 4.0;

fn shot_geometry(
    pose: (Vec3, Vec3),
    projection: Option<&Projection>,
    window: Option<&Window>,
    terrain: &crate::world::TerrainAccess,
) -> ShotGeometry {
    let (position, facing) = pose;
    // La proyección de la corrida, no una constante: el `fov` de esta cámara se
    // mueve al apuntar y al tensar el arco (`camera::rig`), así que escribir 45°
    // sería declarar una geometría que la foto puede no tener.
    let fov_y = match projection {
        Some(Projection::Perspective(perspective)) => perspective.fov,
        _ => f32::NAN,
    };
    let viewport = window.map_or((0, 0), |window| {
        (window.physical_width(), window.physical_height())
    });
    let ground_here = terrain.height_at(position.xz());
    let along = facing.xz().normalize_or_zero();
    let mut ground_profile = Vec::new();
    if along != Vec2::ZERO {
        let mut distance = 0.0;
        while distance <= GROUND_PROFILE_M {
            if let Some(height) = terrain.height_at(position.xz() + along * distance) {
                ground_profile.push((distance, height));
            }
            distance += GROUND_PROFILE_STEP_M;
        }
    }
    ShotGeometry {
        fov_y,
        viewport,
        eye_above_ground_m: ground_here.map(|ground| position.y - ground),
        ground_profile,
    }
}

fn write_legend(
    path: &std::path::Path,
    perf: &crate::perf::PerfToggles,
    inventory: &SceneInventory,
    pose: (Vec3, Vec3),
    geometry: &ShotGeometry,
) {
    use crate::visuals::material_registry::Subject;

    let view = perf.grass_debug_label();
    // **Categorías, no anillos.** El analizador cuenta lo que la corrida declare
    // y no sabe qué es un anillo: la vista `subpixel` reparte por ancho en
    // píxeles, y la que venga después repartirá por otra cosa. Atar el script a
    // la técnica de LOD del momento sería obligarlo a cambiar cada vez que la
    // técnica cambie — y la técnica está justamente en discusión.
    // **Con la vista apagada la leyenda se escribe igual, sin categorías.** Hasta
    // el 2026-08-07 no se escribía, y eso dejaba a la captura del **juego real**
    // —la única que muestra el color que el jugador ve— sin la geometría de
    // cámara, o sea sin eje de distancias. Se descubrió queriendo medir por qué
    // las cartas lejanas se leen de otro color que el pasto cercano, que es una
    // pregunta sobre la imagen jugada y no sobre una vista de diagnóstico. Las
    // categorías sí dependen de la vista; el resto de la leyenda, no.
    let categories: Vec<serde_json::Value> = if view == "off" {
        Vec::new()
    } else if view == "subpixel" {
        crate::visuals::grass_debug::subpixel_legend()
            .into_iter()
            .map(|band| serde_json::json!({ "nombre": band.name, "color": band.color }))
            .collect()
    } else {
        crate::visuals::grass_debug::ring_legend(perf)
            .into_iter()
            .map(|ring| {
                serde_json::json!({
                    "nombre": format!("anillo {}", ring.slot),
                    "color": ring.color,
                    "alcance_m": ring.reach_m,
                    "chunk_m": ring.chunk_m,
                    "densidad_por_m2": ring.density,
                    "tris_por_primitiva": ring.triangles_per_blade,
                    // Un anillo apagado cuenta cero píxeles, que es un número
                    // perfectamente creíble para uno que sí se plantó y no se
                    // ve. La corrida declara cuál es cuál.
                    "plantado": ring.planted,
                })
            })
            .collect()
    };
    let meadow = inventory.subject(Subject::Meadow);
    let flat =
        crate::visuals::grass_debug::GrassDebugView::from_step(perf.grass_debug_step()).is_flat();
    let legend = serde_json::json!({
        "vista": view,
        // Que la vista pinte plano y exacto lo declara la corrida, no una lista
        // en el analizador: quien sabe cuáles lo hacen es el shader.
        "plana": flat,
        // La pose, porque el analizador la necesita para saber si sus propias
        // cuentas valen: el perfil por bandas trata la fila de pantalla como
        // distancia, y eso sólo es cierto con la cámara casi horizontal. Sin
        // este dato el script tendría que suponerlo, que es la clase de error
        // que este trabajo vino a cerrar.
        "camara": {
            "pos": [pose.0.x, pose.0.y, pose.0.z],
            "facing": [pose.1.x, pose.1.y, pose.1.z],
            // Con estos tres la fila de pantalla se convierte en metros; sin
            // ellos el perfil por bandas ordena por distancia pero no la mide.
            "fov_y": geometry.fov_y,
            "viewport_px": [geometry.viewport.0, geometry.viewport.1],
            "ojo_sobre_suelo_m": geometry.eye_above_ground_m,
        },
        // La altura del terreno bajo la línea de vista, cada pocos metros. La
        // conversión fila→distancia supone suelo plano; esto es lo que permite
        // **verificar** la suposición en vez de arrastrarla.
        "perfil_suelo": geometry
            .ground_profile
            .iter()
            .map(|(distance, height)| serde_json::json!([distance, height]))
            .collect::<Vec<_>>(),
        "perillas": {
            "densidad_por_m2": perf.grass_density(),
            "alcance": perf.grass_reach_scale(),
            "anillos": perf.grass_rings_label(),
        },
        "categorias": categories,
        "encuadre": {
            "mallas_visibles": inventory.visible_meshes,
            "triangulos": inventory.triangles,
            "draws": inventory.draws,
            "pradera_triangulos": meadow.triangles,
            "pradera_draws": meadow.draws,
        },
    });
    let path = path.with_extension("json");
    match serde_json::to_string_pretty(&legend) {
        Ok(text) => {
            if let Err(error) = std::fs::write(&path, text) {
                error!("[shot] no se pudo escribir {}: {error}", path.display());
            }
        }
        Err(error) => error!("[shot] no se pudo serializar la leyenda: {error}"),
    }
}

/// Qué hay en cuadro, repartido por sistema.
///
/// Una foto sola no dice si lo que cambió es lo que se estaba mirando. Con el
/// reparto al lado, "se ve distinto" y "hay 40% menos pradera en cuadro" dejan
/// de ser la misma frase.
fn log_framing(inventory: &SceneInventory, records: &crate::visuals::grass::MeadowRecordMemory) {
    let mut parts: Vec<String> = Vec::new();
    for subject in Subject::ALL {
        let tally = inventory.subject(subject);
        if tally.meshes == 0 {
            continue;
        }
        parts.push(format!(
            "{}={} tris/{} draws/{:.1} MB ({:.0}% del cuadro)",
            subject.label(),
            tally.triangles,
            tally.draws,
            tally.vertex_bytes as f64 / 1_048_576.0,
            inventory.triangle_share_of(subject) * 100.0,
        ));
    }
    // Los buffers de registros van aparte porque el inventario cuenta mallas y
    // no `ShaderBuffer`s: desde el Paso 2 la pradera guarda ahí la mayor parte de
    // lo que antes eran vértices, y sin esta línea una corrida declararía una
    // caída de memoria que es en parte mudanza.
    info!(
        "[shot] escena: {} mallas visibles, {} triángulos, {} draws · {} · registros de pradera \
         {:.2} MB en {} chunks",
        inventory.visible_meshes,
        inventory.triangles,
        inventory.draws,
        parts.join(" · "),
        records.bytes as f64 / 1_048_576.0,
        records.chunks,
    );
}

/// Lleva la foto de punta a punta, con la misma máquina de estados que la
/// medición y por la misma razón: el orden de las etapas *es* la lógica.
#[expect(
    clippy::too_many_arguments,
    reason = "una foto declara de dónde es y con qué se sacó: escena, mirador, \
              inventario, perillas y si algún asset falló"
)]
pub fn drive_auto_shot(
    mut commands: Commands,
    mut shot: ResMut<AutoShot>,
    mut next_scene: ResMut<NextState<AppState>>,
    scene: Res<State<AppState>>,
    mut exit: MessageWriter<AppExit>,
    time: Res<Time<Real>>,
    inventory: Res<SceneInventory>,
    perf: Res<crate::perf::PerfToggles>,
    camera: Option<Single<(&GlobalTransform, &Projection), With<Camera3d>>>,
    window: Option<Single<&Window, With<bevy::window::PrimaryWindow>>>,
    terrain: crate::world::TerrainAccess,
    broken: Res<BrokenAssets>,
    records: Res<crate::visuals::grass::MeadowRecordMemory>,
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
            // **Dónde está la cámara, no dónde se le pidió que esté.** La línea
            // de arriba es una intención, y hasta ahora era lo único que el log
            // decía: una foto de un lugar equivocado se reportaba con la pose
            // correcta al lado. Si las dos no coinciden, la foto no es de donde
            // dice ser y todo lo que se concluya de ella es de otra escena.
            if let Some(camera) = &camera {
                let (at, aim) = (camera.0.translation(), camera.0.forward().as_vec3());
                if at.distance(position) > POSE_TOLERANCE_M || aim.dot(facing) < POSE_TOLERANCE_DOT
                {
                    warn!(
                        "[shot] LA CÁMARA NO ESTÁ EN EL MIRADOR: está en \
                         ({:.1},{:.1},{:.1}) mirando ({:.2},{:.2},{:.2}). La foto no es del \
                         lugar que este reporte declara.",
                        at.x, at.y, at.z, aim.x, aim.y, aim.z,
                    );
                }
            } else {
                warn!("[shot] sin cámara: no se puede verificar de dónde es la foto");
            }
            // Lo que la foto no puede mostrar: si dos encuadres discrepan, hace
            // falta saber si cambió lo que se dibuja o sólo cómo se proyecta.
            warn_on_broken_assets(&broken);
            log_framing(&inventory, &records);
            // La geometría sale de la cámara **real**, no del mirador pedido:
            // si las dos discrepan el aviso de arriba ya sonó, y una conversión
            // a metros hecha sobre una pose que la foto no tiene sería un
            // número exacto de otra escena.
            let geometry = shot_geometry(
                camera.as_ref().map_or((position, facing), |camera| {
                    (camera.0.translation(), camera.0.forward().as_vec3())
                }),
                camera.as_ref().map(|camera| camera.1),
                window.as_deref().map(|w| &**w),
                &terrain,
            );
            write_legend(&path, &perf, &inventory, (position, facing), &geometry);
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
