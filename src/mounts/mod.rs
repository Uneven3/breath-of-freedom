//! Compatibility path while the app's call sites move to the sibling crate.
//!
//! Which scenes get a horse is composition and lives in `crate::scene`, which
//! writes `HorseSpawnRequest` on entry; simulation owns how one is built.

pub use bof_simulation::mounts::*;
