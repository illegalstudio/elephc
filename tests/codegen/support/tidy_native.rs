//! Purpose:
//! Locates this machine's managed native `libtidy` artifact so a codegen fixture that
//! links the `elephc_tidy` bridge can resolve libtidy's symbols, and reports whether it
//! is installed at all.
//!
//! Called from:
//! - `super::runner::test_link_plan` when a fixture's link plan names `elephc_tidy`.
//! - `tests/codegen/tidy*` through `skip_without_tidy_native`.
//!
//! Key details:
//! - THIS IS A TEST PROVIDER, NOT A FALLBACK, exactly like `super::xml_native`. It only
//!   reads the durable managed-native cache the production resolver writes
//!   (`ELEPHC_NATIVE_CACHE`, else `$XDG_CACHE_HOME/elephc/native`, else
//!   `$HOME/.cache/elephc/native`). A system or Homebrew libtidy is never consulted.
//! - When the package is absent the fixtures SKIP rather than fail by default, and the
//!   skip prints so it cannot pass unnoticed. CI sets
//!   `ELEPHC_TEST_REQUIRE_TIDY_NATIVE=1`, which turns that skip into a panic naming the
//!   `elephc native add libtidy` recovery.

use std::path::PathBuf;
use std::sync::OnceLock;

use super::native_cache::{find_package_library_dir, native_cache_artifacts_root};
use super::target;

/// The catalog package a linked `elephc_tidy` needs.
pub(crate) const TIDY_NATIVE_PACKAGE: &str = "libtidy";

/// The archive the `libtidy` artifact's `lib/` must hold.
pub(crate) const TIDY_NATIVE_ARCHIVES: &[&str] = &["libtidy.a"];

/// The environment variable that turns a missing artifact from a printed skip into a
/// panic. Set by every CI job that runs the tidy fixtures after materializing the package.
pub(crate) const REQUIRE_ENV: &str = "ELEPHC_TEST_REQUIRE_TIDY_NATIVE";

/// A stable token a log gate may grep for.
pub(crate) const SKIP_GATE_MARKER: &str = "ELEPHC_TIDY_NATIVE_SKIP_GATE";

/// One discovered artifact: its `lib/` directory plus the exact archive paths it provides.
#[derive(Clone, Debug)]
pub(crate) struct TidyNativePackage {
    /// Directory holding the managed archives.
    pub(crate) library_dir: PathBuf,
    /// Exact archive paths, in link order.
    pub(crate) archives: Vec<PathBuf>,
}

/// Returns the managed native `libtidy` artifact a tidy fixture links, or `None`.
pub(crate) fn tidy_native_package() -> Option<&'static TidyNativePackage> {
    static PACKAGE: OnceLock<Option<TidyNativePackage>> = OnceLock::new();
    PACKAGE.get_or_init(discover_package).as_ref()
}

/// Returns the exact archive paths a linked `elephc_tidy` needs, or `None`.
pub(crate) fn tidy_native_archives() -> Option<&'static [PathBuf]> {
    tidy_native_package().map(|package| package.archives.as_slice())
}

/// Returns whether this machine can link a tidy fixture at all.
pub(crate) fn available() -> bool {
    tidy_native_package().is_some()
}

/// Reports a skip for `test_name` and returns whether the caller should return early.
pub(crate) fn skip_without_tidy_native(test_name: &str) -> bool {
    if available() {
        return false;
    }
    let target = target().as_str();
    if std::env::var_os(REQUIRE_ENV).is_some_and(|value| !value.is_empty()) {
        panic!(
            "{test_name}: {REQUIRE_ENV} is set but managed native {TIDY_NATIVE_PACKAGE} is not \
             installed for {target} (run: elephc native add {TIDY_NATIVE_PACKAGE} --target {target})"
        );
    }
    eprintln!(
        "{SKIP_GATE_MARKER} skipping {test_name}: managed native {TIDY_NATIVE_PACKAGE} is not \
         installed for {target} (run: elephc native add {TIDY_NATIVE_PACKAGE} --target {target})"
    );
    true
}

/// Discovers the artifact, returning `None` unless its archive is present.
fn discover_package() -> Option<TidyNativePackage> {
    let artifacts = native_cache_artifacts_root()?;
    let library_dir =
        find_package_library_dir(&artifacts, TIDY_NATIVE_PACKAGE, TIDY_NATIVE_ARCHIVES)?;
    Some(TidyNativePackage {
        archives: TIDY_NATIVE_ARCHIVES
            .iter()
            .map(|archive| library_dir.join(archive))
            .collect(),
        library_dir,
    })
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Discovery guards for the tidy native provider.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Creates fake artifacts under a scratch directory; no real libtidy is needed.

    use super::*;
    use std::path::Path;

    /// Creates an empty scratch directory unique across parallel test threads.
    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "elephc_tidy_native_{label}_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// Writes one fake artifact `lib/` under `artifacts` with the named archives.
    fn fake_artifact(artifacts: &Path, version: &str, revision: &str, archives: &[&str]) -> PathBuf {
        let lib = artifacts
            .join(TIDY_NATIVE_PACKAGE)
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

    /// The catalog archive is required for discovery.
    #[test]
    fn discovery_requires_the_catalog_archive() {
        let root = scratch("archive");
        let artifacts = root.join("artifacts");
        assert!(
            find_package_library_dir(&artifacts, TIDY_NATIVE_PACKAGE, TIDY_NATIVE_ARCHIVES)
                .is_none()
        );
        let complete = fake_artifact(&artifacts, "5.8.0", "r1", TIDY_NATIVE_ARCHIVES);
        assert_eq!(
            find_package_library_dir(&artifacts, TIDY_NATIVE_PACKAGE, TIDY_NATIVE_ARCHIVES),
            Some(complete)
        );
    }
}
