//! **La brizna pertenece al mundo**, no al nivel que la dibuja. Cada baldosa
//! tiene una secuencia determinista: las de índice bajo llegan más lejos y
//! acercarse sólo agrega índices altos. Decisión completa en `BOTWGrass.md`.

use bevy::prelude::*;

use crate::world::forest::{hash_u32, hash_unit};

/// El lado de una baldosa del mundo, en metros.
///
/// Dos y no uno: la baldosa cuantiza la densidad, y el nivel más lejano necesita
/// apenas un par de briznas por m². Con 4 m², redondear su cuenta a un entero
/// cuesta pocos puntos; con 1 m² costaba decenas. Más grande se lee la grilla.
pub(super) const TILE_M: f32 = 2.0;

/// El área de una baldosa, que es el denominador de toda densidad de acá.
pub(super) const TILE_AREA_M2: f32 = TILE_M * TILE_M;

/// Dónde nace una brizna y cuánto mide, **antes** del terreno.
pub(super) struct TileBlade {
    /// Su posición en el mundo, en XZ.
    pub xz: Vec2,
    /// Su altura como fracción del rango autorado, en `[0, 1]`.
    pub height_unit: f32,
}

/// La brizna número `index` de una baldosa.
///
/// La semilla es **la baldosa y el índice, y nada más**: ni el nivel, ni el
/// chunk, ni el orden en que se horneó. Eso es lo que hace que dos niveles que
/// pisan el mismo suelo hablen de las mismas briznas.
pub(super) fn blade_in_tile(tile: IVec2, index: u32) -> TileBlade {
    let seed = hash_u32(
        tile.x.cast_unsigned().wrapping_mul(0x9e37_79b9)
            ^ tile.y.cast_unsigned().wrapping_mul(0x85eb_ca6b)
            ^ index.wrapping_mul(0x0019_6f3d),
    );
    let corner = tile.as_vec2() * TILE_M;
    TileBlade {
        xz: corner + Vec2::new(hash_unit(seed), hash_unit(seed ^ 0x1234_5678)) * TILE_M,
        height_unit: hash_unit(seed ^ 0xdead_beef),
    }
}

/// En qué baldosa cae un punto del mundo.
pub(super) fn tile_at(xz: Vec2) -> IVec2 {
    (xz / TILE_M).floor().as_ivec2()
}

/// Cuántas briznas lleva una baldosa a esta densidad. Redondeado **una sola
/// vez**, acá, para que la cuenta del presupuesto y la de la pantalla sean el
/// mismo número; saturado, para que una densidad absurda no envuelva a una chica
/// y creíble.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a blade count is a non-negative integer bucket, saturated on both ends"
)]
pub(super) fn blades_in_tile(density_per_m2: f32) -> u32 {
    (density_per_m2 * TILE_AREA_M2).round().clamp(0.0, 1e9) as u32
}

/// Hasta dónde se ve cada brizna de una baldosa, por índice.
///
/// **Es la ley de densidad, invertida:** `live_density_at` dice cuántas hacen
/// falta a una distancia, y acá se pregunta hasta qué distancia sigue haciendo
/// falta la número `j`. De ahí sale que la densidad viva sea la que la ley pide,
/// sin escalones. Decreciente por construcción, y un test lo cobra.
pub(super) fn reach_ladder(
    dial: f32,
    scale: f32,
    reach_scale: f32,
    settings: &super::grass::GrassRendererSettings,
) -> Vec<f32> {
    let farthest = super::grass::farthest_reach(reach_scale, settings);
    let nearest = super::grass::NEAREST_INTEREST_M;
    let wanted = |distance: f32| super::grass::live_density_at(distance, dial, scale, settings);
    let count = blades_in_tile(wanted(nearest)) as usize;
    (0..count)
        .map(|index| {
            // Cuántas por m² hacen falta para que **esta** brizna cuente: la
            // baldosa lleva `index + 1` briznas hasta acá.
            let needed = (index + 1) as f32 / TILE_AREA_M2;
            invert_density(needed, nearest, farthest, &wanted)
        })
        .collect()
}

/// La distancia a la que la ley deja de pedir `wanted` briznas por m².
///
/// Bisección y no despeje: la densidad pedida cambia de forma —hoja, púa,
/// carta— en dos escalones, así que no hay una fórmula sola que invertir. Treinta
/// pasos dejan el error muy por debajo del metro sobre 64.
fn invert_density(needed: f32, nearest: f32, farthest: f32, wanted: &dyn Fn(f32) -> f32) -> f32 {
    if wanted(farthest) >= needed {
        return farthest;
    }
    let (mut near, mut far) = (nearest, farthest);
    for _ in 0..30 {
        let middle = f32::midpoint(near, far);
        if wanted(middle) >= needed {
            near = middle;
        } else {
            far = middle;
        }
    }
    near
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIAL: f32 = bof_domain::perf::GRASS_DENSITY_STEPS[0];
    const REACH: f32 = bof_domain::perf::GRASS_REACH_STEPS[0];

    fn ladder() -> Vec<f32> {
        reach_ladder(
            DIAL,
            super::super::grass::reference_scale(),
            REACH,
            &super::super::grass::GrassRendererSettings::default(),
        )
    }

    /// **La propiedad entera del rediseño.** Si acercarse sólo agrega, el
    /// conjunto de briznas vivas a una distancia tiene que contener al de
    /// cualquier distancia mayor — y como una brizna vive mientras su alcance no
    /// se pasó, eso es exactamente que la escalera no suba nunca.
    #[test]
    fn coming_closer_only_ever_adds_blades() {
        let ladder = ladder();
        assert!(ladder.len() > 100, "una escalera corta volvería esto vacuo");
        for (index, pair) in ladder.windows(2).enumerate() {
            assert!(
                pair[0] >= pair[1],
                "la brizna {index} llega menos lejos que la siguiente: {pair:?}",
            );
        }
    }

    /// Y la otra mitad: las que sobreviven a una distancia son **las mismas
    /// briznas**, no otras tantas. Se prueba sobre los índices porque el índice
    /// es la identidad de la brizna dentro de su baldosa.
    #[test]
    fn the_survivors_of_a_far_distance_are_a_prefix_of_the_near_ones() {
        let ladder = ladder();
        let alive = |distance: f32| -> Vec<usize> {
            ladder
                .iter()
                .enumerate()
                .filter(|(_, reach)| **reach >= distance)
                .map(|(index, _)| index)
                .collect()
        };
        for (near, far) in [(3.0, 12.0), (12.0, 30.0), (30.0, 60.0)] {
            let (near, far) = (alive(near), alive(far));
            assert!(far.len() < near.len(), "sin diferencia no hay LOD");
            assert_eq!(far, near[..far.len()], "las lejanas no son un prefijo");
        }
    }

    /// La densidad viva a una distancia tiene que ser **la que la ley pide**: es
    /// lo que hace que este reparto no ralee el campo. Con la cuantización de la
    /// baldosa, dentro de una brizna por baldosa.
    #[test]
    fn the_living_density_is_the_one_the_law_asks_for() {
        let ladder = ladder();
        let scale = super::super::grass::reference_scale();
        for distance in [3.0_f32, 8.0, 16.0, 30.0, 50.0] {
            let alive = ladder.iter().filter(|reach| **reach >= distance).count();
            let live_density = alive as f32 / TILE_AREA_M2;
            let wanted = super::super::grass::live_density_at(
                distance,
                DIAL,
                scale,
                &super::super::grass::GrassRendererSettings::default(),
            );
            assert!(
                (live_density - wanted).abs() <= 1.0 / TILE_AREA_M2 + wanted * 0.02,
                "a {distance} m viven {live_density}/m² y la ley pide {wanted}/m²",
            );
        }
    }

    /// Dos baldosas vecinas no crecen el mismo campo desplazado, y la misma
    /// baldosa lo crece igual siempre — sin eso, caminar y volver reordena la
    /// pradera.
    #[test]
    fn a_tile_grows_the_same_field_every_time_and_its_neighbour_another() {
        let here = blade_in_tile(IVec2::new(3, -7), 5);
        assert_eq!(here.xz, blade_in_tile(IVec2::new(3, -7), 5).xz);
        assert_ne!(
            here.xz,
            blade_in_tile(IVec2::new(4, -7), 5).xz - Vec2::X * TILE_M
        );
        assert_ne!(here.xz, blade_in_tile(IVec2::new(3, -7), 6).xz);
    }

    /// Y cada brizna cae **dentro** de su baldosa: si se saliera, dos baldosas
    /// podrían plantar en el mismo punto y la densidad dejaría de ser la contada.
    #[test]
    fn every_blade_lands_inside_its_own_tile() {
        for index in 0..64 {
            for tile in [IVec2::ZERO, IVec2::new(-5, 12), IVec2::new(40, -3)] {
                let blade = blade_in_tile(tile, index);
                assert_eq!(
                    tile_at(blade.xz),
                    tile,
                    "la brizna {index} se fue de {tile}"
                );
            }
        }
    }
}
