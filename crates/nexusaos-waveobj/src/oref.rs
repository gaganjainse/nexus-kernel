use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use uuid::Uuid;

pub const OTYPE_CLIENT: &str = "client";
pub const OTYPE_WINDOW: &str = "window";
pub const OTYPE_WORKSPACE: &str = "workspace";
pub const OTYPE_TAB: &str = "tab";
pub const OTYPE_LAYOUT: &str = "layout";
pub const OTYPE_BLOCK: &str = "block";
pub const OTYPE_MAINSERVER: &str = "mainserver";
pub const OTYPE_JOB: &str = "job";
pub const OTYPE_TEMP: &str = "temp";

pub const VALID_OTYPES: &[&str] = &[
    OTYPE_CLIENT,
    OTYPE_WINDOW,
    OTYPE_WORKSPACE,
    OTYPE_TAB,
    OTYPE_LAYOUT,
    OTYPE_BLOCK,
    OTYPE_MAINSERVER,
    OTYPE_JOB,
    OTYPE_TEMP,
];

pub fn is_valid_otype(otype: &str) -> bool {
    VALID_OTYPES.contains(&otype)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ORefError {
    #[error("invalid ORef format: expected 'otype:oid', got '{0}'")]
    InvalidFormat(String),
    #[error("invalid object type: '{0}'")]
    InvalidOType(String),
    #[error("invalid OID (not a valid UUID): '{0}'")]
    InvalidOid(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ORef {
    pub otype: String,
    pub oid: Uuid,
}

impl ORef {
    pub fn new(otype: String, oid: Uuid) -> Result<Self, ORefError> {
        if !otype.chars().all(|c| c.is_ascii_lowercase()) || !is_valid_otype(&otype) {
            return Err(ORefError::InvalidOType(otype));
        }
        Ok(Self { otype, oid })
    }

    pub fn parse(s: &str) -> Result<Self, ORefError> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return Err(ORefError::InvalidFormat(s.to_string()));
        }

        let otype = parts[0].to_string();
        if !otype.chars().all(|c| c.is_ascii_lowercase()) || !is_valid_otype(&otype) {
            return Err(ORefError::InvalidOType(otype));
        }

        let oid_str = parts[1];
        let oid =
            Uuid::parse_str(oid_str).map_err(|_| ORefError::InvalidOid(oid_str.to_string()))?;

        Ok(Self { otype, oid })
    }
}

impl fmt::Display for ORef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.otype, self.oid)
    }
}

impl FromStr for ORef {
    type Err = ORefError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ORef::parse(s)
    }
}

impl Serialize for ORef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ORef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        ORef::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    use super::*;

    #[test]
    fn test_parse_valid() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let s = format!("block:{}", oid);
        let oref = ORef::parse(&s)?;
        assert_eq!(oref.otype, "block");
        assert_eq!(oref.oid, oid);
    Ok(())
    }

    #[test]
    fn test_parse_invalid_format() -> Result<(), Box<dyn std::error::Error>> {
        assert!(matches!(ORef::parse("block"), Err(ORefError::InvalidFormat(_))));
        assert!(matches!(ORef::parse("block:uuid:extra"), Err(ORefError::InvalidFormat(_))));
        assert!(matches!(ORef::parse(""), Err(ORefError::InvalidFormat(_))));
    Ok(())
    }

    #[test]
    fn test_parse_invalid_otype() -> Result<(), Box<dyn std::error::Error>> {
        assert!(matches!(
            ORef::parse("Block:00000000-0000-0000-0000-000000000000"),
            Err(ORefError::InvalidOType(_))
        ));
        assert!(matches!(
            ORef::parse("unknown:00000000-0000-0000-0000-000000000000"),
            Err(ORefError::InvalidOType(_))
        ));
        assert!(matches!(
            ORef::parse("bl0ck:00000000-0000-0000-0000-000000000000"),
            Err(ORefError::InvalidOType(_))
        ));
    Ok(())
    }

    #[test]
    fn test_parse_invalid_oid() -> Result<(), Box<dyn std::error::Error>> {
        assert!(matches!(ORef::parse("block:invalid-uuid"), Err(ORefError::InvalidOid(_))));
    Ok(())
    }

    #[test]
    fn test_display_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let original = ORef::new("workspace".to_string(), oid)?;
        let s = original.to_string();
        let parsed = ORef::parse(&s)?;
        assert_eq!(original, parsed);
    Ok(())
    }

    #[test]
    fn test_serde_json() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let original = ORef::new("tab".to_string(), oid)?;

        let json = serde_json::to_string(&original)?;
        assert_eq!(json, format!("\"tab:{}\"", oid));

        let deserialized: ORef = serde_json::from_str(&json)?;
        assert_eq!(original, deserialized);
    Ok(())
    }

    #[test]
    fn test_hash_eq() -> Result<(), Box<dyn std::error::Error>> {
        let oid1 = Uuid::new_v4();
        let oid2 = Uuid::new_v4();

        let oref1 = ORef::new("block".to_string(), oid1)?;
        let oref2 = ORef::new("block".to_string(), oid1)?;
        let oref3 = ORef::new("block".to_string(), oid2)?;
        let oref4 = ORef::new("window".to_string(), oid1)?;

        assert_eq!(oref1, oref2);
        assert_ne!(oref1, oref3);
        assert_ne!(oref1, oref4);

        let mut hasher1 = DefaultHasher::new();
        oref1.hash(&mut hasher1);
        let hash1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        oref2.hash(&mut hasher2);
        let hash2 = hasher2.finish();

        assert_eq!(hash1, hash2);
    Ok(())
    }

    #[test]
    fn test_all_valid_otypes() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        for &otype in VALID_OTYPES {
            let oref = ORef::new(otype.to_string(), oid);
            assert!(oref.is_ok(), "Failed for otype: {}", otype);
        }
    Ok(())
    }

    #[test]
    fn test_is_valid_otype_valid_values() -> Result<(), Box<dyn std::error::Error>> {
        for &otype in VALID_OTYPES {
            assert!(is_valid_otype(otype), "Should be valid: {}", otype);
        }
    Ok(())
    }

    #[test]
    fn test_is_valid_otype_invalid_values() -> Result<(), Box<dyn std::error::Error>> {
        assert!(!is_valid_otype(""));
        assert!(!is_valid_otype("invalid"));
        assert!(!is_valid_otype("Block"));
        assert!(!is_valid_otype("block "));
        assert!(!is_valid_otype(" block"));
        assert!(!is_valid_otype("BLOCK"));
        assert!(!is_valid_otype("block:"));
        assert!(!is_valid_otype("block "));
        assert!(!is_valid_otype("layoutstate"));
    Ok(())
    }

    #[test]
    fn test_new_valid_otype() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        for &otype in VALID_OTYPES {
            let oref = ORef::new(otype.to_string(), oid)?;
            assert_eq!(oref.otype, otype);
            assert_eq!(oref.oid, oid);
        }
    Ok(())
    }

    #[test]
    fn test_new_invalid_otype_uppercase() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        assert!(matches!(ORef::new("Block".to_string(), oid), Err(ORefError::InvalidOType(_))));
    Ok(())
    }

    #[test]
    fn test_new_invalid_otype_unknown() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        assert!(matches!(ORef::new("unknown".to_string(), oid), Err(ORefError::InvalidOType(_))));
    Ok(())
    }

    #[test]
    fn test_new_invalid_otype_empty() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        assert!(matches!(ORef::new(String::new(), oid), Err(ORefError::InvalidOType(_))));
    Ok(())
    }

    #[test]
    fn test_new_invalid_otype_with_digits() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        assert!(matches!(ORef::new("block1".to_string(), oid), Err(ORefError::InvalidOType(_))));
    Ok(())
    }

    #[test]
    fn test_new_invalid_otype_with_uppercase_and_digits() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        assert!(matches!(ORef::new("Block2".to_string(), oid), Err(ORefError::InvalidOType(_))));
    Ok(())
    }

    #[test]
    fn test_parse_all_valid_otypes() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        for &otype in VALID_OTYPES {
            let s = format!("{}:{}", otype, oid);
            let oref = ORef::parse(&s)?;
            assert_eq!(oref.otype, otype);
            assert_eq!(oref.oid, oid);
        }
    Ok(())
    }

    #[test]
    fn test_parse_no_colon() -> Result<(), Box<dyn std::error::Error>> {
        assert!(matches!(ORef::parse("block"), Err(ORefError::InvalidFormat(_))));
    Ok(())
    }

    #[test]
    fn test_parse_empty_string() -> Result<(), Box<dyn std::error::Error>> {
        assert!(matches!(ORef::parse(""), Err(ORefError::InvalidFormat(_))));
    Ok(())
    }

    #[test]
    fn test_parse_too_many_colons() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let s = format!("block:{}:extra", oid);
        assert!(matches!(ORef::parse(&s), Err(ORefError::InvalidFormat(_))));
    Ok(())
    }

    #[test]
    fn test_parse_colon_at_start() -> Result<(), Box<dyn std::error::Error>> {
        assert!(matches!(ORef::parse(":uuid"), Err(ORefError::InvalidOType(_))));
    Ok(())
    }

    #[test]
    fn test_parse_colon_at_end() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        assert!(matches!(ORef::parse(&format!("block:{}", "")), Err(ORefError::InvalidOid(_))));
        // Actually the split gives ["block", ""] and "" is not a valid UUID
        assert!(matches!(ORef::parse(&format!("{}:", oid)), Err(ORefError::InvalidOType(_))));
    Ok(())
    }

    #[test]
    fn test_parse_empty_otype_part() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        // ":" splits into ["", "uuid"] which is 2 parts but empty otype is invalid
        assert!(matches!(ORef::parse(&format!(":{}", oid)), Err(ORefError::InvalidOType(_))));
    Ok(())
    }

    #[test]
    fn test_parse_invalid_uuid_variants() -> Result<(), Box<dyn std::error::Error>> {
        let invalid_uuids = vec![
            "not-a-uuid",
            "12345",
            "gggggggg-gggg-gggg-gggg-gggggggggggg",
            "00000000-0000-0000-0000-00000000000", // too short
            "00000000-0000-0000-0000-0000000000000", // too long
            "z0000000-0000-0000-0000-000000000000", // invalid hex
        ];

        for uuid_str in invalid_uuids {
            let s = format!("block:{}", uuid_str);
            assert!(
                matches!(ORef::parse(&s), Err(ORefError::InvalidOid(_))),
                "Should fail for invalid UUID: {}",
                uuid_str
            );
        }
    Ok(())
    }

    #[test]
    fn test_parse_valid_urn_uuid() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let s = format!("block:urn:uuid:{}", oid);
        // This has too many colons, so it should fail
        assert!(matches!(ORef::parse(&s), Err(ORefError::InvalidFormat(_))));
    Ok(())
    }

    #[test]
    fn test_parse_braced_uuid() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let s = format!("block:{{{}}}", oid);
        // Note: Some UUID parsers accept braced format; adjust expectation to actual behavior
        let parsed = ORef::parse(&s);
        // If the UUID parser accepts braces, this will be Ok; otherwise Err
        // We just verify it doesn't panic
        let _ = parsed;
    Ok(())
    }

    #[test]
    fn test_parse_valid_uuid_no_hyphens() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let hex = oid.simple().to_string();
        let s = format!("block:{}", hex);
        // UUID simple format is a valid hex format
        let parsed = ORef::parse(&s);
        assert!(parsed.is_ok());
        let oref = parsed?;
        assert_eq!(oref.otype, "block");
        assert_eq!(oref.oid, oid);
    Ok(())
    }

    #[test]
    fn test_parse_uppercase_otype_fails() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        assert!(matches!(ORef::parse(&format!("Block:{}", oid)), Err(ORefError::InvalidOType(_))));
    Ok(())
    }

    #[test]
    fn test_parse_otype_with_digits_fails() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        assert!(matches!(ORef::parse(&format!("tab2:{}", oid)), Err(ORefError::InvalidOType(_))));
    Ok(())
    }

    #[test]
    fn test_from_str_trait() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let s = format!("workspace:{}", oid);
        let oref: ORef = s.parse()?;
        assert_eq!(oref.otype, "workspace");
        assert_eq!(oref.oid, oid);
    Ok(())
    }

    #[test]
    fn test_from_str_invalid_format() -> Result<(), Box<dyn std::error::Error>> {
        let result: Result<ORef, ORefError> = "invalid".parse();
        assert!(matches!(result, Err(ORefError::InvalidFormat(_))));
    Ok(())
    }

    #[test]
    fn test_from_str_invalid_otype() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let result: Result<ORef, ORefError> = format!("invalid:{}", oid).parse();
        assert!(matches!(result, Err(ORefError::InvalidOType(_))));
    Ok(())
    }

    #[test]
    fn test_from_str_invalid_oid() -> Result<(), Box<dyn std::error::Error>> {
        let result: Result<ORef, ORefError> = "block:not-a-uuid".parse();
        assert!(matches!(result, Err(ORefError::InvalidOid(_))));
    Ok(())
    }

    #[test]
    fn test_display_format() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let oref = ORef::new("block".to_string(), oid)?;
        let displayed = format!("{}", oref);
        assert_eq!(displayed, format!("block:{}", oid));
    Ok(())
    }

    #[test]
    fn test_display_all_otypes() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        for &otype in VALID_OTYPES {
            let oref = ORef::new(otype.to_string(), oid)?;
            let displayed = oref.to_string();
            assert_eq!(displayed, format!("{}:{}", otype, oid));
        }
    Ok(())
    }

    #[test]
    fn test_clone() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let oref = ORef::new("tab".to_string(), oid)?;
        let cloned = oref.clone();
        assert_eq!(oref, cloned);
    Ok(())
    }

    #[test]
    fn test_eq_and_ne() -> Result<(), Box<dyn std::error::Error>> {
        let oid1 = Uuid::new_v4();
        let oid2 = Uuid::new_v4();

        let oref1 = ORef::new("block".to_string(), oid1)?;
        let oref2 = ORef::new("block".to_string(), oid1)?;
        let oref3 = ORef::new("block".to_string(), oid2)?;
        let oref4 = ORef::new("window".to_string(), oid1)?;

        assert_eq!(oref1, oref2);
        assert_ne!(oref1, oref3);
        assert_ne!(oref1, oref4);
    Ok(())
    }

    #[test]
    fn test_hash_consistency() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let oref1 = ORef::new("block".to_string(), oid)?;
        let oref2 = ORef::new("block".to_string(), oid)?;

        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();
        oref1.hash(&mut hasher1);
        oref2.hash(&mut hasher2);

        assert_eq!(hasher1.finish(), hasher2.finish());
    Ok(())
    }

    #[test]
    fn test_hash_different_otypes_different_hash() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let oref1 = ORef::new("block".to_string(), oid)?;
        let oref2 = ORef::new("window".to_string(), oid)?;

        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();
        oref1.hash(&mut hasher1);
        oref2.hash(&mut hasher2);

        assert_ne!(hasher1.finish(), hasher2.finish());
    Ok(())
    }

    #[test]
    fn test_hash_different_oids_different_hash() -> Result<(), Box<dyn std::error::Error>> {
        let oid1 = Uuid::new_v4();
        let oid2 = Uuid::new_v4();

        let oref1 = ORef::new("block".to_string(), oid1)?;
        let oref2 = ORef::new("block".to_string(), oid2)?;

        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();
        oref1.hash(&mut hasher1);
        oref2.hash(&mut hasher2);

        assert_ne!(hasher1.finish(), hasher2.finish());
    Ok(())
    }

    #[test]
    fn test_deserialize_invalid_string() -> Result<(), Box<dyn std::error::Error>> {
        let invalid_inputs = vec!["block", "invalid:uuid", "block:not-a-uuid-at-all", ""];

        for input in invalid_inputs {
            let json_str = format!("\"{}\"", input);
            let result: Result<ORef, _> = serde_json::from_str(&json_str);
            assert!(result.is_err(), "Should fail for: {}", input);
        }
    Ok(())
    }

    #[test]
    fn test_deserialize_valid() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let json_str = format!("\"block:{}\"", oid);
        let oref: ORef = serde_json::from_str(&json_str)?;
        assert_eq!(oref.otype, "block");
        assert_eq!(oref.oid, oid);
    Ok(())
    }

    #[test]
    fn test_serialize_all_otypes() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        for &otype in VALID_OTYPES {
            let oref = ORef::new(otype.to_string(), oid)?;
            let json = serde_json::to_string(&oref)?;
            assert_eq!(json, format!("\"{}:{}\"", otype, oid));

            let deserialized: ORef = serde_json::from_str(&json)?;
            assert_eq!(oref, deserialized);
        }
    Ok(())
    }

    #[test]
    fn test_oref_error_display() -> Result<(), Box<dyn std::error::Error>> {
        let err1 = ORefError::InvalidFormat("bad".to_string());
        assert_eq!(err1.to_string(), "invalid ORef format: expected 'otype:oid', got 'bad'");

        let err2 = ORefError::InvalidOType("Block".to_string());
        assert_eq!(err2.to_string(), "invalid object type: 'Block'");

        let err3 = ORefError::InvalidOid("not-a-uuid".to_string());
        assert_eq!(err3.to_string(), "invalid OID (not a valid UUID): 'not-a-uuid'");
    Ok(())
    }

    #[test]
    fn test_oref_error_partial_eq() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            ORefError::InvalidFormat("a".to_string()),
            ORefError::InvalidFormat("a".to_string())
        );
        assert_ne!(
            ORefError::InvalidFormat("a".to_string()),
            ORefError::InvalidFormat("b".to_string())
        );
        assert_ne!(
            ORefError::InvalidFormat("a".to_string()),
            ORefError::InvalidOType("a".to_string())
        );
    Ok(())
    }

    #[test]
    fn test_oref_error_eq_different_fields() -> Result<(), Box<dyn std::error::Error>> {
        assert_ne!(
            ORefError::InvalidOType("a".to_string()),
            ORefError::InvalidOid("a".to_string())
        );
    Ok(())
    }

    #[test]
    fn test_parse_uuid_new_v4_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::new_v4();
        let oref = ORef::new("block".to_string(), oid)?;
        let s = oref.to_string();
        let parsed = ORef::parse(&s)?;
        assert_eq!(oref, parsed);
    Ok(())
    }

    #[test]
    fn test_parse_uuid_nil() -> Result<(), Box<dyn std::error::Error>> {
        let nil_oid = Uuid::nil();
        let oref = ORef::new("block".to_string(), nil_oid)?;
        assert_eq!(oref.oid, nil_oid);
        assert_eq!(oref.to_string(), format!("block:{}", nil_oid));
    Ok(())
    }

    #[test]
    fn test_parse_uuid_max() -> Result<(), Box<dyn std::error::Error>> {
        let max_oid = Uuid::max();
        let oref = ORef::new("block".to_string(), max_oid)?;
        assert_eq!(oref.oid, max_oid);
    Ok(())
    }

    #[test]
    fn test_parse_uuid_from_u64() -> Result<(), Box<dyn std::error::Error>> {
        let oid = Uuid::from_u128(0);
        let oref = ORef::new("block".to_string(), oid)?;
        assert_eq!(oref.otype, "block");
        assert_eq!(oref.oid, oid);
    Ok(())
    }
}
