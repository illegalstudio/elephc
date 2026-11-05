//! Purpose:
//! Declares which managed native catalog packages each Elephc bridge links
//! against, in the order the final program link must see them.
//!
//! Called from:
//! - `crate::pipeline::backend`, which turns the planned bridges into
//!   `NativeRequirement`s for the project's managed native packages.
//! - `crate::cli::capability_report`, so `--print-capabilities` names the
//!   packages a project must `elephc native add` before `--with-<name>` links.
//!
//! Key details:
//! - This table is the only place a bridge's native package is written down.
//!   The release probe (`scripts/verify-release-artifact.sh`) reads it back
//!   through `--print-capabilities`, so a bridge that gains a package here is
//!   prepared by the packaged-artifact probe without a second list to update.
//! - Table order is link order. The resolver links packages in the order it
//!   receives them, and `curl` (whose catalog chain carries OpenSSL 3.5.8 for
//!   TLS) stays ahead of `openssl-crypto` (a separate libcrypto 3.6.4 build).
//! - None of these packages has a system-library fallback.

/// Bridge library names paired with the managed native packages they link against.
const BRIDGE_NATIVE_PACKAGES: &[(&str, &[&str])] = &[
    // libcurl itself; the catalog splices in its openssl/zlib dependency chain.
    ("elephc_curl", &["curl"]),
    // The parser is libxml2, reached through the Elephc-owned C shim that the
    // managed package builds and archives next to `libxml2.a`.
    ("elephc_xml", &["libxml2"]),
    // The parser is libtidy, supplied by the managed catalog package.
    ("elephc_tidy", &["libtidy"]),
    // The bridge is a thin adapter over libgmp.
    ("elephc_gmp", &["gmp"]),
    ("elephc_fileinfo", &["libmagic"]),
    // The cipher bridge declares libcrypto's provider/EVP symbols without
    // linking them; the package's zlib dependency supplies `libz.a` after it.
    ("elephc_openssl", &["openssl-crypto"]),
];

/// Returns the managed native packages a planned library set needs, in link order.
pub(super) fn native_packages_for_libraries(link_libraries: &[String]) -> Vec<&'static str> {
    let mut packages = Vec::new();
    for (bridge, bridge_packages) in BRIDGE_NATIVE_PACKAGES {
        if !link_libraries.iter().any(|library| library == bridge) {
            continue;
        }
        for package in *bridge_packages {
            if !packages.contains(package) {
                packages.push(*package);
            }
        }
    }
    packages
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every entry names a bridge the linker table knows, so a rename cannot orphan it.
    #[test]
    fn native_package_bridges_exist_in_the_bridge_table() {
        for (bridge, packages) in BRIDGE_NATIVE_PACKAGES {
            assert!(
                super::super::bridges::BRIDGES
                    .iter()
                    .any(|known| known.lib_name == *bridge),
                "{bridge} is not in BRIDGES"
            );
            assert!(!packages.is_empty(), "{bridge} lists no native package");
        }
    }

    /// Every listed package is a managed catalog package `elephc native add` accepts.
    #[test]
    fn native_packages_are_catalog_packages() {
        for (_, packages) in BRIDGE_NATIVE_PACKAGES {
            for package in *packages {
                assert!(
                    crate::native_deps::packages()
                        .iter()
                        .any(|known| known.name == *package),
                    "{package} is not in the native catalog"
                );
            }
        }
    }

    /// Packages follow table order, not planning order, and appear once.
    #[test]
    fn native_packages_keep_link_order_without_duplicates() {
        let planned = ["elephc_openssl", "elephc_tls", "elephc_curl", "elephc_curl"]
            .map(String::from);
        assert_eq!(
            native_packages_for_libraries(&planned),
            ["curl", "openssl-crypto"]
        );
        assert!(native_packages_for_libraries(&["elephc_tls".to_string()]).is_empty());
    }

    /// A bridge without a native package reports none; one with a package reports it.
    #[test]
    fn native_packages_for_one_bridge() {
        let one = |library: &str| native_packages_for_libraries(&[library.to_string()]);
        assert_eq!(one("elephc_gmp"), ["gmp"]);
        assert!(one("elephc_tls").is_empty());
        assert!(one("not_a_bridge").is_empty());
    }
}
