//! La meseta del acantilado: el relieve que hace que arriba haya terreno, porque
//! un acantilado es el borde de una tierra alta y no una roca sobre el pasto.
//! Autoría y no arranque —el `.ron` es el nivel—, así que se corre a mano y con
//! los pinceles del editor. Fondo en `docs/CLIFFS.md`.

use bevy::prelude::*;

use crate::world::Terrain;

/// Centro de la mesa, al sur de la pared: la roca tapa su borde norte.
const MESA_CENTRE: Vec2 = Vec2::new(0.0, -40.0);
/// Radio de la mesa. La falda del pincel ocupa buena parte: el interior queda
/// llano y el borde baja solo, que es la bajada gradual que se busca.
const MESA_RADIUS: f32 = 16.0;
/// Altura de la tierra alta, absoluta: es lo que deja que la roca declare su
/// coronación y las dos empalmen. Los 12 m salen de medir `sandbox.ron`, donde
/// una mesa de 7 dejaba una pared de 3,4 m.
const MESA_HEIGHT: f32 = 12.0;
/// Pasadas del pincel de aplanar. Con muchas, cualquier punto con falloff > 0
/// termina en la altura objetivo y la mesa se vuelve una torta de bordes
/// rectos; con tres, la falda conserva la forma del falloff.
const MESA_PASSES: usize = 3;

/// Esculpe la meseta sobre el relieve que ya haya. La bajada no necesita pincel
/// propio: sale del falloff del aplanado. Una rampa aparte —el primer intento—
/// alcanzaba la mesa por su propio radio y la hundía.
pub(crate) fn author_plateau(terrain: &mut Terrain) {
    for _ in 0..MESA_PASSES {
        terrain.flatten_area(MESA_CENTRE, MESA_RADIUS, MESA_HEIGHT, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La mesa llega a su altura y la falda baja: sin las dos cosas no es una
    /// meseta, es un escalón o una loma.
    #[test]
    fn the_mesa_is_high_in_the_middle_and_lower_at_its_skirt() {
        let mut terrain = Terrain::flat_for_test();
        author_plateau(&mut terrain);
        let top = terrain.height_at(MESA_CENTRE);
        let skirt = terrain.height_at(MESA_CENTRE + Vec2::new(0.0, -MESA_RADIUS * 0.75));
        let outside = terrain.height_at(MESA_CENTRE + Vec2::new(0.0, -MESA_RADIUS * 2.0));
        assert!(top > MESA_HEIGHT * 0.9, "la mesa se quedó en {top} m");
        assert!(
            skirt < top * 0.8,
            "la falda ({skirt} m) no baja desde {top} m"
        );
        assert!(
            outside.abs() < 1.5,
            "la cola no llegó al llano: {outside} m"
        );
    }

    /// **Lo que hace que el acantilado sea acantilado.** Bajo la pared el
    /// terreno tiene que subir hacia la mesa: si fuera llano, la roca volvería a
    /// ser un peñasco apoyado y arriba no habría dónde caminar.
    #[test]
    fn the_ground_climbs_under_the_wall() {
        let mut terrain = Terrain::flat_for_test();
        author_plateau(&mut terrain);
        let north = terrain.height_at(Vec2::new(0.0, -23.0));
        let south = terrain.height_at(Vec2::new(0.0, -29.0));
        assert!(
            south > north + 1.0,
            "el suelo bajo la pared no sube: {north} m al norte contra {south} al sur"
        );
    }
}

#[cfg(test)]
mod authoring {
    use super::*;

    /// **Esculpe la meseta en los niveles del disco.** No corre con la suite:
    /// es una tarea de una vez, se pide a mano y reescribe assets autorados.
    ///
    /// ```text
    /// cargo test -p breath-of-freedom author_the_cliff_plateau_on_disk -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "reescribe los .ron de Terreno y Mundo; se corre a mano"]
    fn author_the_cliff_plateau_on_disk() {
        // Sólo Terreno: es la escena de autoría de relieve, y la única cuyo
        // nivel existe en disco. Crear el de Mundo cambió su suelo de pasto a
        // tierra, porque un archivo nuevo nace sin capa semántica pintada.
        for path in ["assets/game/world/sandbox.ron"] {
            let mut terrain = Terrain::flat_for_test();
            if let Ok(text) = std::fs::read_to_string(path) {
                terrain.apply_ron(&text).expect("el nivel parsea");
            }
            author_plateau(&mut terrain);
            let ron = terrain.to_ron().expect("el nivel serializa");
            std::fs::write(path, ron).expect("el nivel se escribe");
            println!(
                "[plateau] {path}: mesa {:.2} m, bajo la pared {:.2} → {:.2} m",
                terrain.height_at(MESA_CENTRE),
                terrain.height_at(Vec2::new(0.0, -23.0)),
                terrain.height_at(Vec2::new(0.0, -29.0)),
            );
        }
    }
}
