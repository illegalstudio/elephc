//! Purpose:
//! Locates this machine's managed native `gmp` artifact (`libgmp.a`) so a codegen
//! fixture that links the `elephc_gmp` bridge can resolve libgmp's symbols.
//!
//! Called from:
//! - `super::runner::test_link_plan` when a fixture's link plan names `elephc_gmp`.
//! - `tests/codegen/gmp_phpt.rs` through [`skip_without_gmp_native`].
//!
//! Key details:
//! - This is a TEST PROVIDER, not a fallback: it only reads the durable managed-native
//!   cache the production resolver writes. A system or Homebrew `libgmp.a` is never
//!   consulted, so the PHPT corpus always links the pinned 6.3.0 build.
//! - Missing artifacts skip by default; `ELEPHC_TEST_REQUIRE_GMP_NATIVE=1` turns the
//!   skip into a panic naming the `elephc native add gmp` recovery, which is what CI
//!   sets after materializing the package.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::native_cache::{find_package_library_dir, native_cache_artifacts_root};
use super::target;

/// The catalog package a linked `elephc_gmp` needs.
pub(crate) const GMP_NATIVE_PACKAGE: &str = "gmp";

/// The single archive the artifact's `lib/` must hold.
pub(crate) const GMP_NATIVE_ARCHIVES: &[&str] = &["libgmp.a"];

/// Environment variable that turns a missing artifact into a panic.
pub(crate) const REQUIRE_ENV: &str = "ELEPHC_TEST_REQUIRE_GMP_NATIVE";

/// Stable token a log gate may grep for.
pub(crate) const SKIP_GATE_MARKER: &str = "ELEPHC_GMP_NATIVE_SKIP_GATE";

/// One discovered artifact and its exact archive path.
#[derive(Clone, Debug)]
pub(crate) struct GmpNativePackage {
    /// Directory holding `libgmp.a`.
    pub(crate) library_dir: PathBuf,
    /// Exact archive paths in link order.
    pub(crate) archives: Vec<PathBuf>,
}

/// Returns the managed native `gmp` artifact, or `None` when it is not installed.
pub(crate) fn gmp_native_package() -> Option<&'static GmpNativePackage> {
    static PACKAGE: OnceLock<Option<GmpNativePackage>> = OnceLock::new();
    PACKAGE.get_or_init(discover_package).as_ref()
}

/// Returns the exact archives a `gmp` fixture links.
pub(crate) fn gmp_native_archives() -> Option<&'static [PathBuf]> {
    gmp_native_package().map(|package| package.archives.as_slice())
}

/// Returns whether this machine can link a GMP fixture at all.
pub(crate) fn available() -> bool {
    gmp_native_package().is_some()
}

/// Reports a skip for `test_name` and returns whether the caller should return early.
pub(crate) fn skip_without_gmp_native(test_name: &str) -> bool {
    if available() {
        return false;
    }
    let target = target().as_str();
    if std::env::var_os(REQUIRE_ENV).is_some_and(|value| !value.is_empty()) {
        panic!(
            "{test_name}: {REQUIRE_ENV} is set but managed native {GMP_NATIVE_PACKAGE} is not \
             installed for {target} (run: elephc native add {GMP_NATIVE_PACKAGE} --target {target})"
        );
    }
    eprintln!(
        "{SKIP_GATE_MARKER} skipping {test_name}: managed native {GMP_NATIVE_PACKAGE} is not \
         installed for {target} (run: elephc native add {GMP_NATIVE_PACKAGE} --target {target})"
    );
    true
}

/// Discovers the artifact, returning `None` unless `libgmp.a` is present.
fn discover_package() -> Option<GmpNativePackage> {
    let artifacts = native_cache_artifacts_root()?;
    let library_dir = find_package_library_dir(&artifacts, GMP_NATIVE_PACKAGE, GMP_NATIVE_ARCHIVES)?;
    Some(GmpNativePackage {
        archives: GMP_NATIVE_ARCHIVES
            .iter()
            .map(|archive| library_dir.join(archive))
            .collect(),
        library_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates an empty scratch directory unique across parallel test threads.
    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "elephc_gmp_native_{label}_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// Writes one fake artifact `lib/` under `artifacts`.
    fn fake_artifact(artifacts: &Path, version: &str, revision: &str, archives: &[&str]) -> PathBuf {
        let lib = artifacts
            .join(GMP_NATIVE_PACKAGE)
            .join(version)
            .join(revision)
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

    /// The archive is the only required output.
    #[test]
    fn discovery_requires_libgmp() {
        let root = scratch("archive");
        let artifacts = root.join("artifacts");
        fake_artifact(&artifacts, "6.3.0", "r1", &["README"]);
        assert!(find_package_library_dir(&artifacts, GMP_NATIVE_PACKAGE, GMP_NATIVE_ARCHIVES).is_none());
        let complete = fake_artifact(&artifacts, "6.3.0", "r2", GMP_NATIVE_ARCHIVES);
        assert_eq!(
            find_package_library_dir(&artifacts, GMP_NATIVE_PACKAGE, GMP_NATIVE_ARCHIVES),
            Some(complete)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The newest `(version, revision)` wins.
    #[test]
    fn discovery_prefers_the_newest_artifact() {
        let root = scratch("newest");
        let artifacts = root.join("artifacts");
        fake_artifact(&artifacts, "6.2.1", "r3", GMP_NATIVE_ARCHIVES);
        fake_artifact(&artifacts, "6.3.0", "r1", GMP_NATIVE_ARCHIVES);
        let newest = fake_artifact(&artifacts, "6.3.0", "r2", GMP_NATIVE_ARCHIVES);
        assert_eq!(
            find_package_library_dir(&artifacts, GMP_NATIVE_PACKAGE, GMP_NATIVE_ARCHIVES),
            Some(newest)
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
