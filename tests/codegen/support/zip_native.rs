//! Purpose:
//! Locates this machine's managed native `libzip` artifact and its bzip2/LZMA/zlib/OpenSSL
//! dependency chain so a codegen fixture that links the `elephc_zip` bridge can resolve
//! libzip's symbols, and reports whether they are installed at all.
//!
//! Called from:
//! - `super::runner::test_link_plan` when a fixture's link plan names `elephc_zip`.
//! - `tests/codegen/zip.rs` and `zip_php_src_cases.rs` through [`skip_without_zip_native`].
//!
//! Key details:
//! - THIS IS A TEST PROVIDER, NOT A FALLBACK, exactly like `super::xml_native`. It only
//!   reads the durable managed-native cache the production resolver writes
//!   (`ELEPHC_NATIVE_CACHE`, else `$XDG_CACHE_HOME/elephc/native`, else
//!   `$HOME/.cache/elephc/native`). A system/Homebrew libzip is never consulted.
//! - The production compile path resolves the archives through the full lock/receipt
//!   machinery. The codegen harness has no project manifest, so it discovers the artifact
//!   directories structurally; a mismatch can only fail a test link.
//! - When the packages are absent the fixtures SKIP rather than fail by default, and the skip
//!   prints so it cannot pass unnoticed. CI sets `ELEPHC_TEST_REQUIRE_ZIP_NATIVE=1`, which
//!   turns that skip into a panic naming the `elephc native add libzip` recovery.

use std::path::PathBuf;
use std::sync::OnceLock;

use super::native_cache::{find_package_library_dir, native_cache_artifacts_root};
use super::target;

/// The environment variable that turns a missing artifact from a printed skip into a panic.
pub(crate) const REQUIRE_ENV: &str = "ELEPHC_TEST_REQUIRE_ZIP_NATIVE";

/// A stable token a CI log gate may grep for.
pub(crate) const SKIP_GATE_MARKER: &str = "ELEPHC_ZIP_NATIVE_SKIP_GATE";

/// The managed native packages a linked `elephc_zip` needs, in static link order.
///
/// `libzip.a` references bzip2, LZMA, zlib and libcrypto; `libssl.a` references libcrypto, so
/// the OpenSSL package comes last. Each entry names the exact archives its artifact must
/// hold, which is also the "is this really that package's lib/ directory" proof.
const ZIP_NATIVE_PACKAGES: &[(&str, &[&str])] = &[
    ("libzip", &["libzip.a"]),
    ("bzip2", &["libbz2.a"]),
    ("xz", &["liblzma.a"]),
    ("zlib", &["libz.a"]),
    ("openssl", &["libssl.a", "libcrypto.a"]),
];

/// One discovered package: its `lib/` directory plus exact archive paths in link order.
#[derive(Clone, Debug)]
pub(crate) struct ZipNativePackage {
    pub(crate) name: &'static str,
    pub(crate) library_dir: PathBuf,
    pub(crate) archives: Vec<PathBuf>,
}

/// Returns every managed package a zip fixture links, or `None` when any is missing.
pub(crate) fn zip_native_packages() -> Option<&'static [ZipNativePackage]> {
    static PACKAGES: OnceLock<Option<Vec<ZipNativePackage>>> = OnceLock::new();
    PACKAGES.get_or_init(discover_packages).as_deref()
}

/// Returns whether this machine can link a zip fixture at all.
pub(crate) fn available() -> bool {
    zip_native_packages().is_some()
}

/// Reports a skip for `test_name` and returns whether the caller should return early.
pub(crate) fn skip_without_zip_native(test_name: &str) -> bool {
    if available() {
        return false;
    }
    let target = target().as_str();
    if std::env::var_os(REQUIRE_ENV).is_some_and(|value| !value.is_empty()) {
        panic!(
            "{test_name}: {REQUIRE_ENV} is set but the managed native libzip chain is not \
             installed for {target} (run: elephc native add libzip --target {target})"
        );
    }
    eprintln!(
        "{SKIP_GATE_MARKER} skipping {test_name}: the managed native libzip chain is not \
         installed for {target} (run: elephc native add libzip --target {target})"
    );
    true
}

/// Discovers every package, returning `None` unless all of them are present.
fn discover_packages() -> Option<Vec<ZipNativePackage>> {
    let artifacts = native_cache_artifacts_root()?;
    let mut packages = Vec::new();
    for (package, archives) in ZIP_NATIVE_PACKAGES {
        let library_dir = find_package_library_dir(&artifacts, package, archives)?;
        packages.push(ZipNativePackage {
            name: package,
            archives: archives.iter().map(|archive| library_dir.join(archive)).collect(),
            library_dir,
        });
    }
    Some(packages)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates one fake artifact `lib/` below the harness's expected directory shape.
    fn fake_artifact(artifacts: &std::path::Path, package: &str, archives: &[&str]) -> PathBuf {
        let lib = artifacts
            .join(package)
            .join("1.0.0")
            .join("r1")
            .join("deadbeef")
            .join(target().as_str())
            .join("abi")
            .join("toolchain")
            .join("lib");
        std::fs::create_dir_all(&lib).expect("create fake artifact");
        for archive in archives {
            std::fs::write(lib.join(archive), b"!<arch>\n").expect("write fake archive");
        }
        lib
    }

    /// The package list is the static link order: libzip before its dependencies and libssl
    /// before libcrypto.
    #[test]
    fn native_package_order_is_the_static_link_order() {
        assert_eq!(
            ZIP_NATIVE_PACKAGES.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ["libzip", "bzip2", "xz", "zlib", "openssl"]
        );
        assert_eq!(ZIP_NATIVE_PACKAGES[4].1, ["libssl.a", "libcrypto.a"]);
    }

    /// Every package must contribute its exact archive, not a `-l` name.
    #[test]
    fn discovery_requires_every_archive() {
        let root = std::env::temp_dir().join(format!(
            "elephc_zip_native_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let artifacts = root.join("artifacts");
        fake_artifact(&artifacts, "libzip", &["libzip.a"]);
        assert!(find_package_library_dir(&artifacts, "libzip", &["libzip.a"]).is_some());
        assert!(find_package_library_dir(&artifacts, "openssl", &["libssl.a", "libcrypto.a"]).is_none());
        fake_artifact(&artifacts, "openssl", &["libssl.a"]);
        assert!(find_package_library_dir(&artifacts, "openssl", &["libssl.a", "libcrypto.a"]).is_none());
        fake_artifact(&artifacts, "openssl", &["libssl.a", "libcrypto.a"]);
        assert!(find_package_library_dir(&artifacts, "openssl", &["libssl.a", "libcrypto.a"]).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }
}
