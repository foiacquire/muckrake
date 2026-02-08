use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtectionLevel {
    Editable,
    Protected,
    Immutable,
}

impl ProtectionLevel {
    const fn ordinal(self) -> u8 {
        match self {
            Self::Editable => 0,
            Self::Protected => 1,
            Self::Immutable => 2,
        }
    }
}

impl PartialOrd for ProtectionLevel {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ProtectionLevel {
    fn cmp(&self, other: &Self) -> Ordering {
        self.ordinal().cmp(&other.ordinal())
    }
}

impl fmt::Display for ProtectionLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Immutable => write!(f, "immutable"),
            Self::Protected => write!(f, "protected"),
            Self::Editable => write!(f, "editable"),
        }
    }
}

impl FromStr for ProtectionLevel {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "immutable" => Ok(Self::Immutable),
            "protected" => Ok(Self::Protected),
            "editable" => Ok(Self::Editable),
            other => Err(anyhow::anyhow!("unknown protection level: {other}")),
        }
    }
}

pub fn strictest(levels: &[ProtectionLevel]) -> ProtectionLevel {
    levels
        .iter()
        .copied()
        .max()
        .unwrap_or(ProtectionLevel::Editable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protection_level_roundtrip() {
        for level in &[
            ProtectionLevel::Immutable,
            ProtectionLevel::Protected,
            ProtectionLevel::Editable,
        ] {
            let s = level.to_string();
            let parsed: ProtectionLevel = s.parse().unwrap();
            assert_eq!(&parsed, level);
        }
    }

    #[test]
    fn protection_level_invalid() {
        assert!("bogus".parse::<ProtectionLevel>().is_err());
    }

    #[test]
    fn ord_ordering() {
        assert!(ProtectionLevel::Editable < ProtectionLevel::Protected);
        assert!(ProtectionLevel::Protected < ProtectionLevel::Immutable);
        assert!(ProtectionLevel::Editable < ProtectionLevel::Immutable);
    }

    #[test]
    fn strictest_immutable_wins() {
        let levels = [
            ProtectionLevel::Editable,
            ProtectionLevel::Immutable,
            ProtectionLevel::Protected,
        ];
        assert_eq!(strictest(&levels), ProtectionLevel::Immutable);
    }

    #[test]
    fn strictest_protected_wins() {
        let levels = [ProtectionLevel::Editable, ProtectionLevel::Protected];
        assert_eq!(strictest(&levels), ProtectionLevel::Protected);
    }

    #[test]
    fn strictest_empty_defaults_editable() {
        assert_eq!(strictest(&[]), ProtectionLevel::Editable);
    }

    #[test]
    fn strictest_single_element() {
        assert_eq!(
            strictest(&[ProtectionLevel::Immutable]),
            ProtectionLevel::Immutable
        );
    }
}
