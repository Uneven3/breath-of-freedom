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
    "Horse",
    "Ladder",
    "Moon",
    "PickupFood",
    "PickupMaterial",
    "PickupWeapon",
    "Probe",
    "Steel",
    "String",
    "Sun",
    "Target",
    "TargetPost",
    "TreeTrunk",
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

/// One clip in the player animation contract: the canonical name a
/// rig-compatible authored character ships for a locomotion capability.
/// `required` clips must exist — the build fails without them once an asset
/// opts in via `bof_animset = "player"`. Non-required clips are planned
/// motors/directions (swim/dive, the facing-locked directional axis) validated
/// only if present. Single source of truth shared by the build script and the
/// runtime resolver so the two can never drift.
#[allow(dead_code)]
pub struct ClipSpec {
    pub name: &'static str,
    pub required: bool,
}

#[allow(dead_code)]
pub const PLAYER_CLIP_CONTRACT: &[ClipSpec] = &[
    // Required: one per shipping locomotion motor (see src/movement/motors/).
    ClipSpec {
        name: "AN_Idle",
        required: true,
    },
    ClipSpec {
        name: "AN_Walk",
        required: true,
    },
    ClipSpec {
        name: "AN_Run",
        required: true,
    },
    ClipSpec {
        name: "AN_Sneak",
        required: true,
    },
    ClipSpec {
        name: "AN_Jump",
        required: true,
    },
    ClipSpec {
        name: "AN_Fall",
        required: true,
    },
    ClipSpec {
        name: "AN_Glide",
        required: true,
    },
    ClipSpec {
        name: "AN_Climb",
        required: true,
    },
    ClipSpec {
        name: "AN_Ladder",
        required: true,
    },
    ClipSpec {
        name: "AN_Mantle",
        required: true,
    },
    ClipSpec {
        name: "AN_Vault",
        required: true,
    },
    ClipSpec {
        name: "AN_WallJump",
        required: true,
    },
    ClipSpec {
        name: "AN_EdgeLeap",
        required: true,
    },
    // Planned (roadmap step 3): swim/dive motors and the facing-locked
    // directional axis shared by aim and Zelda-style lock-on.
    ClipSpec {
        name: "AN_Swim",
        required: false,
    },
    ClipSpec {
        name: "AN_Dive",
        required: false,
    },
    ClipSpec {
        name: "AN_WalkBwd",
        required: false,
    },
    ClipSpec {
        name: "AN_WalkStrafeL",
        required: false,
    },
    ClipSpec {
        name: "AN_WalkStrafeR",
        required: false,
    },
    ClipSpec {
        name: "AN_RunBwd",
        required: false,
    },
    ClipSpec {
        name: "AN_RunStrafeL",
        required: false,
    },
    ClipSpec {
        name: "AN_RunStrafeR",
        required: false,
    },
    ClipSpec {
        name: "AN_SneakBwd",
        required: false,
    },
    ClipSpec {
        name: "AN_SneakStrafeL",
        required: false,
    },
    ClipSpec {
        name: "AN_SneakStrafeR",
        required: false,
    },
];

/// Required contract clips absent from `present`. The build script's hard
/// guardrail for a `bof_animset = "player"` character: a non-empty result fails
/// the build, naming exactly which `AN_<Rol>` clips are missing.
#[allow(dead_code)]
pub fn missing_required_player_clips(present: &[&str]) -> Vec<&'static str> {
    PLAYER_CLIP_CONTRACT
        .iter()
        .filter(|spec| spec.required && !present.contains(&spec.name))
        .map(|spec| spec.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_contract_flags_missing_required_clips() {
        let complete: Vec<&str> = PLAYER_CLIP_CONTRACT
            .iter()
            .filter(|spec| spec.required)
            .map(|spec| spec.name)
            .collect();
        assert!(missing_required_player_clips(&complete).is_empty());

        // Planned-only clips do not satisfy the required set.
        assert_eq!(
            missing_required_player_clips(&["AN_Swim", "AN_WalkBwd"]).len(),
            PLAYER_CLIP_CONTRACT.iter().filter(|s| s.required).count()
        );

        let mut short = complete.clone();
        short.retain(|name| *name != "AN_Glide");
        assert_eq!(missing_required_player_clips(&short), vec!["AN_Glide"]);
    }

    #[test]
    fn every_contract_clip_is_a_well_formed_animation_name() {
        for spec in PLAYER_CLIP_CONTRACT {
            assert!(valid_animation_name(spec.name), "{}", spec.name);
        }
    }

    #[test]
    fn asset_keys_have_a_known_category_and_canonical_case() {
        assert!(valid_asset_key("tree_pine_a"));
        assert!(valid_asset_key("char_mannequin"));
        assert!(!valid_asset_key("mannequin"));
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
