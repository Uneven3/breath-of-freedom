//! The grass meadow: a **rolling grid of baked chunks, centred on the camera**.
//!
//! Pure presentation — no collider, no simulation meaning; the ground beneath
//! already reports `Surface(Grass)` for footstep audio, so the grass never needs
//! to be *known* by the simulation.
//!
//! # Why a mesh per chunk and not an entity per blade
//!
//! Density is the whole point of this system, and density is a fight against
//! per-item cost. An entity per blade puts every blade through transform
//! propagation, visibility and change detection every frame: at the density that
//! actually looks like a meadow that is tens of thousands of entities, and the
//! frame is gone before a single triangle is drawn.
//!
//! So the blade is not an entity. A chunk bakes its blades straight into one
//! mesh — position, yaw, height and slope resolved once, at build time — and the
//! ECS holds one entity per chunk. The cost of a blade becomes two triangles and
//! nothing else. It is the same shape as `visuals::terrain`, which bakes 32k
//! triangles of ground into one mesh the same way.
//!
//! The unit matters more than any of it: a blade is **two triangles**. Grouping
//! blades into a modelled clump makes the instance cost six times more, and the
//! only way to stay inside the budget after that is to space the clumps out —
//! which is how a meadow turns into scattered shrubs on bare dirt.
//!
//! # Why the grid rolls
//!
//! The field used to be a fixed 25×25 m square, and a fixed square does not
//! scale to a world: it covered 0.6% of the map while eating 52% of the scene's
//! triangle budget, and stretching it to 320×320 m would have been millions of
//! triangles. Tuning that shape could never fix it.
//!
//! So the field is not a place any more, it is a **neighbourhood**: three rings
//! that exist around wherever the camera is, each with bigger chunks and fewer
//! blades per square metre than the one inside it. Walk, and chunks left behind
//! are rebaked ahead. The blade count is constant whether the map is 25 m or
//! 4 km across, and the budget becomes *per view* instead of per scene — which
//! is the only definition that means anything in an open world.
//!
//! What must **not** change with distance is how thick the field looks. The
//! rings thin out on a derivation rather than on taste: a blade at distance `d`
//! hides `width · H · d / h` of the ground behind it, so the same apparent cover
//! needs `1/d` as many blades. Forty-five blades per m² at 16 m is not a denser
//! field, it is several times the cover anyone can resolve, and the surplus is
//! paid in overdraw — which is exactly what the mobile target cannot afford.
//!
//! **But that derivation is a floor, not a recipe.** Planted at the minimum the
//! field covered the ground and still read as sparse spikes: covering the ground
//! and looking like a meadow are different bars. See [`RINGS`] for where each
//! number actually comes from, and `docs/BOTWGrass.md` for the table.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

use crate::visuals::grass_cover;
use crate::visuals::grass_material::{GrassExtension, GrassMaterial, GrassUniform};
use crate::world::TerrainAccess;
use crate::world::forest::{hash_u32, hash_unit};

/// One LOD ring: how far out it reaches, how big its chunks are, and how thickly
/// it plants them.
///
/// `reach_m` is a Chebyshev radius — a square, not a circle — because the chunks
/// are a square grid and a square boundary keeps a chunk either wholly in a ring
/// or wholly out of it. The seam that leaves between two rings is a strip up to
/// half a chunk wide that may end up slightly denser or sparser than the ideal;
/// it survives review because after the terrain tint (`grass_cover`) a thin spot
/// reads as sparser grass over green ground rather than as a hole. That is why
/// the tint lands before the rings and not after.
struct Ring {
    reach_m: f32,
    chunk_m: f32,
    /// Blades per m², at the shipped density. The knob scales every ring.
    density: f32,
    /// How much wider than [`BLADE_WIDTH`] this ring's blades are.
    ///
    /// **This is 1.0 everywhere, and that is a finding, not an oversight.** The
    /// arithmetic said otherwise: coverage is `blades × width × height × d / h`,
    /// so a blade twice as wide hides twice the ground and half as many are
    /// needed for the same cover. The first version of these rings used ×2 from
    /// 4 m and ×4 from 8 m, and the field came out looking like a bed of fat
    /// spikes — played on 2026-08-05 and rejected on sight.
    ///
    /// The formula was right and the conclusion wrong. Coverage is not what the
    /// eye judges: at 4 m a blade is several pixels across, so its *silhouette*
    /// reads, and a blade twice as wide is not a cheaper equivalent, it is a
    /// different plant. Widening only buys anything once a blade is genuinely
    /// sub-pixel, which at this camera height starts well past this meadow's
    /// reach. Kept as a field rather than deleted so the next person who
    /// rediscovers the arithmetic finds the result of trying it.
    width_scale: f32,
    /// Whether this ring's blades end in two points rather than a flat cut.
    ///
    /// Costs one extra triangle and one extra vertex per blade, so it is only
    /// worth it where a blade covers enough pixels for its outline to read —
    /// the near ring, and nowhere else. This is the *V-split* an earlier draft
    /// of `BOTWGrass.md` listed and a later rewrite dropped in favour of a
    /// curved blade; the user remembered it and it is back, because it solves
    /// the same problem for one triangle instead of four.
    split_tips: bool,
}

/// The three rings, from the camera outward. Each row is **floored** by
/// [`minimum_density`] — a test enforces that — and chosen above it by eye.
///
/// The field has to read equally thick at every distance; what falls with
/// distance is the *cost*, not the look. The lever is blade count: a blade at
/// distance `d` hides `width · H · d / h` of the ground behind it because the
/// viewing angle flattens out, so the density needed for the same cover falls as
/// `1/d`. The near ring's density is judged by eye and was raised twice for
/// that reason; the outer two sit well above their derived minimum, because
/// they are also the rings that catch what the one inside them sheds during
/// [`GROWTH_SPREAD_M`].
///
/// Chunks are about two thirds of the ring's reach, and that ratio was counted
/// rather than picked. Finer chunks hug the ring's boundary and waste less baked
/// geometry, but each one is a draw; coarser chunks overshoot the boundary and
/// bake blades nobody asked for. Counted three ways with the handover overlap in
/// place: `reach/2` gave 108 chunks and 236.160 triangles, `reach/1` gave 48
/// chunks and 419.840, and 5/10/20 m gives **48 chunks and 164.000** — better on
/// both axes than either extreme.
///
/// **What this costs, stated plainly:** 489.200 triangles declared across the
/// full 360° at the worst alignment of the camera against the grid. That is
/// nearly five times the mobile triangle budget, and `perf::budget` declares it
/// as debt with a number rather than hiding it.
///
/// **And since 2026-08-06 it is measured, which changes what the number means.**
/// The `grass` suite (`BOF_BENCH=grass`) in the Pasto box, at eye height looking
/// at the horizon, on the dev machine's Polaris 11:
///
/// | paso | GPU ms | contra el baseline |
/// |---|---:|---:|
/// | baseline (56/m²) | 6,08 | — |
/// | **pasto apagado** | 2,31 | **−3,77** |
/// | densidad 30/m² | 5,00 | −1,08 |
/// | densidad 12/m² | 3,56 | −2,52 |
/// | alcance 75% | 5,54 | −0,54 |
/// | alcance 50% | 3,96 | −2,12 |
/// | render 50% | 2,17 | −3,90 |
///
/// Ruido de esa corrida: 0,25 ms (deriva entre los dos baselines).
///
/// Tres cosas que se leen ahí y que ningún conteo de triángulos podía decir:
///
/// 1. **La pradera es el 62% de la GPU de su caja** — 3,77 de 6,08 ms. Por
///    resta contra un paso en cero, no extrapolando.
/// 2. **Es fill-bound, no vertex-bound.** Bajar la resolución a la mitad —misma
///    geometría, mismos 489.200 triángulos— ahorra 3,90 ms, *más que apagar el
///    pasto entero*. La palanca es cuántos píxeles pinta cada brizna encima de
///    otra, no cuántas hay. Eso es lo que decide que el conteo de arriba sea
///    guardrail y no objetivo — con la salvedad de siempre: en el target
///    tile-based un vértice se paga en bandwidth aunque no produzca un píxel, y
///    eso no se manifiesta en esta máquina.
/// 3. **Densidad y alcance mueven cosas parecidas por caminos distintos.** El
///    alcance al 50% (área a un cuarto) ahorra 2,12; la densidad a 12/m² (79%
///    menos briznas) ahorra 2,52. Ninguna de las dos es gratis y las dos se ven.
///
/// **The minimum is a floor, not a target.** The first version planted every
/// ring at 1,25× its derived minimum, which is what the formula says is needed
/// for the ground not to show through — and the field read as sparse and
/// spiky. Covering the ground and looking like a meadow are different bars, and
/// the second one is higher — twice now the answer to "¿cómo se ve?" has been
/// "más densa", so the near ring sits far above its floor and the outer two
/// above theirs.
const RINGS: [Ring; 3] = [
    Ring {
        // 8 → 10 → **16**, las tres veces por el mismo reporte jugando: *"el
        // crecimiento del pasto está muy cerca del player"*. Este alcance es lo
        // único que decide **a qué distancia** ocurre el crecimiento, porque la
        // dispersión vive adentro del anillo: con 10 m la rampa caía entre 4 y
        // 10 metros, o sea a los pies del jugador, por corta que fuera.
        //
        // Lo que destrabó el paso fue que el usuario separara las dos cosas:
        // *"crece parejo a medida que uno camina, y crece bien — no sé qué es lo
        // que hay que arreglar ahí"*. O sea que el mecanismo
        // ([`GROWTH_RAMP_M`] y [`GROWTH_SPREAD_M`]) ya estaba bien y lo que
        // faltaba era **distancia**, que es otra perilla. Con 16 m la rampa vive
        // entre 10 y 16.
        //
        // **Se intentó no pagarlo y no alcanzó.** La hipótesis previa era que el
        // crecimiento se nota porque una brizna que se encoge destapa tierra, y
        // que con una textura de pradera en el suelo el contraste desaparecería.
        // La textura entró (y quedó buena), pero el veredicto fue *"la textura
        // no maquilla ningún problema que estamos intentando solucionar"*. El
        // fenómeno es geométrico, no de color.
        //
        // El costo es real: 347.600 → 600.000 triángulos declarados, +73%. El
        // área crece con el cuadrado del alcance y éste es el anillo más denso.
        reach_m: 16.0,
        // 10 y no 5, y el tamaño de chunk **no es cosmético**: decide cuánta
        // geometría se hornea fuera del anillo. Medido con este alcance, chunks
        // de 7 dan 509.488 triángulos y 103 draws; de 10, 600.000 y menos de
        // 100. Chunks chicos desperdician menos y cuestan más draws; grandes al
        // revés. Se eligió el lado que respeta `MOBILE_DRAWS`, porque un draw en
        // un tiler cuesta más que unos triángulos que el frustum va a tirar.
        chunk_m: 10.0,
        // 45 → 56 → **40**, y las tres las decidió el ojo del usuario jugando.
        // La última es la interesante: 56 le pareció más de lo necesario y 30
        // —el paso del barrido que probó— demasiado poco. 40 es su estimación
        // del punto justo, con una condición explícita: *"para que las texturas
        // hagan el resto de la pega"*. O sea que esta densidad no se sostiene
        // sola; asume que el suelo debajo aporta, y hoy el suelo sólo tiene un
        // tinte plano hacia [`ROOT_COLOR`]. Bajar más sin darle textura al
        // terreno primero es cómo se vuelve a un césped ralo sobre tierra.
        density: 40.0,
        width_scale: 1.0,
        split_tips: true,
    },
    Ring {
        // Corridos hacia afuera con el interior, manteniendo la proporción: si
        // los anillos se apretaran, el de en medio quedaría demasiado angosto
        // para absorber lo que el interior suelta en 6 m de dispersión.
        reach_m: 24.0,
        chunk_m: 15.0,
        // Sigue al anillo interior en la misma proporción, y no por prolijidad:
        // este anillo es el que **recibe** lo que el interior va soltando
        // durante los 6 m de dispersión. Si la razón entre los dos cambia, el
        // traspaso vuelve a leerse como un escalón de densidad moviéndose con
        // el jugador, que es la mitad del artefacto que `GROWTH_SPREAD_M` ataca
        // por el otro lado.
        density: 20.0,
        width_scale: 1.0,
        // También parte la punta, y no por gusto: como los anillos se solapan
        // durante la banda de transición, este empieza en 2 m — o sea que sus
        // briznas se mezclan con las del anillo interior justo delante de la
        // cámara. Con la punta recta, la mitad de las briznas cercanas se veían
        // distintas de la otra mitad.
        split_tips: true,
    },
    Ring {
        reach_m: 32.0,
        chunk_m: 20.0,
        // Ídem, y es el anillo más barato por brizna en pantalla —a 16-32 m una
        // brizna ocupa poquísimos píxeles— además del que más hace por la
        // sensación de que el campo sigue.
        density: 7.0,
        width_scale: 1.0,
        split_tips: false,
    },
];

/// Assumed camera height above the ground, in metres, for the coverage
/// derivation. Not the camera's real height — the rings are baked once and
/// cannot follow a moving lens — but the height it holds while walking, which
/// is the case the meadow has to look right in.
#[cfg(test)]
const EYE_HEIGHT_M: f32 = 1.6;

/// How far past its derived minimum each ring is planted. Blades land on a
/// hash, not on a lattice, so some clumping is certain and the bare patches it
/// would otherwise leave are the artefact this margin buys off.
#[cfg(test)]
const COVERAGE_MARGIN: f32 = 1.2;

/// Blades per m² needed at distance `d` for the ground not to show through,
/// given blades of `width`.
///
/// The derivation, and the most useful result in `BOTWGrass.md`: a blade of
/// height `H` seen from eye height `h` at distance `d` hides a strip of ground
/// `H · d / h` long — the angle flattens with distance — and `width` across. One
/// over that area is the density needed to cover the ground.
#[cfg(test)]
fn minimum_density(distance_m: f32, width_m: f32) -> f32 {
    let average_height = f32::midpoint(BLADE_HEIGHT_MIN, BLADE_HEIGHT_MAX);
    EYE_HEIGHT_M / (width_m * average_height * distance_m.max(0.5))
}

/// The widest a blade may get. Past roughly four times its authored width a
/// blade stops reading as a blade and starts reading as a card, which is the
/// billboard this system deliberately does not use.
#[cfg(test)]
const MAX_WIDTH_SCALE: f32 = 4.0;

/// The density the rings are written against, so the hub's dial can scale them
/// as a ratio instead of replacing them. Stepping the knob to 25/m² makes the
/// whole ladder 0.56× as thick and keeps its shape, which is what makes the
/// sweep readable: one variable moves, not four.
const REFERENCE_DENSITY: f32 = bof_domain::perf::GRASS_DENSITY_STEPS[0];

/// The reach scale the rings are written against, so the budget and the tests
/// measure the shipped field rather than whatever the dial happens to be on.
#[cfg(test)]
const REFERENCE_REACH: f32 = bof_domain::perf::GRASS_REACH_STEPS[0];

/// How many metres **one** blade takes to grow from nothing to its full height
/// as the camera approaches.
///
/// This is the anti-pop: without it, walking makes a wall of grass appear out of
/// nothing at the edge of a ring. Growth rather than fade because growth is
/// geometry — no blending, no `discard`, no draw order, and none of the early-Z
/// cost that alpha would bring on a tile-based GPU (`BOTWGrass.md`, law 3).
///
/// Short on purpose, and that is the opposite of what it looks like it should
/// be. A blade that takes a metre to grow is one blade among hundreds doing
/// nothing remarkable; what the eye catches is a *band* of them growing
/// together. The width of the phenomenon is [`GROWTH_SPREAD_M`], not this.
const GROWTH_RAMP_M: f32 = 1.0;

/// Over how many metres, inward from each ring's edge, the blades' individual
/// growth thresholds are spread by their hash.
///
/// **This is the fix for "el crecimiento del pasto está muy cerca y es muy
/// notorio", and it took two tries to find because one constant was doing two
/// jobs.** The first attempt shortened the single band from 8 m to 3 m, which
/// moved the growth further out — and made it *more* noticeable, because a
/// narrow band is a sharper wave. Both readings were right: the growth was too
/// close *and* too abrupt, and no single number can fix both.
///
/// Split, they pull in opposite directions and each does its own job. At any
/// given distance a different fraction of the ring is alive, so what the eye
/// reads is density falling off with distance — which is what a real field does
/// — instead of a front of grass sprouting as the player walks into it. The
/// ring behind picks up the density that this one drops, and that is why
/// `handover` in `ring_cells_with_slack` measures against this constant.
const GROWTH_SPREAD_M: f32 = 6.0;

/// How far **below** the ground a blade collapses to, in metres.
///
/// **This is the flicker.** Reported playing three times; the third time with
/// the description that solved it — *"unos pastos que parecen pegados en el piso
/// que parpadean"*. Collapsing a blade toward `ground_y` does not make it
/// vanish: its four vertices reach ground level while the tip keeps its
/// horizontal offset (the baked lean, up to [`BLADE_LEAN`], plus the wind), so
/// what is left is a flat quad **lying coplanar with the terrain**. That
/// z-fights, and the wind shakes it, so it flickers.
///
/// Two diagnoses were wrong before this one, and both are worth remembering.
/// The chunk hysteresis ([`KEEP_SLACK_M`]) fixed a real thrashing bug that was
/// *not* this. And MSAA — the standing hypothesis for two days, now measured at
/// 2,48 ms of frame — would not have touched it: z-fighting is not edge
/// aliasing.
///
/// Sinking the collapse point buries the blade before it degenerates and lets
/// the terrain hide it by depth. It also produces the effect the system wanted
/// all along: the blade **sprouts out of the ground** instead of appearing
/// flattened on it. A blade of height `H` breaks the surface once its growth
/// passes `sink / (H + sink)` — with these numbers, the first fifth of the ramp
/// happens underground.
const GROWTH_SINK_M: f32 = 0.18;

/// At most one chunk is baked per frame **while rolling**.
///
/// Re-baking while the player walks is the only per-frame work this system will
/// ever have, and a frame spike at a chunk boundary would be exactly the kind of
/// stutter the whole design exists to avoid. One per frame means crossing a
/// boundary costs one chunk, not a ring.
const CHUNKS_BAKED_PER_FRAME: usize = 1;

/// Filling an empty grid ignores that limit and bakes the lot in one frame.
///
/// The two cases look alike and are opposites. Rolling is a grid that is already
/// right needing one chunk at its edge, and there the budget is everything.
/// Filling is a grid with nothing in it — pacing that at one chunk per frame
/// does not protect the frame rate, it just makes the meadow grow in from
/// nowhere over several seconds while the player watches. Played on 2026-08-05
/// and reported as "cargó extremadamente lento", which is exactly what it was.
const FILL_IN_ONE_FRAME: bool = true;

/// Blade shape. Wide enough at the base to cover ground, tapered at the tip so
/// it reads as a leaf rather than a strip of paper.
const BLADE_WIDTH: f32 = 0.055;
/// Fraction of the base width kept at the tip.
///
/// Was 0,35 until 2026-08-06 and the tip read as **cut square** rather than
/// pointed — reported playing. 0,18 leaves a point without the two halves of the
/// notch degenerating into slivers.
const BLADE_TIP_TAPER: f32 = 0.18;
/// How far down from the tip the notch between the two points sits, as a
/// fraction of the blade's height. Deep enough to read as two points at arm's
/// length, shallow enough not to turn the blade into a fork.
///
/// Lowered from 0,82 with the taper above: with a narrow tip the notch has to
/// bite deeper to be visible at all, because the two points it separates are now
/// close together.
const TIP_NOTCH_DEPTH: f32 = 0.72;
/// Blade height range in metres, picked per blade.
///
/// Raised from 0,26-0,52 on 2026-08-06: reported playing as "las briznas son muy
/// pequeñas", and it was true — half a metre at the very top is shin-high on a
/// 1,8 m capsule, so the field read as a mown lawn instead of a meadow. These
/// reach the knee and the hip.
///
/// **The ceiling is one metre and it is a hard one**, not taste: the blade's
/// height travels packed in the fraction of `uv1.y`, with the ring's reach in
/// the whole part, and the shader splits them with `floor`/`fract`. A blade of
/// one metre or more would be read as the next ring's reach. A test pins it.
const BLADE_HEIGHT_MIN: f32 = 0.45;
const BLADE_HEIGHT_MAX: f32 = 0.90;
/// How far a tip may lean off vertical, in metres, so the field is not a bed of
/// nails. Deterministic per blade — this is authored variety, not animation.
///
/// Scaled with the height above: kept at 0,16 the taller blades stood
/// noticeably straighter than the short ones used to, which is the uniformity
/// this constant exists to break.
const BLADE_LEAN: f32 = 0.27;

/// Root and tip colours. The root-to-tip gradient is the single biggest reason
/// BOTW grass reads as grass; it used to be baked into a vertex colour and now
/// travels as two uniforms, because it is a pure function of the vertex's
/// height along the blade and a `mix` in the shader costs nothing per frame
/// while sixteen bytes per vertex cost bandwidth on every one of them.
/// **The criterion is the ground the blades stand in**, which on 2026-08-06 is
/// `T_GroundSoil_Albedo.png` — hue 84°, saturation 37%. Not a taste: wherever
/// the field thins, the eye sees blade and ground side by side, and a blade that
/// disagrees with the soil it grows out of reads as two materials rather than
/// one meadow. That claim is checkable by walking to a sparse patch, which is
/// what makes it worth writing down.
///
/// | | tono | lum | sat |
/// |---|---:|---:|---:|
/// | suelo bajo el campo (Soil) | 84° | 43% | 37% |
/// | raíz, antes | 100° | 43% | **22%** |
/// | raíz, ahora | 82° | 31% | 37% |
/// | punta, antes | 90° | 68% | 55% |
/// | punta, ahora | 84° | 57% | 56% |
///
/// The **root** was the defect: 16° off the soil's hue and half its saturation.
/// A field seen from standing height is mostly canopy, so that one colour is
/// what made the whole meadow read as a pale haze instead of grass. The tip only
/// moved enough to join the same hue family; its lightness is what still
/// separates a blade from the floor.
///
/// **Two corrections to how this was arrived at, kept because they cost a round
/// of the user's trust.** The first pair was read off `T_GroundGrass_Albedo.png`
/// — a file that *is not one of the four canonical textures* and never reaches
/// the scene. That it landed close anyway is luck, not method. And the target
/// itself was picked by eye ("that mesh reads better as grass") and then
/// measured against with three decimals, which dresses a preference as a
/// finding. The hue of the soil under the field is a fact; "reads better" was
/// not.
///
/// Unrelated and still open: `T_GroundTallGrass_Albedo.png` sits at **hue 113°**,
/// 29° away from the soil beside it. Two ground textures in one scene that far
/// apart is an art defect this palette cannot fix from here.
pub(super) const ROOT_COLOR: LinearRgba = LinearRgba::rgb(0.093, 0.147, 0.031);
const TIP_COLOR: LinearRgba = LinearRgba::rgb(0.340, 0.622, 0.089);

/// One baked chunk of the meadow: a single mesh holding all its blades.
#[derive(Component)]
pub(super) struct GrassChunk;

/// Which chunk of which ring. The ring is part of the identity because the same
/// patch of ground is covered by different chunks at different distances.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ChunkKey {
    ring: usize,
    cell: IVec2,
}

/// The live grid: which chunks exist right now, and the one material they share.
///
/// One material for the whole meadow, cloned per chunk — the blades batch, and a
/// second material would double the draws for nothing.
#[derive(Resource)]
pub(super) struct GrassField {
    material: Handle<GrassMaterial>,
    live: HashMap<ChunkKey, Entity>,
}

/// Triángulos que la pradera declara a la escena: cada brizna son **dos**.
///
/// Sólo lo consume el presupuesto declarado (`perf::budget`), que es un test: el
/// contador de runtime cuenta lo que la cámara ve, no lo que la escena declara.
/// Mientras este número vivió dentro del módulo, la meadow no la sumaba nadie y
/// una escena con pradera leía como si el pasto fuera gratis.
///
/// Con la grilla rodante ya no es "el campo": es **lo que hay alrededor de la
/// cámara en cualquier momento**, que es constante y no depende del tamaño del
/// mapa. Ese cambio es el punto entero del Paso 4.
#[cfg(test)]
pub(crate) fn meadow_triangles() -> usize {
    // Not `blades * 2`: a notched tip is three triangles, and the two inner
    // rings have one. A budget that assumed two everywhere would under-declare
    // the meadow by a third of its near geometry — the kind of quiet error a
    // budget exists to make loud.
    let period = RINGS
        .iter()
        .map(|ring| ring.chunk_m)
        .fold(0.0_f32, f32::max);
    let mut worst = 0;
    for z in 0..8 {
        for x in 0..8 {
            let focus = Vec2::new(x as f32, z as f32) * (period / 8.0);
            let triangles: usize = RINGS
                .iter()
                .enumerate()
                .map(|(index, ring)| {
                    let per_blade = if ring.split_tips { 3 } else { 2 };
                    ring_cells(index, focus, REFERENCE_REACH).len()
                        * blades_per_chunk(ring, REFERENCE_DENSITY) as usize
                        * per_blade
                })
                .sum();
            worst = worst.max(triangles);
        }
    }
    worst
}

/// The most blades the meadow can ever have standing at once.
///
/// The count wobbles with where the camera falls against the chunk grid: on a
/// boundary a ring needs an extra row of chunks that it does not need in the
/// middle of a cell. An earlier version of this file declared the cost at the
/// origin and asserted in a comment that the origin was the worst alignment,
/// because every ring's grid meets there. **That was wrong** — measured, the
/// origin gives 82.000 blades and an offset camera gives 101.725. A budget that
/// takes the best case and calls it the worst is worse than no budget.
///
/// So it is swept: every alignment inside one cell of the largest chunk, which
/// is the period after which the pattern repeats.
#[cfg(test)]
fn worst_case_blades() -> usize {
    let period = RINGS
        .iter()
        .map(|ring| ring.chunk_m)
        .fold(0.0_f32, f32::max);
    let steps = 8;
    let mut worst = 0;
    for z in 0..steps {
        for x in 0..steps {
            let offset = Vec2::new(x as f32, z as f32) * (period / steps as f32);
            worst = worst.max(neighbourhood_blades(offset));
        }
    }
    worst
}

/// Blades standing around a camera at `focus`.
#[cfg(test)]
fn neighbourhood_blades(focus: Vec2) -> usize {
    RINGS
        .iter()
        .enumerate()
        .map(|(index, ring)| {
            ring_cells(index, focus, REFERENCE_REACH).len()
                * blades_per_chunk(ring, REFERENCE_DENSITY) as usize
        })
        .sum()
}

/// Blades in one chunk of `ring` at a given dial setting. Rounded once, here, so
/// the count on screen and the count in the budget are the same number.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "density is clamped non-negative and a blade count is an integer bucket"
)]
fn blades_per_chunk(ring: &Ring, dial: f32) -> u32 {
    let density = ring.density * (dial / REFERENCE_DENSITY);
    (ring.chunk_m * ring.chunk_m * density).round().max(0.0) as u32
}

/// Which cells of `ring` should exist with the camera at `focus`.
///
/// A chunk is in if it *touches* the ring's square, and out only if the ring
/// inside already covers it *entirely*. Both halves matter, and the second one
/// is the subtle one: judging a chunk by its centre looks equivalent and is not.
/// A 10 m chunk whose centre falls just inside the inner ring gets dropped
/// whole, while the inner ring — made of smaller chunks — never reaches the far
/// side of it. What is left is a hole of bare dirt a few metres from the player,
/// which is the exact artefact this system exists to prevent. The overlap this
/// criterion allows instead is a strip up to half a chunk wide that is a little
/// denser than derived, and denser is not a defect.
fn ring_cells(index: usize, focus: Vec2, reach_scale: f32) -> Vec<IVec2> {
    ring_cells_with_slack(index, focus, 0.0, reach_scale)
}

/// A ring's reach with the reach dial applied, in **whole metres**.
///
/// Whole, and that is a hard constraint rather than tidiness: the reach travels
/// in the integer part of `uv1.y` with the blade's height in the fraction, and
/// the shader splits them with `floor`/`fract`. A reach of 11,5 m would make
/// every blade in the ring report a height 0,5 m taller than it is. Never below
/// one metre, so a dial step can shrink a ring but not erase it.
fn ring_reach(index: usize, reach_scale: f32) -> f32 {
    (RINGS[index].reach_m * reach_scale).round().max(1.0)
}

/// How far past its reach a chunk is kept before being dropped.
///
/// Without it the grid thrashes: a camera sitting on a grid line puts a chunk
/// just inside the boundary on one frame and just outside on the next, so it is
/// despawned and re-baked over and over — which on screen is a patch of grass
/// flickering. Creating uses the exact reach and keeping uses reach + this, so a
/// chunk has to be clearly outside before it goes.
const KEEP_SLACK_M: f32 = 3.0;

#[expect(
    clippy::cast_possible_truncation,
    reason = "chunk coordinates are small integers by construction"
)]
fn ring_cells_with_slack(index: usize, focus: Vec2, slack: f32, reach_scale: f32) -> Vec<IVec2> {
    let ring = &RINGS[index];
    let reach_m = ring_reach(index, reach_scale) + slack;
    let inner_reach = index
        .checked_sub(1)
        .map_or(0.0, |i| ring_reach(i, reach_scale));
    let half = ring.chunk_m * 0.5;
    // One cell of slack: a chunk can touch the ring while its centre sits
    // outside it.
    let span = (reach_m / ring.chunk_m).ceil() as i32 + 1;
    let base = (focus / ring.chunk_m).floor().as_ivec2();

    let mut cells = Vec::new();
    for dz in -span..=span {
        for dx in -span..=span {
            let cell = base + IVec2::new(dx, dz);
            let offset = (cell_centre(cell, ring.chunk_m) - focus).abs();
            // Chebyshev, because the chunks are a square grid: a square boundary
            // is the one that never cuts a chunk in half.
            let nearest = (offset - Vec2::splat(half)).max(Vec2::ZERO).max_element();
            let farthest = (offset + Vec2::splat(half)).max_element();
            // El anillo de afuera empieza *antes* de donde termina el de
            // adentro, por el ancho de la dispersión: en esa franja el interior
            // se va raleando —a cada distancia sobrevive una fracción menor— y
            // el exterior ya está entero, así que la densidad total cruza de una
            // a otra sin escalón. Contra `GROWTH_SPREAD_M` y no contra la rampa,
            // porque la primera brizna del interior empieza a irse al principio
            // de la dispersión, no al final.
            let handover = (inner_reach - GROWTH_SPREAD_M).max(0.0);
            if nearest > reach_m || farthest <= handover {
                continue;
            }
            cells.push(cell);
        }
    }
    cells
}

fn cell_centre(cell: IVec2, chunk_m: f32) -> Vec2 {
    (cell.as_vec2() + Vec2::splat(0.5)) * chunk_m
}

/// Scene entry: start from an empty grid.
///
/// The chunks themselves are `DespawnOnExit`, so leaving a scene already killed
/// them; what this clears is the bookkeeping that would otherwise point at dead
/// entities and make the next scene think its meadow was already built.
pub(super) fn reset_meadow(mut field: ResMut<GrassField>) {
    field.live.clear();
}

pub(super) fn init_meadow_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<GrassMaterial>>,
) {
    commands.insert_resource(GrassField {
        material: materials.add(grass_material()),
        live: HashMap::default(),
    });
}

/// The meadow's material: PBR plus the grass extension.
///
/// `ExtendedMaterial` rather than a pipeline of our own — lighting, shadows,
/// fog and decals keep working, and what the extension owns is only where the
/// base colour and the normal come from.
fn grass_material() -> GrassMaterial {
    GrassMaterial {
        base: StandardMaterial {
            // The extension writes `base_color` per fragment; white here means
            // nothing tints the gradient behind its back.
            base_color: Color::WHITE,
            // Blades are flat quads seen from every side, so both faces must
            // draw — unlike tree bark, where double-siding was pure waste.
            cull_mode: None,
            double_sided: true,
            perceptual_roughness: 0.95,
            reflectance: 0.03,
            ..default()
        },
        extension: GrassExtension {
            grass_data: GrassUniform {
                root_color: ROOT_COLOR,
                tip_color: TIP_COLOR,
                ..default()
            },
            interaction_map: None,
        },
    }
}

/// Roll the grid: drop the chunks that fell behind, bake the ones ahead.
///
/// Reads the camera rather than the player because the LOD has to answer to what
/// the screen shows: zoom out or swing the camera away and it is the camera's
/// neighbourhood that needs blades, not the player's.
#[expect(
    clippy::too_many_arguments,
    reason = "baking a chunk needs the terrain, the assets, the dial and the scene"
)]
pub(super) fn roll_meadow_grid(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut field: ResMut<GrassField>,
    perf: Res<crate::perf::PerfToggles>,
    scene: Res<State<crate::scene::AppState>>,
    terrain: TerrainAccess,
    camera: Option<Single<&GlobalTransform, With<Camera3d>>>,
    mut dial: Local<Option<(usize, usize)>>,
) {
    let Some(camera) = camera else {
        return;
    };
    let focus = camera.translation().xz();
    let density = perf.grass_density();
    let reach_scale = perf.grass_reach_scale();

    // Density and reach are both **baked into the mesh** — the blade count in the
    // geometry, the ring's reach in a vertex attribute — so neither can be
    // rolled into. They are the one event that clears the grid instead of
    // rolling it, and they are checked together because a run that changed both
    // and rebuilt for one would leave half the field describing the old dial.
    let dials = (perf.grass_density_step, perf.grass_reach_step);
    if dial.replace(dials) != Some(dials) {
        for entity in field.live.values() {
            commands.entity(*entity).despawn();
        }
        field.live.clear();
    }

    let wanted: HashSet<ChunkKey> = RINGS
        .iter()
        .enumerate()
        .flat_map(|(ring, _)| {
            ring_cells(ring, focus, reach_scale)
                .into_iter()
                .map(move |cell| ChunkKey { ring, cell })
        })
        .collect();

    // Kept with slack, created without: a chunk on the boundary stays instead of
    // being re-baked every other frame, which is what the flicker was.
    let keep_set: HashSet<ChunkKey> = RINGS
        .iter()
        .enumerate()
        .flat_map(|(ring, _)| {
            ring_cells_with_slack(ring, focus, KEEP_SLACK_M, reach_scale)
                .into_iter()
                .map(move |cell| ChunkKey { ring, cell })
        })
        .collect();

    field.live.retain(|key, entity| {
        let keep = keep_set.contains(key);
        if !keep {
            commands.entity(*entity).despawn();
        }
        keep
    });

    // An empty grid is being filled, not rolled: bake it whole rather than
    // letting the meadow grow in around the player over several seconds.
    let budget = if FILL_IN_ONE_FRAME && field.live.is_empty() {
        usize::MAX
    } else {
        CHUNKS_BAKED_PER_FRAME
    };
    // Collected before the loop so the bake can borrow the field mutably.
    let missing: Vec<ChunkKey> = wanted
        .iter()
        .filter(|key| !field.live.contains_key(*key))
        .take(budget)
        .copied()
        .collect();
    for key in &missing {
        let ring = &RINGS[key.ring];
        // Seeded by ring and cell, not by spawn order: the same ground grows the
        // same blades every session, and walking away and back does not reshuffle
        // the field.
        let ring_salt = u32::try_from(key.ring).unwrap_or(0);
        let seed = hash_u32(
            key.cell.x.cast_unsigned().wrapping_mul(0x9e37_79b9)
                ^ key.cell.y.cast_unsigned().wrapping_mul(0x85eb_ca6b)
                ^ ring_salt.wrapping_mul(0xc2b2_ae35),
        );
        let mesh = build_chunk_mesh(
            &ChunkSpec {
                centre: cell_centre(key.cell, ring.chunk_m),
                chunk_m: ring.chunk_m,
                count: blades_per_chunk(ring, density),
                width_scale: ring.width_scale,
                split_tips: ring.split_tips,
                ring_reach_m: ring_reach(key.ring, reach_scale),
                seed,
            },
            Some(&terrain),
        );
        let entity = commands
            .spawn((
                DespawnOnExit(*scene.get()),
                Name::new(format!(
                    "GrassChunk_r{}_{}_{}",
                    key.ring, key.cell.x, key.cell.y
                )),
                GrassChunk,
                // Su cuenta de triángulos es una decisión, no un descuido: el
                // watchdog de mallas pesadas es para assets, y el presupuesto de
                // la pradera se cobra en `perf::budget`.
                crate::visuals::budget::BakedByDesign,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(field.material.clone()),
                // Blades cast no shadows: thousands of alpha-free slivers in the
                // cascades buy noise, not depth.
                bevy::light::NotShadowCaster,
                // And they receive none either. Shadows were the only lever
                // outside the noise floor in the 2026-07-25 measurement of this
                // box (−0,66 ms), and receiving is the expensive half: a shadow
                // map sample per fragment, over the geometry with more overdraw
                // than anything else in the scene. The blades keep their shape
                // from the root-to-tip gradient and the bowed normal, not from
                // self-shadowing.
                //
                // The cost is real and worth naming: grass under a tree is lit
                // as if the tree were not there. In the `Pasto` box there are no
                // trees; in the world there are, and that is the call to revisit
                // if it reads wrong.
                bevy::light::NotShadowReceiver,
                Transform::default(),
            ))
            .id();
        field.live.insert(*key, entity);
    }
}

/// Tell the shader where the camera is, so the outermost blades can shrink
/// before their chunk disappears.
///
/// One material for the whole meadow means one uniform write per frame, not one
/// per chunk.
pub(super) fn track_meadow_focus(
    field: Res<GrassField>,
    mut materials: ResMut<Assets<GrassMaterial>>,
    camera: Option<Single<&GlobalTransform, With<Camera3d>>>,
    sun: Option<Single<&GlobalTransform, With<DirectionalLight>>>,
    time: Res<Time>,
) {
    let Some(camera) = camera else {
        return;
    };
    let Some(mut material) = materials.get_mut(&field.material) else {
        return;
    };
    let data = &mut material.extension.grass_data;
    data.focus_xz = camera.translation().xz();
    data.growth_ramp = GROWTH_RAMP_M;
    data.growth_spread = GROWTH_SPREAD_M;
    data.growth_sink = GROWTH_SINK_M;
    // The wind is a function of world position and time — there is no per-blade
    // state anywhere, which is why a field of a hundred thousand blades costs
    // one uniform write a frame.
    data.time = time.elapsed_secs_wrapped();
    // Backlit transmission needs to know where the sun is; reading the light's
    // own transform rather than a copy keeps day/night driving it for free.
    if let Some(sun) = sun {
        data.sun_direction = sun.back().as_vec3();
    }
}

/// Bake `count` blades into one mesh, in world space, around `centre`.
///
/// Vertices carry world positions (the chunk entity sits at the origin), so a
/// blade sits on the ground wherever the terrain put it without the chunk having
/// to track a height of its own.
/// Everything one chunk needs to know about itself to bake its blades.
struct ChunkSpec {
    centre: Vec2,
    chunk_m: f32,
    count: u32,
    width_scale: f32,
    split_tips: bool,
    ring_reach_m: f32,
    seed: u32,
}

fn build_chunk_mesh(spec: &ChunkSpec, terrain: Option<&TerrainAccess>) -> Mesh {
    let ChunkSpec {
        centre,
        chunk_m,
        count,
        width_scale,
        split_tips,
        ring_reach_m,
        seed,
    } = *spec;
    let capacity = count as usize;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(capacity * 5);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(capacity * 5);
    let mut blade_data: Vec<[f32; 2]> = Vec::with_capacity(capacity * 5);
    let mut indices: Vec<u32> = Vec::with_capacity(capacity * 9);

    for blade in 0..count {
        let hash = hash_u32(seed ^ blade.wrapping_mul(0x0019_6f3d));
        let u1 = hash_unit(hash);
        let u2 = hash_unit(hash ^ 0x1234_5678);
        let u3 = hash_unit(hash ^ 0x8765_4321);
        let u4 = hash_unit(hash ^ 0xdead_beef);
        let u5 = hash_unit(hash ^ 0x0f0f_0f0f);

        let xz = centre + Vec2::new(u1 - 0.5, u2 - 0.5) * chunk_m;
        let ground = terrain.and_then(|t| t.height_at(xz)).unwrap_or(0.0);
        let slope = terrain.and_then(|t| t.slope_deg_at(xz)).unwrap_or(0.0);
        // No terrain means a test harness, and a test wants blades: flat soil
        // is what the real default cell is anyway.
        let kind = terrain
            .and_then(|t| t.kind_at(xz))
            .unwrap_or(crate::world::TerrainKind::Soil);
        // One rule, shared with the terrain tint — see `grass_cover`. Bare
        // ground here has to be bare ground there or the field grows an edge.
        let cover = grass_cover::coverage(kind, slope);
        if cover <= 0.0 {
            continue;
        }

        let height = (BLADE_HEIGHT_MIN + u4 * (BLADE_HEIGHT_MAX - BLADE_HEIGHT_MIN)) * cover;
        let yaw = u3 * std::f32::consts::TAU;
        // The quad's width runs along `side`; the lean tips it over `side`'s
        // perpendicular so blades do not all fall the same way.
        let side = Vec2::new(yaw.cos(), yaw.sin()) * (BLADE_WIDTH * width_scale * 0.5);
        let lean = Vec2::new(-yaw.sin(), yaw.cos()) * ((u5 - 0.5) * 2.0 * BLADE_LEAN);

        let base = Vec3::new(xz.x, ground, xz.y);
        let tip = base + Vec3::new(lean.x, height, lean.y);
        let taper = BLADE_TIP_TAPER;
        let Ok(first) = u32::try_from(positions.len()) else {
            error!("grass chunk exceeded the u32 mesh-index limit");
            break;
        };

        positions.push([base.x - side.x, base.y, base.z - side.y]);
        positions.push([base.x + side.x, base.y, base.z + side.y]);
        positions.push([tip.x + side.x * taper, tip.y, tip.z + side.y * taper]);
        positions.push([tip.x - side.x * taper, tip.y, tip.z - side.y * taper]);

        // The uv is not a texture coordinate — nothing here samples one. It
        // carries the two things the shader cannot reconstruct from a position:
        //
        // - `y` is the vertex's height along the blade, 0 at the root and 1 at
        //   the tip. The colour gradient reads it, and the wind and the trample
        //   map will multiply their displacement by it.
        // - `x` is the ground height under the blade, which is what lets the
        //   shader collapse a blade toward its own root for the distance fade
        //   without a per-vertex blade height.
        //
        // The normal and the vertex colour that used to sit beside them are
        // gone: both are constants of the system, rebuilt in `grass.wgsl`.
        uvs.push([base.y, 0.0]);
        uvs.push([base.y, 0.0]);
        uvs.push([base.y, 1.0]);
        uvs.push([base.y, 1.0]);

        // Per-blade data the shader cannot derive from a vertex position, packed
        // into eight bytes:
        //
        // - `x` is the blade's hash with the *side* of the quad in its sign:
        //   magnitude picks this blade's colour and its wind phase, sign says
        //   which edge the vertex is on so the normal can bow outward across the
        //   width. Two values in one channel because they are both cheap and
        //   neither deserves four bytes of its own.
        // - `y` is the blade's height in metres, so the wind can move a tall
        //   blade further than a short one instead of shearing the whole field
        //   by the same amount.
        let tint = hash_unit(hash ^ 0x2545_f491);
        // `y` packs two numbers into one channel: the ring's reach in whole
        // metres and the blade's height in the fraction. A blade is never a
        // metre tall and a reach is always a whole number, so `floor` and
        // `fract` separate them exactly — which is cheaper than an attribute
        // nobody would otherwise need.
        let packed = ring_reach_m + height;
        blade_data.push([-tint, packed]);
        blade_data.push([tint, packed]);
        blade_data.push([tint, packed]);
        blade_data.push([-tint, packed]);

        if split_tips {
            // The notched tip: a fifth vertex dipping between the two corners,
            // so the blade ends in two points instead of a flat cut. Up close a
            // straight-topped quad reads as a strip of paper — this is the one
            // shape cue that says "leaf" at arm's length, and it costs one
            // triangle on the only ring where anyone can see it.
            let notch = base.lerp(tip, TIP_NOTCH_DEPTH);
            positions.push([notch.x, notch.y, notch.z]);
            uvs.push([base.y, TIP_NOTCH_DEPTH]);
            blade_data.push([tint, packed]);
            let notch_index = first + 4;
            indices.extend_from_slice(&[first, first + 1, first + 2]);
            indices.extend_from_slice(&[first, first + 2, notch_index]);
            indices.extend_from_slice(&[first, notch_index, first + 3]);
        } else {
            indices.extend_from_slice(&[first, first + 1, first + 2]);
            indices.extend_from_slice(&[first, first + 2, first + 3]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, blade_data);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-ring chunk for the shape tests, so each of them names only what it
    /// actually cares about.
    fn spec(count: u32, split_tips: bool, seed: u32) -> ChunkSpec {
        ChunkSpec {
            centre: Vec2::ZERO,
            chunk_m: 5.0,
            count,
            width_scale: 1.0,
            split_tips,
            ring_reach_m: 8.0,
            seed,
        }
    }

    #[test]
    fn the_density_knob_is_what_actually_lands_on_the_ground() {
        // The failure this system was built to fix: a density that reads well in
        // a constant but arrives on screen divided by twenty. Stated against the
        // ring rather than a literal, so tuning the density cannot break the
        // test that guards it — the count must always be area × density.
        for ring in &RINGS {
            let per_chunk = blades_per_chunk(ring, REFERENCE_DENSITY);
            let expected = (ring.chunk_m * ring.chunk_m * ring.density).round();
            assert_eq!(
                (per_chunk as f32).to_bits(),
                expected.to_bits(),
                "the ring density must reach the ground intact"
            );
        }
    }

    /// The dial scales the ladder instead of flattening it, or the sweep would
    /// be measuring a different shape at every step.
    #[test]
    fn the_dial_scales_every_ring_by_the_same_ratio() {
        let sparse = bof_domain::perf::GRASS_DENSITY_STEPS[2];
        let ratio = sparse / REFERENCE_DENSITY;
        for ring in &RINGS {
            let full = f64::from(blades_per_chunk(ring, REFERENCE_DENSITY));
            let thin = f64::from(blades_per_chunk(ring, sparse));
            assert!(
                (thin - full * f64::from(ratio)).abs() <= 1.0,
                "ring at {} m does not follow the dial",
                ring.reach_m
            );
        }
    }

    /// The rings have to tile the ground, not stack on it: a cell covered by two
    /// rings is a patch paying twice for grass nobody asked for.
    #[test]
    fn no_point_inside_the_reach_is_left_uncovered() {
        let focus = Vec2::new(3.7, -11.2);
        let covered: Vec<(Vec2, f32)> = RINGS
            .iter()
            .enumerate()
            .flat_map(|(index, ring)| {
                ring_cells(index, focus, REFERENCE_REACH)
                    .into_iter()
                    .map(move |cell| (cell_centre(cell, ring.chunk_m), ring.chunk_m * 0.5))
            })
            .collect();

        let outermost = RINGS[RINGS.len() - 1].reach_m;
        let mut along = -outermost;
        while along <= outermost {
            let mut across = -outermost;
            while across <= outermost {
                let point = focus + Vec2::new(along, across);
                let inside = covered.iter().any(|(centre, half)| {
                    let offset = (point - *centre).abs();
                    offset.x <= *half && offset.y <= *half
                });
                assert!(
                    inside,
                    "nothing covers {point:?}, {:.1} m from the camera",
                    (point - focus).length()
                );
                across += 1.7;
            }
            along += 1.7;
        }
    }

    /// The rings are derived from the coverage formula, not chosen by eye. A row
    /// planted below its own minimum shows bare ground, and would do it silently
    /// — the failure reads as "the grass is a bit thin over there".
    #[test]
    fn every_ring_meets_the_density_its_distance_demands() {
        for (index, ring) in RINGS.iter().enumerate() {
            // The demanding point of a ring is its *inner* edge: that is where
            // its blades are seen from closest and each one hides the least
            // ground.
            let inner = index.checked_sub(1).map_or(2.0, |i| RINGS[i].reach_m);
            let needed = minimum_density(inner, BLADE_WIDTH * ring.width_scale);
            assert!(
                ring.density >= needed * COVERAGE_MARGIN,
                "ring reaching {} m plants {}/m2 where {:.1}/m2 is the minimum at {inner} m",
                ring.reach_m,
                ring.density,
                needed * COVERAGE_MARGIN
            );
            // Deliberately no upper bound. There used to be one, pinning every
            // ring to its minimum plus a margin, and it did exactly what it was
            // told: the field covered the ground and looked like sparse spikes.
            // Surplus density is paid in overdraw and that is a real cost, but
            // it is one the measurement decides, not a test — this file's job is
            // to keep a ring from falling *below* what its distance demands.
            assert!(
                ring.width_scale <= MAX_WIDTH_SCALE,
                "a blade {}x its authored width reads as a card, not a blade",
                ring.width_scale
            );
        }
    }

    /// The whole point of the rolling grid: what it costs does not depend on
    /// where the camera stands, so the budget is a property of the view.
    /// The whole point of the rolling grid: what it costs is a property of the
    /// view, not of where in the world that view happens to be. A field that
    /// grew with distance from the origin would be the fixed square again,
    /// wearing a different shape.
    #[test]
    fn the_neighbourhood_costs_the_same_far_from_the_origin_as_near_it() {
        let declared = worst_case_blades();
        for focus in [
            Vec2::new(137.0, -488.0),
            Vec2::new(-2049.5, 903.25),
            Vec2::new(41_000.0, 41_000.0),
        ] {
            let count = neighbourhood_blades(focus);
            assert!(
                count <= declared,
                "at {focus:?} the meadow is {count} blades, over the {declared} declared \
                 at the origin — the declared cost has to be the worst case"
            );
            // And it does not collapse either: an alignment that shed most of
            // the field would mean the grid stops covering the ground.
            assert!(
                count * 3 >= declared * 2,
                "at {focus:?} the meadow shrinks to {count} of {declared} blades"
            );
        }
    }

    /// The declared cost is checked against its per-view ceiling — and its debt
    /// — in `perf::budget`, which is where the mobile budget lives. What this
    /// one keeps is the property that makes that number mean anything: the
    /// neighbourhood is bounded at all, so no camera position can make the
    /// meadow arbitrarily expensive.
    #[test]
    fn the_neighbourhood_is_bounded() {
        let blades = neighbourhood_blades(Vec2::ZERO);
        assert!(blades > 0, "a meadow with no blades is not a meadow");
        // El peor caso, por lo mismo que el conteo de briznas: la alineación
        // cómoda no es la que hay que aguantar.
        let period = RINGS
            .iter()
            .map(|ring| ring.chunk_m)
            .fold(0.0_f32, f32::max);
        let chunks: usize = (0..8)
            .flat_map(|z| (0..8).map(move |x| Vec2::new(x as f32, z as f32) * (period / 8.0)))
            .map(|focus| {
                (0..RINGS.len())
                    .map(|index| ring_cells(index, focus, REFERENCE_REACH).len())
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(0);
        assert!(
            chunks <= crate::perf::budget::MOBILE_DRAWS,
            "{chunks} chunks is over the {} draw budget before anything else draws",
            crate::perf::budget::MOBILE_DRAWS
        );
    }

    #[test]
    fn blades_are_deterministic_per_chunk() {
        // Same chunk, same blades: a field that reshuffles when it is rebuilt
        // makes every visual comparison worthless.
        let a = build_chunk_mesh(&spec(64, false, 11), None);
        let b = build_chunk_mesh(&spec(64, false, 11), None);
        assert_eq!(
            a.attribute(Mesh::ATTRIBUTE_POSITION)
                .map(|values| values.len()),
            b.attribute(Mesh::ATTRIBUTE_POSITION)
                .map(|values| values.len())
        );
    }

    /// The vertex carries only what cannot be derived. Normal and colour are
    /// constants of the system rebuilt in `grass.wgsl`; baking them again would
    /// silently put back the 28 bytes per vertex this system removed, and
    /// nothing on screen would show it.
    #[test]
    fn the_vertex_carries_only_position_and_the_two_derived_values() {
        let mesh = build_chunk_mesh(&spec(8, false, 5), None);
        assert!(mesh.attribute(Mesh::ATTRIBUTE_POSITION).is_some());
        assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
        assert!(
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL).is_none(),
            "the normal is +Y for every blade — the shader rebuilds it"
        );
        assert!(
            mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_none(),
            "the gradient is a function of uv.y between two uniforms"
        );
    }

    /// The shader reads `uv.y` as the height along the blade and `uv.x` as the
    /// ground under it, so the four vertices have to keep meaning root, root,
    /// tip, tip in that order.
    #[test]
    fn a_blades_four_vertices_keep_their_authored_order() {
        let mesh = build_chunk_mesh(&spec(1, false, 5), None);
        let bevy::mesh::VertexAttributeValues::Float32x2(uvs) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0).expect("uvs")
        else {
            panic!("uvs must be Float32x2");
        };
        assert_eq!(uvs[0][1], 0.0, "base-left sits at the root");
        assert_eq!(uvs[1][1], 0.0, "base-right sits at the root");
        assert_eq!(uvs[2][1], 1.0, "tip-right sits at the tip");
        assert_eq!(uvs[3][1], 1.0, "tip-left sits at the tip");
        for uv in uvs {
            assert_eq!(uv[0], 0.0, "flat test ground is at y = 0 for every vertex");
        }
    }

    /// The shader reads the blade hash out of the *sign* of `uv1.x`, so the two
    /// edges of a quad have to disagree in sign and agree in magnitude. Get this
    /// wrong and the normal bows the same way on both sides — which looks like
    /// nothing at all, and would be found by nobody.
    #[test]
    fn the_two_edges_of_a_blade_carry_the_same_hash_with_opposite_signs() {
        let mesh = build_chunk_mesh(&spec(1, false, 9), None);
        let bevy::mesh::VertexAttributeValues::Float32x2(data) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_1).expect("blade data")
        else {
            panic!("blade data must be Float32x2");
        };
        assert!(data[0][0] < 0.0 && data[1][0] > 0.0, "the edges disagree");
        assert!(
            (data[0][0].abs() - data[1][0].abs()).abs() < f32::EPSILON,
            "both edges belong to the same blade and share its hash"
        );
        for vertex in data {
            // `y` packs the ring's reach in the whole part and the blade's
            // height in the fraction — the shader splits them with floor/fract.
            let (reach, height) = (vertex[1].floor(), vertex[1].fract());
            assert_eq!(reach, 8.0, "the blade carries its own ring's reach");
            assert!(
                (BLADE_HEIGHT_MIN..=BLADE_HEIGHT_MAX).contains(&height),
                "every vertex carries its blade's height for the wind to scale"
            );
        }
    }

    /// The packing in `uv1.y` only works while a blade is under a metre tall,
    /// and the blades got a lot taller on 2026-08-06 — 0,90 m against a ceiling
    /// of 1,00. Raising the range one more notch is the kind of edit that looks
    /// like pure taste and silently makes every blade report the *next* ring's
    /// reach, which the shader would read as "you are inside your band" and
    /// collapse the whole field. Cheaper to fail here.
    #[test]
    fn a_blade_stays_under_the_metre_its_packing_allows() {
        // Const block: the packing is a compile-time property of the constants,
        // so the build is the right place to lose, not the test run.
        const {
            assert!(
                BLADE_HEIGHT_MAX < 1.0,
                "blade height rides in the fraction of uv1.y, next to the ring reach"
            );
            assert!(BLADE_HEIGHT_MIN > 0.0 && BLADE_HEIGHT_MIN < BLADE_HEIGHT_MAX);
        }
        for ring in &RINGS {
            assert!(
                (ring.reach_m - ring.reach_m.round()).abs() < f32::EPSILON,
                "a ring's reach is the whole part of the same number, so it must \
                 be a whole number of metres — {} is not",
                ring.reach_m
            );
        }
        // And the same has to hold for every position of the reach dial, which
        // is why `ring_reach` rounds instead of multiplying straight through.
        for scale in bof_domain::perf::GRASS_REACH_STEPS {
            for index in 0..RINGS.len() {
                let reach = ring_reach(index, scale);
                assert!(
                    (reach - reach.round()).abs() < f32::EPSILON && reach >= 1.0,
                    "ring {index} at {scale}x reaches {reach} m, which does not pack"
                );
            }
        }
    }

    /// Shrinking the reach has to shrink the *field*, not just the number: if
    /// the dial moved the reach but the cells were chosen against the authored
    /// value, the sweep would report that reach costs nothing at all.
    #[test]
    fn the_reach_dial_actually_removes_chunks() {
        let focus = Vec2::new(3.7, -12.1);
        let cells = |scale: f32| {
            (0..RINGS.len())
                .map(|index| ring_cells(index, focus, scale).len())
                .sum::<usize>()
        };
        let full = cells(REFERENCE_REACH);
        for scale in bof_domain::perf::GRASS_REACH_STEPS {
            if scale < REFERENCE_REACH {
                assert!(
                    cells(scale) < full,
                    "the dial at {scale}x kept every chunk the full reach did"
                );
            }
        }
    }

    /// The near ring's blades end in two points; the outer ones do not, because
    /// out there the extra triangle buys a silhouette nobody can resolve.
    #[test]
    fn only_split_tips_pay_for_the_extra_triangle() {
        let plain = build_chunk_mesh(&spec(1, false, 9), None);
        let split = build_chunk_mesh(&spec(1, true, 9), None);
        let triangles = |mesh: &Mesh| mesh.indices().map(|i| i.len() / 3).unwrap_or(0);
        assert_eq!(triangles(&plain), 2, "a plain blade is two triangles");
        assert_eq!(triangles(&split), 3, "a notched tip costs exactly one more");
        assert_eq!(plain.count_vertices(), 4);
        assert_eq!(split.count_vertices(), 5);
        assert!(
            RINGS[0].split_tips && !RINGS[RINGS.len() - 1].split_tips,
            "the notch belongs to the near ring and only there"
        );
    }

    #[test]
    fn a_blade_stands_on_the_ground_and_reaches_its_height() {
        let mesh = build_chunk_mesh(&spec(1, false, 3), None);
        let bevy::mesh::VertexAttributeValues::Float32x3(positions) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions")
        else {
            panic!("positions must be Float32x3");
        };
        let base = positions[0][1];
        let tip = positions[2][1];
        assert!(
            base.abs() < 0.001,
            "the root sits on flat ground, got {base}"
        );
        assert!(
            (BLADE_HEIGHT_MIN..=BLADE_HEIGHT_MAX).contains(&(tip - base)),
            "blade height {} outside its authored range",
            tip - base
        );
    }
}
