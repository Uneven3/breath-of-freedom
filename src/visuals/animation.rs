//! Player animation: graph compilation, state→clip mapping, debug browser.
//!
//! The player GLB carries rig and clips in one file, so the gltf loader
//! auto-inserts `AnimationPlayer` and per-bone animation targets; this module
//! only compiles the `AnimationGraph` and attaches it to the rig.

use bevy::gltf::Gltf;
use bevy::prelude::*;
use std::time::Duration;

use super::player::PlayerVisual;
use super::{PLAYER_APPEARANCE, VisualCatalog};
use crate::movement::state::LocomotionState;
use crate::movement::{BodyVelocity, Player};

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
pub(super) struct AnimationLoader(Handle<Gltf>);

/// Rig whose graph is driven by the local player's animation state machine.
/// The marker prevents unrelated animated scenes from receiving player clips.
#[derive(Component)]
pub(super) struct PlayerAnimationRig;

pub(super) fn start_loading_animations(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    catalog: Res<VisualCatalog>,
) {
    let Some(source) = catalog
        .recipe(PLAYER_APPEARANCE)
        .and_then(|recipe| recipe.animation_source.as_ref())
    else {
        error!("[visuals] active player appearance has no animation source");
        return;
    };
    commands.insert_resource(AnimationLoader(asset_server.load(source.clone())));
}

pub(super) fn compile_animation_graph(
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
    let (Some(idle), Some(walk)) = (find("Idle_No_Loop"), find("Walk_Carry_Loop")) else {
        error!(
            "[visuals] Ranger GLB is missing Idle_No_Loop/Walk_Carry_Loop; \
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
        run: walk,
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
pub(super) fn init_player_animation_graph(
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
                PlayerAnimationRig,
            ));
        }
    }
}

pub(super) fn animate_player(
    player_query: Query<(&LocomotionState, &BodyVelocity), With<Player>>,
    mut animated_query: Query<
        (&mut AnimationPlayer, &mut AnimationTransitions),
        With<PlayerAnimationRig>,
    >,
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn player_animation_never_drives_an_unmarked_rig() {
        let mut world = World::new();
        let idle = petgraph::graph::NodeIndex::new(0);
        let walk = petgraph::graph::NodeIndex::new(1);
        world.insert_resource(PlayerAnimations {
            idle,
            walk,
            run: walk,
            clips: Vec::new(),
            graph: Handle::default(),
        });
        world.insert_resource(AnimationDebug::default());
        world.spawn((Player, LocomotionState::Walk, BodyVelocity(Vec3::X)));
        let player_rig = world
            .spawn((
                AnimationPlayer::default(),
                AnimationTransitions::new(),
                PlayerAnimationRig,
            ))
            .id();
        let unrelated_rig = world
            .spawn((AnimationPlayer::default(), AnimationTransitions::new()))
            .id();

        world.run_system_once(animate_player).unwrap();

        assert!(
            world
                .entity(player_rig)
                .get::<AnimationPlayer>()
                .unwrap()
                .is_playing_animation(walk)
        );
        assert!(
            !world
                .entity(unrelated_rig)
                .get::<AnimationPlayer>()
                .unwrap()
                .is_playing_animation(walk)
        );
    }
}
