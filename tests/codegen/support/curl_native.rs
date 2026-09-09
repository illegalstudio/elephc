//! Purpose:
//! Locates this machine's managed native `curl` / `libssh2` / `nghttp2` / `openssl` /
//! `zlib` archives so a codegen fixture that links the `elephc_curl` bridge can resolve
//! libcurl's symbols, and reports whether they are installed at all.
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

use std::path::PathBuf;
use super::native_cache::{find_package_library_dir, native_cache_artifacts_root};
use std::sync::OnceLock;

use super::target;

/// The managed native packages a linked `elephc_curl` needs, paired with the archive
/// filename that proves the directory really is that package's `lib/`. libcurl links
/// against libssh2 (SCP/SFTP), OpenSSL (TLS), zlib (transfer encodings) and nghttp2
/// (HTTP/2); each is a separate catalog package with an unrelated content-hashed path,
/// so each is discovered on its own.
///
/// THE ORDER IS THE STATIC LINK ORDER, and it mirrors what
/// `src/native_deps/catalog.rs`' `CURL_VERSIONS.dependencies` resolves to for the
/// production link: `libssh2.a` has to precede the OpenSSL and zlib archives that
/// satisfy it, and `libnghttp2.a` (which needs nothing further) trails.
const CURL_NATIVE_PACKAGES: &[(&str, &[&str])] = &[
    ("curl", &["libcurl.a"]),
    ("libssh2", &["libssh2.a"]),
    ("openssl", &["libssl.a", "libcrypto.a"]),
    ("zlib", &["libz.a"]),
    ("nghttp2", &["libnghttp2.a"]),
];

/// The Apple frameworks a statically linked libcurl needs: the first three mirror curl
/// 8.21's upstream `APPLE_SECTRUST_LDFLAGS`, and macOS's `Curl_macos_init` references
/// `SCDynamicStoreCopyProxies` from SystemConfiguration. Kept in step with the bridge's
/// `apple_frameworks` in `src/linker/bridges.rs`, which is what the production link uses;
/// that table lives in the binary crate and is not reachable from an integration test, so
/// it is mirrored here rather than imported.
pub(crate) const CURL_APPLE_FRAMEWORKS: &[&str] = &[
    "Security",
    "CoreFoundation",
    "CoreServices",
    "SystemConfiguration",
];

/// One discovered package: the `lib/` directory plus the EXACT ARCHIVE PATHS it provides,
/// in libcurl's own dependency order (curl -> ssh2 -> ssl -> crypto -> z -> nghttp2).
///
/// Exact paths, not `-l` names, for the same reason the production planner uses
/// `LinkItem::managed_archive` (`src/link_planning.rs`): a `-l` name competes with every
/// other named library in the plan. It loses that competition twice over — a `-lz` already
/// emitted for `gzinflate()`/`fopen()` suppresses the managed one (which is what broke the
/// `streams::` fixtures on GNU ld), and even when it is emitted, `-lz` resolves through the
/// `-L` search order and could bind a system zlib into a fixture whose entire purpose is to
/// prove the pinned build. An absolute path can do neither.
#[derive(Clone, Debug)]
pub(crate) struct CurlNativePackage {
    pub(crate) name: &'static str,
    pub(crate) library_dir: PathBuf,
    pub(crate) archives: Vec<PathBuf>,
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

/// THE STABLE TOKEN `scripts/ci/run_curl_codegen_shard.sh` GREPS FOR. It is a made-up
/// identifier rather than a phrase out of the message below, and that is the whole point:
/// the human-readable half of the message names the packages, so it changes whenever the
/// package set does — which is exactly what happened when `libssh2`/`nghttp2` joined and
/// left the CI gate matching a string nothing prints any more. A gate that greps prose is
/// a gate that dies silently the next time the prose is right.
///
/// If this constant is ever renamed, `run_curl_codegen_shard.sh` must be updated in the
/// same commit; nothing else in the tree may depend on its spelling.
pub(crate) const SKIP_GATE_MARKER: &str = "ELEPHC_CURL_NATIVE_SKIP_GATE";

/// Reports a skip for `test_name` and returns whether the caller should return early.
///
/// Printing (rather than silently returning `true`) keeps a skipped curl suite visible in
/// `cargo test -- --nocapture` and in CI logs, so "all green" on a machine without the
/// packages cannot be mistaken for "curl is covered here". The CI shard script turns that
/// visibility into a hard failure by grepping [`SKIP_GATE_MARKER`] out of the log.
pub(crate) fn skip_without_curl_native(test_name: &str) -> bool {
    if available() {
        return false;
    }
    eprintln!(
        "{SKIP_GATE_MARKER} skipping {test_name}: managed native \
         curl/libssh2/nghttp2/openssl/zlib are not installed for {} \
         (run: elephc native add curl --target {})",
        target().as_str(),
        target().as_str()
    );
    true
}

/// Discovers every package, returning `None` unless all of them are present.
fn discover_packages() -> Option<Vec<CurlNativePackage>> {
    let artifacts = native_cache_artifacts_root()?;
    let mut packages = Vec::new();
    for (package, archives) in CURL_NATIVE_PACKAGES {
        let library_dir = find_package_library_dir(&artifacts, package, archives)?;
        packages.push(CurlNativePackage {
            name: package,
            archives: archives.iter().map(|archive| library_dir.join(archive)).collect(),
            library_dir,
        });
    }
    Some(packages)
}
