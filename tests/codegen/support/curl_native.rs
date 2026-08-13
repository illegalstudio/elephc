//! Purpose:
//! Locates this machine's managed native `curl` / `openssl` / `zlib` archives so a
//! codegen fixture that links the `elephc_curl` bridge can resolve libcurl's symbols,
//! and reports whether they are installed at all.
//!
//! Called from:
//! - `super::runner::test_link_plan` when a fixture's link plan names `elephc_curl`.
//! - `tests/codegen/curl/*` through `skip_without_curl_native`.
//!
//! Key details:
//! - THIS IS A TEST PROVIDER, NOT A FALLBACK. It only ever reads the same durable
//!   managed-native cache the production resolver writes (`ELEPHC_NATIVE_CACHE`, else
//!   `$XDG_CACHE_HOME/elephc/native`, else `$HOME/.cache/elephc/native` — the precedence
//!   `elephc::native_deps`' `CacheLayout::from_environment` implements). A system or
//!   Homebrew `libcurl.a` is never consulted, so a fixture either links the exact pinned
//!   8.21.0 build or does not run at all — which is what makes
//!   `curl_version_reports_pinned_libcurl` meaningful.
//! - The production compile path resolves these archives through the full
//!   lock/receipt/toolchain-fingerprint machinery (`elephc::native_deps::resolver`). The
//!   codegen harness links with a bare `ld` invocation and has no project manifest, so it
//!   discovers the artifact directory structurally instead:
//!   `artifacts/<pkg>/<version>/r<recipe>/<source-sha>/<target>/<abi>/<toolchain>/lib`.
//!   Structural discovery is acceptable HERE because a mismatch can only make a test
//!   fail to link, never make a shipped binary link something unverified.
//! - When the packages are absent the fixtures SKIP rather than fail: `elephc native add
//!   curl` is a from-source build of libcurl+OpenSSL, and requiring it of every checkout
//!   (and of CI, which installs only pcre2/zlib today) would make an unrelated `cargo
//!   test` fail for a missing multi-minute native build. The skip prints so it cannot
//!   pass silently unnoticed.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::target;

/// The managed native packages a linked `elephc_curl` needs, paired with the archive
/// filename that proves the directory really is that package's `lib/`. libcurl links
/// against OpenSSL (TLS) and zlib (transfer encodings); both are separate catalog
/// packages with unrelated content-hashed paths, so each is discovered on its own.
const CURL_NATIVE_PACKAGES: &[(&str, &[&str])] = &[
    ("curl", &["libcurl.a"]),
    ("openssl", &["libssl.a", "libcrypto.a"]),
    ("zlib", &["libz.a"]),
];

/// The macOS system frameworks a statically linked libcurl needs for its native
/// resolver/proxy support (`Curl_macos_init` references `CFRelease` and
/// `SCDynamicStoreCopyProxies`). Kept in step with the `elephc_curl` entry's
/// `macos_frameworks` in `src/linker/bridges.rs`, which is what the production link uses;
/// that table lives in the binary crate and is not reachable from an integration test, so
/// it is mirrored here rather than imported.
pub(crate) const CURL_MACOS_FRAMEWORKS: &[&str] =
    &["Security", "CoreFoundation", "SystemConfiguration"];

/// One discovered package: the `lib/` directory plus the `-l` names it provides, in
/// libcurl's own dependency order (curl -> ssl -> crypto -> z).
#[derive(Clone, Debug)]
pub(crate) struct CurlNativePackage {
    pub(crate) library_dir: PathBuf,
    pub(crate) libraries: Vec<String>,
}

/// Returns every managed native package a curl fixture links, or `None` when any of them
/// is missing from this machine's cache.
pub(crate) fn curl_native_packages() -> Option<&'static [CurlNativePackage]> {
    static PACKAGES: OnceLock<Option<Vec<CurlNativePackage>>> = OnceLock::new();
    PACKAGES
        .get_or_init(discover_packages)
        .as_deref()
}

/// Returns whether this machine can link a curl fixture at all.
pub(crate) fn available() -> bool {
    curl_native_packages().is_some()
}

/// Reports a skip for `test_name` and returns whether the caller should return early.
///
/// Printing (rather than silently returning `true`) keeps a skipped curl suite visible in
/// `cargo test -- --nocapture` and in CI logs, so "all green" on a machine without the
/// packages cannot be mistaken for "curl is covered here".
pub(crate) fn skip_without_curl_native(test_name: &str) -> bool {
    if available() {
        return false;
    }
    eprintln!(
        "skipping {test_name}: managed native curl/openssl/zlib are not installed for {} \
         (run: elephc native add curl --target {})",
        target().as_str(),
        target().as_str()
    );
    true
}

/// Discovers all three packages, returning `None` unless every one of them is present.
fn discover_packages() -> Option<Vec<CurlNativePackage>> {
    let artifacts = native_cache_artifacts_root()?;
    let mut packages = Vec::new();
    for (package, archives) in CURL_NATIVE_PACKAGES {
        let library_dir = find_package_library_dir(&artifacts, package, archives)?;
        packages.push(CurlNativePackage {
            library_dir,
            libraries: archives
                .iter()
                .map(|archive| {
                    archive
                        .trim_start_matches("lib")
                        .trim_end_matches(".a")
                        .to_string()
                })
                .collect(),
        });
    }
    Some(packages)
}

/// Resolves the managed native cache's `artifacts/` root using the same environment
/// precedence as `elephc::native_deps`' `CacheLayout::from_environment`.
fn native_cache_artifacts_root() -> Option<PathBuf> {
    let root = if let Some(explicit) = std::env::var_os("ELEPHC_NATIVE_CACHE")
        .filter(|value| !value.is_empty())
    {
        PathBuf::from(explicit)
    } else if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME").filter(|value| !value.is_empty()) {
        PathBuf::from(xdg).join("elephc/native")
    } else {
        PathBuf::from(std::env::var_os("HOME").filter(|value| !value.is_empty())?)
            .join(".cache/elephc/native")
    };
    let artifacts = root.join("artifacts");
    artifacts.is_dir().then_some(artifacts)
}

/// Walks `artifacts/<package>/<version>/r<recipe>/<source-sha>/<target>/<abi>/<toolchain>`
/// and returns the first `lib/` directory that contains every expected archive for the
/// harness target.
///
/// The version/recipe/source/abi/toolchain components are content-addressed and vary per
/// machine, so they are enumerated rather than reconstructed; the TARGET component is
/// matched exactly, so a macOS cache can never satisfy a Linux fixture.
fn find_package_library_dir(
    artifacts: &Path,
    package: &str,
    archives: &[&str],
) -> Option<PathBuf> {
    let target_dir_name = target().as_str();
    let mut level = vec![artifacts.join(package)];
    // version -> recipe -> source sha -> target -> abi -> toolchain fingerprint
    for depth in 0..6 {
        let mut next = Vec::new();
        for dir in level {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    continue;
                }
                // Depth 3 is the target component; anything built for another target is
                // not a candidate for this harness run.
                if depth == 3 && entry.file_name() != *target_dir_name {
                    continue;
                }
                next.push(entry.path());
            }
        }
        level = next;
    }
    level.into_iter().find_map(|dir| {
        let lib = dir.join("lib");
        archives
            .iter()
            .all(|archive| lib.join(archive).is_file())
            .then_some(lib)
    })
}
