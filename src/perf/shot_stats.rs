//! Contar lo que hay en la captura, **en la corrida que la sacó**.
//!
//! # Por qué acá y no en un script
//!
//! Hasta el 2026-08-08 esto era `tools/shot_stats.py`: la corrida escribía un
//! `.json` con la paleta y la geometría, y el script decodificaba el PNG para
//! contar colores. Dos programas y un contrato entre ellos para una pregunta
//! que la corrida ya podía contestar sola — tenía los píxeles en memoria, la
//! paleta, la pose de la cámara y el terreno bajo la línea de vista.
//!
//! El conteo mira **exactamente los bytes que van al archivo**: el mismo
//! `try_into_dynamic().to_rgb8()` que usa `save_to_disk`. Un decodificador
//! propio podía leer otra cosa que el PNG y nadie se enteraba.
//!
//! # Lo que sigue sin hacer
//!
//! No juzga. Cuenta y declara sus propias condiciones — con la vista que no
//! pinta plano, o con relieve bajo la mirada, lo dice y omite el perfil en vez
//! de devolver metros creíbles y falsos.

use bevy::prelude::*;

/// Los anillos de distancia del perfil, en metros. Geométricos y no uniformes a
/// propósito: la cobertura cae como `1/d`, así que lo que importa es el orden de
/// magnitud. Cubren los tres niveles de la pradera (64 m).
const DEFAULT_RANGES: [f32; 11] = [2.0, 3.0, 4.0, 6.0, 8.0, 11.0, 16.0, 22.0, 32.0, 45.0, 64.0];

/// Cuánto puede ondular el suelo bajo la línea de vista antes de que convertir
/// filas en metros deje de valer: la cuenta supone un plano, y con 20 cm de
/// relieve el error ya es del orden del ancho de una banda.
const FLAT_GROUND_TOLERANCE_M: f32 = 0.2;

/// Hasta dónde se muestrea el suelo, y cada cuánto. Cubre el anillo más lejano
/// (64 m) con holgura.
const GROUND_PROFILE_M: f32 = 80.0;
const GROUND_PROFILE_STEP_M: f32 = 4.0;

/// Qué se cuenta: un nombre y el color plano y exacto que lo identifica.
///
/// Los colores salen de `visuals::grass_debug`, nunca escritos acá: una segunda
/// copia de la paleta es la clase de bug que este trabajo vino a cerrar.
pub(super) struct Category {
    pub name: String,
    pub color: [u8; 3],
}

/// Lo que hace falta para convertir una fila de pantalla en una distancia.
///
/// La aritmética es exacta **sólo si el suelo es plano**, así que la corrida no
/// lo supone: muestrea el terreno a lo largo de la línea de vista y guarda el
/// perfil para verificarlo antes de usar la conversión.
pub(super) struct ShotGeometry {
    pub fov_y: f32,
    pub viewport: (u32, u32),
    pub eye_above_ground_m: Option<f32>,
    pub facing: Vec3,
    pub ground_profile: Vec<(f32, f32)>,
}

pub(super) fn shot_geometry(
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
        facing,
        ground_profile,
    }
}

impl ShotGeometry {
    /// A qué distancia del suelo mira cada fila de la imagen, o por qué no se
    /// sabe.
    ///
    /// Con el ojo a `h` sobre suelo plano y la mirada con un ángulo de depresión
    /// `p`, el rayo que sale por la fila `y` baja `p + a(y)`, donde `a(y)` es el
    /// ángulo de esa fila respecto del centro. Toca el suelo a `h / tan(p + a)`;
    /// donde ese ángulo no es positivo la fila mira al cielo, que es infinito y
    /// no un número grande.
    fn row_distances(&self, height: u32) -> Result<Vec<f32>, &'static str> {
        let Some(eye) = self.eye_above_ground_m else {
            return Err("sin altura del ojo sobre el suelo");
        };
        if !self.fov_y.is_finite() || self.fov_y <= 0.0 {
            return Err("sin campo de visión: la cámara no es perspectiva");
        }
        if self.viewport.1 != height {
            // El PNG es de la ventana entera; con `render-scale` la imagen
            // renderizada ocupa una esquina y las filas ya no son las del
            // viewport.
            return Err("el alto de la imagen no es el del viewport");
        }
        let relief = self.ground_relief();
        if relief > FLAT_GROUND_TOLERANCE_M {
            return Err("el suelo bajo la mirada no es plano");
        }
        let Some(facing) = self.facing.try_normalize() else {
            return Err("sin dirección de mirada");
        };

        let pitch = (-facing.y).asin();
        let half = (self.fov_y / 2.0).tan();
        let rows = f32::from(u16::try_from(height).unwrap_or(u16::MAX));
        Ok((0..height)
            .map(|y| {
                let row = f32::from(u16::try_from(y).unwrap_or(u16::MAX));
                // El centro de la fila, llevado a la tangente del ángulo
                // respecto del centro de la imagen. `y = 0` es arriba.
                let offset = half * (1.0 - 2.0 * (row + 0.5) / rows);
                let angle = pitch - offset.atan();
                if angle <= 1e-4 {
                    f32::INFINITY
                } else {
                    eye / angle.tan()
                }
            })
            .collect())
    }

    fn ground_relief(&self) -> f32 {
        let heights: Vec<f32> = self.ground_profile.iter().map(|(_, y)| *y).collect();
        let low = heights.iter().copied().fold(f32::INFINITY, f32::min);
        let high = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        if heights.is_empty() { 0.0 } else { high - low }
    }
}

/// Una banda de distancia del perfil.
///
/// El denominador son los píxeles de esas filas y no un área de mundo: lo que se
/// mide es *qué fracción de lo que se ve a esa distancia es pasto*, que es la
/// pregunta que la derivación de densidad contesta y contra la que se la puede
/// refutar.
pub(super) struct DistanceBand {
    pub near: f32,
    pub far: f32,
    pub rows: u32,
    pub per_category: Vec<u64>,
    pub pixels: u64,
}

impl DistanceBand {
    /// Cuánto de esta banda es pasto, contando cada píxel una sola vez.
    pub fn coverage(&self) -> f64 {
        let covered: u64 = self.per_category.iter().sum();
        ratio(covered, self.pixels)
    }
}

/// Lo que la captura contiene, en números.
pub(super) struct ShotStats {
    pub width: u32,
    pub height: u32,
    pub luminance_mean: f64,
    pub luminance_sd: f64,
    pub saturation_mean: f64,
    pub per_category: Vec<u64>,
    pub pixels: u64,
    pub bands: Vec<DistanceBand>,
    /// Por qué no hay perfil por distancia, cuando no lo hay.
    pub profile_omitted: Option<&'static str>,
}

impl ShotStats {
    pub fn covered(&self) -> u64 {
        self.per_category.iter().sum()
    }
}

fn ratio(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

/// Cuenta la imagen capturada.
///
/// Devuelve `None` cuando el formato de la textura no se puede leer — un
/// silencio acá sería un informe de ceros indistinguible de un campo vacío.
pub(super) fn analyze(
    image: &Image,
    categories: &[Category],
    geometry: &ShotGeometry,
) -> Option<ShotStats> {
    // El mismo camino que `save_to_disk`, y por eso: cuenta los bytes que
    // termina teniendo el archivo, incluido el intercambio de canales que hace
    // falta cuando el swapchain es BGRA.
    let dynamic = match image.clone().try_into_dynamic() {
        Ok(dynamic) => dynamic,
        Err(error) => {
            error!("[shot] la captura no se puede leer para contarla: {error}");
            return None;
        }
    };
    let rgb = dynamic.to_rgb8();
    let (width, height) = (rgb.width(), rgb.height());
    let pixels = u64::from(width) * u64::from(height);
    let data = rgb.as_raw();

    let distances = geometry.row_distances(height);
    let mut bands: Vec<DistanceBand> = DEFAULT_RANGES
        .windows(2)
        .map(|pair| DistanceBand {
            near: pair[0],
            far: pair[1],
            rows: 0,
            per_category: vec![0; categories.len()],
            pixels: 0,
        })
        .collect();

    let mut counts = vec![0u64; categories.len()];
    let mut luminance_sum = 0.0f64;
    let mut luminance_squares = 0.0f64;
    let mut saturation_sum = 0.0f64;

    for y in 0..height {
        let band = distances.as_ref().ok().and_then(|distances| {
            let distance = *distances.get(usize::try_from(y).ok()?)?;
            bands
                .iter()
                .position(|band| band.near <= distance && distance < band.far)
        });
        if let Some(index) = band
            && let Some(band) = bands.get_mut(index)
        {
            band.rows += 1;
            band.pixels += u64::from(width);
        }
        for x in 0..width {
            let start = usize::try_from((u64::from(y) * u64::from(width) + u64::from(x)) * 3)
                .unwrap_or(usize::MAX);
            let Some(&[r, g, b]) = data
                .get(start..start + 3)
                .and_then(|px| px.first_chunk::<3>())
            else {
                continue;
            };
            // Rec. 601, que es la que usa el ojo para "más claro / más oscuro".
            let luminance = 0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b);
            luminance_sum += luminance;
            luminance_squares += luminance * luminance;
            let top = r.max(g).max(b);
            let bottom = r.min(g).min(b);
            if top > 0 {
                saturation_sum += f64::from(top - bottom) / f64::from(top);
            }
            let Some(category) = categories
                .iter()
                .position(|category| category.color == [r, g, b])
            else {
                continue;
            };
            if let Some(count) = counts.get_mut(category) {
                *count += 1;
            }
            if let Some(index) = band
                && let Some(count) = bands
                    .get_mut(index)
                    .and_then(|band| band.per_category.get_mut(category))
            {
                *count += 1;
            }
        }
    }

    let total = if pixels == 0 { 1.0 } else { pixels as f64 };
    let mean = luminance_sum / total;
    // Varianza por el momento de segundo orden; con enteros de 8 bits y un
    // millón de muestras la cancelación catastrófica no llega a importar.
    let variance = (luminance_squares / total - mean * mean).max(0.0);
    Some(ShotStats {
        width,
        height,
        luminance_mean: mean,
        luminance_sd: variance.sqrt(),
        saturation_mean: saturation_sum / total,
        per_category: counts,
        pixels,
        bands: bands.into_iter().filter(|band| band.pixels > 0).collect(),
        profile_omitted: distances.err(),
    })
}

/// El informe, como texto de una sola pieza: un `info!` por línea lo dejaría
/// intercalado con el resto del log justo cuando se lo quiere leer como tabla.
pub(super) fn report(stats: &ShotStats, categories: &[Category], view: &str, flat: bool) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "captura {}x{} · vista '{view}'",
        stats.width, stats.height
    );
    let _ = writeln!(
        out,
        "  luminancia media {:6.1}   desviación {:5.1}   saturación {:5.1}%",
        stats.luminance_mean,
        stats.luminance_sd,
        stats.saturation_mean * 100.0,
    );
    if categories.is_empty() {
        let _ = writeln!(
            out,
            "  sin categorías: la vista no pinta colores planos, así que no hay nada que contar.\n\
               Para el conteo, `BOF_KNOBS=grass-view=medir`."
        );
        return out;
    }
    if !flat {
        let _ = writeln!(
            out,
            "  AVISO: esta vista no pinta colores planos y exactos. La luz y la niebla mueven\n\
               cada píxel, así que este conteo no significa nada."
        );
    }
    let covered = stats.covered();
    let _ = writeln!(
        out,
        "  cobertura: {:6.2}% de la pantalla ({covered} px)",
        ratio(covered, stats.pixels) * 100.0,
    );
    let _ = writeln!(
        out,
        "  {:<32} {:>9} {:>10} {:>9} {:>10}",
        "categoría", "color", "px", "pantalla", "del pasto"
    );
    for (index, category) in categories.iter().enumerate() {
        let count = stats.per_category.get(index).copied().unwrap_or(0);
        let [r, g, b] = category.color;
        let _ = writeln!(
            out,
            "  {:<32} #{r:02X}{g:02X}{b:02X} {count:>10} {:>8.2}% {:>8.1}%",
            category.name,
            ratio(count, stats.pixels) * 100.0,
            ratio(count, covered) * 100.0,
        );
    }

    if let Some(reason) = stats.profile_omitted {
        let _ = writeln!(out, "  perfil por distancia omitido a propósito: {reason}.");
        return out;
    }
    let _ = writeln!(
        out,
        "\n  perfil por distancia (suelo plano, geometría de esta corrida):"
    );
    let header: String = categories
        .iter()
        .enumerate()
        .map(|(index, _)| format!("{index:>7}"))
        .collect::<Vec<_>>()
        .join(" ");
    let _ = writeln!(out, "  distancia      filas {header}    total");
    for band in &stats.bands {
        let cells: String = band
            .per_category
            .iter()
            .map(|count| format!("{:>6.1}%", ratio(*count, band.pixels) * 100.0))
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(
            out,
            "  {:>4.0}-{:<4.0} m {:>7} {cells} {:>7.1}%",
            band.near,
            band.far,
            band.rows,
            band.coverage() * 100.0,
        );
    }
    out
}

/// Lo que el conteo necesita, congelado en el instante del disparo.
pub(super) struct StatsPlan {
    pub categories: Vec<Category>,
    pub view: String,
    pub flat: bool,
    pub geometry: ShotGeometry,
    /// Con qué configuración se sacó, cuando la corrida está barriendo una
    /// perilla. Es la fila de la curva.
    pub sweep_label: Option<String>,
    /// Cuántas briznas por m² están **vivas** a cada distancia del perfil, que es
    /// lo que la ley de cobertura necesita como `λ`. La corrida lo sabe: sale de
    /// la escalera de alcances, no de la perilla.
    pub live_density: Box<dyn Fn(f32) -> f32 + Send + Sync>,
}

/// Una fila de la curva: una configuración, y qué cobertura dio a cada
/// distancia.
pub(crate) struct SweepRow {
    pub label: String,
    pub coverage: f64,
    /// Por banda: sus metros, la cobertura medida y **cuántas briznas vivía**.
    /// La densidad va por banda y no por fila porque la brizna muere con la
    /// distancia: usar el número de la perilla para todas —como se hizo hasta el
    /// 2026-08-08— despeja una huella que absorbe el raleo y sale 3× chica.
    pub bands: Vec<BandSample>,
}

pub(crate) struct BandSample {
    pub near: f32,
    pub far: f32,
    pub coverage: f64,
    pub live_per_m2: f32,
}

/// Lo que el barrido lleva anotado.
///
/// El observador escribe acá porque la captura vuelve de la GPU varios frames
/// después del disparo: para cuando hay número, el sistema que lo pidió ya
/// terminó su turno.
#[derive(Resource, Default)]
pub(crate) struct ShotStatsLog {
    pub rows: Vec<SweepRow>,
}

/// El observador que cuenta la captura cuando vuelve de la GPU.
///
/// Va en la misma entidad que `save_to_disk`, así que el informe describe el
/// archivo que se acaba de escribir y no una segunda captura parecida.
pub(super) fn count_when_captured(
    plan: StatsPlan,
) -> impl FnMut(On<bevy::render::view::screenshot::ScreenshotCaptured>, ResMut<ShotStatsLog>) {
    move |captured, mut log| {
        let Some(stats) = analyze(&captured.image, &plan.categories, &plan.geometry) else {
            return;
        };
        info!(
            "[shot] {}",
            report(&stats, &plan.categories, &plan.view, plan.flat)
        );
        if let Some(label) = &plan.sweep_label {
            log.rows.push(SweepRow {
                label: label.clone(),
                coverage: ratio(stats.covered(), stats.pixels),
                bands: stats
                    .bands
                    .iter()
                    .map(|band| BandSample {
                        near: band.near,
                        far: band.far,
                        coverage: band.coverage(),
                        // En el medio de la banda: la densidad viva cae con la
                        // distancia, así que tomarla en un extremo sesga el
                        // despeje hacia arriba o hacia abajo.
                        live_per_m2: (plan.live_density)(f32::midpoint(band.near, band.far)),
                    })
                    .collect(),
            });
        }
    }
}

/// La curva: una fila por paso de la perilla, una columna por banda de
/// distancia.
///
/// Las columnas salen de la **primera** fila y las demás se buscan por sus
/// metros, no por posición: dos corridas con distinto reparto de filas darían
/// columnas corridas, y una tabla corrida se lee sin notarlo.
pub(super) fn sweep_table(knob: &str, rows: &[SweepRow], blade_width_m: f32) -> String {
    use std::fmt::Write as _;

    let mut out = format!("la curva de cobertura contra '{knob}':\n");
    let Some(columns) = rows.first().map(|row| {
        row.bands
            .iter()
            .map(|band| (band.near, band.far))
            .collect::<Vec<_>>()
    }) else {
        return out + "  (ninguna captura llegó a contarse)\n";
    };
    let header: String = columns
        .iter()
        .map(|(near, far)| format!("{near:>5.0}-{far:<3.0}"))
        .collect::<Vec<_>>()
        .join(" ");
    let _ = writeln!(out, "  {:<12} {:>8} {header}", "paso", "pantalla");
    for row in rows {
        let cells: String = columns
            .iter()
            .map(|(near, far)| {
                band_at(row, *near, *far).map_or_else(
                    || format!("{:>9}", "-"),
                    |band| format!("{:>8.1}%", band.coverage * 100.0),
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(
            out,
            "  {:<12} {:>7.1}% {cells}",
            row.label,
            row.coverage * 100.0
        );
    }
    out + &poisson_law(&columns, rows, blade_width_m)
}

fn band_at(row: &SweepRow, near: f32, far: f32) -> Option<&BandSample> {
    row.bands
        .iter()
        .find(|band| (band.near - near).abs() < 0.01 && (band.far - far).abs() < 0.01)
}

/// Cuánto tapa **una** brizna a cada distancia, despejado de la curva.
///
/// Si caen como puntos independientes, la cobertura es `C = 1 − e^(−λ·a)`: cada
/// paso del barrido propone su propio `a = −ln(1−C)/λ`, y que todos propongan el
/// mismo convierte la tabla en una ley. La dispersión sale al lado del promedio
/// a propósito — un promedio solo no dice si creerle.
///
/// **`λ` es la densidad viva de esa banda, no la de la perilla.** Con la de la
/// perilla, el raleo por distancia se cuela dentro de `a` y la huella sale hasta
/// tres veces más chica de lo que es; esa confusión es la que hizo que la ley
/// pidiera de menos, y sólo se vio cuando el rediseño quitó el solapamiento que
/// la venía compensando.
///
/// La columna `a/(w·d)` es la que importa para corregir la ley: si fuera
/// constante, la huella crecería lineal con la distancia como `minimum_density`
/// supone. Donde no lo sea, ahí la ley pide de más o de menos.
fn poisson_law(columns: &[(f32, f32)], rows: &[SweepRow], blade_width_m: f32) -> String {
    use std::fmt::Write as _;

    let mut out = String::from("  cuánto tapa una brizna, despejado de la curva:\n");
    let _ = writeln!(
        out,
        "  {:<12} {:>8} {:>9} {:>9} {:>10}",
        "banda", "a (m²)", "mínimo", "máximo", "a/(w·d)"
    );
    let mut printed = false;
    for (near, far) in columns {
        let areas: Vec<f64> = rows
            .iter()
            .filter_map(|row| {
                let band = band_at(row, *near, *far)?;
                // Saturada, `ln(1−C)` es ruido dividido por λ: a 100% no queda
                // información sobre cuánto tapa cada brizna, sólo que sobran.
                (band.coverage > 0.0 && band.coverage < 0.999 && band.live_per_m2 > 0.0)
                    .then(|| -(1.0 - band.coverage).ln() / f64::from(band.live_per_m2))
            })
            .collect();
        if areas.len() < 2 {
            continue;
        }
        printed = true;
        let mean = areas.iter().sum::<f64>() / areas.len() as f64;
        let low = areas.iter().copied().fold(f64::INFINITY, f64::min);
        let high = areas.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let middle = f64::from(f32::midpoint(*near, *far));
        let per_width_per_metre = mean / (f64::from(blade_width_m) * middle);
        let _ = writeln!(
            out,
            "  {near:>4.0}-{far:<4.0} m {mean:>9.4} {low:>9.4} {high:>9.4} {per_width_per_metre:>10.3}"
        );
    }
    if printed { out } else { String::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(eye: f32, facing: Vec3, height: u32) -> ShotGeometry {
        ShotGeometry {
            fov_y: std::f32::consts::FRAC_PI_4,
            viewport: (16, height),
            eye_above_ground_m: Some(eye),
            facing,
            ground_profile: vec![(0.0, 0.0), (4.0, 0.0)],
        }
    }

    /// La fila de arriba mira más lejos que la de abajo, y el horizonte es
    /// infinito y no un número grande: lo que ordena el perfil entero.
    #[test]
    fn rows_run_from_far_at_the_top_to_near_at_the_bottom() {
        let distances = geometry(3.0, Vec3::new(0.0, -0.26, -1.0), 64)
            .row_distances(64)
            .expect("suelo plano y cámara declarada");
        assert_eq!(distances.len(), 64);
        for pair in distances.windows(2) {
            assert!(pair[0] >= pair[1], "{:?} no baja con la fila", pair);
        }
        assert!(distances[63].is_finite());
    }

    /// Mirando al horizonte la mitad de arriba es cielo, y el cielo no está a
    /// una distancia del suelo — un número finito ahí metería píxeles de cielo
    /// en la banda más lejana y la diluiría.
    #[test]
    fn rows_above_the_horizon_are_infinite() {
        let distances = geometry(3.0, Vec3::new(0.0, 0.0, -1.0), 64)
            .row_distances(64)
            .expect("suelo plano y cámara declarada");
        assert!(distances[0].is_infinite());
        assert!(distances[63].is_finite());
    }

    /// El relieve no se supone plano: se verifica. Un perfil con lomas devuelve
    /// el motivo, no metros creíbles y equivocados.
    #[test]
    fn a_bumpy_ground_refuses_to_become_metres() {
        let mut geometry = geometry(3.0, Vec3::new(0.0, -0.26, -1.0), 64);
        geometry.ground_profile = vec![(0.0, 0.0), (4.0, 1.5)];
        assert!(geometry.row_distances(64).is_err());
    }

    fn row(live_per_m2: f32, coverage: f64) -> SweepRow {
        SweepRow {
            label: format!("{live_per_m2}/m2"),
            coverage,
            bands: vec![BandSample {
                near: 22.0,
                far: 32.0,
                coverage,
                live_per_m2,
            }],
        }
    }

    /// Un campo que **sí** obedece a Poisson tiene que devolver el área con la
    /// que se lo fabricó. Sin esto, la ley podría estar despejando mal y la
    /// tabla se leería igual de convincente.
    #[test]
    fn a_field_built_from_the_law_gives_its_area_back() {
        const AREA: f64 = 0.068;
        let rows: Vec<SweepRow> = [6.0f32, 12.0, 20.0, 40.0]
            .into_iter()
            .map(|live| row(live, 1.0 - (-f64::from(live) * AREA).exp()))
            .collect();
        let text = poisson_law(&[(22.0, 32.0)], &rows, 0.055);
        assert!(
            text.contains("0.0680"),
            "la ley no recupera el área con la que se fabricó el campo: {text}"
        );
    }

    /// **La densidad que despeja la huella es la viva, no la de la perilla.**
    /// Dos campos con la misma perilla pero distinto raleo dan coberturas
    /// distintas: usando el número de la perilla, la huella salía chica y la ley
    /// terminaba pidiendo menos briznas de las que hacen falta.
    #[test]
    fn the_law_uses_the_density_that_is_alive() {
        const AREA: f64 = 0.1;
        let rows: Vec<SweepRow> = [4.0f32, 10.0, 25.0]
            .into_iter()
            .map(|live| row(live, 1.0 - (-f64::from(live) * AREA).exp()))
            .collect();
        let text = poisson_law(&[(22.0, 32.0)], &rows, 0.055);
        assert!(text.contains("0.1000"), "{text}");
    }

    /// Con una sola muestra no hay nada que ajustar, y un número por banda con
    /// una sola muestra se lee como si estuviera medido.
    #[test]
    fn one_sample_is_not_a_curve() {
        assert!(poisson_law(&[(22.0, 32.0)], &[row(40.0, 0.9)], 0.055).is_empty());
    }

    /// Con `render-scale` la imagen renderizada no llena la ventana, así que las
    /// filas del PNG no son las del viewport y la conversión no vale.
    #[test]
    fn a_viewport_that_is_not_the_image_refuses_to_become_metres() {
        assert!(
            geometry(3.0, Vec3::new(0.0, -0.26, -1.0), 64)
                .row_distances(32)
                .is_err()
        );
    }
}
