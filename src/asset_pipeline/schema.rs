pub const ASSET_CATEGORIES: &[&str] = &["char", "creature", "prop", "structure", "tree", "weapon"];

pub const PALETTE_KEYS: &[&str] = &[
    "Bark",
    "Fletching",
    "FoliageCommon",
    "FoliageGnarled",
    "FoliagePine",
    "GrayboxFloor",
    "GrayboxProp",
    "GrayboxVault",
    "Moon",
    "Steel",
    "String",
    "Sun",
    "Target",
    "Wood",
];

pub fn valid_asset_key(key: &str) -> bool {
    let Some((category, rest)) = key.split_once('_') else {
        return false;
    };
    ASSET_CATEGORIES.contains(&category)
        && !rest.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        && !key.contains("__")
        && !key.ends_with('_')
}

pub fn valid_pascal_token(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(first) if first.is_ascii_uppercase())
        && bytes.all(|byte| byte.is_ascii_alphanumeric())
}

pub fn render_lod(name: &str) -> Option<(&str, u8)> {
    let rest = name
        .strip_prefix("SM_")
        .or_else(|| name.strip_prefix("SK_"))?;
    let (part, lod) = rest.rsplit_once("_LOD")?;
    if !valid_pascal_token(part) {
        return None;
    }
    let level = lod.parse::<u8>().ok()?;
    (level <= 2).then_some((part, level))
}

pub fn is_collision_name(name: &str) -> bool {
    ["UCX_", "UBX_", "UCP_", "USP_", "UCY_"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

pub fn valid_socket_name(name: &str) -> bool {
    name.strip_prefix("SKT_").is_some_and(valid_pascal_token)
}

#[allow(dead_code)] // The build script validates clips; runtime stores no animation names.
pub fn valid_animation_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("AN_") else {
        return false;
    };
    let mut parts = rest.split('_');
    parts.next().is_some_and(valid_pascal_token)
        && parts.all(|part| {
            valid_pascal_token(part)
                || (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_keys_have_a_known_category_and_canonical_case() {
        assert!(valid_asset_key("tree_pine_a"));
        assert!(valid_asset_key("char_ranger_female"));
        assert!(!valid_asset_key("ranger_female"));
        assert!(!valid_asset_key("tree_Pine"));
        assert!(!valid_asset_key("tree__pine"));
    }

    #[test]
    fn render_names_encode_a_contiguous_lod_candidate() {
        assert_eq!(render_lod("SM_Trunk_LOD0"), Some(("Trunk", 0)));
        assert_eq!(render_lod("SK_Body_LOD2"), Some(("Body", 2)));
        assert_eq!(render_lod("SM_bad_part_LOD0"), None);
        assert_eq!(render_lod("SM_Trunk_LOD3"), None);
    }

    #[test]
    fn semantic_helpers_are_unambiguous() {
        assert!(is_collision_name("UCY_Trunk"));
        assert!(valid_socket_name("SKT_MainHand"));
        assert!(valid_animation_name("AN_AttackLight_01"));
        assert!(!valid_animation_name("Idle"));
    }
}
