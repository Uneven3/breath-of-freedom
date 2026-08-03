//! Simulation adapters for the domain-owned body dimensions.

use avian3d::prelude::Collider;

pub use bof_domain::movement::body::BodyDimensions;

pub fn standing_collider(body: BodyDimensions) -> Collider {
    Collider::capsule(body.radius, body.standing_capsule_length)
}

pub fn crouched_collider(body: BodyDimensions) -> Collider {
    Collider::capsule(body.radius, body.crouched_capsule_length)
}
