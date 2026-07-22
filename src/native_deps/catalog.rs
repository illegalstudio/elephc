//! Purpose:
//! Owns the trusted, compiled-in native package catalog and immutable recipes.
//!
//! Called from:
//! - Manifest validation, lock expansion, installation, and compilation resolution.
//!
//! Key details:
//! - Project files select only catalogued names and exact versions; they never supply executable data.

use crate::codegen_support::platform::Target;

use super::error::{NativeError, NativeErrorKind};

/// Verified upstream source metadata embedded in the compiler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceArchive {
    pub https_url: &'static str,
    pub sha256: &'static str,
    pub exact_size: u64,
    pub body_limit: u64,
}

/// One immutable version and recipe in the trusted catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackageVersion {
    pub version: &'static str,
    pub source: SourceArchive,
    pub recipe_revision: u32,
    pub dependencies: &'static [&'static str],
    pub supported_targets: &'static [&'static str],
    pub ordered_link_outputs: &'static [&'static str],
    pub retained_headers: &'static [&'static str],
    pub provides: &'static [&'static str],
}

/// A named package and its default exact version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackageSpec {
    pub name: &'static str,
    pub default_version: &'static str,
    pub versions: &'static [PackageVersion],
}

const TARGETS: &[&str] = &["macos-aarch64", "linux-aarch64", "linux-x86_64"];
const PCRE2_ARCHIVES: &[&str] = &[
    "lib/libelephc_pcre2_shim.a",
    "lib/libpcre2-posix.a",
    "lib/libpcre2-8.a",
];
const PCRE2_HEADERS: &[&str] = &["include/pcre2.h", "include/pcre2posix.h"];
const PCRE2_VERSIONS: &[PackageVersion] = &[PackageVersion {
    version: "10.47",
    source: SourceArchive {
        https_url: "https://github.com/PCRE2Project/pcre2/releases/download/pcre2-10.47/pcre2-10.47.tar.gz",
        sha256: "c08ae2388ef333e8403e670ad70c0a11f1eed021fd88308d7e02f596fcd9dc16",
        exact_size: 2_792_969,
        body_limit: 32 * 1024 * 1024,
    },
    recipe_revision: 1,
    dependencies: &[],
    supported_targets: TARGETS,
    ordered_link_outputs: PCRE2_ARCHIVES,
    retained_headers: PCRE2_HEADERS,
    provides: &["pcre2"],
}];
const PACKAGES: &[PackageSpec] = &[PackageSpec {
    name: "pcre2",
    default_version: "10.47",
    versions: PCRE2_VERSIONS,
}];

/// Returns every package in deterministic catalog order.
pub fn packages() -> &'static [PackageSpec] {
    PACKAGES
}

/// Looks up a package and reports the complete known-name set on failure.
pub fn package(name: &str) -> Result<&'static PackageSpec, NativeError> {
    PACKAGES.iter().find(|package| package.name == name).ok_or_else(|| {
        NativeError::new(
            NativeErrorKind::Catalog,
            format!("unknown native package '{name}'; known packages: {}", known_names()),
        )
    })
}

/// Resolves an exact catalog version, using the package default when omitted.
pub fn version(name: &str, requested: Option<&str>) -> Result<&'static PackageVersion, NativeError> {
    let package = package(name)?;
    let selected = requested.unwrap_or(package.default_version);
    package.versions.iter().find(|version| version.version == selected).ok_or_else(|| {
        NativeError::new(
            NativeErrorKind::Catalog,
            format!("native package '{name}' has no catalogued exact version '{selected}'"),
        )
    })
}

/// Validates that a package recipe supports the selected compiler backend target.
pub fn ensure_target(version: &PackageVersion, target: Target) -> Result<(), NativeError> {
    if !target.supports_current_backend()
        || !version.supported_targets.iter().any(|candidate| *candidate == target.as_str())
    {
        return Err(NativeError::new(
            NativeErrorKind::Catalog,
            format!("native package does not support target '{}'", target.as_str()),
        ));
    }
    Ok(())
}

/// Returns catalog package names as a stable comma-separated diagnostic list.
pub fn known_names() -> String {
    PACKAGES.iter().map(|package| package.name).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the official PCRE2 source identity and immutable archive order.
    #[test]
    fn pcre2_catalog_snapshot_is_exact() {
        let version = version("pcre2", None).expect("catalogue entry");
        assert_eq!(version.version, "10.47");
        assert_eq!(version.source.exact_size, 2_792_969);
        assert_eq!(version.source.sha256, "c08ae2388ef333e8403e670ad70c0a11f1eed021fd88308d7e02f596fcd9dc16");
        assert_eq!(version.ordered_link_outputs, PCRE2_ARCHIVES);
        assert_eq!(version.supported_targets, TARGETS);
    }

    /// Verifies unknown package and version inputs fail closed.
    #[test]
    fn catalog_rejects_unknown_selection() {
        assert!(package("curl").unwrap_err().to_string().contains("known packages: pcre2"));
        assert!(version("pcre2", Some("10.46")).is_err());
    }
}
