//! ¿Puede el jugador pasar por el relieve que el editor acaba de autorar?
//!
//! El terreno no se escala, así que lo empinado se pasa con vault o mantle, y
//! los dos están limitados por **altura de cara**, no por pendiente. Una
//! contrahuella empinada de 2 m es una terraza; el mismo ángulo sostenido 20 m
//! es una pared, y en un mundo sin escalada de terreno una pared es un lugar al
//! que el jugador no puede ir.
//!
//! Por eso lo que se mide no es el ángulo de una cara suelta sino la **subida
//! acumulada de un tramo empinado continuo**: celdas vecinas seguidas cuya
//! pendiente local pasa el límite caminable.

use bevy_math::prelude::Vec2;
use bof_domain::world::MAX_UNWALKABLE_RISE_METRES;

use super::Terrain;
use crate::movement::motor_common::WALKABLE_LIMIT_DEG;

/// El peor tramo empinado que encontró un barrido, y dónde estaba.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SteepestRun {
    /// Subida acumulada del tramo, en metros.
    pub rise: f32,
    /// Índice de grilla `(fila, columna)` donde arranca.
    pub start: (usize, usize),
}

impl SteepestRun {
    pub fn is_traversable(self) -> bool {
        self.rise <= MAX_UNWALKABLE_RISE_METRES
    }
}

impl Terrain {
    /// El tramo empinado continuo más alto de toda la grilla.
    ///
    /// Barre **los dos ejes**: una pared perfectamente alineada con las filas
    /// es invisible para un barrido que sólo recorra columnas.
    pub fn steepest_run(&self) -> SteepestRun {
        self.steepest_run_within(0..self.points, 0..self.points)
    }

    /// Lo mismo, acotado a una ventana de grilla — lo que un trazo puede haber
    /// tocado, que es lo único que hace falta recalcular mientras se esculpe.
    pub fn steepest_run_within(
        &self,
        rows: std::ops::Range<usize>,
        cols: std::ops::Range<usize>,
    ) -> SteepestRun {
        let mut worst = SteepestRun::default();
        for row in rows.clone() {
            let along_columns = |col: usize| (row, col);
            self.scan_line(cols.clone(), along_columns, &mut worst);
        }
        for col in cols.clone() {
            let along_rows = |row: usize| (row, col);
            self.scan_line(rows.clone(), along_rows, &mut worst);
        }
        worst
    }

    /// Recorre una línea de la grilla acumulando la subida mientras los pasos
    /// sigan siendo demasiado empinados para caminarlos, en los dos sentidos:
    /// una bajada empinada es la misma pared vista desde arriba.
    fn scan_line(
        &self,
        line: std::ops::Range<usize>,
        at: impl Fn(usize) -> (usize, usize),
        worst: &mut SteepestRun,
    ) {
        let step_limit = self.spacing() * WALKABLE_LIMIT_DEG.to_radians().tan();
        let mut climbing = 0.0_f32;
        let mut descending = 0.0_f32;
        let mut climb_start = 0;
        let mut descent_start = 0;
        for index in line.clone().skip(1) {
            let (row, col) = at(index);
            let (prev_row, prev_col) = at(index - 1);
            let step = self.height(row, col) - self.height(prev_row, prev_col);
            if step > step_limit {
                if climbing == 0.0 {
                    climb_start = index - 1;
                }
                climbing += step;
                worst.take_if_worse(climbing, at(climb_start));
            } else {
                climbing = 0.0;
            }
            if -step > step_limit {
                if descending == 0.0 {
                    descent_start = index - 1;
                }
                descending -= step;
                worst.take_if_worse(descending, at(descent_start));
            } else {
                descending = 0.0;
            }
        }
    }

    /// El peor tramo empinado **cerca** de un punto del mundo, mirando una
    /// ventana del doble del radio pedido.
    ///
    /// Es una lectura local a propósito: contesta "¿qué acabo de hacer acá?",
    /// que es lo que el autor necesita mientras esculpe. Un tramo que siga más
    /// allá de la ventana se reporta recortado — la respuesta del mapa entero
    /// es [`Terrain::steepest_run`], y cuesta un barrido completo.
    pub fn steepest_run_near(&self, centre: Vec2, radius: f32) -> SteepestRun {
        let (rows, cols) = self.window(centre, centre, radius * 2.0);
        self.steepest_run_within(
            *rows.start()..*rows.end() + 1,
            *cols.start()..*cols.end() + 1,
        )
    }

    pub(super) fn spacing(&self) -> f32 {
        self.extent / self.cells() as f32
    }

    /// Marca cada punto que forma parte de un tramo empinado **intransitable**,
    /// y devuelve cuántos son.
    ///
    /// Marca el tramo entero y no sólo su peor paso: erosionar un escalón suelto
    /// de una pared de 45 m sólo la vuelve una pared de 44 m.
    fn mark_unreachable(&self, marks: &mut [bool]) -> usize {
        marks.fill(false);
        let step_limit = self.spacing() * WALKABLE_LIMIT_DEG.to_radians().tan();
        let points = self.points;
        for line in 0..points {
            for along_rows in [false, true] {
                let at = |i: usize| if along_rows { (i, line) } else { (line, i) };
                self.mark_line(points, step_limit, at, marks);
            }
        }
        marks.iter().filter(|marked| **marked).count()
    }

    fn mark_line(
        &self,
        points: usize,
        step_limit: f32,
        at: impl Fn(usize) -> (usize, usize),
        marks: &mut [bool],
    ) {
        let mut run_start = 0;
        let mut rise = 0.0_f32;
        let mut direction = 0_i8;
        for index in 1..points {
            let (row, col) = at(index);
            let (prev_row, prev_col) = at(index - 1);
            let step = self.height(row, col) - self.height(prev_row, prev_col);
            let way = match step {
                _ if step.abs() <= step_limit => 0,
                _ if step > 0.0 => 1,
                _ => -1,
            };
            if way == 0 || way != direction {
                self.flush_run(&at, run_start, index - 1, rise, marks);
                run_start = index - 1;
                rise = 0.0;
                direction = way;
            }
            if way != 0 {
                rise += step.abs();
            }
        }
        self.flush_run(&at, run_start, points - 1, rise, marks);
    }

    fn flush_run(
        &self,
        at: &impl Fn(usize) -> (usize, usize),
        from: usize,
        to: usize,
        rise: f32,
        marks: &mut [bool],
    ) {
        if rise <= MAX_UNWALKABLE_RISE_METRES {
            return;
        }
        for index in from..=to {
            let (row, col) = at(index);
            marks[row * self.points + col] = true;
        }
    }

    /// Derrite el relieve intransitable hasta que el mapa se pueda recorrer
    /// entero.
    ///
    /// **La región afectada crece sola, y tiene que hacerlo.** Un desnivel de
    /// 40 m necesita 40 m de horizontal para volverse caminable, así que relajar
    /// únicamente las celdas empinadas no alcanza: la banda es más angosta que
    /// la rampa que hay que construir, y la relajación se estanca contra sus
    /// propios bordes fijos. Por eso lo marcado se **acumula y se dilata** una
    /// celda por pasada, hasta que la región es lo bastante ancha para contener
    /// la ladera.
    ///
    /// Lo que ya se andaba nunca entra: una terraza o una loma no forman tramo
    /// intransitable, así que salen intactas. Lo que se va son las paredes —
    /// lugares a los que nadie puede llegar mientras el terreno no se escale— y
    /// vuelven después como mallas colocadas encima, que sí se escalan.
    pub fn relax_until_traversable(&mut self, max_passes: usize) -> RepairReport {
        let before = self.steepest_run().rise;
        let mut marks = vec![false; self.heights.len()];
        let mut affected = vec![false; self.heights.len()];
        let mut report = RepairReport {
            before,
            ..RepairReport::default()
        };
        for pass in 1..=max_passes {
            if self.mark_unreachable(&mut marks) == 0 {
                report.passes = pass - 1;
                report.moved = affected.iter().filter(|touched| **touched).count();
                report.after = self.steepest_run().rise;
                return report;
            }
            self.grow_affected(&marks, &mut affected);
            self.relax_marked(&affected, pass % 2 == 0);
            self.relief_revision = self.relief_revision.wrapping_add(1);
            report.passes = pass;
        }
        report.moved = affected.iter().filter(|touched| **touched).count();
        report.after = self.steepest_run().rise;
        report
    }

    /// Suma lo recién marcado a la región afectada y la ensancha una celda.
    fn grow_affected(&self, marks: &[bool], affected: &mut [bool]) {
        let points = self.points;
        let previous = affected.to_vec();
        for row in 0..points {
            for col in 0..points {
                let idx = row * points + col;
                if marks[idx] {
                    affected[idx] = true;
                    continue;
                }
                let touches_affected = [
                    (row > 0, idx.wrapping_sub(points)),
                    (row + 1 < points, idx + points),
                    (col > 0, idx.wrapping_sub(1)),
                    (col + 1 < points, idx + 1),
                ]
                .into_iter()
                .any(|(inside, neighbour)| inside && previous[neighbour]);
                if touches_affected {
                    affected[idx] = true;
                }
            }
        }
    }

    /// Una pasada de relajación sobre los puntos marcados, **en sitio**.
    ///
    /// Los puntos sin marcar hacen de borde fijo, así que esto resuelve un
    /// Laplace con Dirichlet: lo marcado se derrite hacia lo que lo rodea, que
    /// es lo que hace que una pared se convierta en ladera en vez de hundir el
    /// mapa entero. Escribir en sitio y alternar el sentido del barrido propaga
    /// la corrección a lo largo de la pasada en vez de una celda por pasada; el
    /// paso mayor que uno es sobrerrelajación, estable mientras no llegue a 2.
    fn relax_marked(&mut self, marks: &[bool], reversed: bool) {
        const OVER_RELAXATION: f32 = 1.8;
        let spacing = self.spacing();
        let points = self.points;
        for step in 0..marks.len() {
            let idx = if reversed {
                marks.len() - 1 - step
            } else {
                step
            };
            if !marks[idx] {
                continue;
            }
            let average = super::sculpt::neighbour_average(&self.heights, points, spacing, idx);
            self.heights[idx] += OVER_RELAXATION * (average - self.heights[idx]);
        }
    }
}

/// Lo que hizo una reparación, para que el editor pueda decirlo en voz alta.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RepairReport {
    pub passes: usize,
    /// Puntos marcados en la peor pasada.
    pub moved: usize,
    pub before: f32,
    pub after: f32,
}

impl RepairReport {
    pub fn converged(self) -> bool {
        self.after <= MAX_UNWALKABLE_RISE_METRES
    }
}

impl SteepestRun {
    fn take_if_worse(&mut self, rise: f32, start: (usize, usize)) {
        if rise > self.rise {
            self.rise = rise;
            self.start = start;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Una grilla con un perfil de alturas puesto a mano sobre las columnas,
    /// constante a lo largo de las filas.
    ///
    /// La cola sostiene la última altura hasta el borde a propósito: si el
    /// perfil volviera a cero, cada test estaría midiendo el acantilado de su
    /// propio andamio en vez de la forma que quiso describir.
    fn terrain_from_column_profile(profile: &[f32]) -> Terrain {
        let mut terrain = Terrain::flat();
        let points = terrain.points();
        let mid = points / 2;
        for col in mid..points {
            let height = profile[(col - mid).min(profile.len() - 1)];
            for row in 0..points {
                terrain.heights[row * points + col] = height;
            }
        }
        terrain
    }

    #[test]
    fn flat_ground_has_no_steep_run() {
        assert_eq!(Terrain::flat().steepest_run().rise, 0.0);
    }

    /// Una contrahuella sola es lo que el mantle sube: no puede contar como
    /// pared por más vertical que sea.
    #[test]
    fn a_single_riser_is_its_own_height() {
        let terrain = terrain_from_column_profile(&[0.0, 2.0, 2.0, 2.0]);
        let worst = terrain.steepest_run();
        assert!((worst.rise - 2.0).abs() < 1e-4, "midió {}", worst.rise);
        assert!(worst.is_traversable());
    }

    /// **La diferencia con medir el ángulo.** Cada escalón de esta escalera es
    /// tan vertical como una pared, y la escalera se sube igual: hay huella
    /// caminable entre uno y otro, así que ningún tramo empinado se encadena.
    #[test]
    fn a_staircase_never_accumulates_across_its_treads() {
        let terrain = terrain_from_column_profile(&[0.0, 2.0, 2.0, 4.0, 4.0, 6.0, 6.0]);
        let worst = terrain.steepest_run();
        assert!((worst.rise - 2.0).abs() < 1e-4, "midió {}", worst.rise);
        assert!(
            worst.is_traversable(),
            "una escalera de contrahuellas mantleables es transitable, no una pared"
        );
    }

    /// Y la misma altura sin huellas sí es una pared.
    #[test]
    fn the_same_rise_without_treads_is_a_wall() {
        let terrain = terrain_from_column_profile(&[0.0, 2.0, 4.0, 6.0]);
        let worst = terrain.steepest_run();
        assert!((worst.rise - 6.0).abs() < 1e-4, "midió {}", worst.rise);
        assert!(!worst.is_traversable());
    }

    /// Una pendiente sostenida bajo el límite caminable no acumula nada, aunque
    /// suba treinta metros.
    #[test]
    fn a_long_walkable_slope_is_not_a_run() {
        let gentle = Terrain::flat().spacing() * (WALKABLE_LIMIT_DEG - 5.0).to_radians().tan();
        let profile: Vec<f32> = (0..80).map(|i| i as f32 * gentle).collect();
        let worst = terrain_from_column_profile(&profile).steepest_run();
        assert_eq!(
            worst.rise, 0.0,
            "midió {} sobre terreno caminable",
            worst.rise
        );
    }

    /// Bajar una pared es la misma pared: el jugador que cae 20 m tampoco
    /// puede volver.
    #[test]
    fn a_drop_counts_as_much_as_a_climb() {
        let up = terrain_from_column_profile(&[0.0, 3.0, 6.0, 9.0]).steepest_run();
        let down = terrain_from_column_profile(&[9.0, 6.0, 3.0, 0.0]).steepest_run();
        assert!(
            (up.rise - down.rise).abs() < 1e-4,
            "{} vs {}",
            up.rise,
            down.rise
        );
    }

    /// Un barrido que sólo recorra columnas no ve una pared alineada con ellas.
    #[test]
    fn a_wall_along_the_other_axis_is_still_found() {
        let mut terrain = Terrain::flat();
        let points = terrain.points();
        let mid = points / 2;
        for row in mid..points {
            let height = [0.0_f32, 4.0, 8.0][(row - mid).min(2)];
            for col in 0..points {
                terrain.heights[row * points + col] = height;
            }
        }
        assert!(
            !terrain.steepest_run().is_traversable(),
            "la pared existe aunque corra a lo largo de las filas"
        );
    }

    /// **La reparación sobre el terreno real**, que es el único caso que
    /// importa: `sandbox.ron` tiene un tramo de 68,83 m y hay que poder dejarlo
    /// recorrible sin tocar lo que ya se anda.
    #[test]
    fn repairing_the_authored_terrain_makes_it_traversable() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/game/world/sandbox.ron"
        );
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("[terrain] {path} missing — skipping");
            return;
        };
        let mut terrain = Terrain::flat();
        terrain
            .apply_ron(&text)
            .expect("el terreno autorado parsea");
        let report = terrain.relax_until_traversable(400);
        println!(
            "[terrain] reparación: {} pasadas, {} puntos, {:.2} m → {:.2} m",
            report.passes, report.moved, report.before, report.after
        );
        assert!(
            report.converged(),
            "quedó un tramo de {:.2} m después de {} pasadas",
            report.after,
            report.passes
        );
    }

    /// Y no se lleva puesto lo que ya era transitable: una escalera de
    /// contrahuellas mantleables sale intacta.
    #[test]
    fn repairing_leaves_traversable_relief_untouched() {
        let mut terrain = terrain_from_column_profile(&[0.0, 2.0, 2.0, 4.0, 4.0, 6.0, 6.0]);
        let before = terrain.clone();
        let report = terrain.relax_until_traversable(400);
        assert_eq!(report.passes, 0, "no había nada que reparar");
        let points = terrain.points();
        for row in (0..points).step_by(37) {
            for col in 0..points {
                assert!(
                    (terrain.height(row, col) - before.height(row, col)).abs() < 1e-6,
                    "la reparación movió ({row}, {col}), que ya se andaba"
                );
            }
        }
    }

    /// **Arregla el archivo del nivel en el disco.** No corre con la suite: es
    /// una tarea de una vez, se pide a mano y reescribe un asset autorado.
    ///
    /// ```text
    /// cargo test -p breath_of_freedom_simulation repair_the_level_file_on_disk -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "reescribe assets/game/world/sandbox.ron; se corre a mano"]
    fn repair_the_level_file_on_disk() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/game/world/sandbox.ron"
        );
        let text = std::fs::read_to_string(path).expect("el nivel existe");
        let mut terrain = Terrain::flat();
        terrain.apply_ron(&text).expect("el nivel parsea");

        let report = terrain.relax_until_traversable(400);
        assert!(
            report.converged(),
            "quedó un tramo de {:.2} m tras {} pasadas",
            report.after,
            report.passes
        );

        let ron = terrain.to_ron().expect("el nivel serializa");
        std::fs::write(path, ron).expect("el nivel se escribe");
        println!(
            "[terrain] {path}\n  {} pasadas · {} puntos movidos · peor tramo {:.2} m → {:.2} m",
            report.passes, report.moved, report.before, report.after
        );
    }
}
