//! Scenes: **what exists, and when.**
//!
//! Until now every piece of the world was spawned in `Startup` and lived
//! forever — the graybox layout, 179 trees, 1,500 grass tufts, the player. That
//! made the terrain editor a lie: you could sculpt relief, but you were always
//! sculpting *under* a forest that `main.rs` decided to spawn, and a saved
//! heightfield was never "the level", only "the relief beneath the hardcoded
//! scene".
//!
//! Two lines this module draws.
//!
//! **Infrastructure vs. scene content.** Infrastructure lives for the whole
//! process: camera, UI panels, the arrow pool, loaded animation assets, the
//! editor's focus owner and HUD. Scene content is born and dies with its scene:
//! terrain, sky, lights, graybox geometry, the forest, the meadow, the player.
//! It carries [`DespawnOnExit`], so leaving a scene removes it with no cleanup
//! system to keep in sync.
//!
//! **Scenes are data, not code.** [`SCENES`] is a table: one row per scene,
//! naming its terrain file and ticking off what it contains. Adding a test box
//! is a row, not a module — the same andamiaje→dato move the project already
//! made for the graybox layout and the tree catalogue. The menu is generated
//! from the table, so it can never drift from what actually loads.
//!
//! Each scene owns **its own heightmap**, so sculpting the combat box cannot
//! disturb the traversal course, and the world scene keeps the real terrain. The
//! editor is not a scene: **F5 sculpts wherever you are** and `Ctrl+S` writes to
//! the current scene's file.

pub mod menu;

use bevy::prelude::*;
use bof_domain::scene::SceneScoped;

use crate::input::SetCursorGrab;

/// Which scene the app is in. `MainMenu` is the default, so the process no
/// longer boots straight into a world.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    Scene(SceneId),
}

/// The scenes on offer. Most are **test boxes**: the smallest world that lets
/// you judge one thing while you are building it, without the rest of the game
/// arguing with it. Building grass? Take the grass box — flat ground, a body to
/// walk with, and nothing else on screen to blame. When the feature is ready it
/// joins [`SceneId::World`], where everything is together.
///
/// Boxes are meant to come and go with the work. Adding one is a variant here
/// and a row in [`SCENES`]; deleting one when its feature lands is the same two
/// lines in reverse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneId {
    Traversal,
    Combat,
    Grass,
    CardMesh,
    Sandbox,
    World,
}

/// The pieces a scene can be built from — **one flag per visible system**, so a
/// box can take exactly the piece it is testing and leave everything else out.
/// That granularity is the whole point: "just the grass" has to be expressible,
/// or the box cannot isolate the thing you are working on.
///
/// Booleans rather than a list of systems: the table stays readable at a glance,
/// and a scene cannot ask for a piece nothing knows how to build.
#[derive(Debug, Clone, Copy)]
pub struct Contents {
    /// Boxes, walls, ramps, rock, ladder — the obstacle course.
    pub course: bool,
    /// Straight, curved and derived stairs.
    pub stairs: bool,
    /// Archery/melee practice targets.
    pub targets: bool,
    /// Ground items near spawn.
    pub pickups: bool,
    /// The deterministic forest.
    pub forest: bool,
    /// The grass meadow.
    pub meadow: bool,
    /// The isolated grass-card comparison lab.
    pub card_mesh_lab: bool,
    /// The graybox enemy pair.
    pub enemies: bool,
    /// A rideable horse.
    pub horse: bool,
}

impl Contents {
    /// Bare: terrain, a body and light. Every box starts here and adds.
    const NONE: Self = Self {
        course: false,
        stairs: false,
        targets: false,
        pickups: false,
        forest: false,
        meadow: false,
        card_mesh_lab: false,
        enemies: false,
        horse: false,
    };
}

/// One row per scene. **This table is the level list.**
pub struct SceneDef {
    pub id: SceneId,
    pub label: &'static str,
    pub hint: &'static str,
    /// The scene's own heightmap. Sculpting one scene never touches another.
    pub terrain_file: &'static str,
    pub contents: Contents,
}

pub const SCENES: &[SceneDef] = &[
    SceneDef {
        id: SceneId::Traversal,
        label: "Traversal",
        hint: "curso y escaleras — locomoción sin nada más en pantalla",
        terrain_file: "assets/game/world/traversal.ron",
        contents: Contents {
            course: true,
            stairs: true,
            ..Contents::NONE
        },
    },
    SceneDef {
        id: SceneId::Combat,
        label: "Combate",
        hint: "dianas, pickups y bokobos en campo abierto",
        terrain_file: "assets/game/world/combat.ron",
        contents: Contents {
            targets: true,
            pickups: true,
            enemies: true,
            ..Contents::NONE
        },
    },
    SceneDef {
        id: SceneId::Grass,
        label: "Pasto",
        hint: "solo la pradera — para trabajar el pasto sin nada que lo tape",
        terrain_file: "assets/game/world/grass.ron",
        contents: Contents {
            meadow: true,
            ..Contents::NONE
        },
    },
    SceneDef {
        id: SceneId::CardMesh,
        label: "Card mesh",
        hint: "cartas y referencias de pasto — comparar siluetas aisladas",
        terrain_file: "assets/game/world/card_mesh.ron",
        contents: Contents {
            card_mesh_lab: true,
            ..Contents::NONE
        },
    },
    SceneDef {
        id: SceneId::Sandbox,
        label: "Terreno",
        hint: "lienzo limpio: relieve, cuerpo y luz — esculpir y medir",
        terrain_file: "assets/game/world/sandbox.ron",
        contents: Contents::NONE,
    },
    SceneDef {
        id: SceneId::World,
        label: "Mundo",
        hint: "todo junto: curso, escaleras, bosque, pradera, enemigos, caballo",
        terrain_file: "assets/game/world/world.ron",
        contents: Contents {
            course: true,
            stairs: true,
            targets: true,
            pickups: true,
            forest: true,
            meadow: true,
            card_mesh_lab: false,
            enemies: true,
            horse: true,
        },
    },
];

impl SceneId {
    pub const ALL: [SceneId; 6] = [
        SceneId::Traversal,
        SceneId::Combat,
        SceneId::Grass,
        SceneId::CardMesh,
        SceneId::Sandbox,
        SceneId::World,
    ];

    /// The scene whose label matches `raw`, case- and accent-insensitively
    /// enough for typing it into an environment variable.
    pub fn from_label(raw: &str) -> Option<Self> {
        let raw = raw.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|id| id.def().label.to_lowercase() == raw)
    }

    /// The labels a human may type, for the error message when they mistype one.
    pub fn labels() -> String {
        Self::ALL
            .iter()
            .map(|id| id.def().label)
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn def(self) -> &'static SceneDef {
        match self {
            SceneId::Traversal => &SCENES[0],
            SceneId::Combat => &SCENES[1],
            SceneId::Grass => &SCENES[2],
            SceneId::CardMesh => &SCENES[3],
            SceneId::Sandbox => &SCENES[4],
            SceneId::World => &SCENES[5],
        }
    }
}

/// The scene currently being played, for systems that need to know *which* one
/// without matching on the whole state. Absent in the menu.
pub fn current_scene(state: &State<AppState>) -> Option<&'static SceneDef> {
    match state.get() {
        AppState::MainMenu => None,
        AppState::Scene(id) => Some(id.def()),
    }
}

/// Returns to the menu from any scene. **F10**, not Escape: Escape already
/// belongs to `input::cursor_control` (release the cursor, then quit), and
/// quietly repurposing it would break a key the developer already relies on.
const LEAVE_SCENE_KEY: KeyCode = KeyCode::F10;

/// Phases of building a scene. The ground has to exist before anything that
/// stands on it can be placed: with a flat floor a spawn point could be a
/// constant, but on sculpted relief an actor placed before the terrain lands
/// underground — and a heightfield does not catch you from below.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneBuild {
    /// Terrain, sky, static geometry.
    Ground,
    /// Actors and anything else positioned relative to the ground.
    Actors,
}

/// Run condition: the current scene declares this piece of content.
pub fn scene_has(wants: fn(&Contents) -> bool) -> impl Fn(Res<State<AppState>>) -> bool {
    move |state: Res<State<AppState>>| current_scene(&state).is_some_and(|def| wants(&def.contents))
}

/// Lee `BOF_SCENE`. Un nombre que no existe **no arranca en el menú en
/// silencio**: avisa y nombra las escenas válidas, porque pedir una caja y
/// recibir otra es exactamente el error que esta variable existe para evitar.
/// Pide la escena de `BOF_SCENE` desde `Startup`, y no con `insert_state`.
///
/// La diferencia costó un panic: arrancar ya dentro del estado hace correr el
/// `OnEnter` de la escena en el mismo frame que `Startup`, o sea antes de que
/// existan los recursos que ese `OnEnter` da por sentados — `reset_meadow`
/// murió buscando `GrassField`. Pidiéndolo acá la transición ocurre en el frame
/// siguiente, por exactamente el mismo camino que un clic en el menú.
fn enter_configured_boot_scene(mut next: ResMut<NextState<AppState>>) {
    if let Some(id) = configured_boot_scene() {
        next.set(AppState::Scene(id));
    }
}

fn configured_boot_scene() -> Option<SceneId> {
    let raw = std::env::var("BOF_SCENE").ok()?;
    match SceneId::from_label(&raw) {
        Some(id) => Some(id),
        None => {
            error!(
                "[scene] BOF_SCENE={raw} no nombra ninguna escena; hay: {}",
                SceneId::labels()
            );
            None
        }
    }
}

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>();
        // `BOF_SCENE=Pasto cargo run` arranca dentro de la caja en vez del menú.
        // Trabajar un sistema es entrar a su caja decenas de veces al día, y el
        // menú de por medio hace que la escena bajo prueba dependa de que nadie
        // se equivoque de botón — el 2026-08-06 se juzgó el pasto en el Mundo
        // por eso mismo.
        app.add_systems(Startup, enter_configured_boot_scene);
        app.add_plugins(menu::MenuPlugin);
        for id in SceneId::ALL {
            app.configure_sets(
                OnEnter(AppState::Scene(id)),
                (SceneBuild::Ground, SceneBuild::Actors).chain(),
            );
            // Simulation owns *how* the ground, a player, an enemy or a horse
            // is built; this table owns *when and which scene gets one*. The
            // spawn requests go through the same messages the debug hub writes,
            // so there is one spawn path, not two.
            app.add_systems(
                OnEnter(AppState::Scene(id)),
                setup_terrain.in_set(SceneBuild::Ground),
            );
            app.add_systems(
                OnEnter(AppState::Scene(id)),
                (
                    bof_simulation::player::spawn_player,
                    request_scene_enemies.run_if(scene_has(|c| c.enemies)),
                    request_scene_horse.run_if(scene_has(|c| c.horse)),
                )
                    .in_set(SceneBuild::Actors),
            );
        }
        app.add_systems(OnEnter(AppState::MainMenu), release_cursor);
        app.add_systems(OnExit(AppState::MainMenu), capture_cursor);
        app.add_systems(
            Update,
            leave_scene.run_if(not(in_state(AppState::MainMenu))),
        );
        // Simulation only declares that an entity is scene-scoped. This app
        // adapter binds it to the concrete state without leaking `AppState`
        // into the domain/simulation boundary.
        app.add_systems(PostUpdate, bind_scene_scoped_entities);
    }
}

/// Each scene names its own heightmap in [`SCENES`]; that file *is* the level.
/// Simulation loads and rebuilds it, this decides which one.
pub fn terrain_file(state: &State<AppState>) -> Option<&'static str> {
    current_scene(state).map(|def| def.terrain_file)
}

fn setup_terrain(mut commands: Commands, state: Res<State<AppState>>) {
    bof_simulation::world::spawn_terrain(&mut commands, terrain_file(&state));
}

fn request_scene_enemies(mut requests: MessageWriter<bof_simulation::enemies::BokoboSpawnRequest>) {
    requests.write(bof_simulation::enemies::BokoboSpawnRequest::Ensure);
}

fn request_scene_horse(mut requests: MessageWriter<bof_simulation::mounts::HorseSpawnRequest>) {
    requests.write(bof_simulation::mounts::HorseSpawnRequest::Ensure);
}

fn bind_scene_scoped_entities(
    mut commands: Commands,
    state: Res<State<AppState>>,
    unbound: Query<Entity, (With<SceneScoped>, Without<DespawnOnExit<AppState>>)>,
) {
    let current = *state.get();
    if !matches!(current, AppState::Scene(_)) {
        return;
    }
    for entity in &unbound {
        commands.entity(entity).insert(DespawnOnExit(current));
    }
}

/// The menu is a pointer UI, so it needs the cursor back. Routed through
/// `input`, which stays the single writer of `CursorOptions`.
fn release_cursor(mut grab: MessageWriter<SetCursorGrab>) {
    grab.write(SetCursorGrab(false));
}

fn capture_cursor(mut grab: MessageWriter<SetCursorGrab>) {
    grab.write(SetCursorGrab(true));
}

fn leave_scene(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<AppState>>) {
    if keys.just_pressed(LEAVE_SCENE_KEY) {
        next.set(AppState::MainMenu);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scene_id_has_exactly_one_row() {
        assert_eq!(SCENES.len(), SceneId::ALL.len());
        for id in SceneId::ALL {
            let matches = SCENES.iter().filter(|def| def.id == id).count();
            assert_eq!(matches, 1, "{id:?} should have exactly one row");
        }
    }

    #[test]
    fn every_scene_has_its_own_terrain_file() {
        // The point of per-scene heightmaps: sculpting one box must not disturb
        // another. Sharing a path would silently break that.
        let mut files: Vec<&str> = SCENES.iter().map(|def| def.terrain_file).collect();
        let total = files.len();
        files.sort_unstable();
        files.dedup();
        assert_eq!(files.len(), total, "two scenes share a terrain file");
    }

    #[test]
    fn a_test_box_carries_only_the_piece_it_is_testing() {
        // What makes a box useful: nothing else on screen to blame. If the grass
        // box ever picks up a second piece, it stops isolating the grass.
        let grass = SceneId::Grass.def().contents;
        assert!(grass.meadow, "the grass box must have the meadow");
        assert!(
            !(grass.course
                || grass.stairs
                || grass.targets
                || grass.pickups
                || grass.forest
                || grass.card_mesh_lab
                || grass.enemies
                || grass.horse),
            "the grass box must carry nothing else"
        );
    }

    #[test]
    fn the_world_scene_gathers_every_shipping_piece() {
        // The other end of the table: whatever a box proves out has somewhere to
        // land. A piece that exists but no scene ever builds is dead weight.
        let world = SceneId::World.def().contents;
        assert!(
            world.course
                && world.stairs
                && world.targets
                && world.pickups
                && world.forest
                && world.meadow
                && world.enemies
                && world.horse
                && !world.card_mesh_lab,
            "the world scene should gather every piece"
        );
    }

    #[test]
    fn the_card_mesh_lab_is_exclusive() {
        let lab = SceneId::CardMesh.def().contents;
        assert!(lab.card_mesh_lab, "the lab must declare itself");
        assert!(
            !(lab.course
                || lab.stairs
                || lab.targets
                || lab.pickups
                || lab.forest
                || lab.meadow
                || lab.enemies
                || lab.horse),
            "the lab must not carry game content"
        );
    }

    #[test]
    fn the_card_mesh_lab_has_a_real_flat_heightmap() {
        let text = include_str!("../../assets/game/world/card_mesh.ron");
        let mut terrain = bof_simulation::world::Terrain::flat_for_test();
        terrain.apply_ron(text).expect("the card-mesh level loads");
        for point in [
            Vec2::new(-120.0, -120.0),
            Vec2::ZERO,
            Vec2::new(120.0, 120.0),
        ] {
            assert_eq!(
                terrain.height_at(point),
                0.0,
                "the lab ground must stay flat"
            );
        }
    }

    #[test]
    fn the_sandbox_stays_empty() {
        // It doubles as the measurement case: anything spawned in it can hide
        // the terrain's own cost.
        let contents = SceneId::Sandbox.def().contents;
        assert!(
            !(contents.course
                || contents.stairs
                || contents.targets
                || contents.pickups
                || contents.forest
                || contents.meadow
                || contents.card_mesh_lab
                || contents.enemies
                || contents.horse),
            "the sandbox must stay bare"
        );
    }

    #[test]
    fn scene_scoped_content_dies_before_the_sandbox_starts() {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.init_state::<AppState>();
        app.add_systems(PostUpdate, bind_scene_scoped_entities);

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Scene(SceneId::World));
        app.update();

        let actor = app.world_mut().spawn(SceneScoped).id();
        app.update();
        assert!(
            app.world()
                .entity(actor)
                .contains::<DespawnOnExit<AppState>>()
        );

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Scene(SceneId::Sandbox));
        app.update();

        assert!(app.world().get_entity(actor).is_err());
    }
}
