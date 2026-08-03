pub const ACTION_COUNT: usize = 21;

/// Domain-neutral actions resolved from hardware before gameplay reads them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IntentAction {
    MoveForward,
    MoveBack,
    MoveLeft,
    MoveRight,
    LookUp,
    LookDown,
    LookLeft,
    LookRight,
    Jump,
    Sprint,
    Sneak,
    ClimbToggle,
    Mantle,
    Vault,
    Glide,
    Attack,
    Aim,
    Interact,
    UseItem,
    CycleWeapon,
    /// Toggle Zelda-style lock-on onto the nearest enemy near the crosshair.
    LockOn,
}

impl IntentAction {
    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn bit(self) -> u32 {
        1 << self.index()
    }
}
