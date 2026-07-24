//! Player animation: graph compilation, state→clip mapping, debug browser.
//!
//! The player GLB carries rig and clips in one file, so the gltf loader
//! auto-inserts `AnimationPlayer` and per-bone animation targets; this module
//! only compiles the `AnimationGraph` and attaches it to the rig.

use bevy::gltf::Gltf;
use bevy::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

use super::player::PlayerVisual;
use super::{PLAYER_APPEARANCE, VisualCatalog};
use crate::movement::intents::{Intents, StrafeDir};
use crate::movement::state::LocomotionState;
use crate::movement::{BodyVelocity, Player};

/// The semantic locomotion clips the player state machine can drive, kept
/// independent of any one GLB's clip names. A rig plugs in by naming its clips
/// with the canonical `AN_<Role>` vocabulary ([`AnimationRole::canonical`]); the
/// vendor UAL2 mannequin is bridged by the aliases in [`ROLE_TABLE`]. See
/// `docs/ASSET_PIPELINE.md` for the stable contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationRole {
    Idle,
    Walk,
    Run,
    Sneak,
    Jump,
    Fall,
    Glide,
    Climb,
    Ladder,
    Mantle,
    Vault,
    WallJump,
    EdgeLeap,
    // Facing-relative variants of the grounded planar roles, driven when the
    // body faces a locked/aimed target and the stick is a strafe (see
    // `directional_role`). Absent in the placeholder, so each falls back to its
    // base role until an authored rig ships the clip.
    WalkBwd,
    WalkStrafeL,
    WalkStrafeR,
    RunBwd,
    RunStrafeL,
    RunStrafeR,
    SneakBwd,
    SneakStrafeL,
    SneakStrafeR,
}

impl AnimationRole {
    /// The canonical authored clip name (`AN_<Accion>`) a rig-compatible asset
    /// should ship to bind this role with zero code. Resolution prefers this
    /// name over any vendor alias.
    pub const fn canonical(self) -> &'static str {
        match self {
            AnimationRole::Idle => "AN_Idle",
            AnimationRole::Walk => "AN_Walk",
            AnimationRole::Run => "AN_Run",
            AnimationRole::Sneak => "AN_Sneak",
            AnimationRole::Jump => "AN_Jump",
            AnimationRole::Fall => "AN_Fall",
            AnimationRole::Glide => "AN_Glide",
            AnimationRole::Climb => "AN_Climb",
            AnimationRole::Ladder => "AN_Ladder",
            AnimationRole::Mantle => "AN_Mantle",
            AnimationRole::Vault => "AN_Vault",
            AnimationRole::WallJump => "AN_WallJump",
            AnimationRole::EdgeLeap => "AN_EdgeLeap",
            AnimationRole::WalkBwd => "AN_WalkBwd",
            AnimationRole::WalkStrafeL => "AN_WalkStrafeL",
            AnimationRole::WalkStrafeR => "AN_WalkStrafeR",
            AnimationRole::RunBwd => "AN_RunBwd",
            AnimationRole::RunStrafeL => "AN_RunStrafeL",
            AnimationRole::RunStrafeR => "AN_RunStrafeR",
            AnimationRole::SneakBwd => "AN_SneakBwd",
            AnimationRole::SneakStrafeL => "AN_SneakStrafeL",
            AnimationRole::SneakStrafeR => "AN_SneakStrafeR",
        }
    }
}

/// Refine a grounded planar base role (`Walk`/`Run`/`Sneak`) into its
/// facing-relative variant from the movement's [`StrafeDir`]. `Forward`/`Idle`
/// and non-planar roles stay as-is. Under free movement the body faces where it
/// moves, so `StrafeDir` is always `Forward` and this is a no-op — directional
/// clips only surface under lock-on/aim.
fn directional_role(base: AnimationRole, strafe: StrafeDir) -> AnimationRole {
    use AnimationRole::*;
    match (base, strafe) {
        (Walk, StrafeDir::Back) => WalkBwd,
        (Walk, StrafeDir::Left) => WalkStrafeL,
        (Walk, StrafeDir::Right) => WalkStrafeR,
        (Run, StrafeDir::Back) => RunBwd,
        (Run, StrafeDir::Left) => RunStrafeL,
        (Run, StrafeDir::Right) => RunStrafeR,
        (Sneak, StrafeDir::Back) => SneakBwd,
        (Sneak, StrafeDir::Left) => SneakStrafeL,
        (Sneak, StrafeDir::Right) => SneakStrafeR,
        _ => base,
    }
}

/// One row of the clip-resolution contract. A role binds to the first name in
/// `vendor_aliases` present in the GLB — after the canonical name is tried — and
/// otherwise borrows the clip resolved for `fallback`, so partial rigs degrade
/// gracefully instead of freezing. `fallback: None` marks a required role.
struct RoleClips {
    role: AnimationRole,
    /// Non-canonical names accepted for this role, in priority order. Lets the
    /// vendor UAL2 mannequin bind without re-authoring its clip names.
    vendor_aliases: &'static [&'static str],
    fallback: Option<AnimationRole>,
}

/// The player clip contract. Every non-`Idle` role chains toward `Idle`, so if
/// `Idle` resolves the whole table does. Kept acyclic on purpose.
///
/// Vendor aliases list UAL1 (locomotion) names first, then UAL2 (action) names,
/// so the table binds whichever library sources the player without edits. UAL1
/// lacks climb/wall clips; those roles degrade through their fallback.
const ROLE_TABLE: &[RoleClips] = &[
    RoleClips {
        role: AnimationRole::Idle,
        vendor_aliases: &["Idle_Loop", "Idle_No_Loop"],
        fallback: None,
    },
    RoleClips {
        role: AnimationRole::Walk,
        vendor_aliases: &["Walk_Loop", "Walk_Formal_Loop", "Walk_Carry_Loop"],
        fallback: Some(AnimationRole::Idle),
    },
    RoleClips {
        role: AnimationRole::Run,
        vendor_aliases: &["Sprint_Loop", "Jog_Fwd_Loop", "Run", "Sprint"],
        fallback: Some(AnimationRole::Walk),
    },
    RoleClips {
        role: AnimationRole::Sneak,
        vendor_aliases: &["Crouch_Fwd_Loop", "Crouch_Walk", "SneakWalk"],
        fallback: Some(AnimationRole::Walk),
    },
    RoleClips {
        role: AnimationRole::Jump,
        vendor_aliases: &["Jump_Start", "NinjaJump_Start"],
        fallback: Some(AnimationRole::Idle),
    },
    RoleClips {
        role: AnimationRole::Fall,
        vendor_aliases: &["Jump_Loop", "Fall_Loop", "NinjaJump_Idle_Loop"],
        fallback: Some(AnimationRole::Jump),
    },
    RoleClips {
        role: AnimationRole::Glide,
        vendor_aliases: &["Glide_Loop", "NinjaJump_Idle_Loop"],
        fallback: Some(AnimationRole::Fall),
    },
    RoleClips {
        role: AnimationRole::Climb,
        vendor_aliases: &["Climb_Loop", "ClimbUp_1m"],
        fallback: Some(AnimationRole::Idle),
    },
    RoleClips {
        role: AnimationRole::Ladder,
        vendor_aliases: &["ClimbUp_1m"],
        fallback: Some(AnimationRole::Climb),
    },
    RoleClips {
        role: AnimationRole::Mantle,
        vendor_aliases: &["ClimbUp_1m"],
        fallback: Some(AnimationRole::Climb),
    },
    RoleClips {
        role: AnimationRole::Vault,
        vendor_aliases: &["ClimbUp_1m"],
        fallback: Some(AnimationRole::Jump),
    },
    RoleClips {
        role: AnimationRole::WallJump,
        vendor_aliases: &["NinjaJump_Start"],
        fallback: Some(AnimationRole::Jump),
    },
    RoleClips {
        role: AnimationRole::EdgeLeap,
        vendor_aliases: &["NinjaJump_Start"],
        fallback: Some(AnimationRole::Jump),
    },
    // Directional variants: no placeholder clip, so each borrows its base
    // (forward) clip until an authored rig ships the strafe/back animation.
    RoleClips {
        role: AnimationRole::WalkBwd,
        vendor_aliases: &[],
        fallback: Some(AnimationRole::Walk),
    },
    RoleClips {
        role: AnimationRole::WalkStrafeL,
        vendor_aliases: &[],
        fallback: Some(AnimationRole::Walk),
    },
    RoleClips {
        role: AnimationRole::WalkStrafeR,
        vendor_aliases: &[],
        fallback: Some(AnimationRole::Walk),
    },
    RoleClips {
        role: AnimationRole::RunBwd,
        vendor_aliases: &[],
        fallback: Some(AnimationRole::Run),
    },
    RoleClips {
        role: AnimationRole::RunStrafeL,
        vendor_aliases: &[],
        fallback: Some(AnimationRole::Run),
    },
    RoleClips {
        role: AnimationRole::RunStrafeR,
        vendor_aliases: &[],
        fallback: Some(AnimationRole::Run),
    },
    RoleClips {
        role: AnimationRole::SneakBwd,
        vendor_aliases: &[],
        fallback: Some(AnimationRole::Sneak),
    },
    RoleClips {
        role: AnimationRole::SneakStrafeL,
        vendor_aliases: &[],
        fallback: Some(AnimationRole::Sneak),
    },
    RoleClips {
        role: AnimationRole::SneakStrafeR,
        vendor_aliases: &[],
        fallback: Some(AnimationRole::Sneak),
    },
];

/// The clip that presents a given locomotion state, before the idle-when-still
/// refinement applied in `animate_player`.
fn role_for_state(state: LocomotionState) -> AnimationRole {
    match state {
        LocomotionState::Walk | LocomotionState::Stairs => AnimationRole::Walk,
        LocomotionState::Sprint => AnimationRole::Run,
        LocomotionState::Sneak => AnimationRole::Sneak,
        LocomotionState::Jump => AnimationRole::Jump,
        LocomotionState::Fall => AnimationRole::Fall,
        LocomotionState::Glide => AnimationRole::Glide,
        LocomotionState::Climb => AnimationRole::Climb,
        LocomotionState::Ladder => AnimationRole::Ladder,
        LocomotionState::Mantle => AnimationRole::Mantle,
        LocomotionState::AutoVault => AnimationRole::Vault,
        LocomotionState::WallJump => AnimationRole::WallJump,
        LocomotionState::EdgeLeap => AnimationRole::EdgeLeap,
    }
}

/// A role's *own* clip: the canonical `AN_<Rol>` name first, then vendor
/// aliases. `None` means the rig ships no clip for this role — the caller must
/// borrow one through the fallback chain (and should log the gap).
fn resolve_direct(
    role: AnimationRole,
    by_name: &HashMap<&str, petgraph::graph::NodeIndex>,
) -> Option<petgraph::graph::NodeIndex> {
    let entry = ROLE_TABLE.iter().find(|row| row.role == role)?;
    if let Some(node) = by_name.get(role.canonical()) {
        return Some(*node);
    }
    entry
        .vendor_aliases
        .iter()
        .find_map(|alias| by_name.get(*alias).copied())
}

/// Resolve one role to a clip node: its own clip, else the fallback role's
/// resolution. `None` only if the chain reaches a required role whose clips are
/// all absent.
fn resolve_role(
    role: AnimationRole,
    by_name: &HashMap<&str, petgraph::graph::NodeIndex>,
) -> Option<petgraph::graph::NodeIndex> {
    if let Some(node) = resolve_direct(role, by_name) {
        return Some(node);
    }
    let entry = ROLE_TABLE.iter().find(|row| row.role == role)?;
    resolve_role(entry.fallback?, by_name)
}

#[derive(Resource)]
pub struct PlayerAnimations {
    /// The always-present base clip; every role resolves to at least this.
    pub idle: petgraph::graph::NodeIndex,
    /// One entry per [`AnimationRole`], each resolved through [`ROLE_TABLE`].
    pub roles: HashMap<AnimationRole, petgraph::graph::NodeIndex>,
    /// Every clip in the player GLB, sorted by name, for the debug browser.
    pub clips: Vec<(String, petgraph::graph::NodeIndex)>,
    pub graph: Handle<AnimationGraph>,
}

impl PlayerAnimations {
    /// The clip node for a role, falling back to `idle` if (impossibly) absent.
    fn node(&self, role: AnimationRole) -> petgraph::graph::NodeIndex {
        self.roles.get(&role).copied().unwrap_or(self.idle)
    }
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
pub(super) struct AnimationLoader(Vec<Handle<Gltf>>);

/// Rig whose graph is driven by the local player's animation state machine.
/// The marker prevents unrelated animated scenes from receiving player clips.
#[derive(Component)]
pub(super) struct PlayerAnimationRig;

pub(super) fn start_loading_animations(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    catalog: Res<VisualCatalog>,
) {
    let sources = catalog
        .recipe(PLAYER_APPEARANCE)
        .map(|recipe| recipe.animation_sources.as_slice())
        .unwrap_or_default();
    if sources.is_empty() {
        error!("[visuals] active player appearance has no animation source");
        return;
    }
    let handles = sources
        .iter()
        .map(|source| asset_server.load(source.clone()))
        .collect();
    commands.insert_resource(AnimationLoader(handles));
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
    // Wait until every source GLB (UAL1 + UAL2) has finished loading before
    // compiling, so no source's clips are silently dropped.
    let mut loaded = Vec::with_capacity(loader.0.len());
    for handle in &loader.0 {
        let Some(gltf) = gltfs.get(handle) else {
            return;
        };
        loaded.push(gltf);
    }

    // Merge every clip across sources so the debug browser can play any of them
    // and the resolver can bind roles from either library. On a name collision
    // the earlier source wins (UAL1 locomotion over UAL2).
    let mut graph = AnimationGraph::new();
    let mut seen = std::collections::HashSet::new();
    let mut clips: Vec<(String, petgraph::graph::NodeIndex)> = Vec::new();
    for gltf in loaded {
        for (name, clip) in &gltf.named_animations {
            if !seen.insert(name.to_string()) {
                continue;
            }
            clips.push((
                name.to_string(),
                graph.add_clip(clip.clone(), 1.0, graph.root),
            ));
        }
    }
    clips.sort_by(|a, b| a.0.cmp(&b.0));

    let by_name: HashMap<&str, petgraph::graph::NodeIndex> = clips
        .iter()
        .map(|(name, node)| (name.as_str(), *node))
        .collect();
    let Some(idle) = resolve_role(AnimationRole::Idle, &by_name) else {
        error!(
            "[visuals] player GLB has no Idle clip ({} nor any vendor alias); \
             animation disabled. Available clips: {:?}",
            AnimationRole::Idle.canonical(),
            clips.iter().map(|(name, _)| name).collect::<Vec<_>>()
        );
        commands.remove_resource::<AnimationLoader>();
        return;
    };

    // Every role chains to Idle, so each entry resolves once Idle does.
    let roles: HashMap<AnimationRole, petgraph::graph::NodeIndex> = ROLE_TABLE
        .iter()
        .map(|row| (row.role, resolve_role(row.role, &by_name).unwrap_or(idle)))
        .collect();

    // Name every role served by its fallback (no clip of its own), so the gap
    // is measurable while the rig is a placeholder. The authored player rig
    // closes these at compile time via `bof_animset = "player"` (build.rs).
    let missing: Vec<&str> = ROLE_TABLE
        .iter()
        .filter(|row| resolve_direct(row.role, &by_name).is_none())
        .map(|row| row.role.canonical())
        .collect();
    if !missing.is_empty() {
        debug!(
            "[visuals] no direct clip for {:?}; using fallback. Author these to fill the gaps.",
            missing
        );
    }

    let graph_handle = graphs.add(graph);
    info!(
        "[visuals] compiled player AnimationGraph with {} clips; role bindings: {:?}",
        clips.len(),
        ROLE_TABLE
            .iter()
            .map(|row| (
                row.role,
                clips
                    .iter()
                    .find(|(_, node)| *node == roles[&row.role])
                    .map(|(name, _)| name.as_str())
                    .unwrap_or("?"),
            ))
            .collect::<Vec<_>>()
    );

    commands.insert_resource(PlayerAnimations {
        idle,
        roles,
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
    player_query: Query<(&LocomotionState, &BodyVelocity, &Intents), With<Player>>,
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
    let Ok((state, velocity, intents)) = player_query.single() else {
        return;
    };

    for (mut player, mut transitions) in &mut animated_query {
        let (target_node, speed_multiplier) = if debug.enabled && !anims.clips.is_empty() {
            // Debug browser override: play the selected clip at normal speed.
            (anims.clips[debug.index % anims.clips.len()].1, 1.0)
        } else {
            // Ground states show Idle when effectively still; airborne/climb
            // states always present their own clip.
            let base = match *state {
                LocomotionState::Walk | LocomotionState::Sneak | LocomotionState::Stairs => {
                    if velocity.0.xz().length() > 0.1 {
                        role_for_state(*state)
                    } else {
                        AnimationRole::Idle
                    }
                }
                other => role_for_state(other),
            };
            // Refine the grounded planar roles into their facing-relative variant
            // (strafe/back), a no-op under free movement where the move is always
            // forward. Resolves to the base clip until strafe clips are authored.
            let role = match *state {
                LocomotionState::Walk
                | LocomotionState::Sprint
                | LocomotionState::Sneak
                | LocomotionState::Stairs => directional_role(base, intents.planar.strafe_dir()),
                _ => base,
            };

            // UAL1 provides real Walk/Jog/Sprint/Crouch clips, so each role
            // plays at its authored speed. Matching clip speed to actual
            // movement speed (foot-slide fix) is a later tuning knob.
            (anims.node(role), 1.0)
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

    fn nodes<'a>(names: &[&'a str]) -> HashMap<&'a str, petgraph::graph::NodeIndex> {
        names
            .iter()
            .enumerate()
            .map(|(i, name)| (*name, petgraph::graph::NodeIndex::new(i)))
            .collect()
    }

    #[test]
    fn every_role_is_declared_in_the_contract() {
        // The resolver's canonical names and the build-time contract share one
        // source of truth: every role the state machine can drive is declared in
        // PLAYER_CLIP_CONTRACT (base roles required, directional variants
        // planned), so build.rs validates exactly what the runtime can bind.
        use crate::asset_pipeline::schema::PLAYER_CLIP_CONTRACT;
        for row in ROLE_TABLE {
            let name = row.role.canonical();
            assert!(
                PLAYER_CLIP_CONTRACT.iter().any(|spec| spec.name == name),
                "{name} missing from PLAYER_CLIP_CONTRACT"
            );
        }
    }

    #[test]
    fn directional_role_refines_grounded_strafe_and_leaves_forward_alone() {
        use AnimationRole::*;
        assert_eq!(directional_role(Walk, StrafeDir::Left), WalkStrafeL);
        assert_eq!(directional_role(Walk, StrafeDir::Back), WalkBwd);
        assert_eq!(directional_role(Run, StrafeDir::Right), RunStrafeR);
        assert_eq!(directional_role(Sneak, StrafeDir::Back), SneakBwd);
        // Forward and idle keep the base role; free movement is always forward.
        assert_eq!(directional_role(Walk, StrafeDir::Forward), Walk);
        assert_eq!(directional_role(Run, StrafeDir::Idle), Run);
        // Non-planar roles are never refined.
        assert_eq!(directional_role(Jump, StrafeDir::Left), Jump);
    }

    #[test]
    fn every_role_resolves_once_idle_is_present() {
        // Only the base idle clip exists: the whole table must still resolve,
        // every role collapsing onto idle through its fallback chain.
        let by_name = nodes(&["Idle_No_Loop"]);
        let idle = by_name["Idle_No_Loop"];
        for row in ROLE_TABLE {
            assert_eq!(resolve_role(row.role, &by_name), Some(idle));
        }
    }

    #[test]
    fn vendor_aliases_bind_the_mannequin_and_canonical_names_win() {
        // The UAL1 mannequin's locomotion clip set: real Walk/Sprint/Crouch/Jump
        // bind directly; UAL1 has no climb/wall clips, so those degrade.
        let by_name = nodes(&[
            "Idle_Loop",
            "Walk_Loop",
            "Sprint_Loop",
            "Crouch_Fwd_Loop",
            "Jump_Start",
            "Jump_Loop",
        ]);
        let expect = |role: AnimationRole, name: &str| {
            assert_eq!(
                resolve_role(role, &by_name),
                Some(by_name[name]),
                "{role:?}"
            );
        };
        expect(AnimationRole::Idle, "Idle_Loop");
        expect(AnimationRole::Walk, "Walk_Loop");
        expect(AnimationRole::Run, "Sprint_Loop");
        expect(AnimationRole::Sneak, "Crouch_Fwd_Loop");
        expect(AnimationRole::Jump, "Jump_Start");
        expect(AnimationRole::Fall, "Jump_Loop");
        // No wall/climb clip in UAL1: fall back to Jump / Idle respectively.
        expect(AnimationRole::WallJump, "Jump_Start");
        expect(AnimationRole::Climb, "Idle_Loop");

        // A rig-compatible asset that ships the canonical name overrides the
        // vendor alias with zero code, proving the plug-and-play contract.
        let authored = nodes(&["Idle_Loop", "Walk_Loop", "AN_Run"]);
        assert_eq!(
            resolve_role(AnimationRole::Run, &authored),
            Some(authored["AN_Run"])
        );
    }

    #[test]
    fn player_animation_never_drives_an_unmarked_rig() {
        let mut world = World::new();
        let idle = petgraph::graph::NodeIndex::new(0);
        let walk = petgraph::graph::NodeIndex::new(1);
        let roles = ROLE_TABLE
            .iter()
            .map(|row| {
                let node = if row.role == AnimationRole::Idle {
                    idle
                } else {
                    walk
                };
                (row.role, node)
            })
            .collect();
        world.insert_resource(PlayerAnimations {
            idle,
            roles,
            clips: Vec::new(),
            graph: Handle::default(),
        });
        world.insert_resource(AnimationDebug::default());
        world.spawn((
            Player,
            LocomotionState::Walk,
            BodyVelocity(Vec3::X),
            Intents::default(),
        ));
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
