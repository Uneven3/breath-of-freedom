#[derive(Debug, Clone, Copy)]
pub struct GeneratedAsset {
    pub key: &'static str,
    pub path: &'static str,
    pub profile: Option<&'static str>,
    pub sockets: &'static [GeneratedSocket],
    pub colliders: &'static [GeneratedCollider],
}

#[derive(Debug, Clone, Copy)]
pub struct GeneratedSocket {
    pub name: &'static str,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratedColliderKind {
    Box,
    Capsule,
    ConvexHull,
    Cylinder,
    Sphere,
}

impl GeneratedColliderKind {
    pub const ALL: [Self; 5] = [
        Self::Box,
        Self::Capsule,
        Self::ConvexHull,
        Self::Cylinder,
        Self::Sphere,
    ];
}

#[derive(Debug, Clone, Copy)]
pub struct GeneratedCollider {
    pub name: &'static str,
    pub kind: GeneratedColliderKind,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub size: [f32; 3],
    pub points: &'static [[f32; 3]],
    pub climbable: bool,
    pub material_kind: Option<&'static str>,
}

include!(concat!(env!("OUT_DIR"), "/authored_assets.rs"));
