//! Purpose:
//! Defines the selected PHP compatibility version for version-sensitive compiler surfaces.
//! Keeps parsing, ordering, and numeric `PHP_VERSION_ID` conversion in one typed model.
//!
//! Called from:
//! - `crate::cli::parse_args()` when normalizing `--php-version`.
//! - Version-sensitive standard-library preludes such as `crate::pdo_prelude`.
//!
//! Key details:
//! - PHP 8.4 remains the default so existing compiler behavior is stable.
//! - Ordering is semantic because every supported value has the same major version.

use std::fmt;
use std::str::FromStr;

/// PHP compatibility versions accepted by the compiler.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PhpVersion {
    Php80,
    Php81,
    Php82,
    Php83,
    Php84,
    Php85,
    Php86,
}

impl PhpVersion {
    /// Every accepted version in ascending semantic order.
    pub const ALL: [Self; 7] = [
        Self::Php80,
        Self::Php81,
        Self::Php82,
        Self::Php83,
        Self::Php84,
        Self::Php85,
        Self::Php86,
    ];

    /// Returns the canonical CLI spelling for this compatibility version.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Php80 => "8.0",
            Self::Php81 => "8.1",
            Self::Php82 => "8.2",
            Self::Php83 => "8.3",
            Self::Php84 => "8.4",
            Self::Php85 => "8.5",
            Self::Php86 => "8.6",
        }
    }

    /// Returns the comma-separated values accepted by `--php-version`.
    pub fn accepted_values() -> String {
        Self::ALL
            .iter()
            .map(|version| version.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl Default for PhpVersion {
    /// Keeps the established elephc PDO compatibility baseline on PHP 8.4.
    fn default() -> Self {
        Self::Php84
    }
}

impl fmt::Display for PhpVersion {
    /// Formats the version in canonical `major.minor` form.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PhpVersion {
    type Err = String;

    /// Parses an exact supported `major.minor` spelling without accepting patch versions.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "8.0" => Ok(Self::Php80),
            "8.1" => Ok(Self::Php81),
            "8.2" => Ok(Self::Php82),
            "8.3" => Ok(Self::Php83),
            "8.4" => Ok(Self::Php84),
            "8.5" => Ok(Self::Php85),
            "8.6" => Ok(Self::Php86),
            other => Err(format!(
                "Invalid PHP version '{}': expected one of: {}",
                other,
                Self::accepted_values()
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every advertised version round-trips through its canonical CLI spelling.
    #[test]
    fn supported_versions_round_trip() {
        for version in PhpVersion::ALL {
            assert_eq!(version.as_str().parse::<PhpVersion>(), Ok(version));
        }
    }

    /// Verifies patch versions are rejected so compatibility selection is never ambiguous.
    #[test]
    fn patch_versions_are_rejected() {
        assert!("8.4.1".parse::<PhpVersion>().is_err());
    }

    /// Verifies the enum's derived ordering follows semantic PHP version order.
    #[test]
    fn versions_have_semantic_order() {
        assert!(PhpVersion::Php84 < PhpVersion::Php85);
    }
}
