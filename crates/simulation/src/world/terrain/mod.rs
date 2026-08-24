//! The terrain: the ground as **editable data**, and the single source of truth
//! the physics collider, the visual mesh and the footstep audio all derive from.
//!
//! Two channels, which is what a map needs to be more than a shape:
//! - **relief** — the height grid, in metres, on the grid's corners.
//! - **meaning** — what each cell is *made of* ([`TerrainKind`]), on the quads.
//!
//! Data lives here in `world`; presentation generates the flat-shaded visual and
//! the application owns the editor. One grid feeds several representations via
//! change detection: mutating [`Terrain`] re-triggers the mesh rebuild, and the
//! collider rebuild when the *relief* specifically moved
//! ([`Terrain::relief_revision`]).
//!
//! **The grid owns *how* it changes.** Every brush the editor offers is a method
//! here; the editor only decides *where and when* one fires. That split is what
//! keeps a new brush from needing a new system — it is a closure handed to
//! its shared brush traversal, or [`Terrain::paint_area`] for semantics.

use std::path::Path;

use avian3d::prelude::*;
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use bevy_log::{info, warn};
use bevy_math::prelude::*;
use bevy_transform::prelude::*;
use serde::{Deserialize, Serialize};

use super::NonClimbable;
use super::terrain_kind::TerrainKind;
use bof_domain::props::Instance;
use bof_domain::world::WORLD_SIZE;

/// Grid cells per side; the heightfield has `CELLS + 1` points per side.
///
/// **The number that matters is metres per cell, and it comes from the body
/// that walks on it — not from how big the world is.** At `WORLD_SIZE / CELLS`
/// = 0.5 m, two constraints already in this codebase are what fix it:
///
/// - `editor::MIN_RADIUS` is 2 m, the smallest brush. A brush needs about four
///   points across to shape anything; below that it moves one or two vertices
///   and produces the spiky tents that ruled out `CELLS = 64`.
/// - `Terrain::slope_deg_at` takes its finite difference over a fixed 0.5 m.
///   That step has to stay at or under one cell, or the function silently stops
///   estimating the local gradient and starts estimating a cell-to-cell
///   difference — and it feeds grass seeding, so the change would show up as
///   the meadow moving.
///
/// The old 2.5 m was set against a flat graybox, where nothing needed detail
/// finer than the brush. It made a vault ledge (0.3–1.4 m of relief)
/// impossible to author at all.
///
/// The cost is declared, not hidden: 819,200 triangles, unchunked and without
/// LOD, always in frame. It has its own ceiling in `perf::budget`, out of the
/// per-scene sum, for the same reason the meadow left it.
const CELLS: usize = 640;

// Each scene names its own heightmap. **That file is the level**: the editor
// writes it and `spawn_terrain` loads it on entry, so a scene starts on the
// ground the last session shaped — and sculpting one scene cannot disturb
// another. Which file belongs to which scene is composition, so the app looks
// it up in its own table and passes the path here.

/// Heights are clamped to this band. Not a design limit — a guard so a stuck
/// mouse button cannot push the ground somewhere the camera and the physics
/// solver stop making sense.
const MIN_HEIGHT: f32 = -60.0;
const MAX_HEIGHT: f32 = 120.0;

/// How much a raise stroke relaxes *itself*, per metre of height it just added.
///
/// This is the whole reason sculpting feels soft instead of spiky. A pure raise
/// applies `delta * falloff` to the same points every frame, so holding the
/// button integrates the falloff curve into a tent with a sharp apex. Bleeding a
/// little smoothing into each application keeps the dome round while it grows,
/// which is what "one continuous stroke" is supposed to look like.
const RELAX_PER_METRE: f32 = 6.0;
/// Ceiling on that per-application relax, so a big single step cannot flatten
/// the area it was meant to lift.
const MAX_RELAX_PER_STEP: f32 = 0.3;

/// World size of one value-noise cell, in metres. Bigger than the 2.5 m grid
/// spacing on purpose: per-vertex randomness reads as salt-and-pepper, while
/// interpolating over ~12 m reads as terrain.
const NOISE_CELL: f32 = 12.0;

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to a finite non-negative grid bound before truncation"
)]
fn bounded_grid_index(value: f32, last: usize) -> usize {
    value.clamp(0.0, last as f32) as usize
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "noise coordinates are finite authored world positions; signed bits intentionally wrap"
)]
fn noise_coordinate(value: f32) -> u32 {
    (value.floor() as i32).cast_unsigned()
}

/// The ground as editable data: a height grid plus what each cell is **made
/// of**. Authoritative: the collider and the visual mesh are both derived from
/// it. Row-major `points × points`, where the row indexes X and the column
/// indexes Z (parry heightfield convention).
///
/// Two channels on one grid, because they answer two different questions and are
/// sampled differently. Heights live on the `points × points` **corners** and
/// interpolate — the surface between them is a real slope. Kinds live on the
/// `cells × cells` **quads** and do not: a patch of ground is stone or it is
/// sand, and there is no meaningful halfway.
#[derive(Component, Debug, Clone)]
pub struct Terrain {
    /// Heights in world units, row-major (`row * points + col`).
    heights: Vec<f32>,
    /// What each cell is made of, row-major (`row * cells + col`).
    kinds: Vec<TerrainKind>,
    /// Bumped every time [`Terrain::heights`] actually changes.
    ///
    /// `Changed<Terrain>` cannot tell the two channels apart, and the physics
    /// collider is derived from the heights alone. Without this, painting a patch
    /// of sand would rebuild a 16k-point heightfield that is bit-identical to the
    /// one already there — the most expensive thing this module does, for nothing.
    relief_revision: u32,
    /// Discrete objects an author placed on this terrain — never a model, a
    /// path or a material, just what and where. See [`bof_domain::props`].
    instances: Vec<Instance>,
    /// Bumped every time [`Terrain::instances`] actually changes.
    ///
    /// Same reason as [`Terrain::relief_revision`]: `Changed<Terrain>` fires on
    /// every sculpt or paint stroke too, and presentation's instance sync has
    /// to tell "an instance moved" from "the ground under it moved" without
    /// paying to rebuild every placed prop on every brush frame.
    instances_revision: u32,
    /// Bumped only when [`Terrain::instances`] is **replaced wholesale**,
    /// today only by [`Terrain::apply_ron`] — a `Ctrl+L` reload can grow the
    /// list too, but every entry may have moved. `instances_revision` alone
    /// cannot tell that apart from one prop appended to what was already
    /// there; presentation needs this second signal to know which happened.
    instances_generation: u32,
    /// Points per side (`CELLS + 1`).
    points: usize,
    /// World size spanned on each of X and Z.
    extent: f32,
}

/// The on-disk shape of a level. Resolution and extent travel with the data so a
/// file authored before a `CELLS` or `WORLD_SIZE` change still loads —
/// [`Terrain::apply_ron`] resamples in world space instead of rejecting.
#[derive(Default, Serialize, Deserialize)]
struct TerrainFile {
    points: usize,
    extent: f32,
    heights: Vec<f32>,
    /// The semantic layer, run-length encoded, cell-major.
    ///
    /// Run-length because a painted map is mostly uniform: a fresh level is a
    /// single run, and a level with a meadow on it is a few hundred. That keeps
    /// the one part of the file a human can actually read *readable* — you can
    /// see at a glance that a level is all soil except one patch — next to 16k
    /// heights that will never be anything but noise.
    ///
    /// `default` on purpose: levels saved before this layer existed have no
    /// field, and an empty list means "all the default kind" rather than an
    /// error. That is what keeps the terrain files already on disk loadable.
    #[serde(default)]
    kinds: Vec<KindRun>,
    /// The instance layer: "here is a `kind`" as rows of world-space data.
    ///
    /// `default` for the same reason as `kinds`: the five levels saved before
    /// this layer existed have no field, and an empty list is the honest
    /// reading of "nobody placed anything yet". Never resampled — instances
    /// are already in metres, unlike `heights`.
    #[serde(default)]
    instances: Vec<Instance>,
}

/// Read-only seam for terrain queries outside the terrain owner.
///
/// The current world has one grid, but consumers ask this service for samples
/// or entity membership instead of encoding that cardinality with
/// `Query::single`. A future chunk lookup therefore changes here, not in every
/// player, movement, editor and presentation system.
#[derive(SystemParam)]
pub struct TerrainAccess<'w, 's> {
    terrains: Query<'w, 's, (Entity, &'static Terrain)>,
    changed: Query<'w, 's, (Entity, &'static Terrain), Changed<Terrain>>,
}

impl TerrainAccess<'_, '_> {
    pub fn height_at(&self, world_xz: Vec2) -> Option<f32> {
        self.primary().map(|terrain| terrain.height_at(world_xz))
    }

    pub fn kind_at(&self, world_xz: Vec2) -> Option<TerrainKind> {
        self.primary().map(|terrain| terrain.kind_at(world_xz))
    }

    pub fn slope_deg_at(&self, world_xz: Vec2) -> Option<f32> {
        self.primary().map(|terrain| terrain.slope_deg_at(world_xz))
    }

    pub fn contains(&self, entity: Entity) -> bool {
        self.terrains.contains(entity)
    }

    pub fn primary(&self) -> Option<&Terrain> {
        self.terrains.single().ok().map(|(_, terrain)| terrain)
    }

    pub fn changed(&self) -> impl Iterator<Item = (Entity, &Terrain)> {
        self.changed.iter()
    }
}

/// `(how many cells, of what)`. Serialises as `(1234, Soil)`.
#[derive(Serialize, Deserialize)]
struct KindRun(u32, TerrainKind);

/// A copy of both channels, for the editor's undo history.
///
/// Opaque on purpose: only [`Terrain::restore`] can put one back, so history
/// cannot invent a grid the terrain never had. It carries the semantic layer too
/// — undo has to take back a paint stroke as readily as a sculpt one, and a
/// snapshot of half the data would silently resurrect the other half.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainSnapshot {
    heights: Vec<f32>,
    kinds: Vec<TerrainKind>,
}

fn grid_xz(points: usize, extent: f32, idx: usize) -> Vec2 {
    let last = (points - 1) as f32;
    Vec2::new(
        ((idx / points) as f32 / last - 0.5) * extent,
        ((idx % points) as f32 / last - 0.5) * extent,
    )
}

fn sample_bilinear(heights: &[f32], points: usize, row: f32, col: f32) -> f32 {
    let last = points - 1;
    let r0 = bounded_grid_index(row.floor(), last);
    let c0 = bounded_grid_index(col.floor(), last);
    let r1 = (r0 + 1).min(last);
    let c1 = (c0 + 1).min(last);
    let fr = row - r0 as f32;
    let fc = col - c0 as f32;
    let top = heights[r0 * points + c0].lerp(heights[r0 * points + c1], fc);
    let bottom = heights[r1 * points + c0].lerp(heights[r1 * points + c1], fc);
    top.lerp(bottom, fr)
}

pub mod instances;
pub mod persist;
pub mod query;
pub mod sculpt;
pub mod traversable;

impl Terrain {
    fn flat() -> Self {
        let points = CELLS + 1;
        Self {
            heights: vec![0.0; points * points],
            kinds: vec![TerrainKind::default(); CELLS * CELLS],
            relief_revision: 0,
            instances: Vec::new(),
            instances_revision: 0,
            instances_generation: 0,
            points,
            extent: WORLD_SIZE,
        }
    }

    /// A flat grid for tests in other crates (the editor's history tests, the
    /// terrain mesh tests and the polygon budget all need one, and only this
    /// module may construct a `Terrain`). Not `#[cfg(test)]`: a `cfg` flag does
    /// not cross a crate boundary, so the fixture has to ship to be usable.
    pub fn flat_for_test() -> Self {
        Self::flat()
    }

    /// The heightfield collider derived from the grid. `scale` maps parry's unit
    /// rectangle to the world extent; heights pass through at `scale.y = 1`.
    ///
    /// **This function is why the cell diagonal is not ours to choose.** Which way a
    /// quad is cut into two triangles is parry's decision, and everything that reads
    /// this grid has to follow it.
    ///
    /// A quad's four corners are only coplanar on flat or uniformly sloping ground.
    /// Anywhere the relief twists, the two possible diagonals describe two *different
    /// surfaces*, and the collider is authoritative because it is the one bodies
    /// stand on.
    ///
    /// `parry3d` cuts along the **anti-diagonal** — from `(row, col+1)` to
    /// `(row+1, col)`. Its `HeightField::triangles_at` picks the main diagonal only
    /// for cells flagged `ZIGZAG_SUBDIVISION`, and the default cell status is empty
    /// (`heightfield3.rs`: `Array2::repeat(.., HeightFieldCellStatus::default())`),
    /// so no cell is ever flagged. Nothing in our code path sets it, and reaching it
    /// would need `parry` as a direct dependency.
    ///
    /// Found by playing: the collider visibly disagreed with the ground "where the
    /// terrain is very uneven", which is exactly the shape of this bug. Measured
    /// before the fix on hard-sculpted ground: **0.33 m at worst, and 36% of sampled
    /// points off by more than a centimetre**. It cost nothing on the flat graybox
    /// floor this code was written against, which is why it survived this long — and
    /// why the test that guards it now sweeps a grid instead of six points.
    fn to_collider(&self) -> Collider {
        let rows: Vec<Vec<f32>> = (0..self.points)
            .map(|row| (0..self.points).map(|col| self.height(row, col)).collect())
            .collect();
        Collider::heightfield(rows, Vec3::new(self.extent, 1.0, self.extent))
    }
}

/// Tracks which version of the relief the collider on this entity was built
/// from, so the rebuild can tell a sculpt edit from a paint edit.
#[derive(Component)]
pub(super) struct ColliderRevision(u32);

/// Rebuild the heightfield collider whenever the **relief** changes. Reacts to
/// `Changed`, which also fires the frame the terrain spawns — rebuilding once
/// more then is harmless.
///
/// `Changed<Terrain>` is necessary but not sufficient: the semantic layer shares
/// the component and changes far more often than the heights do while painting,
/// and this rebuild is the single most expensive thing an edit can trigger. The
/// revision check is what keeps painting off the physics path entirely.
pub(super) fn rebuild_terrain_collider(
    mut terrain: Query<(&Terrain, &mut Collider, &mut ColliderRevision), Changed<Terrain>>,
) {
    for (terrain, mut collider, mut built) in &mut terrain {
        if built.0 == terrain.relief_revision() {
            continue;
        }
        built.0 = terrain.relief_revision();
        *collider = terrain.to_collider();
    }
}

/// Spawn the single terrain entity: the height grid plus its derived collider.
/// Static world geometry, so it stays on the `Default` collision layer (no
/// `CollisionLayers`) where ledge sensing can see it.
///
/// No `Surface` component, unlike every other piece of ground. One component
/// could only name one surface for all 320 m of terrain, which is exactly the
/// thing the semantic layer exists to stop being true; the ground probe reads
/// [`Terrain::kind_at`] at the contact point instead. Leaving a stale `Surface`
/// on the entity would have given the probe two answers to choose from.
///
/// The saved level, if there is one, *is* the starting ground — a missing file
/// is the normal first-run case, not an error.
pub fn spawn_terrain(commands: &mut Commands, file: Option<&str>) {
    let mut terrain = Terrain::flat();
    if let Some(file) = file
        && Path::new(file).exists()
    {
        match std::fs::read_to_string(file)
            .map_err(|error| error.to_string())
            .and_then(|text| terrain.apply_ron(&text))
        {
            Ok(()) => info!("[world] terrain loaded from {file}"),
            Err(error) => warn!("[world] terrain load failed ({error}); starting flat"),
        }
    }
    let collider = terrain.to_collider();
    let revision = ColliderRevision(terrain.relief_revision());
    commands.spawn((
        // The lifetime is declared, not bound: the app turns `SceneScoped` into
        // its own state-specific cleanup, so this never names `AppState`.
        bof_domain::scene::SceneScoped,
        Name::new("Terrain"),
        terrain,
        collider,
        revision,
        // A margin gives this thin, one-sided surface virtual thickness for
        // stable depenetration — but it also lifts the effective surface by its
        // own size, and 0.1 m of that is **visible**: the body rests a hand's
        // width above the mesh you can see, and the debug wireframe draws the
        // inflated shape floating over the ground. Kept at the smallest value
        // that still helps the solver; the actual anti-penetration job belongs
        // to `movement::services::ground::lift_actors_out_of_terrain`, which
        // corrects against the real surface instead of hiding under a cushion.
        CollisionMargin(0.02),
        RigidBody::Static,
        Transform::default(),
        // **El suelo no se escala.** Un heightmap no puede plegarse, así que no
        // representa una vertical: lo que parecía pared siempre fue una rampa
        // del ancho de una celda. Lo escalable son objetos colocados encima.
        NonClimbable,
    ));
}
#[cfg(test)]
mod tests {

    /// **El suelo dejó de ser escalable el 2026-08-23.** Un heightmap no puede
    /// plegarse, así que nunca pudo representar la pared que el sensor buscaba;
    /// lo escalable son los peñascos colocados encima.
    #[test]
    fn the_ground_is_not_climbable() {
        let mut world = World::new();
        let mut queue = bevy_ecs::world::CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        spawn_terrain(&mut commands, None);
        queue.apply(&mut world);
        let mut terrains = world.query_filtered::<(), (With<Terrain>, With<NonClimbable>)>();
        assert_eq!(
            terrains.iter(&world).count(),
            1,
            "el terreno tiene que nacer con NonClimbable"
        );
    }
    use super::persist::{decode_kinds, encode_kinds};
    use super::*;
    use bevy_ecs::system::RunSystemOnce;
    use bof_domain::props::PropKind;

    fn read_access(terrain: TerrainAccess) -> (Option<f32>, Option<TerrainKind>, usize) {
        (
            terrain.height_at(Vec2::ZERO),
            terrain.kind_at(Vec2::ZERO),
            terrain.changed().count(),
        )
    }

    #[test]
    fn terrain_access_is_the_single_seam_for_sampling_the_current_grid() {
        let mut world = World::new();
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 8.0, 3.0);
        terrain.paint_area(Vec2::ZERO, 8.0, TerrainKind::Rock);
        world.spawn(terrain);

        let (height, kind, changed) = world.run_system_once(read_access).unwrap();
        assert!(height.is_some_and(|height| height > 0.0));
        assert_eq!(kind, Some(TerrainKind::Rock));
        assert_eq!(changed, 1);
    }

    /// Height at the grid point nearest the world origin.
    fn center_height(terrain: &Terrain) -> f32 {
        let mid = terrain.points() / 2;
        terrain.height(mid, mid)
    }

    #[test]
    fn raising_lifts_the_centre_more_than_the_rim() {
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 20.0, 2.0);
        let mid = terrain.points() / 2;
        let centre = terrain.height(mid, mid);
        let near_rim = terrain.height(mid + 7, mid);
        assert!(centre > near_rim, "{centre} should exceed {near_rim}");
        assert!(near_rim > 0.0, "the falloff should still reach the rim");
    }

    /// Diez segundos de trazo a fuerza máxima, que es el peor caso que el
    /// editor permite pedir.
    fn hold_for_ten_seconds(carve: bool, radius: f32) -> Terrain {
        let mut terrain = Terrain::flat();
        let per_step = -1.5 * 3.0 / 60.0;
        for _ in 0..600 {
            if carve {
                terrain.carve_area(Vec2::ZERO, radius, per_step);
            } else {
                terrain.raise_area(Vec2::ZERO, radius, per_step);
            }
        }
        terrain
    }

    /// **Reporte, no aserción.** Qué tan alto es el tramo empinado que cada
    /// pincel produce, para decidir con el número en la mano si el pincel tiene
    /// que clampear o basta con avisar.
    #[test]
    fn report_the_steep_runs_each_brush_can_author() {
        let limit = bof_domain::world::MAX_UNWALKABLE_RISE_METRES;
        println!("\n[terrain] tramo empinado por pincel (tope {limit} m)");
        for radius in [2.0_f32, 6.0, 12.0, 25.0, 40.0] {
            let raised = hold_for_ten_seconds(false, radius).steepest_run();
            let carved = hold_for_ten_seconds(true, radius).steepest_run();
            println!(
                "  radio {radius:>4} m   elevar {:>6.2} m {}   acantilado {:>6.2} m {}",
                raised.rise,
                if raised.is_traversable() {
                    "ok "
                } else {
                    "MURO"
                },
                carved.rise,
                if carved.is_traversable() {
                    "ok "
                } else {
                    "MURO"
                },
            );
        }

        let mut terraced = Terrain::flat();
        for step in 0..600 {
            let slope = step as f32 * 0.02;
            terraced.raise_area(Vec2::new(slope, 0.0), 6.0, 0.05);
        }
        let before = terraced.steepest_run();
        terraced.terrace_area(Vec2::ZERO, 40.0, 2.0, 1.0);
        println!(
            "  terrazas         antes {:>6.2} m   después {:>6.2} m",
            before.rise,
            terraced.steepest_run().rise
        );

        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/game/world/sandbox.ron"
        );
        if let Ok(text) = std::fs::read_to_string(path) {
            let mut authored = Terrain::flat();
            authored
                .apply_ron(&text)
                .expect("the authored terrain parses");
            let worst = authored.steepest_run();
            println!(
                "  sandbox.ron      peor tramo {:>6.2} m en {:?} — {}",
                worst.rise,
                worst.start,
                if worst.is_traversable() {
                    "transitable"
                } else {
                    "MURO"
                }
            );
        }
    }

    /// **Los dos pinceles hacen cosas distintas, y por eso es diferencial.**
    ///
    /// La versión de este test que vivió hasta el 2026-08-23 medía **ángulos**
    /// y pedía 60° para que el jugador pudiera escalar lo que el editor sabía
    /// construir. Esa premisa murió dos veces: el terreno ya no se escala, y el
    /// ángulo nunca fue la magnitud correcta —vault y mantle pasan una
    /// contrahuella empinada; lo que no se pasa es una cara empinada **alta**.
    ///
    /// Medido el 2026-08-23 con `steepest_run`, sosteniendo diez segundos a
    /// fuerza máxima: al radio por defecto `raise_area` acumula 1,53 m —bajo el
    /// alcance del mantle, o sea terreno por el que se anda— y `carve_area`
    /// llega a 45 m, que es telón de fondo y no un lugar.
    #[test]
    fn only_the_cliff_brush_authors_ground_the_player_cannot_cross() {
        let relaxed = hold_for_ten_seconds(false, 6.0).steepest_run();
        let carved = hold_for_ten_seconds(true, 6.0).steepest_run();

        assert!(
            relaxed.is_traversable(),
            "`raise_area` acumuló {} m de tramo empinado al radio por defecto: \
             el pincel de todos los días no debería poder hacer una pared",
            relaxed.rise
        );
        assert!(
            !carved.is_traversable(),
            "`carve_area` sólo acumuló {} m, y con eso se cruza caminando: \
             el pincel de acantilado no cumple su único trabajo",
            carved.rise
        );
    }

    /// **Y el pincel de todos los días sí puede hacer una pared si el radio es
    /// grande.** Medido el 2026-08-23: 1,53 m a 6 m de radio, 7,56 m a 12 m,
    /// 21,37 m a 40 m. No es un bug del pincel —a esa escala el autor está
    /// modelando una montaña y sabe lo que hace— pero sí es lo que el editor
    /// tiene que poder mostrarle mientras esculpe.
    #[test]
    fn a_wide_raise_can_still_author_a_wall() {
        let wide = hold_for_ten_seconds(false, 40.0).steepest_run();
        assert!(
            !wide.is_traversable(),
            "a 40 m de radio `raise_area` acumuló sólo {} m: si esto se volvió \
             transitable, el aviso en vivo del editor dejó de tener sentido",
            wide.rise
        );
    }

    /// El otro síntoma del mismo día: no se podían cavar pozos hondos.
    #[test]
    fn the_cliff_brush_digs_far_deeper_than_the_relaxing_one() {
        let mid = Terrain::flat().points() / 2;
        let relaxed = hold_for_ten_seconds(false, 6.0).height(mid, mid);
        let carved = hold_for_ten_seconds(true, 6.0).height(mid, mid);
        assert!(
            carved < relaxed * 10.0,
            "el pozo a pico ({carved} m) tiene que ser mucho más hondo que el \
             relajado ({relaxed} m)"
        );
    }

    #[test]
    fn a_held_raise_stays_rounder_than_the_raw_falloff() {
        // The feel fix, stated as a test: integrating many small applications
        // must not sharpen the apex relative to its surroundings. A pure
        // `delta * falloff` stack does; the self-relaxing stroke does not.
        let mut sculpted = Terrain::flat();
        for _ in 0..120 {
            sculpted.raise_area(Vec2::ZERO, 15.0, 0.025);
        }
        let mut raw = Terrain::flat();
        for _ in 0..120 {
            raw.brush(Vec2::ZERO, 15.0, |_, _, falloff| 0.025 * falloff);
        }
        let mid = sculpted.points() / 2;
        // Ratio of apex to a point a third of the way out: lower means rounder.
        let sculpted_ratio = sculpted.height(mid, mid) / sculpted.height(mid, mid + 2);
        let raw_ratio = raw.height(mid, mid) / raw.height(mid, mid + 2);
        assert!(
            sculpted_ratio < raw_ratio,
            "self-relaxing stroke ({sculpted_ratio}) should be rounder than raw ({raw_ratio})"
        );
    }

    #[test]
    fn the_brush_window_does_not_clip_the_reach_of_the_radius() {
        // Visiting only the window the stroke can reach is an optimisation; it
        // must not change *what* a stroke touches. Points just inside the radius
        // still move, points just outside still do not.
        let mut terrain = Terrain::flat();
        let radius = 20.0;
        terrain.raise_area(Vec2::ZERO, radius, 3.0);
        let mid = terrain.points() / 2;
        let spacing = WORLD_SIZE / CELLS as f32;
        let inside = bounded_grid_index((radius / spacing).floor(), terrain.points()) - 1;
        let outside = bounded_grid_index((radius / spacing).ceil(), terrain.points()) + 1;
        assert!(
            terrain.height(mid + inside, mid) > 0.0,
            "a point inside the radius should have moved"
        );
        assert_eq!(
            terrain.height(mid + outside, mid),
            0.0,
            "a point outside the radius must stay put"
        );
    }

    #[test]
    fn a_stroke_at_the_world_edge_stays_inside_the_grid() {
        // The window is clamped, so a brush hanging off the corner must neither
        // panic nor wrap around to the opposite edge.
        let mut terrain = Terrain::flat();
        let corner = Vec2::splat(WORLD_SIZE * 0.5);
        terrain.raise_area(corner, 30.0, 4.0);
        let last = terrain.points() - 1;
        assert!(terrain.height(last, last) > 0.0, "the corner should lift");
        assert_eq!(
            terrain.height(0, 0),
            0.0,
            "the opposite corner must be untouched"
        );
    }

    #[test]
    fn smoothing_erodes_a_spike() {
        let mut terrain = Terrain::flat();
        terrain.brush(Vec2::ZERO, 4.0, |_, _, falloff| 10.0 * falloff);
        let before = center_height(&terrain);
        terrain.smooth_area(Vec2::ZERO, 20.0, 0.5);
        assert!(center_height(&terrain) < before);
    }

    #[test]
    fn flattening_converges_on_the_target() {
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 20.0, 5.0);
        for _ in 0..40 {
            terrain.flatten_area(Vec2::ZERO, 20.0, 3.0, 0.5);
        }
        assert!((center_height(&terrain) - 3.0).abs() < 0.05);
    }

    #[test]
    fn a_ramp_interpolates_between_its_two_ends() {
        let mut terrain = Terrain::flat();
        let from = Vec2::new(-30.0, 0.0);
        let to = Vec2::new(30.0, 0.0);
        for _ in 0..60 {
            terrain.ramp_area(from, 0.0, to, 12.0, 8.0, 0.5);
        }
        let mid = terrain.points() / 2;
        // Sample along +X (the row axis) at the ramp's midpoint and near its top.
        let middle = terrain.height(mid, mid);
        let high = terrain.height(mid + 11, mid);
        assert!(
            (middle - 6.0).abs() < 1.0,
            "midpoint {middle} should sit near half the rise"
        );
        assert!(high > middle, "{high} should be above {middle}");
    }

    #[test]
    fn noise_is_deterministic_and_two_sided() {
        let mut a = Terrain::flat();
        let mut b = Terrain::flat();
        a.noise_area(Vec2::ZERO, 40.0, 2.0, 7);
        b.noise_area(Vec2::ZERO, 40.0, 2.0, 7);
        assert_eq!(a.snapshot(), b.snapshot());
        // La ventana va en **metros**, no en índices: `NOISE_CELL` mide 12 m, y
        // un ancho contado en pasos de grilla se encoge con la resolución hasta
        // caber dentro de una sola celda de ruido, donde todo tiene el mismo
        // signo. El test seguiría pasando en verde sin mirar nada.
        let spacing = WORLD_SIZE / CELLS as f32;
        let half_window = (15.0 / spacing).round() as usize;
        let mid = a.points() / 2;
        let window: Vec<f32> = (mid - half_window..mid + half_window)
            .map(|row| a.height(row, mid))
            .collect();
        assert!(
            window.iter().any(|h| *h > 0.0) && window.iter().any(|h| *h < 0.0),
            "noise should push both up and down, got {window:?}"
        );
    }

    #[test]
    fn a_different_seed_gives_a_different_pattern() {
        let mut a = Terrain::flat();
        let mut b = Terrain::flat();
        a.noise_area(Vec2::ZERO, 40.0, 2.0, 1);
        b.noise_area(Vec2::ZERO, 40.0, 2.0, 2);
        assert_ne!(a.snapshot(), b.snapshot());
    }

    #[test]
    fn terracing_snaps_heights_onto_shelves() {
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 40.0, 7.3);
        for _ in 0..60 {
            terrain.terrace_area(Vec2::ZERO, 40.0, 2.0, 0.5);
        }
        let height = center_height(&terrain);
        let remainder = (height / 2.0 - (height / 2.0).round()).abs();
        assert!(remainder < 0.05, "{height} should sit on a 2 m shelf");
    }

    #[test]
    fn heights_stay_inside_the_guard_band() {
        let mut terrain = Terrain::flat();
        for _ in 0..200 {
            terrain.raise_area(Vec2::ZERO, 10.0, 5.0);
        }
        assert!(center_height(&terrain) <= MAX_HEIGHT);
    }

    #[test]
    fn height_at_lands_on_the_grid_points_it_passes_through() {
        let mut terrain = Terrain::flat();
        terrain.noise_area(Vec2::ZERO, 60.0, 4.0, 11);
        // Sampling exactly at a grid point must return that point's height —
        // true of any correct interpolation, and the cheapest way to catch an
        // off-by-one in the cell lookup.
        for (row, col) in [(40, 40), (41, 40), (40, 41), (64, 64)] {
            let xz = terrain.point_xz(row, col);
            let sampled = terrain.height_at(xz);
            let expected = terrain.height(row, col);
            assert!(
                (sampled - expected).abs() < 0.001,
                "at ({row},{col}): sampled {sampled}, grid says {expected}"
            );
        }
    }

    #[test]
    fn height_at_stays_on_the_triangles_not_above_them() {
        // The hovering bug: a bilinear patch bulges above the triangulated
        // surface inside a quad. Build a saddle — the worst case, where the two
        // disagree most — and check the sample sits on the triangle.
        let mut terrain = Terrain::flat();
        let points = terrain.points;
        let (row, col) = (60, 60);
        terrain.heights[row * points + col] = 0.0;
        terrain.heights[(row + 1) * points + col + 1] = 0.0;
        terrain.heights[(row + 1) * points + col] = 10.0;
        terrain.heights[row * points + col + 1] = 10.0;

        // The cell centre sits exactly on the shared diagonal, so the surface
        // there is the height of that diagonal's endpoints — and bilinear would
        // say 5.0, the average of all four corners, which is the bug this guards.
        //
        // Which endpoints those are depends on which diagonal parry cuts, and
        // this assertion used to say `0.0`: it had the *other* diagonal baked in,
        // and went on passing while the collider and the mesh described two
        // different surfaces. So it now checks the invariant it actually cares
        // about — the sample is on a triangle, not on the bulge — and gets the
        // diagonal from the collider instead of from an assumption.
        let centre = (terrain.point_xz(row, col) + terrain.point_xz(row + 1, col + 1)) * 0.5;
        let sampled = terrain.height_at(centre);

        let collider = terrain.to_collider();
        let ray =
            avian3d::parry::query::Ray::new(Vec3::new(centre.x, 500.0, centre.y), Vec3::NEG_Y);
        let hit = collider
            .shape_scaled()
            .cast_local_ray(&ray, 1000.0, true)
            .expect("the ray must hit the terrain");
        let on_collider = 500.0 - hit;

        assert!(
            (sampled - on_collider).abs() < 0.01,
            "the grid says {sampled} where the collider says {on_collider}"
        );
        assert!(
            (sampled - 5.0).abs() > 1.0,
            "{sampled} is the bilinear average — the sample left the triangle"
        );
    }

    #[test]
    fn the_collider_surface_matches_the_grid_everywhere() {
        // The wireframe looked offset from the ground. This settles it without
        // interpreting a screenshot: shoot a ray straight down at the actual
        // collider and compare where it lands with what `height_at` (the surface
        // the mesh is built from) claims. Any mapping mistake — transposed axes,
        // a shifted origin, the wrong diagonal — shows up here as a mismatch.
        //
        // **Swept, not sampled.** This test used to check six hand-picked points
        // and it passed for days while a third of the terrain was wrong: cutting
        // the cell along the other diagonal than parry does is invisible on even
        // ground and grows with the twist of each quad, so six points on gentle
        // relief hit none of it. Hence: hard-sculpted ground, and a grid of
        // samples deliberately offset from the cell centres so they land all over
        // the inside of the quads, where the two triangulations differ most.
        // See `Terrain::to_collider`.
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 60.0, 12.0);
        terrain.noise_area(Vec2::ZERO, 150.0, 6.0, 7);
        terrain.terrace_area(Vec2::new(30.0, 30.0), 40.0, 2.0, 1.0);
        let collider = terrain.to_collider();
        let shape = collider.shape_scaled();

        let mut worst = 0.0_f32;
        let mut worst_at = Vec2::ZERO;
        let mut checked = 0;
        for i in 0..60 {
            for j in 0..60 {
                // 2.51 m step against 2.5 m cells: the sample drifts across the
                // cell instead of landing on the same spot of every one.
                let xz = Vec2::new(-75.0 + i as f32 * 2.51, -75.0 + j as f32 * 2.51);
                let ray =
                    avian3d::parry::query::Ray::new(Vec3::new(xz.x, 500.0, xz.y), Vec3::NEG_Y);
                let Some(hit) = shape.cast_local_ray(&ray, 1000.0, true) else {
                    continue;
                };
                checked += 1;
                let error = (500.0 - hit - terrain.height_at(xz)).abs();
                if error > worst {
                    worst = error;
                    worst_at = xz;
                }
            }
        }
        assert!(checked > 3000, "the sweep hit only {checked} points");
        assert!(
            worst < 0.01,
            "collider and grid disagree by {worst} m at {worst_at:?}"
        );
    }

    #[test]
    fn a_snapshot_restores_the_grid_it_came_from() {
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 20.0, 4.0);
        let snapshot = terrain.snapshot();
        terrain.raise_area(Vec2::new(20.0, 20.0), 20.0, -6.0);
        assert_ne!(terrain.snapshot(), snapshot);
        assert!(terrain.restore(&snapshot));
        assert_eq!(terrain.snapshot(), snapshot);
    }

    #[test]
    fn a_snapshot_of_the_wrong_size_is_refused() {
        let mut terrain = Terrain::flat();
        let stale = TerrainSnapshot {
            heights: vec![0.0; 4],
            kinds: vec![TerrainKind::default(); 1],
        };
        assert!(!terrain.restore(&stale));
    }

    #[test]
    fn a_snapshot_carries_the_semantic_layer_too() {
        // Undo has to take back a paint stroke as readily as a sculpt one. A
        // snapshot of only the heights would restore the shape and leave the
        // painted meaning behind — the level would come back half undone.
        let mut terrain = Terrain::flat();
        let before = terrain.snapshot();
        assert!(terrain.paint_area(Vec2::ZERO, 20.0, TerrainKind::Rock));
        assert_ne!(terrain.snapshot(), before);
        assert!(terrain.restore(&before));
        assert_eq!(terrain.kind_at(Vec2::ZERO), TerrainKind::Soil);
    }

    #[test]
    fn painting_covers_the_radius_and_nothing_past_it() {
        let mut terrain = Terrain::flat();
        let radius = 20.0;
        assert!(terrain.paint_area(Vec2::ZERO, radius, TerrainKind::Rock));
        assert_eq!(terrain.kind_at(Vec2::ZERO), TerrainKind::Rock);
        // A cell centre just inside the circle is painted; well outside is not.
        let spacing = WORLD_SIZE / CELLS as f32;
        assert_eq!(
            terrain.kind_at(Vec2::new(radius - spacing * 1.5, 0.0)),
            TerrainKind::Rock
        );
        assert_eq!(
            terrain.kind_at(Vec2::new(radius + spacing * 1.5, 0.0)),
            TerrainKind::Soil
        );
    }

    #[test]
    fn painting_the_same_patch_twice_changes_nothing() {
        // Paint has no rate and no falloff, so a held button must be idempotent.
        // The editor relies on this to know whether a stroke is worth an undo
        // step: a brush that always reports a change files empty history.
        let mut terrain = Terrain::flat();
        assert!(terrain.paint_area(Vec2::ZERO, 15.0, TerrainKind::Sand));
        assert!(!terrain.paint_area(Vec2::ZERO, 15.0, TerrainKind::Sand));
    }

    #[test]
    fn painting_at_the_world_edge_stays_inside_the_grid() {
        // Same guard the sculpt window needs: the cell window is clamped, so a
        // stroke centred on the corner must not index past the end.
        let mut terrain = Terrain::flat();
        let edge = WORLD_SIZE / 2.0;
        terrain.paint_area(Vec2::new(edge, edge), 30.0, TerrainKind::Rock);
        terrain.paint_area(Vec2::new(-edge, -edge), 30.0, TerrainKind::Sand);
        let last = terrain.cells() - 1;
        assert_eq!(terrain.kind(last, last), TerrainKind::Rock);
        assert_eq!(terrain.kind(0, 0), TerrainKind::Sand);
    }

    #[test]
    fn painting_never_touches_the_relief_revision() {
        // The guard that keeps painting off the physics path: the collider is
        // derived from the heights alone, and rebuilding it is the most expensive
        // thing an edit can trigger. A paint stroke must be invisible to it.
        let mut terrain = Terrain::flat();
        let before = terrain.relief_revision();
        assert!(terrain.paint_area(Vec2::ZERO, 20.0, TerrainKind::TallGrass));
        assert_eq!(
            terrain.relief_revision(),
            before,
            "painting moved no ground"
        );

        terrain.raise_area(Vec2::ZERO, 20.0, 1.0);
        assert_ne!(
            terrain.relief_revision(),
            before,
            "sculpting did move ground"
        );
    }

    #[test]
    fn undoing_a_paint_stroke_does_not_rebuild_the_collider() {
        // The same guard on the way back. `restore` writes both channels, so it
        // has to check whether the relief actually differs rather than assume it.
        let mut terrain = Terrain::flat();
        let before = terrain.snapshot();
        terrain.paint_area(Vec2::ZERO, 20.0, TerrainKind::Rock);
        let revision = terrain.relief_revision();
        assert!(terrain.restore(&before));
        assert_eq!(terrain.relief_revision(), revision);

        // ...but undoing a sculpt stroke must.
        terrain.raise_area(Vec2::ZERO, 20.0, 2.0);
        let sculpted = terrain.relief_revision();
        assert!(terrain.restore(&before));
        assert_ne!(terrain.relief_revision(), sculpted);
    }

    #[test]
    fn the_semantic_layer_round_trips_through_disk() {
        let mut terrain = Terrain::flat();
        terrain.paint_area(Vec2::ZERO, 25.0, TerrainKind::TallGrass);
        terrain.paint_area(Vec2::new(60.0, -40.0), 12.0, TerrainKind::Rock);
        let expected = terrain.snapshot();
        let text = terrain.to_ron().expect("serialises");
        let mut loaded = Terrain::flat();
        loaded.apply_ron(&text).expect("loads");
        assert_eq!(loaded.snapshot(), expected);
    }

    #[test]
    fn a_level_saved_before_the_semantic_layer_still_loads() {
        // The terrain files already on disk have no `kinds` field. They are the
        // levels of the sessions before this one, and losing them to a format
        // change is exactly what the file-is-the-level rule forbids.
        //
        // Both load paths, because `apply_ron` branches on whether the file's
        // resolution matches the current one and the real files take the branch a
        // small hand-written fixture does not: `sandbox.ron` on disk today is
        // `points: 129, extent: 320.0` with no `kinds`.
        let resampled = "(\n    points: 3,\n    extent: 320.0,\n    heights: [0.0, 1.0, 2.0, \
                         3.0, 4.0, 5.0, 6.0, 7.0, 8.0],\n)\n";
        let native = format!(
            "(points: {}, extent: {WORLD_SIZE:?}, heights: {:?})",
            CELLS + 1,
            vec![0.0f32; (CELLS + 1) * (CELLS + 1)]
        );
        for text in [resampled.to_string(), native] {
            let mut terrain = Terrain::flat();
            terrain.paint_area(Vec2::ZERO, 40.0, TerrainKind::Rock);
            terrain
                .apply_ron(&text)
                .expect("a file without kinds loads");
            // And it loads as unpainted, rather than leaving whatever the previous
            // level had painted in place: the file is the level, all of it.
            assert_eq!(terrain.kind_at(Vec2::ZERO), TerrainKind::Soil);
        }
    }

    #[test]
    fn a_truncated_semantic_layer_is_refused() {
        // Run-length encoding's failure mode: runs that do not add up would
        // offset every cell after the gap, sliding the whole map's meaning
        // sideways. Silently accepting that is worse than not loading.
        let text = "(\n    points: 3,\n    extent: 320.0,\n    heights: [0.0, 0.0, 0.0, \
                    0.0, 0.0, 0.0, 0.0, 0.0, 0.0],\n    kinds: [(2, Rock)],\n)\n";
        let mut terrain = Terrain::flat();
        let error = terrain
            .apply_ron(text)
            .expect_err("a short run list must be refused");
        assert!(error.contains("semantic layer"), "unhelpful error: {error}");
    }

    #[test]
    fn run_length_encoding_survives_every_pattern_it_can_meet() {
        // Encode/decode is the one place a bug corrupts a level silently, so it
        // is pinned on the shapes that actually occur: uniform (a fresh level),
        // one patch (a painted one), and alternating (the worst case for runs).
        let cells = 8;
        let uniform = vec![TerrainKind::Soil; cells * cells];
        let mut patch = uniform.clone();
        for cell in patch.iter_mut().skip(10).take(7) {
            *cell = TerrainKind::TallGrass;
        }
        let striped: Vec<TerrainKind> = (0..cells * cells)
            .map(|i| TerrainKind::ALL[i % TerrainKind::ALL.len()])
            .collect();
        for original in [uniform, patch, striped] {
            let runs = encode_kinds(&original);
            assert_eq!(decode_kinds(&runs, cells).expect("decodes"), original);
        }
        // And the point of the encoding: a fresh level is a single run.
        assert_eq!(encode_kinds(&vec![TerrainKind::Soil; 4096]).len(), 1);
    }

    #[test]
    fn a_semantic_layer_from_another_resolution_is_resampled_without_inventing_kinds() {
        // Heights resample bilinearly; kinds must not. Interpolating them would
        // produce values between `Rock` and `TallGrass` — which is to say kinds
        // nobody painted, appearing the first time `CELLS` changes.
        let coarse = TerrainFile {
            points: 3,
            extent: WORLD_SIZE,
            heights: vec![0.0; 9],
            // 2×2 cells: rock along the low-x row, sand along the high-x row.
            kinds: vec![KindRun(2, TerrainKind::Rock), KindRun(2, TerrainKind::Sand)],
            instances: Vec::new(),
        };
        let text = ron::ser::to_string(&coarse).expect("serialises");
        let mut terrain = Terrain::flat();
        terrain.apply_ron(&text).expect("loads");
        let quarter = WORLD_SIZE / 4.0;
        assert_eq!(terrain.kind_at(Vec2::new(-quarter, 0.0)), TerrainKind::Rock);
        assert_eq!(terrain.kind_at(Vec2::new(quarter, 0.0)), TerrainKind::Sand);
        // Every cell holds a kind that was actually in the file.
        for row in 0..terrain.cells() {
            for col in 0..terrain.cells() {
                let kind = terrain.kind(row, col);
                assert!(
                    kind == TerrainKind::Rock || kind == TerrainKind::Sand,
                    "resampling invented {kind:?} at ({row}, {col})"
                );
            }
        }
    }

    #[test]
    fn a_saved_grid_round_trips() {
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 25.0, 6.0);
        terrain.noise_area(Vec2::ZERO, 40.0, 1.5, 3);
        let expected = terrain.snapshot();
        let text = terrain.to_ron().expect("serialises");
        let mut loaded = Terrain::flat();
        loaded.apply_ron(&text).expect("loads");
        assert_eq!(loaded.snapshot(), expected);
    }

    #[test]
    fn instances_round_trip_through_disk_on_the_direct_copy_branch() {
        // Same points/extent as the file: `apply_ron` takes the direct-copy
        // branch, not the resample one. Both branches carry instances through
        // unchanged, but only a test that exercises each branch separately
        // could tell them apart if one silently dropped instances.
        let mut terrain = Terrain::flat();
        terrain.place_instance(PropKind::GrassA, Vec2::new(12.0, -34.0), 0.7, 1.2);
        terrain.place_instance(PropKind::GrassTallA, Vec2::new(-5.0, 5.0), 0.0, 0.9);
        let text = terrain.to_ron().expect("serialises");
        let mut loaded = Terrain::flat();
        loaded.apply_ron(&text).expect("loads");
        assert_eq!(loaded.instances(), terrain.instances());
    }

    #[test]
    fn instances_survive_a_resample_unchanged() {
        // Different points/extent than the file: `apply_ron` takes the
        // resample branch. Instances must come through byte-for-byte — they
        // are already in metres, so resampling must not touch them at all.
        let coarse = TerrainFile {
            points: 3,
            extent: WORLD_SIZE,
            heights: vec![0.0; 9],
            kinds: Vec::new(),
            instances: vec![Instance {
                kind: PropKind::GrassDryA,
                xz: Vec2::new(40.0, -10.0),
                yaw: 1.1,
                scale: 1.0,
            }],
        };
        let text = ron::ser::to_string(&coarse).expect("serialises");
        let mut terrain = Terrain::flat();
        assert_ne!(
            terrain.points(),
            coarse.points,
            "must exercise the resample branch"
        );
        terrain.apply_ron(&text).expect("loads");
        assert_eq!(terrain.instances(), coarse.instances);
    }

    #[test]
    fn a_level_saved_before_the_instance_layer_still_loads() {
        // The five levels on disk today have no `instances` field. Losing that
        // would be exactly the format-change breakage `#[serde(default)]`
        // exists to prevent.
        let text = format!(
            "(points: {}, extent: {WORLD_SIZE:?}, heights: {:?})",
            CELLS + 1,
            vec![0.0f32; (CELLS + 1) * (CELLS + 1)]
        );
        let mut terrain = Terrain::flat();
        terrain.place_instance(PropKind::GrassA, Vec2::ZERO, 0.0, 1.0);
        terrain
            .apply_ron(&text)
            .expect("a file without instances loads");
        assert!(
            terrain.instances().is_empty(),
            "the file is the level, all of it — a missing field means none placed"
        );
    }

    #[test]
    fn a_grid_saved_at_another_resolution_is_resampled() {
        // A file authored before a `CELLS` change must still load: half the
        // resolution, same world, so the shape survives even though the sample
        // count does not.
        let coarse = TerrainFile {
            points: 5,
            extent: WORLD_SIZE,
            heights: (0..25).map(|i| (i / 5) as f32).collect(),
            kinds: Vec::new(),
            instances: Vec::new(),
        };
        let text = ron::ser::to_string(&coarse).expect("serialises");
        let mut terrain = Terrain::flat();
        terrain.apply_ron(&text).expect("loads");
        assert_eq!(terrain.points(), CELLS + 1);
        let last = CELLS;
        assert!((terrain.height(0, 0) - 0.0).abs() < 0.001);
        assert!((terrain.height(last, 0) - 4.0).abs() < 0.001);
        // Interpolated, not snapped: the middle row lands between the shelves.
        assert!((terrain.height(last / 2, 0) - 2.0).abs() < 0.05);
    }

    #[test]
    fn a_hand_written_file_loads() {
        // The shape a human (or a migration script) would type: pretty fields,
        // compact array, trailing commas. Pins the on-disk format so a change to
        // the serializer's config cannot silently orphan authored levels.
        let text = "(\n    points: 3,\n    extent: 320.0,\n    heights: [0.0, 1.0, 2.0, \
                    3.0, 4.0, 5.0, 6.0, 7.0, 8.0],\n)\n";
        let mut terrain = Terrain::flat();
        terrain.apply_ron(text).expect("hand-written RON loads");
        assert!((terrain.height(0, 0) - 0.0).abs() < 0.001);
        assert!((terrain.height(CELLS, CELLS) - 8.0).abs() < 0.001);
    }

    #[test]
    fn a_grid_saved_for_a_smaller_world_keeps_its_metres() {
        // The file's `extent` is not decoration: a level authored for a 160 m
        // world must land on the middle 160 m of a 320 m one, at its authored
        // heights. Stretching it to fill the world instead would halve every
        // slope — ground that looks authored and walks wrong.
        let half = WORLD_SIZE / 2.0;
        let coarse = TerrainFile {
            points: 3,
            extent: half,
            heights: vec![0.0, 0.0, 0.0, 10.0, 10.0, 10.0, 20.0, 20.0, 20.0],
            kinds: Vec::new(),
            instances: Vec::new(),
        };
        let text = ron::ser::to_string(&coarse).expect("serialises");
        let mut terrain = Terrain::flat();
        terrain.apply_ron(&text).expect("loads");
        // The file spans x ∈ [-80, 80] with height 0 → 20 across it.
        assert!((terrain.height_at(Vec2::new(-half / 2.0, 0.0)) - 0.0).abs() < 0.05);
        assert!((terrain.height_at(Vec2::ZERO) - 10.0).abs() < 0.05);
        assert!((terrain.height_at(Vec2::new(half / 2.0, 0.0)) - 20.0).abs() < 0.05);
        // Outside the file's world the edge height carries on, rather than the
        // level being stretched to reach the corner.
        assert!((terrain.height_at(Vec2::new(WORLD_SIZE / 2.0, 0.0)) - 20.0).abs() < 0.05);
    }

    #[test]
    fn a_file_with_a_non_finite_height_is_refused() {
        // A NaN reaches parry as a heightfield vertex and quietly breaks every
        // contact against it, so it has to die at the door.
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 20.0, 3.0);
        let expected = terrain.snapshot();
        let poisoned = "(points: 2, extent: 320.0, heights: [0.0, NaN, 0.0, 0.0])";
        assert!(terrain.apply_ron(poisoned).is_err());
        assert_eq!(terrain.snapshot(), expected);
        assert!(
            terrain
                .apply_ron("(points: 2, extent: 0.0, heights: [0.0, 0.0, 0.0, 0.0])")
                .is_err()
        );
        assert_eq!(terrain.snapshot(), expected);
    }

    #[test]
    fn a_loaded_height_outside_the_guard_band_is_clamped() {
        let mut terrain = Terrain::flat();
        let text = format!(
            "(points: 2, extent: 320.0, heights: [{}, {}, {}, {}])",
            MAX_HEIGHT * 10.0,
            MAX_HEIGHT * 10.0,
            MIN_HEIGHT * 10.0,
            MIN_HEIGHT * 10.0
        );
        terrain.apply_ron(&text).expect("loads");
        let heights = terrain.snapshot().heights;
        let peak = heights.iter().cloned().fold(f32::MIN, f32::max);
        let pit = heights.iter().cloned().fold(f32::MAX, f32::min);
        assert!(peak <= MAX_HEIGHT, "{peak} should be capped");
        assert!(pit >= MIN_HEIGHT, "{pit} should be floored");
    }

    #[test]
    fn a_malformed_file_is_rejected_without_touching_the_grid() {
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 20.0, 3.0);
        let expected = terrain.snapshot();
        // Parses as RON but lies about its own dimensions.
        assert!(
            terrain
                .apply_ron("(points: 4, extent: 10.0, heights: [1.0])")
                .is_err()
        );
        assert!(terrain.apply_ron("not ron at all").is_err());
        assert_eq!(terrain.snapshot(), expected);
    }

    #[test]
    fn slope_deg_at_calculates_flat_and_inclined_slopes() {
        let flat_terrain = Terrain::flat();
        assert_eq!(flat_terrain.slope_deg_at(Vec2::ZERO), 0.0);

        let mut sloped_terrain = Terrain::flat();
        sloped_terrain.ramp_area(Vec2::ZERO, 0.0, Vec2::new(10.0, 0.0), 10.0, 5.0, 1.0);
        let slope = sloped_terrain.slope_deg_at(Vec2::new(5.0, 0.0));
        assert!(slope > 0.0, "slope on ramp should be positive, got {slope}");
    }

    /// Reads the surface the way the ground probe does — off the collider, not
    /// off the grid — and reports how the authored relief is distributed across
    /// the 60° line that separates floor from not-floor.
    ///
    /// The number that matters is not "how steep is the canyon". It is **how
    /// often a walking body crosses that line**: a slope that is uniformly 70°
    /// is a wall and reads as one, but walkable ground speckled with 63°
    /// triangles flips `grounded` every time a foot lands on one, which is the
    /// `Walk`↔`Fall` buzz. So the transect count below walks in a straight line
    /// at one tick's worth of distance per sample and counts sign changes.
    ///
    /// Run it with:
    /// `cargo test -p breath_of_freedom_simulation authored_relief -- --nocapture`
    #[test]
    fn report_how_often_the_authored_relief_crosses_the_walkable_limit() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/game/world/sandbox.ron"
        );
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("[terrain] {path} missing — skipping the report");
            return;
        };
        let mut terrain = Terrain::flat();
        terrain
            .apply_ron(&text)
            .expect("the authored terrain parses");
        let collider = terrain.to_collider();
        let shape = collider.shape_scaled();

        // Read, not spelled: a literal here would go on reporting the old
        // line after the threshold moves, and this test only prints — it has
        // no assertion that would catch it lying.
        let walkable_limit_deg = crate::movement::motor_common::WALKABLE_LIMIT_DEG;
        let normal_at = |xz: Vec2| {
            let ray = avian3d::parry::query::Ray::new(Vec3::new(xz.x, 500.0, xz.y), Vec3::NEG_Y);
            shape
                .cast_local_ray_and_get_normal(&ray, 1000.0, true)
                .map(|hit| Vec3::from(hit.normal))
        };
        let angle_at = |xz: Vec2| normal_at(xz).map(|n| n.y.clamp(-1.0, 1.0).acos().to_degrees());

        // --- 1. Where the relief sits relative to the limit, over the whole map.
        let mut buckets = [0_u32; 10]; // 0-10, 10-20, … 90+
        let mut sampled = 0_u32;
        let mut past_limit = 0_u32;
        let mut in_the_treacherous_band = 0_u32; // 60-70°: not floor, not wall
        let half = terrain.extent * 0.5;
        let step = 1.0_f32;
        let mut x = -half + step;
        while x < half - step {
            let mut z = -half + step;
            while z < half - step {
                if let Some(deg) = angle_at(Vec2::new(x, z)) {
                    sampled += 1;
                    buckets[((deg / 10.0) as usize).min(9)] += 1;
                    if deg > walkable_limit_deg {
                        past_limit += 1;
                        if deg < 70.0 {
                            in_the_treacherous_band += 1;
                        }
                    }
                }
                z += step;
            }
            x += step;
        }

        println!("\n[terrain] {sampled} samples, 1 m grid, read off the collider");
        for (i, count) in buckets.iter().enumerate() {
            let share = *count as f32 / sampled as f32 * 100.0;
            println!(
                "  {:>2}-{:>2}° {:>7} {:>5.2}% {}",
                i * 10,
                i * 10 + 10,
                count,
                share,
                "#".repeat((share * 2.0).round() as usize)
            );
        }
        println!(
            "[terrain] past the {walkable_limit_deg}° limit: {past_limit} ({:.2}%), of which \
             {in_the_treacherous_band} sit in the 60-70° band ({:.2}% of the map)",
            past_limit as f32 / sampled as f32 * 100.0,
            in_the_treacherous_band as f32 / sampled as f32 * 100.0,
        );

        // --- 2. How often a walking body crosses the limit.
        // One tick of walking at 5 m/s over a 60 Hz fixed step.
        let tick_step = 5.0 / 60.0;
        let transect = |from: Vec2, to: Vec2, label: &str| {
            let span = to - from;
            let steps = (span.length() / tick_step) as usize;
            let dir = span.normalize_or_zero();
            let mut crossings = 0_u32;
            let mut ticks_past_limit = 0_u32;
            let mut previous: Option<bool> = None;
            let mut counted = 0_u32;
            for i in 0..steps {
                let Some(deg) = angle_at(from + dir * (i as f32 * tick_step)) else {
                    continue;
                };
                counted += 1;
                let past = deg > walkable_limit_deg;
                if past {
                    ticks_past_limit += 1;
                }
                if previous.is_some_and(|was| was != past) {
                    crossings += 1;
                }
                previous = Some(past);
            }
            println!(
                "[terrain] {label}: {counted} ticks, {crossings} crossings of the limit, \
                 {ticks_past_limit} ticks past it ({:.1}%)",
                ticks_past_limit as f32 / counted.max(1) as f32 * 100.0
            );
            crossings
        };

        // The three places the played session actually buzzed, from the log.
        transect(
            Vec2::new(70.0, 32.0),
            Vec2::new(95.0, 32.0),
            "canyon x70→95 @z32",
        );
        transect(
            Vec2::new(82.0, 20.0),
            Vec2::new(82.0, 45.0),
            "canyon z20→45 @x82",
        );
        transect(
            Vec2::new(40.0, 0.0),
            Vec2::new(60.0, 0.0),
            "deep pit x40→60 @z0",
        );

        assert!(sampled > 0, "the authored terrain produced no samples");
    }
}
