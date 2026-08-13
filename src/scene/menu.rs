//! The main menu: the one place that chooses which scene to build.
//!
//! Deliberately plain — it is a router, not a title screen. **Its rows are
//! generated from [`SCENES`]**, so the menu cannot drift from what actually
//! loads: adding a test box to the table adds it here, with its own heightmap,
//! without touching this file.
//!
//! It carries `DespawnOnExit(MainMenu)` like any scene content, so entering a
//! world removes it without a teardown system.

use bevy::feathers::controls::FeathersButton;
use bevy::feathers::theme::ThemedText;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

use super::{AppState, SCENES, SceneId};
use crate::presentation::theme::{ACCENT, BORDER, TEXT_MUTED, body_font};

/// What a menu row does when clicked: enter a scene, or leave.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum MenuChoice {
    Enter(SceneId),
    Quit,
}

/// Number keys select scenes in table order.
fn digit_key(index: usize) -> Option<KeyCode> {
    [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ]
    .get(index)
    .copied()
}

pub(super) struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::MainMenu), spawn_menu);
        app.add_systems(Update, choose_by_digit.run_if(in_state(AppState::MainMenu)));
    }
}

fn spawn_menu(mut commands: Commands) {
    commands
        .spawn((
            Name::new("MainMenu"),
            DespawnOnExit(AppState::MainMenu),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.03, 0.035, 0.92)),
            GlobalZIndex(200),
        ))
        .with_children(|screen| {
            screen.spawn((
                Text::new("BREATH OF FREEDOM"),
                body_font(34.0),
                TextColor(ACCENT),
                Node {
                    margin: UiRect::bottom(Val::Px(20.0)),
                    ..default()
                },
            ));
            for (index, def) in SCENES.iter().enumerate() {
                let digit = digit_key(index)
                    .map(|_| format!("{}  ", index + 1))
                    .unwrap_or_default();
                let lab = if def.authoring.grass_lab {
                    "  [Grass Lab]"
                } else {
                    ""
                };
                spawn_row(
                    screen,
                    MenuChoice::Enter(def.id),
                    &format!("{digit}{}{lab}", def.label),
                    def.hint,
                );
            }
            spawn_row(screen, MenuChoice::Quit, "Salir", "cierra el juego");
            screen.spawn((
                Text::new("F10 vuelve acá · F5 edita las escenas que declaran edición de terreno"),
                body_font(13.0),
                TextColor(TEXT_MUTED),
                Node {
                    margin: UiRect::top(Val::Px(18.0)),
                    ..default()
                },
            ));
        });
}

fn spawn_row(screen: &mut ChildSpawnerCommands, choice: MenuChoice, label: &str, hint: &str) {
    let label = label.to_string();
    let hint = hint.to_string();
    screen.spawn(choice).apply_scene(bsn! {
        @FeathersButton
        Node {
            width: Val::Px(520.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
        }
        BorderColor::all(BORDER)
        on(activate_menu_choice)
        Children [
            (Text({label}) ThemedText TextFont { font_size: FontSize::Px(20.0) }),
            (Text({hint}) ThemedText TextFont { font_size: FontSize::Px(13.0) }),
        ]
    });
}

fn activate_menu_choice(
    activate: On<Activate>,
    choices: Query<&MenuChoice>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(choice) = choices.get(activate.entity) else {
        return;
    };
    match *choice {
        MenuChoice::Enter(id) => next.set(AppState::Scene(id)),
        MenuChoice::Quit => {
            exit.write(AppExit::Success);
        }
    }
}

/// El mouse ya dispara `activate_menu_choice` vía `bevy_ui_widgets::Activate`;
/// esto sólo cubre el atajo de número, que no pasa por ningún botón.
fn choose_by_digit(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<AppState>>) {
    for (index, def) in SCENES.iter().enumerate() {
        if digit_key(index).is_some_and(|key| keys.just_pressed(key)) {
            next.set(AppState::Scene(def.id));
        }
    }
}
