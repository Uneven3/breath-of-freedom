//! Stamina pool (max 100, starts full).
//!
//! Only stamina's own mutators (`drain`/`recover`, called by motors) change it
//! (see `docs/architecture/movement.md`).

use bevy::prelude::*;

#[derive(Component, Debug, Clone)]
pub struct Stamina {
    current: f32,
    max: f32,
}

impl Default for Stamina {
    fn default() -> Self {
        Self {
            current: 100.0,
            max: 100.0,
        }
    }
}

impl Stamina {
    pub fn drain(&mut self, amount: f32) {
        self.current = (self.current - amount).clamp(0.0, self.max);
    }
    pub fn recover(&mut self, amount: f32) {
        self.current = (self.current + amount).clamp(0.0, self.max);
    }
    pub fn is_exhausted(&self) -> bool {
        self.current <= 0.0
    }
    pub fn current(&self) -> f32 {
        self.current
    }
    pub fn max(&self) -> f32 {
        self.max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_and_recover_clamp() {
        let mut s = Stamina::default();
        s.drain(150.0);
        assert_eq!(s.current(), 0.0);
        assert!(s.is_exhausted());
        s.recover(250.0);
        assert_eq!(s.current(), s.max());
    }
}
