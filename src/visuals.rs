//! Player visual mesh, decoupled from the physics body.
//!
//! A standalone mesh entity interpolates toward the kinematic body each render
//! frame, which (a) smooths the 60 Hz fixed-step motion on high-refresh
//! displays and (b) dips −0.4 in Sneak so crouching reads visually even though
//! the collider, not the mesh, is what actually shrinks.

use bevy::gltf::Gltf;
use bevy::prelude::*;
use std::time::Duration;

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
                spawn_probe_visual,
                despawn_orphaned_probe_visual,
                spawn_enemy_visual,
                despawn_orphaned_enemy_visual,
                interpolate_visual,
                interpolate_probe_visual,
                interpolate_enemy_visual,
                tint_enemy_visual,
                compile_animation_graph,
                init_player_animation_graph,
                animate_player,
            ),
        );
    }
}

fn spawn_visual(mut commands: Commands, asset_server: Res<AssetServer>) {
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
        });
}

type BodyFilter = (With<Player>, Without<PlayerVisual>);

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
fn tint_enemy_visual(
    enemies: Query<&Awareness, With<Enemy>>,
    visuals: Query<(&EnemyVisual, &MeshMaterial3d<StandardMaterial>)>,
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
