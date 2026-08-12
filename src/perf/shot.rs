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
use serde::Serialize;

use super::budget::SceneInventory;
use super::shot_stats::{self, Category, ShotGeometry, shot_geometry};
use super::suite::BenchSuite;
use crate::perf::PerfKnob;
use crate::scene::AppState;
use crate::visuals::material_registry::Subject;

const SHOT_ENV: &str = "BOF_SHOT";
const OUT_ENV: &str = "BOF_SHOT_OUT";
const POSE_ENV: &str = "BOF_SHOT_POSE";
const SWEEP_ENV: &str = "BOF_SHOT_SWEEP";

/// Un mirador escrito a mano, `x,y,z:dx,dy,dz`.
///
/// Rompe a propósito la regla de que la foto mira desde donde mide: es para
/// **reproducir una queja**. Cuando el usuario dice "desde arriba se ve ralo", el
/// mirador canónico no sirve para verificarlo, y verificar antes de opinar es
/// justo lo que esta herramienta existe para permitir.
fn configured_pose() -> Result<Option<(Vec3, Vec3)>, &'static str> {
    let raw = match std::env::var(POSE_ENV) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            error!("[shot] {POSE_ENV} no es texto válido");
            return Err("BOF_SHOT_POSE is not valid Unicode");
        }
    };
    match parse_pose(&raw) {
        Ok(pose) => Ok(Some(pose)),
        Err(reason) => {
            error!("[shot] {POSE_ENV}={raw:?} no es x,y,z:dx,dy,dz válido: {reason}");
            Err(reason)
        }
    }
}

fn parse_pose(raw: &str) -> Result<(Vec3, Vec3), &'static str> {
    let (from, towards) = raw
        .split_once(':')
        .ok_or("BOF_SHOT_POSE needs one ':' separator")?;
    let triple = |text: &str| -> Result<Vec3, &'static str> {
        let mut parts = text.split(',').map(|n| n.trim().parse::<f32>());
        let vector = Vec3::new(
            parts
                .next()
                .ok_or("a vector needs three numbers")?
                .map_err(|_| "a coordinate is not a number")?,
            parts
                .next()
                .ok_or("a vector needs three numbers")?
                .map_err(|_| "a coordinate is not a number")?,
            parts
                .next()
                .ok_or("a vector needs three numbers")?
                .map_err(|_| "a coordinate is not a number")?,
        );
        if parts.next().is_some() {
            return Err("a vector has more than three numbers");
        }
        if !vector.is_finite() {
            return Err("coordinates must be finite");
        }
        Ok(vector)
    };
    let position = triple(from)?;
    let facing = triple(towards)?.normalize_or_zero();
    if facing == Vec3::ZERO {
        return Err("facing direction cannot be zero");
    }
    Ok((position, facing))
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

/// Hard stop for a renderer that never returns the asynchronous capture.
const CAPTURE_TIMEOUT_SECS: f32 = 30.0;

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
    /// Foto pedida; esperando la confirmación del observer que recibió sus
    /// píxeles. El número es el contador que esa captura tiene que alcanzar.
    AwaitingCapture(u32),
}

/// La perilla que la corrida recorre entera, un disparo por paso.
///
/// **Una foto sola no es una curva.** El Paso 1 de `BOTWGrass.md` pide cobertura
/// contra densidad *a varias distancias*, y sacarla a mano eran diez corridas
/// que había que acordarse de nombrar igual. Acá es una: la corrida mueve la
/// perilla, deja que la pradera se replante, dispara, y al final imprime la
/// tabla con una fila por paso.
#[derive(Clone, Copy)]
struct Sweep {
    knob: PerfKnob,
    step: usize,
}

/// `BOF_SHOT_SWEEP=grass-density`. Un nombre que no existe **no barre nada** y
/// lo dice: una corrida que se cree un barrido y saca una foto sola es un
/// reporte de una tabla de una fila, que se lee como si fuera la respuesta.
fn configured_sweep() -> Result<Option<Sweep>, &'static str> {
    let raw = match std::env::var(SWEEP_ENV) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            error!("[shot] {SWEEP_ENV} no es texto válido");
            return Err("BOF_SHOT_SWEEP is not valid Unicode");
        }
    };
    parse_sweep(&raw).map(Some)
}

fn parse_sweep(raw: &str) -> Result<Sweep, &'static str> {
    let name = raw.trim();
    match PerfKnob::ALL.into_iter().find(|knob| knob.label() == name) {
        Some(knob) => Ok(Sweep { knob, step: 0 }),
        None => {
            error!(
                "[shot] {SWEEP_ENV}={name} no nombra ninguna perilla; hay: {}",
                PerfKnob::ALL
                    .iter()
                    .map(|knob| knob.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            Err("BOF_SHOT_SWEEP does not name a knob")
        }
    }
}

#[derive(Resource)]
pub struct AutoShot {
    suite: BenchSuite,
    out: PathBuf,
    stage: Stage,
    elapsed: f32,
    sweep: Option<Sweep>,
    pose_override: Option<(Vec3, Vec3)>,
    invalid: bool,
}

/// Number of automatic screenshot entities whose asynchronous result reached
/// the main world. Incremented only by observers attached by `AutoShot`, so an
/// F7 capture cannot accidentally advance a sweep.
#[derive(Resource, Default)]
pub(super) struct ShotCaptureProgress(u32);

fn save_auto_shot(
    path: PathBuf,
) -> impl FnMut(
    On<bevy::render::view::screenshot::ScreenshotCaptured>,
    ResMut<ShotCaptureProgress>,
    ResMut<AutoShot>,
) {
    move |captured, mut progress, mut shot| {
        let result = captured
            .image
            .clone()
            .try_into_dynamic()
            .map_err(|error| format!("formato de pantalla incomprensible: {error}"))
            .and_then(|image| {
                image
                    .to_rgb8()
                    .save(&path)
                    .map_err(|error| format!("error de escritura: {error}"))
            });
        match result {
            Ok(()) => info!("[shot] captura guardada en {}", path.display()),
            Err(error) => {
                error!("[shot] no se pudo guardar {}: {error}", path.display());
                shot.invalid = true;
            }
        }
        progress.0 += 1;
    }
}

impl AutoShot {
    /// El mirador escrito a mano si lo hay, y si no el de la suite.
    fn pose(&self) -> (Vec3, Vec3) {
        self.pose_override.unwrap_or_else(|| self.suite.vantage())
    }

    /// Una foto por suite, o una por paso cuando la corrida barre: dos pasos que
    /// pisaran el mismo archivo dejarían la tabla sin sus imágenes.
    fn path(&self) -> PathBuf {
        match self.sweep {
            Some(sweep) => self.out.join(format!(
                "{}_{}_{}.png",
                self.suite.label(),
                sweep.knob.label(),
                sweep.step
            )),
            None => self.out.join(format!("{}.png", self.suite.label())),
        }
    }
}

/// Lee `BOF_SHOT`. Igual que `BOF_BENCH`, un nombre inválido **no arranca el
/// juego**: pedir una foto y recibir una sesión jugable es la clase de silencio
/// que hace perder una tarde.
pub(super) fn configured_auto_shot() -> Result<Option<AutoShot>, &'static str> {
    let raw = match std::env::var(SHOT_ENV) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            error!(
                "[shot] {SHOT_ENV} no es texto válido; suites: {}",
                BenchSuite::labels()
            );
            return Err("BOF_SHOT is not valid Unicode");
        }
    };
    let Some(suite) = BenchSuite::from_label(&raw) else {
        error!(
            "[shot] {SHOT_ENV}={raw} no nombra ninguna suite; hay: {}",
            BenchSuite::labels()
        );
        return Err("BOF_SHOT does not name a suite");
    };
    let out = std::env::var(OUT_ENV).map_or_else(|_| PathBuf::from(DEFAULT_OUT), PathBuf::from);
    Ok(Some(AutoShot {
        suite,
        out,
        stage: Stage::default(),
        elapsed: 0.0,
        sweep: configured_sweep()?,
        pose_override: configured_pose()?,
        invalid: false,
    }))
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
    settings: Res<crate::visuals::grass::GrassRendererSettings>,
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
    write_legend(&path, &perf, &settings, &inventory, camera_pose, &geometry);
    commands
        .spawn(Screenshot::primary_window())
        .observe(shot_stats::count_when_captured(stats_plan(
            &perf, &settings, geometry, None,
        )))
        .observe(save_to_disk(path));
}

/// Lo que el conteo necesita saber, tomado en el momento del disparo.
///
/// Viaja al observador por valor y no por consulta: cuando la captura vuelve de
/// la GPU ya pasaron frames, y las perillas o la cámara pueden haberse movido.
/// Contar con la configuración de *después* sería describir otra foto.
fn stats_plan(
    perf: &crate::perf::PerfToggles,
    settings: &crate::visuals::grass::GrassRendererSettings,
    geometry: ShotGeometry,
    sweep_label: Option<String>,
) -> shot_stats::StatsPlan {
    shot_stats::StatsPlan {
        categories: shot_categories(perf, settings),
        view: perf.grass_debug_label().to_string(),
        flat: crate::visuals::grass_debug::GrassDebugView::from_step(perf.grass_debug_step())
            .is_flat(),
        geometry,
        sweep_label,
        live_density: {
            // Congelado igual que el resto del plan: cuando la captura vuelve de
            // la GPU, las perillas pueden haberse movido.
            let (dial, reach) = (perf.grass_density(), perf.grass_reach_scale());
            let settings = *settings;
            Box::new(move |distance| {
                crate::visuals::grass::live_blades_per_m2(distance, dial, reach, &settings)
            })
        },
    }
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

/// Qué cuenta el analizador en esta corrida: un nombre y el color plano que lo
/// identifica.
///
/// **Categorías, no anillos.** Lo que se cuenta lo declara la vista puesta: la
/// vista `subpixel` reparte por ancho en píxeles, y la que venga después
/// repartirá por otra cosa. Atar el conteo a la técnica de LOD del momento lo
/// obligaría a cambiar cada vez que la técnica cambie — y la técnica está
/// justamente en discusión.
///
/// Con la vista apagada no hay nada plano que contar, y la lista vacía es lo que
/// hace que el informe se limite a describir la imagen.
fn shot_categories(
    perf: &crate::perf::PerfToggles,
    settings: &crate::visuals::grass::GrassRendererSettings,
) -> Vec<Category> {
    match perf.grass_debug_label() {
        "off" => Vec::new(),
        "subpixel" => crate::visuals::grass_debug::subpixel_legend()
            .into_iter()
            .map(|band| Category {
                name: band.name,
                color: band.color,
            })
            .collect(),
        "medir-forma" => crate::visuals::grass_debug::shape_measure_legend()
            .into_iter()
            .map(|shape| Category {
                name: shape.name,
                color: shape.color,
            })
            .collect(),
        _ => crate::visuals::grass_debug::ring_legend(perf, settings)
            .into_iter()
            .map(|ring| Category {
                name: format!("anillo {}", ring.slot),
                color: ring.color,
            })
            .collect(),
    }
}

#[derive(Serialize)]
struct Legend<'a> {
    vista: &'a str,
    plana: bool,
    camara: LegendCamera,
    perfil_suelo: &'a [(f32, f32)],
    perillas: LegendKnobs<'a>,
    categorias: Vec<LegendCategory<'a>>,
    encuadre: LegendFraming,
}

#[derive(Serialize)]
struct LegendCamera {
    pos: [f32; 3],
    facing: [f32; 3],
    fov_y: f32,
    viewport_px: [u32; 2],
    ojo_sobre_suelo_m: Option<f32>,
}

#[derive(Serialize)]
struct LegendKnobs<'a> {
    densidad_por_m2: f32,
    alcance: f32,
    anillos: &'a str,
}

#[derive(Serialize)]
struct LegendCategory<'a> {
    nombre: &'a str,
    color: [u8; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    alcance_m: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chunk_m: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    densidad_por_m2: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tris_por_primitiva: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plantado: Option<bool>,
}

#[derive(Serialize)]
struct LegendFraming {
    mallas_visibles: u32,
    triangulos: usize,
    batches_estimados: usize,
    pradera_triangulos: usize,
    pradera_batches_estimados: usize,
}

fn legend_path(path: &std::path::Path) -> PathBuf {
    path.with_extension("ron")
}

/// Escribe, al lado del PNG, con qué configuración se sacó la foto.
///
/// Ya no es un contrato con nadie —el conteo vive en la misma corrida desde el
/// 2026-08-08— sino el registro de la captura: qué perillas, qué anillos se
/// plantaron y desde dónde. Sin eso, dos PNG del mismo encuadre son dos imágenes
/// que no se pueden comparar.
fn write_legend(
    path: &std::path::Path,
    perf: &crate::perf::PerfToggles,
    settings: &crate::visuals::grass::GrassRendererSettings,
    inventory: &SceneInventory,
    pose: (Vec3, Vec3),
    geometry: &ShotGeometry,
) {
    use crate::visuals::material_registry::Subject;

    let view = perf.grass_debug_label();
    // Los nombres y los colores salen de la misma lista que el conteo usa: una
    // leyenda que dijera otra cosa que el informe describiría otra captura.
    // **Con la vista apagada la leyenda se escribe igual, sin categorías.** Hasta
    // el 2026-08-07 no se escribía, y eso dejaba a la captura del **juego real**
    // —la única que muestra el color que el jugador ve— sin la geometría de
    // cámara, o sea sin eje de distancias.
    let named = shot_categories(perf, settings);
    let categories = if view == "subpixel" {
        named
            .iter()
            .map(|band| LegendCategory {
                nombre: &band.name,
                color: band.color,
                alcance_m: None,
                chunk_m: None,
                densidad_por_m2: None,
                tris_por_primitiva: None,
                plantado: None,
            })
            .collect()
    } else {
        named
            .iter()
            .zip(crate::visuals::grass_debug::ring_legend(perf, settings))
            .map(|(category, ring)| LegendCategory {
                nombre: &category.name,
                color: category.color,
                alcance_m: Some(ring.reach_m),
                chunk_m: Some(ring.chunk_m),
                densidad_por_m2: Some(ring.density),
                tris_por_primitiva: Some(ring.triangles_per_blade),
                // Un anillo apagado cuenta cero píxeles, que es un número
                // perfectamente creíble para uno que sí se plantó y no se ve.
                // La corrida declara cuál es cuál.
                plantado: Some(ring.planted),
            })
            .collect()
    };
    let meadow = inventory.subject(Subject::Meadow);
    let flat =
        crate::visuals::grass_debug::GrassDebugView::from_step(perf.grass_debug_step()).is_flat();
    let legend = Legend {
        vista: view,
        // Que la vista pinte plano y exacto lo declara el shader, que es quien
        // sabe cuáles lo hacen.
        plana: flat,
        camara: LegendCamera {
            pos: pose.0.to_array(),
            facing: pose.1.to_array(),
            // Con estos tres la fila de pantalla se convierte en metros, que es
            // el eje x del perfil por distancia.
            fov_y: geometry.fov_y,
            viewport_px: [geometry.viewport.0, geometry.viewport.1],
            ojo_sobre_suelo_m: geometry.eye_above_ground_m,
        },
        // La altura del terreno bajo la línea de vista, cada pocos metros. La
        // conversión fila→distancia supone suelo plano; esto es lo que permite
        // **verificar** la suposición en vez de arrastrarla.
        perfil_suelo: &geometry.ground_profile,
        perillas: LegendKnobs {
            densidad_por_m2: perf.grass_density(),
            alcance: perf.grass_reach_scale(),
            anillos: perf.grass_rings_label(),
        },
        categorias: categories,
        encuadre: LegendFraming {
            mallas_visibles: inventory.visible_meshes,
            triangulos: inventory.triangles,
            batches_estimados: inventory.draws,
            pradera_triangulos: meadow.triangles,
            pradera_batches_estimados: meadow.draws,
        },
    };
    let path = legend_path(path);
    let pretty = ron::ser::PrettyConfig::default().compact_arrays(true);
    match ron::ser::to_string_pretty(&legend, pretty) {
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
            "{}={} tris/{} draws~/{:.1} MB ({:.0}% del cuadro)",
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
        "[shot] escena: {} mallas visibles, {} triángulos, {} draws~ · {} · registros de pradera \
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
pub(super) fn drive_auto_shot(
    mut commands: Commands,
    mut shot: ResMut<AutoShot>,
    mut next_scene: ResMut<NextState<AppState>>,
    scene: Res<State<AppState>>,
    mut exit: MessageWriter<AppExit>,
    time: Res<Time<Real>>,
    inventory: Res<SceneInventory>,
    mut perf: ResMut<crate::perf::PerfToggles>,
    settings: Res<crate::visuals::grass::GrassRendererSettings>,
    camera: Option<Single<(&GlobalTransform, &Projection), With<Camera3d>>>,
    window: Option<Single<&Window, With<bevy::window::PrimaryWindow>>>,
    terrain: crate::world::TerrainAccess,
    broken: Res<BrokenAssets>,
    records: Res<crate::visuals::grass::MeadowRecordMemory>,
    log: Res<shot_stats::ShotStatsLog>,
    progress: Res<ShotCaptureProgress>,
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
                    shot.invalid = true;
                    warn!(
                        "[shot] LA CÁMARA NO ESTÁ EN EL MIRADOR: está en \
                         ({:.1},{:.1},{:.1}) mirando ({:.2},{:.2},{:.2}). La foto no es del \
                         lugar que este reporte declara.",
                        at.x, at.y, at.z, aim.x, aim.y, aim.z,
                    );
                }
            } else {
                shot.invalid = true;
                warn!("[shot] sin cámara: no se puede verificar de dónde es la foto");
            }
            // Lo que la foto no puede mostrar: si dos encuadres discrepan, hace
            // falta saber si cambió lo que se dibuja o sólo cómo se proyecta.
            warn_on_broken_assets(&broken);
            shot.invalid |= broken.count() > 0;
            log_framing(&inventory, &records);
            // La geometría sale de la cámara **real**, no del mirador pedido:
            // si las dos discrepan el aviso de arriba ya sonó, y una conversión
            // a metros hecha sobre una pose que la foto no tiene sería un
            // número exacto de otra escena.
            let actual_pose = camera.as_ref().map_or((position, facing), |camera| {
                (camera.0.translation(), camera.0.forward().as_vec3())
            });
            let geometry = shot_geometry(
                actual_pose,
                camera.as_ref().map(|camera| camera.1),
                window.as_deref().map(|w| &**w),
                &terrain,
            );
            write_legend(&path, &perf, &settings, &inventory, actual_pose, &geometry);
            let sweep_label = shot.sweep.map(|sweep| perf.knob_value(sweep.knob));
            commands
                .spawn(Screenshot::primary_window())
                .observe(shot_stats::count_when_captured(stats_plan(
                    &perf,
                    &settings,
                    geometry,
                    sweep_label,
                )))
                .observe(save_auto_shot(path));
            shot.stage = Stage::AwaitingCapture(progress.0 + 1);
            shot.elapsed = 0.0;
        }
        Stage::AwaitingCapture(expected) => {
            shot.elapsed += time.delta_secs();
            if progress.0 < expected {
                if shot.elapsed >= CAPTURE_TIMEOUT_SECS {
                    error!("[shot] la GPU no devolvió la captura en {CAPTURE_TIMEOUT_SECS:.0}s");
                    exit.write(AppExit::error());
                }
                return;
            }
            if advance_sweep(&mut shot, &mut perf) {
                shot.stage = Stage::Settling;
                shot.elapsed = 0.0;
            } else {
                if let Some(sweep) = shot.sweep {
                    info!(
                        "[shot] {}",
                        shot_stats::sweep_table(
                            sweep.knob.label(),
                            &log.rows,
                            settings.blade_width_m
                        )
                    );
                }
                exit.write(if shot.invalid {
                    AppExit::error()
                } else {
                    AppExit::Success
                });
            }
        }
    }
}

/// Mueve el barrido al paso siguiente, o dice que ya no quedan.
///
/// Toca las perillas directamente y no por `KnobRequest` porque una corrida de
/// captura **es** dueña de la configuración mientras dura: entró a la escena,
/// paró la cámara en el mirador y va a salir sola. Un pedido diferido se
/// aplicaría un frame después del disparo, o sea a la foto equivocada.
fn advance_sweep(shot: &mut AutoShot, perf: &mut crate::perf::PerfToggles) -> bool {
    let Some(sweep) = &mut shot.sweep else {
        return false;
    };
    let next = sweep.step + 1;
    if next >= sweep.knob.steps() {
        return false;
    }
    sweep.step = next;
    perf.set_knob_step(sweep.knob, next);
    info!(
        "[shot] barrido: {} = {}",
        sweep.knob.label(),
        perf.knob_value(sweep.knob)
    );
    true
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
            sweep: None,
            pose_override: None,
            invalid: false,
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
        assert_eq!(
            legend_path(&shot.path()),
            PathBuf::from("target/shots/grass.ron")
        );
    }

    /// Barriendo, cada paso deja su propia imagen: si todos pisaran el mismo
    /// archivo, la tabla saldría igual y sólo la última foto la respaldaría.
    #[test]
    fn every_step_of_a_sweep_keeps_its_own_image() {
        let mut shot = shot(BenchSuite::Grass, Stage::Settling);
        shot.sweep = Some(Sweep {
            knob: PerfKnob::GrassDensity,
            step: 0,
        });
        let mut perf = crate::perf::PerfToggles::default();
        let mut paths = vec![shot.path()];
        while advance_sweep(&mut shot, &mut perf) {
            paths.push(shot.path());
        }
        assert_eq!(paths.len(), PerfKnob::GrassDensity.steps());
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), PerfKnob::GrassDensity.steps());
    }

    /// El barrido recorre la escalera **entera** y para: una vuelta de más
    /// repetiría el primer paso creyendo que es otro.
    #[test]
    fn a_sweep_walks_its_ladder_once() {
        let mut shot = shot(BenchSuite::Grass, Stage::Settling);
        shot.sweep = Some(Sweep {
            knob: PerfKnob::GrassRings,
            step: 0,
        });
        let mut perf = crate::perf::PerfToggles::default();
        let mut values = vec![perf.knob_value(PerfKnob::GrassRings)];
        while advance_sweep(&mut shot, &mut perf) {
            values.push(perf.knob_value(PerfKnob::GrassRings));
        }
        assert_eq!(values.len(), PerfKnob::GrassRings.steps());
        assert!(!advance_sweep(&mut shot, &mut perf));
    }

    /// Sin barrido, la corrida sale después de una foto. Es lo que separa
    /// `BOF_SHOT` de `BOF_SHOT_SWEEP`, y nada más lo sostiene.
    #[test]
    fn a_plain_shot_never_advances() {
        let mut shot = shot(BenchSuite::Grass, Stage::Settling);
        let mut perf = crate::perf::PerfToggles::default();
        assert!(!advance_sweep(&mut shot, &mut perf));
    }

    /// A misspelled sweep used to silently take one ordinary screenshot and
    /// exit 0, leaving a plausible one-row "curve" behind.
    #[test]
    fn an_unknown_sweep_is_rejected_instead_of_becoming_one_shot() {
        assert!(parse_sweep("grass-density").is_ok());
        assert!(parse_sweep("grass-densitty").is_err());
        assert!(parse_sweep("").is_err());
    }

    #[test]
    fn malformed_reproduction_poses_are_rejected_not_silently_replaced() {
        let (position, facing) = parse_pose("1,2,3:0,0,-2").expect("valid pose");
        assert_eq!(position, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(facing, Vec3::NEG_Z);

        for malformed in [
            "",
            "1,2,3",
            "1,2:0,0,-1",
            "1,2,3,4:0,0,-1",
            "1,2,3:0,0,0",
            "NaN,2,3:0,0,-1",
        ] {
            assert!(parse_pose(malformed).is_err(), "accepted {malformed:?}");
        }
    }
}
