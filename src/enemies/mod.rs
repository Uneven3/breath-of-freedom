//! Compatibility path while the app's call sites move to the sibling crate.
//!
//! Which scenes get enemies is composition and lives in `crate::scene`, which
//! writes `BokoboSpawnRequest` on entry; simulation owns how one is built.

pub use bof_simulation::enemies::*;
