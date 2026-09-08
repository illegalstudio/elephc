//! Purpose:
//! Defines the selected PHP compatibility version for version-sensitive compiler surfaces
//! and for the `since` field of every shared symbol contract.
//! Keeps parsing, ordering, and numeric `PHP_VERSION_ID` conversion in one typed model.
//!
//! Called from:
//! - `elephc::cli::compile_config()` when normalizing `--php-version` (through the
//!   `elephc::php_version` re-export).
//! - Version-sensitive standard-library preludes such as `elephc::pdo_prelude`.
//! - Shared function, class, and constant contracts (`since`), and the documentation
//!   generator that compares them with the pinned PHP baseline.
//!
//! Key details:
//! - PHP 8.5 is the default maintained profile, matching the current compiler baseline.
//! - Ordering is semantic because every supported value has the same major version.
//! - This type lives in the dependency-neutral contract crate so Magician and the
//!   compiler share one definition; the compiler re-exports it unchanged.

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

    /// Maintained stable profiles used for automatic project-profile selection.
    ///
    /// Historical 8.0/8.1 and preview 8.6 remain explicitly selectable, but an
    /// unpinned project is never moved to either end of that compatibility range.
    pub const MAINTAINED: [Self; 4] = [
        Self::Php82,
        Self::Php83,
        Self::Php84,
        Self::Php85,
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

    /// Returns the canonical `major.minor` spelling accepted by the CLI.
    pub const fn spelling(self) -> &'static str {
        self.as_str()
    }

    /// Parses an accepted `major.minor` spelling without emitting a diagnostic.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|profile| profile.spelling() == value)
    }

    /// Returns the comma-separated values accepted by `--php-version`.
    pub fn accepted_values() -> String {
        Self::ALL
            .iter()
            .map(|version| version.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Returns PHP's numeric `PHP_VERSION_ID` representation for this profile.
    pub const fn version_id(self) -> u32 {
        match self {
            Self::Php80 => 80000,
            Self::Php81 => 80100,
            Self::Php82 => 80200,
            Self::Php83 => 80300,
            Self::Php84 => 80400,
            Self::Php85 => 80500,
            Self::Php86 => 80600,
        }
    }

    /// Returns the profile's `PHP_VERSION` and `phpversion()` value.
    pub const fn version_string(self) -> &'static str {
        match self {
            Self::Php80 => "8.0.0",
            Self::Php81 => "8.1.0",
            Self::Php82 => "8.2.0",
            Self::Php83 => "8.3.0",
            Self::Php84 => "8.4.0",
            Self::Php85 => "8.5.0",
            Self::Php86 => "8.6.0",
        }
    }

    /// Returns `PHP_MAJOR_VERSION` for this profile.
    pub const fn major(self) -> u32 {
        self.version_id() / 10_000
    }

    /// Returns `PHP_MINOR_VERSION` for this profile.
    pub const fn minor(self) -> u32 {
        (self.version_id() / 100) % 100
    }

    /// Returns `PHP_RELEASE_VERSION`, pinned to zero for language profiles.
    pub const fn release(self) -> u32 {
        self.version_id() % 100
    }

    /// Returns the empty prerelease suffix used by stable language profiles.
    pub const fn extra_version(self) -> &'static str {
        ""
    }

    /// Returns E_ALL, excluding E_STRICT from PHP 8.4 onward.
    pub const fn all_error_levels(self) -> i64 {
        if self.version_id() >= 80400 { 30719 } else { 32767 }
    }

    /// Returns the matching Zend Engine language-profile version.
    pub const fn zend_version(self) -> &'static str {
        match self {
            Self::Php80 => "4.0.0",
            Self::Php81 => "4.1.0",
            Self::Php82 => "4.2.0",
            Self::Php83 => "4.3.0",
            Self::Php84 => "4.4.0",
            Self::Php85 => "4.5.0",
            Self::Php86 => "4.6.0",
        }
    }
}

impl Default for PhpVersion {
    /// Selects the newest maintained PHP compatibility profile by default.
    fn default() -> Self {
        Self::Php85
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
        assert!("8.5.10-dev".parse::<PhpVersion>().is_err());
        assert!("8.6-dev".parse::<PhpVersion>().is_err());
    }

    /// Minor profiles report no upstream patch or prerelease version.
    #[test]
    fn profiles_do_not_report_development_versions() {
        for version in PhpVersion::ALL {
            assert_eq!(version.release(), 0);
            assert_eq!(version.extra_version(), "");
            assert!(!version.version_string().contains('-'));
            assert!(!version.zend_version().contains('-'));
        }
    }

    /// E_ALL changes at the PHP 8.4 minor-version boundary.
    #[test]
    fn all_error_levels_follow_the_minor_profile() {
        assert_eq!(PhpVersion::Php83.all_error_levels(), 32767);
        assert_eq!(PhpVersion::Php84.all_error_levels(), 30719);
        assert_eq!(PhpVersion::Php85.all_error_levels(), 30719);
        assert_eq!(PhpVersion::Php86.all_error_levels(), 30719);
    }

    /// Verifies the enum's derived ordering follows semantic PHP version order.
    #[test]
    fn versions_have_semantic_order() {
        assert!(PhpVersion::Php84 < PhpVersion::Php85);
    }
}
