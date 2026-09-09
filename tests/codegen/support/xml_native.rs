//! Purpose:
//! Locates this machine's managed native `libxml2` artifact — the Elephc-owned
//! `libelephc_libxml2_shim.a` plus `libxml2.a` — so a codegen fixture that links the
//! `elephc_xml` bridge can resolve libxml2's symbols, and reports whether it is installed
//! at all.
//!
//! Called from:
//! - `super::runner::test_link_plan` when a fixture's link plan names `elephc_xml`.
//! - `tests/codegen/xml/*` through `skip_without_xml_native`.
//!
//! Key details:
//! - THIS IS A TEST PROVIDER, NOT A FALLBACK, exactly like `super::curl_native`. It only
//!   ever reads the durable managed-native cache the production resolver writes
//!   (`ELEPHC_NATIVE_CACHE`, else `$XDG_CACHE_HOME/elephc/native`, else
//!   `$HOME/.cache/elephc/native` — the precedence `elephc::native_deps`'
//!   `CacheLayout::from_environment` implements). A system or Homebrew `libxml2.a` is
//!   never consulted: the bridge's C ABI is the shim's `elephc_libxml2_v1_*` surface, which
//!   only the catalog recipe builds, so a fixture either links the exact pinned 2.15.3
//!   build or does not run at all.
//! - The production compile path resolves the archives through the full
//!   lock/receipt/toolchain-fingerprint machinery (`elephc::native_deps::resolver`). The
//!   codegen harness links with a bare `ld` invocation and has no project manifest, so it
//!   discovers the artifact directory structurally instead:
//!   `artifacts/libxml2/<version>/r<recipe>/<source-sha>/<target>/<abi>/<toolchain>/lib`.
//!   Structural discovery is acceptable HERE because a mismatch can only make a test fail
//!   to link, never make a shipped binary link something unverified.
//! - When the package is absent the fixtures SKIP rather than fail by default, and the
//!   skip prints so it cannot pass unnoticed. CI sets `ELEPHC_TEST_REQUIRE_XML_NATIVE=1`,
//!   which turns that skip into a panic naming the `elephc native add libxml2` recovery:
//!   every shard job materializes the package first, so a missing artifact there is a
//!   provisioning bug, not a developer machine without a from-source build.

use std::path::{Path, PathBuf};
use super::native_cache::{find_package_library_dir, native_cache_artifacts_root};
use std::sync::OnceLock;

use super::target;

/// The catalog package a linked `elephc_xml` needs.
pub(crate) const XML_NATIVE_PACKAGE: &str = "libxml2";

/// The archives the `libxml2` artifact's `lib/` must hold, IN LINK ORDER, mirroring the
/// catalog's artifact outputs: the shim first (its `elephc_libxml2_v1_*` entry points are
/// what the bridge calls, and they reference `xml*` symbols), then libxml2 itself.
pub(crate) const XML_NATIVE_ARCHIVES: &[&str] = &["libelephc_libxml2_shim.a", "libxml2.a"];

/// The Apple system libraries a statically linked libxml2 needs: its encoding handlers
/// call iconv, which glibc provides from libc but every Apple SDK ships as a separate
/// `libiconv.tbd`. Kept in step with the bridge's `apple_libraries` in
/// `src/linker/bridges.rs`, which is what the production link uses; that table lives in
/// the binary crate and is not reachable from an integration test, so it is mirrored here
/// rather than imported (exactly like `curl_native::CURL_APPLE_FRAMEWORKS`).
pub(crate) const XML_APPLE_LIBRARIES: &[&str] = &["iconv"];

/// The environment variable that turns a missing artifact from a printed skip into a
/// panic. Set by every CI job that runs `tests/codegen/xml` after materializing the
/// package, so a provisioning regression fails loudly instead of quietly shrinking the
/// suite.
pub(crate) const REQUIRE_ENV: &str = "ELEPHC_TEST_REQUIRE_XML_NATIVE";

/// A STABLE TOKEN a log gate may grep for, made-up rather than a phrase of the message,
/// for the reason `curl_native::SKIP_GATE_MARKER` documents: prose changes, tokens do not.
pub(crate) const SKIP_GATE_MARKER: &str = "ELEPHC_XML_NATIVE_SKIP_GATE";

/// One discovered artifact: its `lib/` directory plus the EXACT ARCHIVE PATHS it provides,
/// in link order (shim, then libxml2).
///
/// Exact paths, not `-l` names, for the same reason `curl_native` and the production
/// planner (`src/link_planning.rs`) use `LinkItem::managed_archive`: a `-l` name competes
/// with every other named library in the plan and resolves through the `-L` search order,
/// so `-lxml2` could bind a system libxml2 — which does not even carry the shim — into a
/// fixture whose entire purpose is proving the pinned build.
#[derive(Clone, Debug)]
pub(crate) struct XmlNativePackage {
    pub(crate) library_dir: PathBuf,
    pub(crate) archives: Vec<PathBuf>,
}

/// Returns the managed native `libxml2` artifact an xml fixture links, or `None` when it
/// is missing from this machine's cache.
pub(crate) fn xml_native_package() -> Option<&'static XmlNativePackage> {
    static PACKAGE: OnceLock<Option<XmlNativePackage>> = OnceLock::new();
    PACKAGE.get_or_init(discover_package).as_ref()
}

/// Returns the exact archive paths a linked `elephc_xml` needs, in link order, or `None`
/// when the artifact is missing.
pub(crate) fn xml_native_archives() -> Option<&'static [PathBuf]> {
    xml_native_package().map(|package| package.archives.as_slice())
}

/// Returns whether this machine can link an xml fixture at all.
pub(crate) fn available() -> bool {
    xml_native_package().is_some()
}

/// Reports a skip for `test_name` and returns whether the caller should return early.
///
/// Returns `false` (run the fixture) when the artifact is present. Otherwise: with
/// [`REQUIRE_ENV`] set, PANICS with the `elephc native add libxml2` recovery — the CI
/// contract, where the package was materialized before the shard ran and its absence is
/// a bug; without it, prints a visible skip line and returns `true`, so a developer
/// machine without a from-source libxml2 build still gets a green, honest `cargo test`.
pub(crate) fn skip_without_xml_native(test_name: &str) -> bool {
    if available() {
        return false;
    }
    let target = target().as_str();
    if std::env::var_os(REQUIRE_ENV).is_some_and(|value| !value.is_empty()) {
        panic!(
            "{test_name}: {REQUIRE_ENV} is set but managed native {XML_NATIVE_PACKAGE} is not \
             installed for {target} (run: elephc native add {XML_NATIVE_PACKAGE} --target {target})"
        );
    }
    eprintln!(
        "{SKIP_GATE_MARKER} skipping {test_name}: managed native {XML_NATIVE_PACKAGE} is not \
         installed for {target} (run: elephc native add {XML_NATIVE_PACKAGE} --target {target})"
    );
    true
}

/// Discovers the artifact, returning `None` unless both archives are present.
fn discover_package() -> Option<XmlNativePackage> {
    let artifacts = native_cache_artifacts_root()?;
    let library_dir = find_package_library_dir(&artifacts, XML_NATIVE_PACKAGE, XML_NATIVE_ARCHIVES)?;
    Some(XmlNativePackage {
        archives: XML_NATIVE_ARCHIVES
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
            "elephc_xml_native_{label}_{}_{:?}",
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
            .join(XML_NATIVE_PACKAGE)
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

    /// Both archives are required: a `lib/` holding only `libxml2.a` (no shim) is not a
    /// candidate, because the bridge calls the shim's ABI, never libxml2's directly.
    #[test]
    fn discovery_requires_the_shim_and_the_library() {
        let root = scratch("both");
        let artifacts = root.join("artifacts");
        fake_artifact(&artifacts, "2.15.3", "r1", &["libxml2.a"]);
        assert!(find_package_library_dir(&artifacts, XML_NATIVE_PACKAGE, XML_NATIVE_ARCHIVES).is_none());

        let complete = fake_artifact(&artifacts, "2.15.3", "r2", XML_NATIVE_ARCHIVES);
        assert_eq!(
            find_package_library_dir(&artifacts, XML_NATIVE_PACKAGE, XML_NATIVE_ARCHIVES),
            Some(complete)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The newest `(version, revision)` wins, comparing versions numerically.
    #[test]
    fn discovery_prefers_the_newest_version_then_revision() {
        let root = scratch("newest");
        let artifacts = root.join("artifacts");
        fake_artifact(&artifacts, "2.9.14", "r3", XML_NATIVE_ARCHIVES);
        fake_artifact(&artifacts, "2.15.3", "r1", XML_NATIVE_ARCHIVES);
        let newest = fake_artifact(&artifacts, "2.15.3", "r2", XML_NATIVE_ARCHIVES);
        assert_eq!(
            find_package_library_dir(&artifacts, XML_NATIVE_PACKAGE, XML_NATIVE_ARCHIVES),
            Some(newest)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An artifact built for another target is never a candidate for this harness run.
    #[test]
    fn discovery_ignores_other_targets() {
        let root = scratch("target");
        let artifacts = root.join("artifacts");
        let lib = artifacts
            .join(XML_NATIVE_PACKAGE)
            .join("2.15.3/r1/deadbeef/not-this-target/abi/toolchain/lib");
        std::fs::create_dir_all(&lib).expect("create foreign artifact");
        for archive in XML_NATIVE_ARCHIVES {
            std::fs::write(lib.join(archive), b"!<arch>\n").expect("write fake archive");
        }
        assert!(find_package_library_dir(&artifacts, XML_NATIVE_PACKAGE, XML_NATIVE_ARCHIVES).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The archive order is the link order the catalog declares: shim, then libxml2.
    #[test]
    fn archive_order_is_shim_then_library() {
        assert_eq!(XML_NATIVE_ARCHIVES, &["libelephc_libxml2_shim.a", "libxml2.a"]);
        assert_eq!(XML_APPLE_LIBRARIES, &["iconv"]);
    }
}
