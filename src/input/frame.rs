use bevy::prelude::*;

use super::action::{ACTION_COUNT, IntentAction};

pub const MAX_INPUT_SOURCES: usize = 8;
pub const LOCAL_INPUT_SOURCE: InputSource = InputSource(0);

/// Identifies a producer of resolved actions (local hardware, AI, or network).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InputSource(pub u8);

impl InputSource {
    pub const fn slot(self) -> Option<usize> {
        let slot = self.0 as usize;
        if slot < MAX_INPUT_SOURCES {
            Some(slot)
        } else {
            None
        }
    }
}

/// Link from an actor to the source its Brain reads.
#[derive(Component, Debug, Clone, Copy)]
pub struct InputControlledBy(pub InputSource);

/// Camera-relative orientation owned by input/control, not presentation.
#[derive(Component, Debug, Default)]
pub struct ControlOrientation {
    pub yaw: f32,
    pub pitch: f32,
}

/// Serializable action state for one source. Held actions overwrite every
/// render frame; trigger generations persist until each consumer observes them.
#[derive(Debug, Clone, Copy)]
pub struct ActionFrame {
    held: u32,
    generations: [u64; ACTION_COUNT],
}

impl Default for ActionFrame {
    fn default() -> Self {
        Self {
            held: 0,
            generations: [0; ACTION_COUNT],
        }
    }
}

impl ActionFrame {
    pub fn pressed(&self, action: IntentAction) -> bool {
        self.held & action.bit() != 0
    }

    pub fn generation(&self, action: IntentAction) -> u64 {
        self.generations[action.index()]
    }

    fn set_pressed(&mut self, action: IntentAction, pressed: bool) {
        if pressed {
            self.held |= action.bit();
        } else {
            self.held &= !action.bit();
        }
    }

    fn trigger(&mut self, action: IntentAction) {
        self.generations[action.index()] = self.generations[action.index()].wrapping_add(1);
    }
}

/// Fixed-capacity snapshots published by Input. Future AI/network producers
/// write their assigned source slot; gameplay only reads it.
#[derive(Resource)]
pub struct ActiveActions {
    frames: [ActionFrame; MAX_INPUT_SOURCES],
}

impl Default for ActiveActions {
    fn default() -> Self {
        Self {
            frames: [ActionFrame::default(); MAX_INPUT_SOURCES],
        }
    }
}

impl ActiveActions {
    pub fn frame(&self, source: InputSource) -> Option<&ActionFrame> {
        source.slot().map(|slot| &self.frames[slot])
    }

    /// Updates a held action for a source. Network and AI adapters use this
    /// API instead of reaching into hardware-specific input code.
    pub(crate) fn set_pressed(&mut self, source: InputSource, action: IntentAction, pressed: bool) {
        if let Some(frame) = self.frame_mut(source) {
            frame.set_pressed(action, pressed);
        }
    }

    /// Publishes an edge-triggered action for a source.
    pub(crate) fn trigger(&mut self, source: InputSource, action: IntentAction) {
        if let Some(frame) = self.frame_mut(source) {
            frame.trigger(action);
        }
    }

    fn frame_mut(&mut self, source: InputSource) -> Option<&mut ActionFrame> {
        source.slot().map(|slot| &mut self.frames[slot])
    }
}

/// Per-consumer trigger cursor. Consumers never mutate the shared snapshot.
#[derive(Component, Default)]
pub struct InputConsumeCursor {
    seen_generations: [u64; ACTION_COUNT],
}

impl InputConsumeCursor {
    pub fn consume(&mut self, frame: &ActionFrame, action: IntentAction) -> bool {
        let generation = frame.generation(action);
        let seen = &mut self.seen_generations[action.index()];
        if *seen == generation {
            return false;
        }
        *seen = generation;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_is_consumed_once_per_cursor() {
        let mut frame = ActionFrame::default();
        let mut first = InputConsumeCursor::default();
        let mut second = InputConsumeCursor::default();

        frame.trigger(IntentAction::ClimbToggle);
        assert!(first.consume(&frame, IntentAction::ClimbToggle));
        assert!(!first.consume(&frame, IntentAction::ClimbToggle));
        assert!(second.consume(&frame, IntentAction::ClimbToggle));
    }

    #[test]
    fn sources_are_isolated() {
        let mut actions = ActiveActions::default();
        actions
            .frame_mut(LOCAL_INPUT_SOURCE)
            .unwrap()
            .set_pressed(IntentAction::Sprint, true);

        assert!(
            actions
                .frame(LOCAL_INPUT_SOURCE)
                .unwrap()
                .pressed(IntentAction::Sprint)
        );
        assert!(
            !actions
                .frame(InputSource(1))
                .unwrap()
                .pressed(IntentAction::Sprint)
        );
    }
}
