//! Input resolution: hardware is sampled once per render frame and published
//! as fixed-capacity action snapshots for gameplay consumers.

pub mod action;
pub mod frame;

use bevy::app::AppExit;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use action::IntentAction;
use frame::{ActiveActions, ControlOrientation, InputControlledBy, LOCAL_INPUT_SOURCE};

pub use frame::InputConsumeCursor;

/// Shared with the debug freecam so its look feel matches the player's.
pub(crate) const MOUSE_SENSITIVITY: f32 = 0.003;
const PITCH_MIN: f32 = -1.2;
const PITCH_MAX: f32 = 1.2;

/// Whether local pointer input currently controls the local source.
#[derive(Resource)]
pub struct PointerCaptured(pub bool);

/// Presentation focus owners. While at least one owner is registered, local
/// gameplay actions and camera look are neutralized and the pointer is
/// released. Fixed capacity keeps `PreUpdate` allocation-free.
#[derive(Resource, Default)]
pub struct ModalInputFocus {
    owners: [Option<Entity>; 8],
    len: usize,
}

impl ModalInputFocus {
    pub fn is_active(&self) -> bool {
        self.len != 0
    }

    pub fn purge_despawned(&mut self, entities: &bevy::ecs::entity::Entities) -> bool {
        let mut changed = false;
        let mut i = 0;
        while i < self.len {
            if let Some(owner) = self.owners[i]
                && !entities.contains(owner)
            {
                self.len -= 1;
                self.owners[i] = self.owners[self.len];
                self.owners[self.len] = None;
                changed = true;
                continue;
            }
            i += 1;
        }
        changed
    }

    fn acquire(&mut self, owner: Entity) -> bool {
        if self.owners[..self.len].contains(&Some(owner)) {
            return false;
        }
        if self.len == self.owners.len() {
            warn!("modal input focus capacity exhausted; rejecting {owner:?}");
            return false;
        }
        self.owners[self.len] = Some(owner);
        self.len += 1;
        true
    }

    fn release(&mut self, owner: Entity) -> bool {
        let Some(index) = self.owners[..self.len]
            .iter()
            .position(|candidate| *candidate == Some(owner))
        else {
            return false;
        };
        self.len -= 1;
        self.owners[index] = self.owners[self.len];
        self.owners[self.len] = None;
        true
    }
}

/// Input owns modal focus and validates releases, so presentation layers do
/// not mutate cursor or gameplay input resources directly.
#[derive(Message, Debug, Clone, Copy)]
pub enum ModalInputFocusRequest {
    Acquire(Entity),
    Release(Entity),
}

/// Presentation asks Input to grab/release the pointer; Input stays the sole
/// writer of `CursorOptions` (§7). The debug freecam uses this for hold-to-look
/// instead of touching the cursor itself.
#[derive(Message, Debug, Clone, Copy)]
pub struct SetCursorGrab(pub bool);

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum InputSet {
    UpdateOrientation,
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveActions>();
        app.insert_resource(PointerCaptured(true));
        app.init_resource::<ModalInputFocus>();
        app.add_message::<ModalInputFocusRequest>();
        app.add_message::<SetCursorGrab>();
        app.add_systems(Startup, grab_cursor);
        // Everything the fixed-step simulation reads (actions AND orientation)
        // must resolve in PreUpdate: Bevy runs FixedUpdate *before* Update in
        // each frame, so an Update-schedule writer is always one frame stale
        // for FixedUpdate consumers (movement direction, bow aim).
        app.add_systems(
            PreUpdate,
            (
                apply_modal_focus_requests,
                resolve_local_actions,
                cursor_control,
                update_local_orientation,
                // Last, so an explicit presentation grab request (freecam
                // hold-to-look) overrides the click/escape cursor policy.
                apply_cursor_grab_requests,
            )
                .chain()
                .in_set(InputSet::UpdateOrientation)
                // Bevy refreshes ButtonInput/AccumulatedMouseMotion in
                // PreUpdate too — order after them or we read last frame's
                // hardware state nondeterministically.
                .after(bevy::input::InputSystems),
        );
    }
}

fn resolve_local_actions(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    captured: Res<PointerCaptured>,
    focus: Res<ModalInputFocus>,
    mut actions: ResMut<ActiveActions>,
) {
    if focus.is_active() {
        actions.clear_pressed(LOCAL_INPUT_SOURCE);
        return;
    }
    for (action, key) in LOCAL_HELD_BINDINGS {
        actions.set_pressed(LOCAL_INPUT_SOURCE, action, keys.pressed(key));
    }
    // Attack on left mouse or F / aim on right mouse or Q (keyboard alternates
    // for pads without buttons). Mouse is gated on the pointer being captured
    // — the same click that re-captures the cursor must not also act; keys
    // need no gate.
    let attack_held =
        (captured.0 && mouse.pressed(MouseButton::Left)) || keys.pressed(KeyCode::KeyF);
    actions.set_pressed(LOCAL_INPUT_SOURCE, IntentAction::Attack, attack_held);
    if (captured.0 && mouse.just_pressed(MouseButton::Left)) || keys.just_pressed(KeyCode::KeyF) {
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::Attack);
    }
    let aim_held = (captured.0 && mouse.pressed(MouseButton::Right)) || keys.pressed(KeyCode::KeyQ);
    actions.set_pressed(LOCAL_INPUT_SOURCE, IntentAction::Aim, aim_held);
    actions.set_pressed(
        LOCAL_INPUT_SOURCE,
        IntentAction::Sprint,
        keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight),
    );
    actions.set_pressed(
        LOCAL_INPUT_SOURCE,
        IntentAction::Sneak,
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight),
    );
    if keys.just_pressed(KeyCode::Digit1) {
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::ClimbToggle);
    }
    if keys.just_pressed(KeyCode::Space) {
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::Jump);
    }
    if keys.just_pressed(KeyCode::KeyE) {
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::Interact);
    }
    if keys.just_pressed(KeyCode::KeyC) {
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::UseItem);
    }
    if keys.just_pressed(KeyCode::Digit4) {
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::CycleWeapon);
    }
    // Lock-on toggle: middle mouse (PC standard) or T alternate. Mouse gated on
    // capture like Attack/Aim so the recapturing click never also locks on.
    if (captured.0 && mouse.just_pressed(MouseButton::Middle)) || keys.just_pressed(KeyCode::KeyT) {
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::LockOn);
    }
}

const LOCAL_HELD_BINDINGS: [(IntentAction, KeyCode); 12] = [
    (IntentAction::MoveForward, KeyCode::KeyW),
    (IntentAction::MoveBack, KeyCode::KeyS),
    (IntentAction::MoveLeft, KeyCode::KeyA),
    (IntentAction::MoveRight, KeyCode::KeyD),
    (IntentAction::Jump, KeyCode::Space),
    (IntentAction::Glide, KeyCode::Space),
    (IntentAction::Mantle, KeyCode::Digit2),
    (IntentAction::Vault, KeyCode::Digit3),
    (IntentAction::LookUp, KeyCode::KeyI),
    (IntentAction::LookDown, KeyCode::KeyK),
    (IntentAction::LookLeft, KeyCode::KeyJ),
    (IntentAction::LookRight, KeyCode::KeyL),
];

fn grab_cursor(
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut captured: ResMut<PointerCaptured>,
) {
    set_cursor(&mut cursor, &mut captured, true);
}

fn cursor_control(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    focus: Res<ModalInputFocus>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut captured: ResMut<PointerCaptured>,
    mut exit: MessageWriter<AppExit>,
) {
    if focus.is_active() {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        if captured.0 {
            set_cursor(&mut cursor, &mut captured, false);
        } else {
            exit.write(AppExit::Success);
        }
    } else if !captured.0 && mouse.just_pressed(MouseButton::Left) {
        set_cursor(&mut cursor, &mut captured, true);
    }
}

/// Applies presentation's cursor-grab requests, keeping Input the single writer
/// of `CursorOptions`. Only the last request in a frame matters.
fn apply_cursor_grab_requests(
    mut requests: MessageReader<SetCursorGrab>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut captured: ResMut<PointerCaptured>,
) {
    if let Some(SetCursorGrab(grab)) = requests.read().last().copied() {
        set_cursor(&mut cursor, &mut captured, grab);
    }
}

fn set_cursor(
    cursor: &mut Query<&mut CursorOptions, With<PrimaryWindow>>,
    captured: &mut PointerCaptured,
    grab: bool,
) {
    if let Ok(mut options) = cursor.single_mut() {
        options.grab_mode = if grab {
            CursorGrabMode::Locked
        } else {
            CursorGrabMode::None
        };
        options.visible = !grab;
    }
    captured.0 = grab;
}

fn apply_modal_focus_requests(
    mut requests: MessageReader<ModalInputFocusRequest>,
    mut focus: ResMut<ModalInputFocus>,
    entities: &bevy::ecs::entity::Entities,
    mut actions: ResMut<ActiveActions>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut captured: ResMut<PointerCaptured>,
) {
    let mut changed = focus.purge_despawned(entities);
    for request in requests.read() {
        match *request {
            ModalInputFocusRequest::Acquire(owner) => {
                if entities.contains(owner) && focus.acquire(owner) {
                    actions.clear_pressed(LOCAL_INPUT_SOURCE);
                    actions.discard_pending(LOCAL_INPUT_SOURCE);
                    changed = true;
                }
            }
            ModalInputFocusRequest::Release(owner) => changed |= focus.release(owner),
        }
    }
    if changed {
        set_cursor(&mut cursor, &mut captured, !focus.is_active());
    }
}

fn update_local_orientation(
    motion: Res<AccumulatedMouseMotion>,
    captured: Res<PointerCaptured>,
    focus: Res<ModalInputFocus>,
    mut q: Query<(&InputControlledBy, &mut ControlOrientation)>,
) {
    if focus.is_active() || !captured.0 || motion.delta == Vec2::ZERO {
        return;
    }
    for (source, mut orientation) in &mut q {
        if source.0 != LOCAL_INPUT_SOURCE {
            continue;
        }
        orientation.yaw -= motion.delta.x * MOUSE_SENSITIVITY;
        orientation.pitch =
            (orientation.pitch - motion.delta.y * MOUSE_SENSITIVITY).clamp(PITCH_MIN, PITCH_MAX);
    }
}

#[cfg(test)]
mod modal_focus_tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    #[test]
    fn modal_focus_neutralizes_held_local_actions() {
        let mut world = World::new();
        world.init_resource::<ButtonInput<KeyCode>>();
        world.init_resource::<ButtonInput<MouseButton>>();
        world.insert_resource(PointerCaptured(false));
        let mut focus = ModalInputFocus::default();
        assert!(focus.acquire(Entity::PLACEHOLDER));
        world.insert_resource(focus);
        let mut actions = ActiveActions::default();
        actions.set_pressed(LOCAL_INPUT_SOURCE, IntentAction::MoveForward, true);
        world.insert_resource(actions);

        world.run_system_once(resolve_local_actions).unwrap();

        assert!(
            !world
                .resource::<ActiveActions>()
                .frame(LOCAL_INPUT_SOURCE)
                .unwrap()
                .pressed(IntentAction::MoveForward)
        );
    }

    #[test]
    fn modal_focus_remains_active_until_every_owner_releases() {
        let first = Entity::from_raw_u32(1).unwrap();
        let second = Entity::from_raw_u32(2).unwrap();
        let mut focus = ModalInputFocus::default();

        assert!(focus.acquire(first));
        assert!(focus.acquire(second));
        assert!(focus.release(first));
        assert!(focus.is_active());
        assert!(focus.release(second));
        assert!(!focus.is_active());

        assert!(focus.acquire(first));
        assert!(focus.acquire(second));
        assert!(focus.release(second));
        assert!(focus.is_active());
        assert!(focus.release(first));
        assert!(!focus.is_active());
    }

    #[test]
    fn modal_focus_purges_despawned_owner_and_releases_active_state() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let mut focus = ModalInputFocus::default();
        assert!(focus.acquire(entity));
        assert!(focus.is_active());

        world.despawn(entity);

        assert!(focus.purge_despawned(world.entities()));
        assert!(!focus.is_active());
    }
}
