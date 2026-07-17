//! Player visual mesh, decoupled from the physics body.
//!
//! A standalone mesh entity interpolates toward the kinematic body each render
//! frame, which (a) smooths the 60 Hz fixed-step motion on high-refresh
//! displays and (b) dips −0.4 in Sneak so crouching reads visually even though
//! the collider, not the mesh, is what actually shrinks.

use bevy::gltf::Gltf;
use bevy::prelude::*;
use std::time::Duration;

use crate::combat::motors::attack::ComboLocal;
use crate::combat::state::CombatState;
use crate::enemies::Enemy;
use crate::enemies::perception::Awareness;
use crate::mounts::data::Horse;
use crate::movement::BodyVelocity;
use crate::movement::Player;
use crate::movement::body::BodyDimensions;
use crate::movement::probe_data::TraversalProbe;
use crate::movement::state::LocomotionState;

const INTERPOLATION_SPEED: f32 = 20.0;
const SNEAK_Y_OFFSET: f32 = -0.4;

/// Authored height of the Prototype.glb rig in meters (feet to head, taken
/// from the mesh's POSITION bounds). The visual is scaled so this matches the
/// player capsule's full height.
const PLAYER_MODEL_HEIGHT: f32 = 1.83;

#[derive(Component)]
pub struct PlayerVisual;

#[derive(Component)]
pub struct BowVisualRoot;

#[derive(Component)]
pub struct BowArrowVisual;

/// Uniform link from any visual root back to its simulation actor, so
/// cross-cutting presentation effects (jelly, hit flash — see
/// `presentation::juice`) treat player/probe/enemy visuals alike.
#[derive(Component, Clone, Copy)]
pub struct VisualOf(pub Entity);

#[derive(Component)]
struct TraversalProbeVisual {
    actor: Entity,
}

#[derive(Component)]
struct EnemyVisual {
    actor: Entity,
}

#[derive(Component)]
struct HorseVisual {
    actor: Entity,
}

type ProbeActorQuery<'a> = (&'a Transform, &'a LocomotionState);
type ProbeActorFilter = (With<TraversalProbe>, Without<TraversalProbeVisual>);
type ProbeVisualQuery<'a> = (&'a mut Transform, &'a TraversalProbeVisual);
type ProbeVisualFilter = (With<TraversalProbeVisual>, Without<TraversalProbe>);

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AnimationDebug>();
        app.add_systems(Startup, (spawn_visual, start_loading_animations));
        app.add_systems(
            Update,
            (
                link_player_visual,
                spawn_probe_visual,
                despawn_orphaned_probe_visual,
                spawn_enemy_visual,
                despawn_orphaned_enemy_visual,
                spawn_horse_visual,
                despawn_orphaned_horse_visual,
                interpolate_visual,
                interpolate_probe_visual,
                interpolate_enemy_visual,
                interpolate_horse_visual,
                tint_enemy_visual,
                spawn_swing_vfx,
                fade_swing_vfx,
                compile_animation_graph,
                init_player_animation_graph,
                animate_player,
                animate_bow_visual,
            ),
        );
    }
}

fn spawn_visual(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands
        .spawn((
            PlayerVisual,
            Name::new("PlayerVisual"),
            Transform::from_xyz(0.0, 1.5, 0.0),
            Visibility::default(),
        ))
        .with_children(|parent| {
            // Load the player prototype GLB scene (rig + animations in one file)
            let player_scene = asset_server.load("Prototype.glb#Scene0");

            // The model is authored with its pivot/origin at its feet.
            // Offset down by the half-height so feet touch the ground, rotate 180 deg
            // around Y so the model faces forward (-Z in Bevy) instead of backward (+Z),
            // and scale it so the rig's height matches the capsule's full height.
            let model_scale =
                2.0 * BodyDimensions::PLAYER.standing_half_height() / PLAYER_MODEL_HEIGHT;
            parent.spawn((
                WorldAssetRoot(player_scene),
                Transform::from_xyz(0.0, -BodyDimensions::PLAYER.standing_half_height(), 0.0)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
                    .with_scale(Vec3::splat(model_scale)),
            ));

            // Spawn the Bow Visual Root at Combat's bow socket (the arrow
            // spawn point), so the arrow visibly leaves the bow.
            parent
                .spawn((
                    BowVisualRoot,
                    Name::new("BowVisualRoot"),
                    Visibility::Hidden,
                    Transform::from_translation(crate::combat::motors::aim::BOW_SOCKET_LOCAL)
                        .with_rotation(Quat::from_rotation_y(0.12) * Quat::from_rotation_z(-0.18)),
                ))
                .with_children(|bow_parent| {
                    let wood_material = materials.add(StandardMaterial {
                        base_color: Color::srgb(0.4, 0.25, 0.15), // Brown wood
                        perceptual_roughness: 0.8,
                        ..default()
                    });
                    let string_material = materials.add(StandardMaterial {
                        base_color: Color::srgb(0.9, 0.9, 0.95), // Off-white string
                        unlit: true,
                        ..default()
                    });
                    let steel_material = materials.add(StandardMaterial {
                        base_color: Color::srgb(0.7, 0.7, 0.75), // Steel arrow head
                        perceptual_roughness: 0.1,
                        ..default()
                    });
                    let fletch_material = materials.add(StandardMaterial {
                        base_color: Color::srgb(0.85, 0.15, 0.15), // Red fletching
                        ..default()
                    });

                    // Bow limbs
                    // Handle (center vertical part)
                    bow_parent.spawn((
                        Mesh3d(meshes.add(Cuboid::new(0.04, 0.2, 0.04))),
                        MeshMaterial3d(wood_material.clone()),
                        Transform::from_xyz(0.0, 0.0, 0.0),
                        Visibility::default(),
                    ));
                    // Upper limb (angled forward)
                    bow_parent.spawn((
                        Mesh3d(meshes.add(Cuboid::new(0.03, 0.45, 0.03))),
                        MeshMaterial3d(wood_material.clone()),
                        Transform::from_xyz(0.0, 0.28, -0.06)
                            .with_rotation(Quat::from_rotation_x(-0.35)),
                        Visibility::default(),
                    ));
                    // Lower limb (angled forward)
                    bow_parent.spawn((
                        Mesh3d(meshes.add(Cuboid::new(0.03, 0.45, 0.03))),
                        MeshMaterial3d(wood_material.clone()),
                        Transform::from_xyz(0.0, -0.28, -0.06)
                            .with_rotation(Quat::from_rotation_x(0.35)),
                        Visibility::default(),
                    ));
                    // Bowstring (from top tip to bottom tip)
                    bow_parent.spawn((
                        Mesh3d(meshes.add(Cuboid::new(0.006, 0.95, 0.006))),
                        MeshMaterial3d(string_material),
                        Transform::from_xyz(0.0, 0.0, 0.1),
                        Visibility::default(),
                    ));

                    // Arrow
                    bow_parent
                        .spawn((
                            BowArrowVisual,
                            Name::new("BowArrowVisual"),
                            Visibility::default(),
                            Transform::from_xyz(0.0, 0.0, 0.1), // initially pulled back/resting on string
                        ))
                        .with_children(|arrow_parent| {
                            // Shaft (brown)
                            arrow_parent.spawn((
                                Mesh3d(meshes.add(Cuboid::new(0.015, 0.015, 0.65))),
                                MeshMaterial3d(wood_material),
                                Transform::from_xyz(0.0, 0.0, -0.3), // aligned forward (-Z)
                                Visibility::default(),
                            ));
                            // Arrowhead (silver/steel)
                            arrow_parent.spawn((
                                Mesh3d(meshes.add(Cuboid::new(0.035, 0.035, 0.07))),
                                MeshMaterial3d(steel_material),
                                Transform::from_xyz(0.0, 0.0, -0.65), // tip of the shaft
                                Visibility::default(),
                            ));
                            // Fletching (red)
                            arrow_parent.spawn((
                                Mesh3d(meshes.add(Cuboid::new(0.01, 0.05, 0.08))),
                                MeshMaterial3d(fletch_material),
                                Transform::from_xyz(0.0, 0.0, -0.05), // near the back
                                Visibility::default(),
                            ));
                        });
                });
        });
}

type BodyFilter = (With<Player>, Without<PlayerVisual>);

type UnlinkedPlayerVisual = (With<PlayerVisual>, Without<VisualOf>);

/// The player visual is spawned in `Startup` before the player body's spawn
/// command applies; link them lazily once both exist.
fn link_player_visual(
    mut commands: Commands,
    visual: Option<Single<Entity, UnlinkedPlayerVisual>>,
    player: Option<Single<Entity, With<Player>>>,
) {
    if let (Some(visual), Some(player)) = (visual, player) {
        commands.entity(*visual).insert(VisualOf(*player));
    }
}

fn interpolate_visual(
    player: Single<(&Transform, &LocomotionState), BodyFilter>,
    mut visual: Single<&mut Transform, With<PlayerVisual>>,
    time: Res<Time>,
) {
    let (body, state) = *player;
    let t = (INTERPOLATION_SPEED * time.delta_secs()).clamp(0.0, 1.0);
    let offset = if *state == LocomotionState::Sneak {
        SNEAK_Y_OFFSET
    } else {
        0.0
    };

    // X/Z track the body directly; Y and rotation interpolate.
    let target_y = body.translation.y + offset;
    visual.translation.x = body.translation.x;
    visual.translation.z = body.translation.z;
    visual.translation.y += (target_y - visual.translation.y) * t;
    visual.rotation = visual.rotation.slerp(body.rotation, t);
}

fn spawn_probe_visual(
    mut commands: Commands,
    probes: Query<(Entity, &Transform, &BodyDimensions), Added<TraversalProbe>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (actor, transform, body) in &probes {
        commands.spawn((
            TraversalProbeVisual { actor },
            VisualOf(actor),
            Name::new("TraversalProbeVisual"),
            Mesh3d(meshes.add(Capsule3d::new(body.radius, body.standing_capsule_length))),
            MeshMaterial3d(materials.add(Color::srgb(0.85, 0.3, 0.25))),
            *transform,
        ));
    }
}

/// Despawn orphaned probe visuals when their actor entity is gone.
fn despawn_orphaned_probe_visual(
    mut commands: Commands,
    visuals: Query<(Entity, &TraversalProbeVisual)>,
    actors: Query<(), With<TraversalProbe>>,
) {
    for (vis_entity, probe_vis) in &visuals {
        if actors.get(probe_vis.actor).is_err() {
            commands.entity(vis_entity).despawn();
        }
    }
}

/// Graybox awareness feedback (the "?/!" stand-in): calm, suspicious, alerted.
const ENEMY_CALM_COLOR: Color = Color::srgb(0.45, 0.2, 0.55);
const ENEMY_SUSPICIOUS_COLOR: Color = Color::srgb(0.9, 0.6, 0.1);
const ENEMY_ALERTED_COLOR: Color = Color::srgb(0.85, 0.12, 0.08);

fn spawn_enemy_visual(
    mut commands: Commands,
    enemies: Query<(Entity, &Transform, &BodyDimensions), Added<Enemy>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (actor, transform, body) in &enemies {
        commands.spawn((
            EnemyVisual { actor },
            VisualOf(actor),
            Name::new("EnemyVisual"),
            Mesh3d(meshes.add(Capsule3d::new(body.radius, body.standing_capsule_length))),
            // Per-enemy material instance: `tint_enemy_visual` mutates it.
            MeshMaterial3d(materials.add(ENEMY_CALM_COLOR)),
            *transform,
        ));
    }
}

/// Tint each enemy capsule by its `Awareness` tier so playtesting reads the
/// meter without UI. Read-only over simulation state, like every visual.
/// Skips visuals mid hit-flash — the flash owns the material until it expires
/// (`presentation::juice`).
fn tint_enemy_visual(
    enemies: Query<&Awareness, With<Enemy>>,
    visuals: Query<
        (&EnemyVisual, &MeshMaterial3d<StandardMaterial>),
        Without<crate::presentation::juice::HitFlash>,
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (enemy_vis, material_handle) in &visuals {
        let Ok(awareness) = enemies.get(enemy_vis.actor) else {
            continue;
        };
        let color = if awareness.is_alerted() {
            ENEMY_ALERTED_COLOR
        } else if awareness.is_suspicious() {
            ENEMY_SUSPICIOUS_COLOR
        } else {
            ENEMY_CALM_COLOR
        };
        if let Some(mut material) = materials.get_mut(&material_handle.0)
            && material.base_color != color
        {
            material.base_color = color;
        }
    }
}

fn despawn_orphaned_enemy_visual(
    mut commands: Commands,
    visuals: Query<(Entity, &EnemyVisual)>,
    actors: Query<(), With<Enemy>>,
) {
    for (vis_entity, enemy_vis) in &visuals {
        if actors.get(enemy_vis.actor).is_err() {
            commands.entity(vis_entity).despawn();
        }
    }
}

/// Graybox horse presentation. The simulation entity carries no mesh,
/// material or asset handle; this disposable visual follows it by `VisualOf`.
fn spawn_horse_visual(
    mut commands: Commands,
    horses: Query<(Entity, &Transform, &BodyDimensions), Added<Horse>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (actor, transform, body) in &horses {
        commands.spawn((
            HorseVisual { actor },
            VisualOf(actor),
            Name::new("HorseVisual"),
            Mesh3d(meshes.add(Capsule3d::new(body.radius, body.standing_capsule_length))),
            MeshMaterial3d(materials.add(Color::srgb(0.42, 0.23, 0.1))),
            transform.with_scale(Vec3::new(0.9, 0.9, 1.45)),
        ));
    }
}

fn despawn_orphaned_horse_visual(
    mut commands: Commands,
    visuals: Query<(Entity, &HorseVisual)>,
    actors: Query<(), With<Horse>>,
) {
    for (visual, horse) in &visuals {
        if actors.get(horse.actor).is_err() {
            commands.entity(visual).despawn();
        }
    }
}

type HorseActorFilter = (With<Horse>, Without<HorseVisual>);
type HorseVisualFilter = (With<HorseVisual>, Without<Horse>);

fn interpolate_horse_visual(
    actors: Query<&Transform, HorseActorFilter>,
    mut visuals: Query<(&mut Transform, &HorseVisual), HorseVisualFilter>,
    time: Res<Time>,
) {
    let t = (INTERPOLATION_SPEED * time.delta_secs()).clamp(0.0, 1.0);
    for (mut visual, horse) in &mut visuals {
        let Ok(body) = actors.get(horse.actor) else {
            continue;
        };
        visual.translation.x = body.translation.x;
        visual.translation.z = body.translation.z;
        visual.translation.y += (body.translation.y - visual.translation.y) * t;
        visual.rotation = visual.rotation.slerp(body.rotation, t);
    }
}

type EnemyActorQuery<'a> = &'a Transform;
type EnemyActorFilter = (With<Enemy>, Without<EnemyVisual>);
type EnemyVisualQuery<'a> = (&'a mut Transform, &'a EnemyVisual);
type EnemyVisualFilter = (With<EnemyVisual>, Without<Enemy>);

fn interpolate_enemy_visual(
    actors: Query<EnemyActorQuery, EnemyActorFilter>,
    mut visuals: Query<EnemyVisualQuery, EnemyVisualFilter>,
    time: Res<Time>,
) {
    let t = (INTERPOLATION_SPEED * time.delta_secs()).clamp(0.0, 1.0);
    for (mut visual, enemy) in &mut visuals {
        let Ok(body) = actors.get(enemy.actor) else {
            continue;
        };
        visual.translation.x = body.translation.x;
        visual.translation.z = body.translation.z;
        visual.translation.y += (body.translation.y - visual.translation.y) * t;
        visual.rotation = visual.rotation.slerp(body.rotation, t);
    }
}

fn interpolate_probe_visual(
    actors: Query<ProbeActorQuery, ProbeActorFilter>,
    mut visuals: Query<ProbeVisualQuery, ProbeVisualFilter>,
    time: Res<Time>,
) {
    let t = (INTERPOLATION_SPEED * time.delta_secs()).clamp(0.0, 1.0);
    for (mut visual, probe) in &mut visuals {
        let Ok((body, state)) = actors.get(probe.actor) else {
            continue;
        };
        let offset = if *state == LocomotionState::Sneak {
            SNEAK_Y_OFFSET
        } else {
            0.0
        };
        visual.translation.x = body.translation.x;
        visual.translation.z = body.translation.z;
        visual.translation.y += (body.translation.y + offset - visual.translation.y) * t;
        visual.rotation = visual.rotation.slerp(body.rotation, t);
    }
}

/// Swing VFX placeholder while combat has no animations: a translucent arc
/// sector flashes in front of the attacker during `Active`. Read-only over
/// simulation state (like the enemy tint); replaced by real animation later
/// without touching Combat.
const SWING_VFX_SECS: f32 = 0.16;

#[derive(Component)]
struct SwingVfx {
    remaining: f32,
}

type SwingSourceQuery<'a> = (&'a Transform, &'a ComboLocal, &'a CombatState);

fn spawn_swing_vfx(
    mut commands: Commands,
    attackers: Query<SwingSourceQuery, Changed<CombatState>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (transform, combo, state) in &attackers {
        if *state != CombatState::Active {
            continue;
        }
        let Some(step) = combo.current_step() else {
            continue;
        };
        // `CircularSector` is an XY-plane fan opening along +Y: tilt it flat
        // (+Y → -Z) so it opens along the attacker's forward.
        let lie_flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        commands.spawn((
            SwingVfx {
                remaining: SWING_VFX_SECS,
            },
            Name::new("SwingVfx"),
            Mesh3d(meshes.add(CircularSector::from_degrees(step.reach, step.arc_deg))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.95, 0.95, 0.7, 0.45),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                ..default()
            })),
            Transform::from_translation(transform.translation + Vec3::Y * 0.35)
                .with_rotation(transform.rotation * lie_flat),
        ));
    }
}

fn fade_swing_vfx(
    mut commands: Commands,
    time: Res<Time>,
    mut swings: Query<(Entity, &mut SwingVfx)>,
) {
    for (entity, mut swing) in &mut swings {
        swing.remaining -= time.delta_secs();
        if swing.remaining <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

#[derive(Resource)]
pub struct PlayerAnimations {
    pub idle: petgraph::graph::NodeIndex,
    pub walk: petgraph::graph::NodeIndex,
    pub run: petgraph::graph::NodeIndex,
    /// Every clip in the player GLB, sorted by name, for the debug browser.
    pub clips: Vec<(String, petgraph::graph::NodeIndex)>,
    pub graph: Handle<AnimationGraph>,
}

/// Debug clip browser: while enabled, `animate_player` plays the selected
/// clip from `PlayerAnimations::clips` instead of following the locomotion
/// state machine. Toggled with F7, cycled with [ and ] (see `debug.rs`).
#[derive(Resource, Default)]
pub struct AnimationDebug {
    pub enabled: bool,
    pub index: usize,
}

#[derive(Resource)]
struct AnimationLoader(Handle<Gltf>);

fn start_loading_animations(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(AnimationLoader(asset_server.load("Prototype.glb")));
}

fn compile_animation_graph(
    mut commands: Commands,
    loader: Option<Res<AnimationLoader>>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let Some(loader) = loader else {
        return;
    };
    let Some(gltf) = gltfs.get(&loader.0) else {
        return;
    };

    // Register every clip in the GLB so the debug browser can play any of
    // them; the locomotion state machine only drives idle/walk/run.
    let mut graph = AnimationGraph::new();
    let mut clips: Vec<(String, petgraph::graph::NodeIndex)> = gltf
        .named_animations
        .iter()
        .map(|(name, clip)| {
            (
                name.to_string(),
                graph.add_clip(clip.clone(), 1.0, graph.root),
            )
        })
        .collect();
    clips.sort_by(|a, b| a.0.cmp(&b.0));

    let find = |wanted: &str| {
        clips
            .iter()
            .find(|(name, _)| name == wanted)
            .map(|&(_, node)| node)
    };
    let (Some(idle), Some(walk), Some(run)) =
        (find("Idle_Loop"), find("Walk_Loop"), find("Sprint_Loop"))
    else {
        error!(
            "[visuals] player GLB is missing Idle_Loop/Walk_Loop/Sprint_Loop; \
             animation disabled. Available clips: {:?}",
            clips.iter().map(|(name, _)| name).collect::<Vec<_>>()
        );
        commands.remove_resource::<AnimationLoader>();
        return;
    };

    let graph_handle = graphs.add(graph);
    info!(
        "[visuals] compiled player AnimationGraph with {} clips: {:?}",
        clips.len(),
        clips.iter().map(|(name, _)| name).collect::<Vec<_>>()
    );

    commands.insert_resource(PlayerAnimations {
        idle,
        walk,
        run,
        clips,
        graph: graph_handle,
    });

    // Clean up the loader resource
    commands.remove_resource::<AnimationLoader>();
}

/// The player GLB carries its animations, so the gltf loader already inserted
/// an `AnimationPlayer` on the rig root and `AnimationTargetId`/`AnimatedBy`
/// on every animated bone; only the graph handle and transitions remain to be
/// attached here.
fn init_player_animation_graph(
    mut commands: Commands,
    rigs: Query<Entity, (With<AnimationPlayer>, Without<AnimationGraphHandle>)>,
    parent_query: Query<&ChildOf>,
    visual_query: Query<(), With<PlayerVisual>>,
    animations: Option<Res<PlayerAnimations>>,
) {
    let Some(anims) = animations else {
        return;
    };
    for entity in &rigs {
        // Traverse up the tree to verify this rig is under PlayerVisual
        let mut is_player_descendant = false;
        let mut current = entity;
        while let Ok(parent_relation) = parent_query.get(current) {
            let parent = parent_relation.parent();
            if visual_query.get(parent).is_ok() {
                is_player_descendant = true;
                break;
            }
            current = parent;
        }

        if is_player_descendant {
            commands.entity(entity).insert((
                AnimationGraphHandle(anims.graph.clone()),
                AnimationTransitions::new(),
            ));
        }
    }
}

fn animate_player(
    player_query: Query<(&LocomotionState, &BodyVelocity), With<Player>>,
    mut animated_query: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    anims: Option<Res<PlayerAnimations>>,
    debug: Res<AnimationDebug>,
) {
    let Some(anims) = anims else {
        return;
    };
    let Ok((state, velocity)) = player_query.single() else {
        return;
    };

    for (mut player, mut transitions) in &mut animated_query {
        let (target_node, speed_multiplier) = if debug.enabled && !anims.clips.is_empty() {
            // Debug browser override: play the selected clip at normal speed.
            (anims.clips[debug.index % anims.clips.len()].1, 1.0)
        } else {
            let target_node = match *state {
                LocomotionState::Sprint => anims.run,
                LocomotionState::Walk | LocomotionState::Sneak | LocomotionState::Stairs => {
                    let speed = velocity.0.xz().length();
                    if speed > 0.1 { anims.walk } else { anims.idle }
                }
                _ => anims.idle,
            };

            // Adjust animation playback speed dynamically based on active state
            let speed_multiplier = match *state {
                LocomotionState::Sneak => 0.5, // Slow crawl/sneak
                LocomotionState::Walk | LocomotionState::Stairs => 1.0, // Normal walking speed
                LocomotionState::Sprint => 1.25, // Faster running animation
                _ => 1.0,
            };
            (target_node, speed_multiplier)
        };

        if !player.is_playing_animation(target_node) {
            transitions
                .play(&mut player, target_node, Duration::from_secs_f32(0.25))
                .set_speed(speed_multiplier)
                .repeat();
        } else if let Some(active) = player.animation_mut(target_node) {
            active.set_speed(speed_multiplier);
        }
    }
}

#[allow(clippy::type_complexity)]
fn animate_bow_visual(
    player: Single<(&CombatState, &crate::combat::motors::aim::DrawStrength), With<Player>>,
    mut bow_root: Query<(&mut Visibility, &Children), With<BowVisualRoot>>,
    mut arrow: Query<
        (&mut Visibility, &mut Transform),
        (With<BowArrowVisual>, Without<BowVisualRoot>),
    >,
) {
    let (state, draw) = *player;
    let aiming = matches!(state, CombatState::Aiming);

    for (mut visibility, children) in &mut bow_root {
        *visibility = if aiming {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };

        if aiming {
            // The arrow is only visible if the player is actively drawing the string or holding a charge.
            let arrow_visible = draw.charging || draw.factor > 0.0;

            for &child in children {
                if let Ok((mut arrow_vis, mut transform)) = arrow.get_mut(child) {
                    *arrow_vis = if arrow_visible {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };

                    if arrow_visible {
                        // Pull the arrow back by factor * 0.42 meters
                        transform.translation.z = 0.1 + draw.factor * 0.42;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn orphaned_horse_visual_despawns_without_touching_simulation() {
        let mut world = World::new();
        let missing_horse = world.spawn_empty().id();
        world.entity_mut(missing_horse).despawn();
        let visual = world
            .spawn((
                HorseVisual {
                    actor: missing_horse,
                },
                VisualOf(missing_horse),
            ))
            .id();

        world
            .run_system_once(despawn_orphaned_horse_visual)
            .unwrap();
        world.flush();

        assert!(world.get_entity(visual).is_err());
    }
}
