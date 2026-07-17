//! Transient combat VFX placeholders.
//!
//! Swing arc: while combat has no animations, a translucent arc sector
//! flashes in front of the attacker during `Active`. Read-only over
//! simulation state (like the enemy tint); replaced by real animation later
//! without touching Combat.

use bevy::prelude::*;

use crate::combat::motors::attack::ComboLocal;
use crate::combat::state::CombatState;

const SWING_VFX_SECS: f32 = 0.16;

#[derive(Component)]
pub(super) struct SwingVfx {
    remaining: f32,
}

type SwingSourceQuery<'a> = (&'a Transform, &'a ComboLocal, &'a CombatState);

pub(super) fn spawn_swing_vfx(
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

pub(super) fn fade_swing_vfx(
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
