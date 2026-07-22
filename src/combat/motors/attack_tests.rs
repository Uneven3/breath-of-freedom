use super::attack::*;
use super::attack_resolution::{SNEAKSTRIKE_MULT, final_damage};
use bevy::prelude::*;

use crate::combat::context_data::{CombatContext, MountedCombatProfile};
use crate::combat::intent::CombatIntents;
use crate::combat::proposal::{CombatProposalBuffer, Priority, TransitionProposal};
use crate::combat::state::CombatState;
use crate::combat::weapon::WeaponProfile;
use crate::enemies::perception::DirectThreatMessage;
use crate::movement::Actor;
use crate::movement::state::LocomotionState;
use bevy::ecs::system::RunSystemOnce;

fn armed_actor(state: CombatState, mut local: ComboLocal, pressed: bool) -> impl Bundle {
    if state != CombatState::Idle && local.snapshot.is_none() {
        local.snapshot = Some(WeaponProfile::GRAYBOX_SWORD);
    }
    (
        Actor,
        CombatIntents {
            attack: crate::combat::intent::AttackIntent {
                pressed,
                held: pressed,
            },
            ..default()
        },
        state,
        WeaponProfile::GRAYBOX_SWORD,
        CombatContext::default(),
        MountedCombatProfile::HORSE,
        local,
        CombatProposalBuffer::default(),
    )
}

fn proposals(world: &World, entity: Entity) -> Vec<TransitionProposal> {
    world
        .entity(entity)
        .get::<CombatProposalBuffer>()
        .unwrap()
        .iter()
        .cloned()
        .collect()
}

#[test]
fn pressing_attack_from_idle_proposes_windup() {
    let mut world = World::new();
    let armed = world
        .spawn(armed_actor(CombatState::Idle, ComboLocal::default(), true))
        .id();
    let calm = world
        .spawn(armed_actor(CombatState::Idle, ComboLocal::default(), false))
        .id();

    world.run_system_once(propose).unwrap();

    let out = proposals(&world, armed);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].target_state, CombatState::Windup);
    assert_eq!(out[0].category, Priority::PlayerRequested);
    assert!(proposals(&world, calm).is_empty());
}

#[test]
fn mounted_profile_is_snapshotted_without_mutating_the_base_weapon() {
    let mut world = World::new();
    world.init_resource::<Messages<crate::combat::context::SetMountedCombatMessage>>();
    let actor = world
        .spawn(armed_actor(CombatState::Idle, ComboLocal::default(), true))
        .id();
    world.write_message(crate::combat::context::SetMountedCombatMessage {
        actor,
        mounted: true,
    });
    world
        .run_system_once(crate::combat::context::apply_mounted_context)
        .unwrap();
    world.run_system_once(propose).unwrap();

    assert_eq!(
        world.entity(actor).get::<ComboLocal>().unwrap().snapshot,
        Some(WeaponProfile::MOUNTED_SWORD)
    );
    assert_eq!(
        *world.entity(actor).get::<WeaponProfile>().unwrap(),
        WeaponProfile::GRAYBOX_SWORD
    );

    world.write_message(crate::combat::context::SetMountedCombatMessage {
        actor,
        mounted: false,
    });
    world
        .run_system_once(crate::combat::context::apply_mounted_context)
        .unwrap();
    assert_eq!(
        world.entity(actor).get::<ComboLocal>().unwrap().snapshot,
        Some(WeaponProfile::MOUNTED_SWORD),
        "an active action keeps its initial effective profile"
    );
}

#[test]
fn windup_holds_then_advances_to_active() {
    let step0 = WeaponProfile::GRAYBOX_SWORD.step(0).unwrap();

    let mut world = World::new();
    let holding = world
        .spawn(armed_actor(
            CombatState::Windup,
            ComboLocal {
                phase_elapsed: step0.windup_secs * 0.5,
                ..default()
            },
            false,
        ))
        .id();
    let done = world
        .spawn(armed_actor(
            CombatState::Windup,
            ComboLocal {
                phase_elapsed: step0.windup_secs,
                ..default()
            },
            false,
        ))
        .id();

    world.run_system_once(propose).unwrap();

    assert_eq!(
        proposals(&world, holding)[0].target_state,
        CombatState::Windup
    );
    assert_eq!(proposals(&world, done)[0].target_state, CombatState::Active);
    assert!(
        proposals(&world, done)[0].override_weight > proposals(&world, holding)[0].override_weight,
        "the advance must out-arbitrate a hold"
    );
}

#[test]
fn recovery_chains_only_buffered_inside_the_window() {
    let step0 = WeaponProfile::GRAYBOX_SWORD.step(0).unwrap();

    let mut world = World::new();
    // Buffered press, inside the window → chain to step 1.
    let chains = world
        .spawn(armed_actor(
            CombatState::Recovery,
            ComboLocal {
                buffered: true,
                phase_elapsed: step0.chain_window_secs * 0.5,
                ..default()
            },
            false,
        ))
        .id();
    // Buffered but too late → just holds recovery.
    let late = world
        .spawn(armed_actor(
            CombatState::Recovery,
            ComboLocal {
                buffered: true,
                phase_elapsed: step0.chain_window_secs + 0.01,
                ..default()
            },
            false,
        ))
        .id();

    world.run_system_once(propose).unwrap();

    let chained = proposals(&world, chains);
    assert_eq!(chained[0].target_state, CombatState::Windup);
    assert_eq!(chained[0].source_id, "attack_chain");
    assert_eq!(
        world.entity(chains).get::<ComboLocal>().unwrap().step,
        1,
        "chaining advances the combo step"
    );

    assert_eq!(
        proposals(&world, late)[0].target_state,
        CombatState::Recovery
    );
    assert_eq!(world.entity(late).get::<ComboLocal>().unwrap().step, 0);
}

#[test]
fn finisher_never_chains_and_expired_recovery_goes_silent() {
    let finisher_index = 2;
    let finisher = WeaponProfile::GRAYBOX_SWORD.step(finisher_index).unwrap();

    let mut world = World::new();
    let at_finisher = world
        .spawn(armed_actor(
            CombatState::Recovery,
            ComboLocal {
                step: finisher_index,
                buffered: true,
                phase_elapsed: 0.01,
                ..default()
            },
            false,
        ))
        .id();
    let expired = world
        .spawn(armed_actor(
            CombatState::Recovery,
            ComboLocal {
                phase_elapsed: finisher.recovery_secs + 1.0,
                ..default()
            },
            false,
        ))
        .id();

    world.run_system_once(propose).unwrap();

    assert_eq!(
        proposals(&world, at_finisher)[0].target_state,
        CombatState::Recovery,
        "the finisher's buffered press must not chain past the end"
    );
    assert!(
        proposals(&world, expired).is_empty(),
        "an expired recovery proposes nothing: Idle wins by silence"
    );
}

#[test]
fn combo_state_does_not_bleed_between_actors() {
    let mut world = World::new();
    let fresh = world
        .spawn(armed_actor(CombatState::Idle, ComboLocal::default(), true))
        .id();
    let mid_combo = world
        .spawn(armed_actor(
            CombatState::Recovery,
            ComboLocal {
                step: 1,
                buffered: false,
                phase_elapsed: 0.05,
                ..default()
            },
            false,
        ))
        .id();

    world.run_system_once(propose).unwrap();

    assert_eq!(world.entity(fresh).get::<ComboLocal>().unwrap().step, 0);
    assert_eq!(world.entity(mid_combo).get::<ComboLocal>().unwrap().step, 1);
    assert_eq!(
        proposals(&world, fresh)[0].target_state,
        CombatState::Windup
    );
    assert_eq!(
        proposals(&world, mid_combo)[0].target_state,
        CombatState::Recovery
    );
}

#[test]
fn sneakstrike_multiplies_only_unaware_targets_while_sneaking() {
    let base = 10.0;
    assert_eq!(
        final_damage(base, 1.0, true, true),
        10.0,
        "alerted: no bonus"
    );
    assert_eq!(
        final_damage(base, 1.0, false, false),
        10.0,
        "unaware but not sneaking: normal melee"
    );
    assert_eq!(
        final_damage(base, 1.0, false, true),
        10.0 * SNEAKSTRIKE_MULT,
        "unaware + sneaking = sneakstrike"
    );
    assert_eq!(final_damage(base, 1.6, true, false), 16.0);
}

#[test]
fn swing_arc_accepts_front_and_rejects_flanks() {
    let pos = Vec3::new(0.0, 1.0, 0.0);
    let forward = Vec3::NEG_Z;
    assert!(within_swing_arc(
        pos,
        forward,
        Vec3::new(0.0, 1.0, -1.5),
        100.0
    ));
    assert!(within_swing_arc(
        pos,
        forward,
        Vec3::new(0.7, 1.0, -1.0),
        100.0
    ));
    assert!(!within_swing_arc(
        pos,
        forward,
        Vec3::new(1.5, 1.0, 0.0),
        100.0
    ));
    assert!(!within_swing_arc(
        pos,
        forward,
        Vec3::new(0.0, 1.0, 1.5),
        100.0
    ));
}

#[test]
fn active_swing_deduplicates_targets() {
    let mut swing = ActiveSwing::default();
    let target = Entity::PLACEHOLDER;
    assert!(!swing.contains(target));
    assert!(swing.insert(target));
    assert!(swing.contains(target));
    swing.clear();
    assert!(!swing.contains(target));
}

#[test]
fn active_swing_rejects_hits_beyond_ledger_capacity() {
    let mut swing = ActiveSwing::default();
    for index in 0..8 {
        assert!(swing.insert(Entity::from_raw_u32(index).unwrap()));
    }

    let overflow = Entity::from_raw_u32(9).unwrap();
    assert!(!swing.insert(overflow));
    assert!(!swing.contains(overflow));
}

#[test]
fn hostile_immunity_blocks_all_melee_outcomes() {
    let mut world = World::new();
    world.init_resource::<Messages<MeleeHitMessage>>();
    world.init_resource::<Messages<crate::health::DamageRequestMessage>>();
    world.init_resource::<Messages<DirectThreatMessage>>();
    world.init_resource::<Messages<crate::movement::constraints::BodyImpulseMessage>>();
    world.init_resource::<Messages<HitImpactMessage>>();
    let attacker = world
        .spawn((
            Actor,
            ComboLocal {
                snapshot: Some(WeaponProfile::GRAYBOX_SWORD),
                ..default()
            },
            LocomotionState::Walk,
            Name::new("Owner"),
        ))
        .id();
    let target = world
        .spawn((
            Transform::from_xyz(0.0, 1.0, -1.0),
            crate::health::HostileInteractionImmunity(attacker),
        ))
        .id();
    world.write_message(MeleeHitMessage {
        attacker,
        target,
        attacker_pos: Vec3::ZERO,
        step: 0,
    });

    world.run_system_once(resolve_melee_hits).unwrap();

    assert!(
        world
            .resource::<Messages<crate::health::DamageRequestMessage>>()
            .is_empty()
    );
    assert!(world.resource::<Messages<DirectThreatMessage>>().is_empty());
    assert!(
        world
            .resource::<Messages<crate::movement::constraints::BodyImpulseMessage>>()
            .is_empty()
    );
    assert!(world.resource::<Messages<HitImpactMessage>>().is_empty());
}
