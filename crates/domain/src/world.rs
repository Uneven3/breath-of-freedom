//! World constants and the deterministic scatter hash.
//!
//! Both sides need these and neither owns them: the terrain grid derives its
//! spacing from [`WORLD_SIZE`], the authored layout places its perimeter from
//! it, and [`hash_u32`] seeds the forest (simulation) and the grass meadow
//! (presentation) from the same value, so the two agree on where a blade or a
//! trunk lands without one calling the other.

/// Side of the playable square, in metres.
pub const WORLD_SIZE: f32 = 320.0;

/// Deterministic scatter hash.
pub fn hash_u32(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

/// Deterministic scatter hash in `[0, 1]`.
pub fn hash_unit(value: u32) -> f32 {
    hash_u32(value) as f32 / u32::MAX as f32
}
