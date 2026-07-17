//! Projectiles — arrows in ballistic flight.
//!
//! Combat emits [`SpawnProjectileMessage`] (this plugin owns the type — the
//! receiver owns the contract); the arrow then belongs to this plugin: a
//! kinematic point swept with a ray each fixed tick under gravity. On hitting
//! an actor-layer collider it resolves damage (stealth bonus against a
//! non-alerted target, per `docs/architecture/combat.md` § Relaciones) and
//! feeds the same feedback channels as melee (`HitImpactMessage`,
//! `DirectThreatMessage`, knockback); on world geometry it sticks and fades.
//! Spawn messages are consumed the same tick they are emitted (this plugin's
//! fixed systems are pinned after `CombatSet::EmitConstraints`); the arrow
//! entity flies from the next tick, when its spawn command has applied — a
//! stable 1-tick latency, same accepted criterion as the constraint channel.

use avian3d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::combat::motors::attack::HitImpactMessage;
use crate::enemies::perception::{Awareness, DirectThreatMessage};
use crate::movement::GRAVITY;
use crate::movement::constraints::BodyImpulseMessage;
use crate::world::GameLayer;

/// Bonus multiplier for an arrow into a target that has not noticed you.
/// The arrow needs no `Sneak` from the shooter — contract in `combat.md`.
const ARROW_STEALTH_MULT: f32 = 4.0;
/// Knockback carried by an arrow (much lighter than melee).
const ARROW_KNOCKBACK: f32 = 2.5;
/// Flight/stuck lifetimes (s).
const FLIGHT_TTL_SECS: f32 = 8.0;
const STUCK_TTL_SECS: f32 = 4.0;

/// Trail particles left behind flying arrows.
const TRAIL_TTL_SECS: f32 = 0.28;
const TRAIL_EMIT_INTERVAL: f32 = 0.016;
const TRAIL_PARTICLE_SIZE: f32 = 0.04;

/// Ask Projectiles to launch an arrow. Owned by this plugin; Combat emits it.
#[derive(Message, Debug, Clone, Copy)]
pub struct SpawnProjectileMessage {
    pub shooter: Entity,
    pub origin: Vec3,
    pub velocity: Vec3,
    pub damage: f32,
}

#[derive(Component)]
pub struct Arrow {
    velocity: Vec3,
    shooter: Entity,
    damage: f32,
    /// Remaining flight time; sticking swaps it for the stuck countdown.
    remaining: f32,
    stuck: bool,
    trail_timer: f32,
}

#[derive(Component)]
struct TrailParticle {
    remaining: f32,
}

/// Shared arrow/trail render assets, created once at startup: spawns in
/// `FixedUpdate` clone handles instead of allocating assets (Constitución
/// §18), and the shared material keeps the renderer batching particles.
#[derive(Resource)]
pub(crate) struct ArrowAssets {
    arrow_mesh: Handle<Mesh>,
    arrow_material: Handle<StandardMaterial>,
    trail_mesh: Handle<Mesh>,
    trail_material: Handle<StandardMaterial>,
}

/// Ordering handle for the ballistic step within `FixedUpdate`. Health
/// applies damage after it (arrows are the latest same-tick damage emitter).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProjectilesSet {
    Simulate,
}

pub struct ProjectilesPlugin;

impl Plugin for ProjectilesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpawnProjectileMessage>();
        app.add_systems(Startup, init_arrow_assets);
        // Pinned after Combat so the spawn message is read the same tick it
        // is written — unordered, the scheduler could put these before the
        // writer and the latency would flip between 0 and 1 ticks per run.
        app.configure_sets(
            FixedUpdate,
            ProjectilesSet::Simulate.after(crate::combat::CombatSet::EmitConstraints),
        );
        app.add_systems(
            FixedUpdate,
            (spawn_arrows, fly_arrows)
                .chain()
                .in_set(ProjectilesSet::Simulate),
        );
        app.add_systems(Update, tick_trail_particles);
    }
}

fn init_arrow_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(ArrowAssets {
        arrow_mesh: meshes.add(Cuboid::new(0.05, 0.05, 0.55)),
        arrow_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.85, 0.7),
            unlit: true,
            ..default()
        }),
        trail_mesh: meshes.add(Sphere::new(TRAIL_PARTICLE_SIZE)),
        trail_material: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.95, 0.7, 0.8),
            unlit: true,
            ..default()
        }),
    });
}

fn spawn_arrows(
    mut commands: Commands,
    mut spawns: MessageReader<SpawnProjectileMessage>,
    assets: Res<ArrowAssets>,
) {
    for spawn in spawns.read() {
        commands.spawn((
            Arrow {
                velocity: spawn.velocity,
                shooter: spawn.shooter,
                damage: spawn.damage,
                remaining: FLIGHT_TTL_SECS,
                stuck: false,
                trail_timer: 0.0,
            },
            Name::new("Arrow"),
            Mesh3d(assets.arrow_mesh.clone()),
            MeshMaterial3d(assets.arrow_material.clone()),
            Transform::from_translation(spawn.origin)
                .looking_to(Dir3::new(spawn.velocity).unwrap_or(Dir3::NEG_Z), Vec3::Y),
        ));
    }
}

/// One ballistic step: gravity, then a ray sweep over this tick's path. The
/// ray sees everything except the shooter — world geometry stops arrows,
/// actor-layer colliders take the hit.
type ArrowQuery<'a> = (Entity, &'a mut Arrow, &'a mut Transform);

/// The consequence channels a landed arrow feeds — same set as melee.
#[derive(SystemParam)]
pub struct HitOutcomes<'w> {
    damage: MessageWriter<'w, crate::health::DamageRequestMessage>,
    impacts: MessageWriter<'w, HitImpactMessage>,
    threats: MessageWriter<'w, DirectThreatMessage>,
    impulses: MessageWriter<'w, BodyImpulseMessage>,
}

#[derive(SystemParam)]
pub(crate) struct BallisticWorld<'w, 's> {
    spatial: SpatialQuery<'w, 's>,
    layers: Query<'w, 's, &'static CollisionLayers>,
    targets: Query<
        'w,
        's,
        (
            Option<&'static Awareness>,
            Option<&'static Name>,
            Option<&'static crate::health::HostileInteractionImmunity>,
        ),
    >,
}

pub fn fly_arrows(
    mut commands: Commands,
    time: Res<Time>,
    mut arrows: Query<ArrowQuery>,
    mut outcomes: HitOutcomes,
    assets: Res<ArrowAssets>,
    world: BallisticWorld,
) {
    let dt = time.delta_secs();
    for (entity, mut arrow, mut transform) in &mut arrows {
        arrow.remaining -= dt;
        if arrow.remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        if arrow.stuck {
            continue;
        }

        arrow.velocity.y -= GRAVITY * 0.22 * dt;
        let step = arrow.velocity * dt;
        let Ok(direction) = Dir3::new(step) else {
            continue;
        };
        let filter = SpatialQueryFilter::from_excluded_entities([arrow.shooter, entity]);
        match world.spatial.cast_ray(
            transform.translation,
            direction,
            step.length(),
            true,
            &filter,
        ) {
            Some(hit) => {
                let hit_point = transform.translation + *direction * hit.distance;
                let struck_actor = world
                    .layers
                    .get(hit.entity)
                    .is_ok_and(|l| l.memberships.has_all(GameLayer::Actor));
                if struck_actor {
                    resolve_arrow_hit(&arrow, hit.entity, hit_point, &world.targets, &mut outcomes);
                    commands.entity(entity).despawn();
                } else {
                    // World geometry: stick where it landed and fade out.
                    transform.translation = hit_point;
                    arrow.stuck = true;
                    arrow.remaining = STUCK_TTL_SECS;
                }
            }
            None => {
                transform.translation += step;
                transform.look_to(direction, Vec3::Y);
            }
        }

        // Emit trail particles behind the arrow at a fixed interval.
        arrow.trail_timer += dt;
        if arrow.trail_timer >= TRAIL_EMIT_INTERVAL {
            arrow.trail_timer -= TRAIL_EMIT_INTERVAL;
            commands.spawn((
                TrailParticle {
                    remaining: TRAIL_TTL_SECS,
                },
                Mesh3d(assets.trail_mesh.clone()),
                MeshMaterial3d(assets.trail_material.clone()),
                Transform::from_translation(transform.translation),
            ));
        }
    }
}

/// Arrow damage rule, pure: bonus against a target that hasn't noticed you —
/// no `Sneak` required from the shooter, unlike the melee sneakstrike.
pub(crate) fn arrow_damage(base: f32, target_alerted: bool) -> (f32, bool) {
    if target_alerted {
        (base, false)
    } else {
        (base * ARROW_STEALTH_MULT, true)
    }
}

fn resolve_arrow_hit(
    arrow: &Arrow,
    target: Entity,
    hit_point: Vec3,
    targets: &Query<(
        Option<&Awareness>,
        Option<&Name>,
        Option<&crate::health::HostileInteractionImmunity>,
    )>,
    outcomes: &mut HitOutcomes,
) {
    let (awareness, name, immunity) = targets.get(target).unwrap_or((None, None, None));
    if immunity.is_some_and(|immunity| immunity.blocks(arrow.shooter)) {
        return;
    }
    // A target without an Awareness meter (practice target, another player)
    // counts as alerted: no stealth bonus.
    let target_alerted = awareness.is_none_or(Awareness::is_alerted);
    let (damage, critical) = arrow_damage(arrow.damage, target_alerted);

    if critical {
        let label = name.map(Name::as_str).unwrap_or("target");
        info!("[combat] STEALTH SHOT on {label}");
    }

    outcomes.damage.write(crate::health::DamageRequestMessage {
        target,
        amount: damage,
        source: Some(arrow.shooter),
    });
    outcomes.impacts.write(HitImpactMessage {
        target,
        position: hit_point,
        damage,
        critical,
    });
    let planar = Vec3::new(arrow.velocity.x, 0.0, arrow.velocity.z).normalize_or_zero();
    outcomes.impulses.write(BodyImpulseMessage {
        entity: target,
        impulse: planar * ARROW_KNOCKBACK,
    });
    // The struck enemy learns roughly where the shot came from: back along
    // the arrow's flight.
    outcomes.threats.write(DirectThreatMessage {
        enemy: target,
        threat_position: hit_point - arrow.velocity.normalize_or_zero() * 6.0,
    });
}

fn tick_trail_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut TrailParticle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut particle, mut transform) in &mut particles {
        particle.remaining -= dt;
        if particle.remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let t = (particle.remaining / TRAIL_TTL_SECS).max(0.01);
        transform.scale = Vec3::splat(t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[derive(Component)]
    struct TestTarget;

    fn resolve_test_arrow(
        arrows: Query<&Arrow>,
        targets: Query<(
            Option<&Awareness>,
            Option<&Name>,
            Option<&crate::health::HostileInteractionImmunity>,
        )>,
        target: Single<Entity, With<TestTarget>>,
        mut outcomes: HitOutcomes,
    ) {
        let arrow = arrows.single().unwrap();
        resolve_arrow_hit(arrow, *target, Vec3::ZERO, &targets, &mut outcomes);
    }

    #[test]
    fn arrows_reward_hitting_the_unaware() {
        assert_eq!(arrow_damage(12.0, true), (12.0, false));
        assert_eq!(arrow_damage(12.0, false), (12.0 * ARROW_STEALTH_MULT, true));
    }

    #[test]
    fn hostile_immunity_blocks_all_arrow_outcomes() {
        let mut world = World::new();
        world.init_resource::<Messages<crate::health::DamageRequestMessage>>();
        world.init_resource::<Messages<HitImpactMessage>>();
        world.init_resource::<Messages<DirectThreatMessage>>();
        world.init_resource::<Messages<BodyImpulseMessage>>();
        let shooter = world.spawn_empty().id();
        world.spawn(Arrow {
            velocity: Vec3::NEG_Z,
            shooter,
            damage: 10.0,
            remaining: 1.0,
            stuck: false,
            trail_timer: 0.0,
        });
        world.spawn((
            TestTarget,
            crate::health::HostileInteractionImmunity(shooter),
        ));

        world.run_system_once(resolve_test_arrow).unwrap();

        assert!(
            world
                .resource::<Messages<crate::health::DamageRequestMessage>>()
                .is_empty()
        );
        assert!(world.resource::<Messages<HitImpactMessage>>().is_empty());
        assert!(world.resource::<Messages<DirectThreatMessage>>().is_empty());
        assert!(world.resource::<Messages<BodyImpulseMessage>>().is_empty());
    }
}
