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

/// Targets both catalogued packages build for.
///
/// pcre2 and zlib are plain autoconf/make projects with no host assumptions
/// beyond a working C compiler, so the iOS entries carry no per-package caveat.
/// They do require an SDK-aware compiler: `resolve_toolchain` already refuses
/// any cross target without explicit `ELEPHC_NATIVE_CC`/`AR`/`RANLIB`, and for
/// iOS those must carry `-target` and `-isysroot` or configure will silently
/// probe the host instead.
const TARGETS: &[&str] = &[
    "macos-aarch64",
    "ios-arm64",
    "ios-sim-arm64",
    "linux-aarch64",
    "linux-x86_64",
];
const PCRE2_ARCHIVES: &[&str] = &[
    "lib/libelephc_pcre2_shim.a",
    "lib/libpcre2-posix.a",
    "lib/libpcre2-8.a",
];
const PCRE2_HEADERS: &[&str] = &["include/pcre2.h", "include/pcre2posix.h"];
const ZLIB_ARCHIVES: &[&str] = &["lib/libz.a"];
const ZLIB_HEADERS: &[&str] = &["include/zlib.h", "include/zconf.h"];
const PCRE2_VERSIONS: &[PackageVersion] = &[PackageVersion {
    version: "10.47",
    source: SourceArchive {
        https_url: "https://github.com/PCRE2Project/pcre2/releases/download/pcre2-10.47/pcre2-10.47.tar.gz",
        sha256: "c08ae2388ef333e8403e670ad70c0a11f1eed021fd88308d7e02f596fcd9dc16",
        exact_size: 2_792_969,
        body_limit: 32 * 1024 * 1024,
    },
    recipe_revision: 2,
    dependencies: &[],
    supported_targets: TARGETS,
    ordered_link_outputs: PCRE2_ARCHIVES,
    retained_headers: PCRE2_HEADERS,
    provides: &["pcre2"],
}];
const ZLIB_VERSIONS: &[PackageVersion] = &[PackageVersion {
    version: "1.3.2",
    source: SourceArchive {
        https_url:
            "https://github.com/madler/zlib/releases/download/v1.3.2/zlib-1.3.2.tar.gz",
        sha256: "bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16",
        exact_size: 1_502_830,
        body_limit: 16 * 1024 * 1024,
    },
    recipe_revision: 1,
    dependencies: &[],
    supported_targets: TARGETS,
    ordered_link_outputs: ZLIB_ARCHIVES,
    retained_headers: ZLIB_HEADERS,
    provides: &["zlib"],
}];
const PACKAGES: &[PackageSpec] = &[
    PackageSpec {
        name: "pcre2",
        default_version: "10.47",
        versions: PCRE2_VERSIONS,
    },
    PackageSpec {
        name: "zlib",
        default_version: "1.3.2",
        versions: ZLIB_VERSIONS,
    },
];

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

    /// Verifies the official zlib source identity and static archive contract.
    #[test]
    fn zlib_catalog_snapshot_is_exact() {
        let version = version("zlib", None).expect("catalogue entry");
        assert_eq!(version.version, "1.3.2");
        assert_eq!(version.source.exact_size, 1_502_830);
        assert_eq!(
            version.source.sha256,
            "bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16"
        );
        assert_eq!(version.ordered_link_outputs, ZLIB_ARCHIVES);
        assert_eq!(version.retained_headers, ZLIB_HEADERS);
        assert_eq!(version.supported_targets, TARGETS);
    }

    /// Verifies both catalogued packages accept the iOS targets, and that the
    /// device and simulator entries are separate — they resolve to different
    /// artifact directories, so one must not stand in for the other.
    #[test]
    fn catalog_accepts_ios_targets() {
        use crate::codegen_support::platform::{AppleVariant, Arch};

        for name in ["pcre2", "zlib"] {
            let entry = version(name, None).expect("catalogue entry");
            for target in [
                Target::new_apple(Arch::AArch64, AppleVariant::IOS),
                Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            ] {
                ensure_target(entry, target).unwrap_or_else(|error| {
                    panic!("{name} must support {}: {error}", target.as_str())
                });
            }
            assert!(
                entry.supported_targets.contains(&"ios-arm64")
                    && entry.supported_targets.contains(&"ios-sim-arm64"),
                "{name} must list both iOS targets distinctly"
            );
        }
    }

    /// Verifies unknown package and version inputs fail closed.
    #[test]
    fn catalog_rejects_unknown_selection() {
        assert!(package("curl")
            .unwrap_err()
            .to_string()
            .contains("known packages: pcre2, zlib"));
        assert!(version("pcre2", Some("10.46")).is_err());
    }
}
