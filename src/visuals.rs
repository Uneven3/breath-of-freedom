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
use crate::combat::weapon::WeaponProfile;
use crate::enemies::Enemy;
use crate::enemies::perception::Awareness;
use crate::movement::BodyVelocity;
use crate::movement::Player;
use crate::movement::body::BodyDimensions;
use crate::movement::probe_data::TraversalProbe;
use crate::movement::state::LocomotionState;

const INTERPOLATION_SPEED: f32 = 20.0;
const SNEAK_Y_OFFSET: f32 = -0.4;

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

type ProbeActorQuery<'a> = (&'a Transform, &'a LocomotionState);
type ProbeActorFilter = (With<TraversalProbe>, Without<TraversalProbeVisual>);
type ProbeVisualQuery<'a> = (&'a mut Transform, &'a TraversalProbeVisual);
type ProbeVisualFilter = (With<TraversalProbeVisual>, Without<TraversalProbe>);

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_visual, start_loading_animations));
        app.add_systems(
            Update,
            (
                link_player_visual,
                spawn_probe_visual,
                despawn_orphaned_probe_visual,
                spawn_enemy_visual,
                despawn_orphaned_enemy_visual,
                interpolate_visual,
                interpolate_probe_visual,
                interpolate_enemy_visual,
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
            // Load the Knight GLB scene
            let knight_scene =
                asset_server.load("KayKit_Adventurers_2.0_FREE/Characters/gltf/Knight.glb#Scene0");

            // KayKit models are modeled with their pivot/origin at their feet.
            // Offset down by the half-height so feet touch the ground, and rotate 180 deg
            // around Y so the model faces forward (-Z in Bevy) instead of backward (+Z).
            parent.spawn((
                WorldAssetRoot(knight_scene),
                Transform::from_xyz(0.0, -BodyDimensions::PLAYER.standing_half_height(), 0.0)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
            ));

            // Spawn the Bow Visual Root
            // Positioned slightly right, chest level, pointing forward (-Z)
            parent.spawn((
                BowVisualRoot,
                Name::new("BowVisualRoot"),
                Visibility::Hidden,
                Transform::from_xyz(0.35, 0.15, -0.55)
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
                bow_parent.spawn((
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

type SwingSourceQuery<'a> = (
    &'a Transform,
    &'a WeaponProfile,
    &'a ComboLocal,
    &'a CombatState,
);

fn spawn_swing_vfx(
    mut commands: Commands,
    attackers: Query<SwingSourceQuery, Changed<CombatState>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (transform, weapon, combo, state) in &attackers {
        if *state != CombatState::Active {
            continue;
        }
        let Some(step) = weapon.step(combo.step) else {
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
    pub graph: Handle<AnimationGraph>,
}

#[derive(Resource)]
struct AnimationLoader {
    general: Handle<Gltf>,
    movement: Handle<Gltf>,
}

fn start_loading_animations(mut commands: Commands, asset_server: Res<AssetServer>) {
    let general = asset_server
        .load("KayKit_Adventurers_2.0_FREE/Animations/gltf/Rig_Medium/Rig_Medium_General.glb");
    let movement = asset_server.load(
        "KayKit_Adventurers_2.0_FREE/Animations/gltf/Rig_Medium/Rig_Medium_MovementBasic.glb",
    );
    commands.insert_resource(AnimationLoader { general, movement });
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
    if let (Some(general_gltf_asset), Some(movement_gltf_asset)) =
        (gltfs.get(&loader.general), gltfs.get(&loader.movement))
    {
        let idle_clip = general_gltf_asset.named_animations["Idle_A"].clone();
        let walk_clip = movement_gltf_asset.named_animations["Walking_A"].clone();
        let run_clip = movement_gltf_asset.named_animations["Running_A"].clone();

        let mut graph = AnimationGraph::new();
        // Register clips as nodes in the graph
        let idle = graph.add_clip(idle_clip, 1.0, graph.root);
        let walk = graph.add_clip(walk_clip, 1.0, graph.root);
        let run = graph.add_clip(run_clip, 1.0, graph.root);

        let graph_handle = graphs.add(graph);

        commands.insert_resource(PlayerAnimations {
            idle,
            walk,
            run,
            graph: graph_handle,
        });

        // Clean up the loader resource
        commands.remove_resource::<AnimationLoader>();
        info!("[visuals] Programmatically compiled Player AnimationGraph successfully!");
    }
}

fn link_descendants(
    commands: &mut Commands,
    entity: Entity,
    path: Vec<Name>,
    children_query: &Query<&Children>,
    names_query: &Query<&Name>,
    player_entity: Entity,
) {
    let mut new_path = path;
    if let Ok(name) = names_query.get(entity) {
        new_path.push(name.clone());

        let target_id = bevy::animation::AnimationTargetId::from_names(new_path.iter());

        commands
            .entity(entity)
            .insert((bevy::animation::AnimatedBy(player_entity), target_id));
    }

    if let Ok(children) = children_query.get(entity) {
        for child in children.iter() {
            link_descendants(
                commands,
                child,
                new_path.clone(),
                children_query,
                names_query,
                player_entity,
            );
        }
    }
}

fn init_player_animation_graph(
    mut commands: Commands,
    scene_query: Query<(Entity, &Name), Without<AnimationGraphHandle>>,
    parent_query: Query<&ChildOf>,
    visual_query: Query<(), With<PlayerVisual>>,
    children_query: Query<&Children>,
    names_query: Query<&Name>,
    animations: Option<Res<PlayerAnimations>>,
) {
    let Some(anims) = animations else {
        return;
    };
    for (entity, name) in &scene_query {
        if name.as_str() == "Scene" {
            // Traverse up the tree to verify this "Scene" entity is under PlayerVisual
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
                    AnimationPlayer::default(),
                    AnimationGraphHandle(anims.graph.clone()),
                    AnimationTransitions::new(),
                ));

                // Recursively insert AnimatedBy and AnimationTargetId components on all descendant nodes
                if let Ok(children) = children_query.get(entity) {
                    for child in children.iter() {
                        link_descendants(
                            &mut commands,
                            child,
                            vec![],
                            &children_query,
                            &names_query,
                            entity,
                        );
                    }
                }
            }
        }
    }
}

fn animate_player(
    player_query: Query<(&LocomotionState, &BodyVelocity), With<Player>>,
    mut animated_query: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    anims: Option<Res<PlayerAnimations>>,
) {
    let Some(anims) = anims else {
        return;
    };
    let Ok((state, velocity)) = player_query.single() else {
        return;
    };

    for (mut player, mut transitions) in &mut animated_query {
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
    mut arrow: Query<(&mut Visibility, &mut Transform), (With<BowArrowVisual>, Without<BowVisualRoot>)>,
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
