//! World constants and the deterministic scatter hash.
//!
//! Both sides need these and neither owns them: the terrain grid derives its
//! spacing from [`WORLD_SIZE`], the authored layout places its perimeter from
//! it, and [`hash_u32`] seeds the forest (simulation) and the grass meadow
//! (presentation) from the same value, so the two agree on where a blade or a
//! trunk lands without one calling the other.

/// Side of the playable square, in metres.
pub const WORLD_SIZE: f32 = 320.0;

/// La subida más alta que un tramo de terreno **no caminable** puede acumular
/// antes de volverse intransitable.
///
/// El terreno no se escala —un heightmap no puede plegarse, así que no
/// representa una vertical— y por eso lo empinado sólo se pasa con vault o
/// mantle, que están limitados por **altura de cara**, no por pendiente. Una
/// contrahuella de 2 m se sube; una pared de 20 m hecha del mismo ángulo, no.
pub const MAX_UNWALKABLE_RISE_METRES: f32 = 2.5;

const _: () = {
    // La regla de autoría no es un número propio: es el alcance del mantle. Si
    // uno se mueve sin el otro, el editor autoriza relieve que nadie puede pasar.
    assert!(
        MAX_UNWALKABLE_RISE_METRES
            == crate::movement::sensing::LedgeSensing::PLAYER.mantle_max_height
    );
};

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
