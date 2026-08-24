//! `Ctrl+R`: dejar el mapa recorrible.
//!
//! El terreno no se escala, así que una pared en el heightmap es un lugar al que
//! el jugador no puede llegar. Este comando la derrite hasta que todo el mapa se
//! pueda recorrer — y los acantilados vuelven después como mallas colocadas
//! encima, que sí se escalan.
//!
//! Es deliberado que sea una tecla y no una migración al cargar: borra relieve
//! autorado, así que la decisión de cuándo aplicarlo —y de guardarlo o no— es del
//! autor. Queda en el historial, así que `Ctrl+Z` lo devuelve.
//!
//! **La tecla no se lee acá.** `Ctrl+R` entra por `persist`, que ya posee la
//! familia `Ctrl+<letra>` del editor: agregar un lector de hardware nuevo haría
//! crecer la deuda C2, y `tests/architecture.rs` sólo la deja encoger.

use bevy::prelude::*;

use super::history::SculptHistory;
use crate::world::Terrain;

/// Tope de pasadas de relajación. Medido el 2026-08-23 sobre `sandbox.ron`: 35
/// pasadas llevan el peor tramo de 68,83 m a 2,36 m. El tope existe para que un
/// relieve patológico no cuelgue el editor, no porque se espere alcanzarlo.
const MAX_REPAIR_PASSES: usize = 400;

pub(super) fn make_traversable(terrain: &mut Terrain, history: &mut SculptHistory) {
    let before = terrain.snapshot();
    let report = terrain.relax_until_traversable(MAX_REPAIR_PASSES);
    if report.passes == 0 {
        info!("[editor] el mapa ya se recorre entero; nada que reparar");
        return;
    }
    history.record_snapshot(before);
    if report.converged() {
        info!(
            "[editor] mapa recorrible: {} pasadas, {} puntos, peor tramo {:.2} m → {:.2} m",
            report.passes, report.moved, report.before, report.after
        );
    } else {
        warn!(
            "[editor] quedó un tramo de {:.2} m tras {} pasadas (tope {MAX_REPAIR_PASSES}); \
             volvé a aplicar Ctrl+R o suavizá esa zona a mano",
            report.after, report.passes
        );
    }
}
