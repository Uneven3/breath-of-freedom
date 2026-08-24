//! Escombro: las piedras sueltas que rompen la línea donde un peñasco corta el
//! suelo.
//!
//! Por qué existe y por qué va sin collider: `docs/CLIFFS.md`. En una línea:
//! una intersección limpia entre dos superficies se ve, y lo que la tapa es
//! poner algo encima del borde — que es lo que la naturaleza pone igual.

use bevy::prelude::*;
use bof_domain::world::hash_unit;

use super::crags;
use super::layout::{Anchor, settle};
use crate::asset_pipeline::materials::MaterialPalette;
use crate::scene::AppState;

const DEBRIS_MATERIAL: &str = "GrayboxProp";

/// Radio de la piedra base, antes de la escala por instancia. Lo que lo acota
/// no es el vault —sin collider no hay nada que detectar— sino que se atraviesa:
/// a 20 cm de asomo nadie lo nota, a medio metro sí.
const PEBBLE_RADIUS: f32 = 0.28;

/// Cuánto del cuerpo queda bajo tierra: una piedra apoyada entera tiene el
/// mismo problema de contacto que la roca grande, a escala.
const PEBBLE_BURIED_SHARE: f32 = 0.5;

/// Metros de contorno por piedra. Más juntas se leen como una guarda de jardín;
/// más lejos dejan ver el corte que vienen a tapar, que fue el primer intento.
const METRES_PER_PEBBLE: f32 = 1.6;

/// Cuántas piedras entran alrededor de una huella de semiejes `half`.
///
/// El perímetro sale de la aproximación de Ramanujan; la exacta es una integral
/// elíptica y acá se está eligiendo un entero.
fn pebble_count(half: Vec2) -> usize {
    let (a, b) = (half.x.max(half.y), half.x.min(half.y));
    let h = ((a - b) / (a + b)).powi(2);
    let perimeter =
        std::f32::consts::PI * (a + b) * (1.0 + 3.0 * h / (10.0 + (4.0 - 3.0 * h).sqrt()));
    ((perimeter / METRES_PER_PEBBLE).round() as usize).clamp(4, 48)
}

/// El radio de la elipse en un azimut. Es lo que hace que las piedras sigan el
/// contorno real de la pieza y no un círculo: con semiejes de 9 y 3 m, la
/// diferencia entre los dos es de seis metros de roca.
fn elliptic_radius(half: Vec2, angle: f32) -> f32 {
    let (sin, cos) = angle.sin_cos();
    let denominator = (cos / half.x).powi(2) + (sin / half.y).powi(2);
    if denominator <= f32::EPSILON {
        return half.max_element();
    }
    denominator.sqrt().recip()
}

pub(super) fn setup_debris(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<MaterialPalette>,
    state: Res<State<AppState>>,
    ground: super::TerrainAccess,
) {
    let scene = *state.get();
    let ground = Some(&ground);
    // Una sola malla para todas las piedras: la variedad va por `Transform`, que
    // es gratis, y así el escombro entero es un batch en vez de uno por piedra.
    let pebble = meshes.add(crags::small_stone_mesh(PEBBLE_RADIUS, 0xdeb2_15ee));
    let material = palette.handle(DEBRIS_MATERIAL);
    for (centre, half, seed) in crags::footprints() {
        for (index, placement) in placements(centre, half, seed).enumerate() {
            commands.spawn((
                DespawnOnExit(scene),
                Name::new(format!("Escombro {seed:x}/{index}")),
                Mesh3d(pebble.clone()),
                MeshMaterial3d(material.clone()),
                placement.with_ground(ground),
            ));
        }
    }
}

/// Dónde y cómo va una piedra, antes de saber a qué altura está el suelo.
struct Placement {
    xz: Vec2,
    scale: f32,
    yaw: f32,
}

impl Placement {
    fn with_ground(self, ground: Option<&impl super::layout::GroundHeight>) -> Transform {
        let sink = -PEBBLE_RADIUS * self.scale * PEBBLE_BURIED_SHARE;
        Transform::from_translation(settle(
            Vec3::new(self.xz.x, sink, self.xz.y),
            Anchor::Ground,
            ground,
        ))
        .with_scale(Vec3::splat(self.scale))
        .with_rotation(Quat::from_rotation_y(self.yaw))
    }
}

/// Las piedras de un peñasco, repartidas por el contorno. El desorden no es
/// decoración: equiespaciadas sobre la elipse exacta leen como una guarda de
/// jardín, que es la costura que vienen a esconder.
fn placements(centre: Vec3, half: Vec2, seed: u32) -> impl Iterator<Item = Placement> {
    let count = pebble_count(half);
    (0..count).map(move |index| {
        let jitter = |salt: u32| hash_unit(seed.wrapping_add(index as u32 * 7 + salt));
        let step = std::f32::consts::TAU / count as f32;
        let angle = index as f32 * step + (jitter(1) - 0.5) * step * 1.8;
        // Desde adentro del pie hasta bien afuera: las que caen por debajo de 1
        // asoman contra la roca en vez de rodearla, y son las que impiden que la
        // tirada entera se lea como un collar puesto alrededor.
        let reach = elliptic_radius(half, angle) * (0.84 + jitter(2) * 0.4);
        Placement {
            xz: centre.xz() + Vec2::from_angle(angle) * reach,
            scale: 0.7 + jitter(3) * 0.65,
            yaw: jitter(4) * std::f32::consts::TAU,
        }
    })
}

/// Lo que el escombro declara al presupuesto, con la misma cuenta que el spawn.
#[cfg(test)]
pub(crate) fn triangle_count() -> usize {
    // Una subdivisión: 80 triángulos, que es lo que `small_stone_mesh` genera.
    let per_pebble = 80;
    crags::footprints()
        .map(|(_, half, _)| pebble_count(half) * per_pebble)
        .sum()
}
