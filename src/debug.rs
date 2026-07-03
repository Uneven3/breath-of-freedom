//! On-screen debug readout.
//!
//! A real HUD (state / stamina / velocity / grounded) to see what each motor
//! is doing. Read-only: it never mutates gameplay state (Constitution §7).

use bevy::prelude::*;

use crate::movement::facts::GroundFacts;
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{BodyVelocity, Player};

#[derive(Component)]
struct DebugText;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_debug_text);
        app.add_systems(Update, update_debug_text);
    }
}

fn spawn_debug_text(mut commands: Commands) {
    commands.spawn((
        DebugText,
        Text::new("…"),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            ..default()
        },
    ));
}

fn update_debug_text(
    player: Single<(&LocomotionState, &Stamina, &BodyVelocity, &GroundFacts), With<Player>>,
    mut text: Single<&mut Text, With<DebugText>>,
) {
    let (state, stamina, vel, ground) = *player;
    let speed = vel.0.length();
    text.0 = format!(
        "state: {:?}\nstamina: {:.0}/{:.0}\nvel: ({:.2}, {:.2}, {:.2})  |v|={:.2}\ngrounded: {}",
        state,
        stamina.current(),
        stamina.max(),
        vel.0.x,
        vel.0.y,
        vel.0.z,
        speed,
        ground.grounded,
    );
}
