//! Purpose:
//! RUNTIME CA-TRUST-STORE DISCOVERY: decides, once per process, whether this build's
//! COMPILE-TIME CA bundle path is usable on the machine the binary is actually running
//! on, and picks a replacement from a fixed list of well-known root-store locations when
//! it is not. The answer is applied to every easy handle as an ordinary
//! `CURLOPT_CAINFO` (`crate::abi::apply_discovered_cainfo`).
//!
//! Called from:
//! - `crate::abi::elephc_curl_easy_init` and `crate::abi::elephc_curl_easy_reset`, the
//!   two moments a handle is known to carry no user CA configuration yet.
//! - `crate::abi::elephc_curl_easy_setopt_str`, to step back out of the way the moment
//!   the PHP program sets `CURLOPT_CAINFO`/`CURLOPT_CAPATH` itself.
//!
//! Key details:
//! - WHY THIS EXISTS. libcurl's `configure` auto-detects a CA bundle ON THE BUILD MACHINE
//!   and bakes that ABSOLUTE PATH into the archive as `CURL_CA_BUNDLE` (pinned curl
//!   8.21.0, `acinclude.m4`'s `CURL_CHECK_CA_BUNDLE`; the managed recipe in
//!   `src/native_deps/recipes/curl.rs` passes no `--with-ca-*` flag, so the auto-detection
//!   is what runs). `Curl_ssl_easy_config_complete` (`lib/vtls/vtls_config.c`) then applies
//!   that constant to every transfer that has no application CA setting. A binary built on
//!   macOS therefore carries `/etc/ssl/cert.pem`, and running it on Debian — where the
//!   store lives at `/etc/ssl/certs/ca-certificates.crt` — fails EVERY verified HTTPS
//!   transfer with `CURLE_SSL_CACERT_BADFILE` (77), because there is nothing at the baked
//!   path to read. MEASURED on this build: `curl_version_info()->cainfo` is
//!   `/etc/ssl/cert.pem`, `->capath` is NULL.
//! - THE BAKED PATH CAN ALSO BE ABSENT ENTIRELY. `CURL_CHECK_CA_BUNDLE` skips detection
//!   when `cross_compiling` is `yes`, which is exactly what the managed recipe triggers by
//!   passing `--host=` for a non-host target. A cross-built libcurl has NO `CURL_CA_BUNDLE`
//!   at all, so without this module every verified HTTPS transfer from a cross-compiled
//!   elephc binary fails with no trust anchors configured. [`resolve`] handles that as
//!   `baked == None`, which falls straight through to the probe list.
//! - THERE IS NO OpenSSL FALLBACK IN THIS BUILD, so `SSL_CERT_FILE`/`SSL_CERT_DIR` are
//!   dead letters and are deliberately NOT honored here. `X509_STORE_set_default_paths`
//!   (the only call that would make OpenSSL read those two env vars) sits behind
//!   `#ifdef CURL_CA_FALLBACK` in `lib/vtls/openssl.c`, and `--with-ca-fallback` defaults
//!   to `no` and is not passed by the recipe. MEASURED, not assumed: the
//!   `"OpenSSL default paths (fallback)"` trace string that branch emits does not occur
//!   anywhere in the built `libcurl.a`.
//! - `CURL_CA_BUNDLE` (the ENV VAR) IS honored, and that is a DELIBERATE DIVERGENCE from
//!   both php and libcurl. php-src offers the `curl.cainfo` php.ini setting instead and
//!   reads no environment at all; libcurl the LIBRARY reads no environment either — the
//!   `curl_getenv("CURL_CA_BUNDLE")` in the pinned source is in `src/tool_operate.c`, the
//!   command-line tool, which elephc does not link. Honoring it here gives an operator a
//!   PROCESS-WIDE escape hatch for a store this list does not know about (a bundle mounted
//!   into a distroless container, say), which `CURLOPT_CAINFO` cannot provide when the
//!   handles are created by third-party PHP code. It can only ever REDIRECT verification,
//!   never weaken it: an unreadable value still fails closed with `CURLE_SSL_CACERT_BADFILE`.
//! - THE PROBE LIST IS FIXED, ABSOLUTE, AND ROOT-OWNED BY CONVENTION. Nothing here derives
//!   a path from the working directory, `$HOME`, `PATH`, or any other writable-by-default
//!   location, and nothing here ever disables or relaxes verification: when no store is
//!   found, the handle is left EXACTLY as libcurl configured it and libcurl reports its own
//!   error. Discovery is only ever allowed to turn a broken configuration into a working
//!   one.
//! - DIRECTORY STORES (`CURLOPT_CAPATH`) ARE NEVER DISCOVERED, only files. The pinned
//!   `configure` will bake a `CURL_CA_PATH` when `/etc/ssl/certs` holds hash-named
//!   certificates (it did not on this build: `->capath` is NULL), and a capath needs an
//!   OpenSSL `c_rehash` layout that a plain `stat` cannot confirm. A bundle file is
//!   checkable, so that is all this module claims.

use std::ffi::CString;
use std::path::Path;
use std::sync::OnceLock;

use crate::easy;

/// `CURLOPT_CAINFO`: the PEM bundle FILE libcurl verifies server certificates against.
pub(crate) const CURLOPT_CAINFO: i32 = 10065;
/// `CURLOPT_CAPATH`: a DIRECTORY of hash-named certificates, the other half of the pair a
/// PHP program can use to configure trust. Never set by this module — only watched, so
/// discovery gets out of the way when the program configures trust itself.
pub(crate) const CURLOPT_CAPATH: i32 = 10097;

/// The environment variable an operator can point at a CA bundle to override discovery
/// process-wide. Named for parity with the `curl` command-line tool, which reads the same
/// variable (pinned curl 8.21.0, `src/tool_operate.c`). See this module's header for why
/// elephc honors it where php and libcurl-the-library do not.
pub(crate) const CA_BUNDLE_ENV: &str = "CURL_CA_BUNDLE";

/// THE FIXED PROBE LIST, in most-specific-population-first order. Every entry is an
/// absolute path to a FILE that a distribution's own ca-certificates package installs;
/// none of them is writable by an unprivileged user on a correctly installed system.
///
/// Entries 1-6 are the list Go's `crypto/x509` (`root_linux.go`'s `certFiles`) and
/// `rustls-native-certs` both probe, in their order. Entries 7-8 are the two extra paths
/// THIS libcurl's own `configure` would have found had it run on the target machine
/// (`acinclude.m4`'s `CURL_CHECK_CA_BUNDLE` probes
/// `/etc/ssl/certs/ca-certificates.crt`, `/etc/pki/tls/certs/ca-bundle.crt`,
/// `/usr/share/ssl/certs/ca-bundle.crt`, `/usr/local/share/certs/ca-root-nss.crt`,
/// `/etc/ssl/cert.pem`), which is what makes runtime discovery a strict SUPERSET of what a
/// native build of the same recipe would have baked in — the property that lets a shipped
/// binary behave like a locally built one.
pub(crate) const CANDIDATE_CA_FILES: &[&str] = &[
    // Debian, Ubuntu, Gentoo, Arch, Alpine, NixOS.
    "/etc/ssl/certs/ca-certificates.crt",
    // Fedora, RHEL 6.
    "/etc/pki/tls/certs/ca-bundle.crt",
    // openSUSE.
    "/etc/ssl/ca-bundle.pem",
    // OpenELEC.
    "/etc/pki/tls/cacert.pem",
    // RHEL 7+ / CentOS, the `ca-trust` extracted bundle.
    "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
    // macOS, Alpine, FreeBSD/OpenBSD.
    "/etc/ssl/cert.pem",
    // Legacy RHEL/Fedora, from curl's own `configure` probe list.
    "/usr/share/ssl/certs/ca-bundle.crt",
    // FreeBSD's `ca_root_nss`, from curl's own `configure` probe list.
    "/usr/local/share/certs/ca-root-nss.crt",
];

/// What discovery decided for this process: `Some(path)` to set as `CURLOPT_CAINFO` on
/// every handle that carries no CA configuration of its own, `None` to leave libcurl's
/// own configuration untouched.
///
/// THE PURE CORE OF THIS MODULE — no I/O of its own, no environment of its own, no
/// libcurl. `baked` is `curl_version_info()->cainfo` (`None` for a build that has no
/// `CURL_CA_BUNDLE` at all), `env_override` is `$CURL_CA_BUNDLE`, and `exists` answers
/// whether a path names a readable regular file. Every one of them is a parameter
/// precisely so the whole decision table is unit-testable on a machine whose real CA
/// layout is fixed (`crate::tests::ca_discovery`).
///
/// Resolution order:
/// 1. `$CURL_CA_BUNDLE`, when it is a non-empty ABSOLUTE path with no interior NUL. It is
///    NOT existence-checked: an operator naming a bundle is an instruction, and a wrong
///    one must fail closed with libcurl's own `CURLE_SSL_CACERT_BADFILE` rather than be
///    silently replaced by a guess from the probe list. Relative paths are refused because
///    a CA store resolved against the process's working directory is exactly the
///    writable-location hazard this module refuses to introduce.
/// 2. The baked-in default, when a file exists there: answer `None` and change NOTHING, so
///    a binary running on its own build machine (or any same-layout system) behaves
///    EXACTLY as it did before this module existed.
/// 3. The first [`CANDIDATE_CA_FILES`] entry that exists.
/// 4. Otherwise `None`: no store was found, so the handle is left alone and libcurl
///    reports its own error. Discovery NEVER disables verification to make a transfer
///    succeed.
pub(crate) fn resolve(
    baked: Option<&str>,
    env_override: Option<&str>,
    exists: &dyn Fn(&str) -> bool,
) -> Option<String> {
    if let Some(value) = env_override {
        if !value.is_empty() && value.starts_with('/') && !value.contains('\0') {
            return Some(value.to_string());
        }
        // A set-but-unusable value falls through rather than aborting discovery: it is
        // not an instruction this module can carry out, so the ordinary answer is still
        // the best one available.
    }
    if let Some(path) = baked {
        if exists(path) {
            return None;
        }
    }
    CANDIDATE_CA_FILES
        .iter()
        .find(|candidate| exists(candidate))
        .map(|candidate| (*candidate).to_string())
}

/// This process's discovery answer as a ready-to-hand-to-libcurl C string, or `None` when
/// nothing should be set.
///
/// RESOLVED EXACTLY ONCE PER PROCESS, on the first `curl_init()`, and cached for the
/// process's lifetime. That staleness is DELIBERATE, not an oversight: the value it
/// replaces (`CURL_CA_BUNDLE`) is a compile-time constant that never changes either, the
/// alternative is up to eight `stat` calls on every `curl_init()`/`curl_reset()`, and a
/// long-running program that installed or removed a system CA store mid-run would get a
/// trust store that changes under it between two handles. A program that needs the new
/// store restarts, exactly as it would to pick up a new `CURL_CA_BUNDLE`.
///
/// The `CString` is leaked into a `'static` `OnceLock` rather than rebuilt per handle:
/// libcurl COPIES string options (`Curl_setstropt`), so the pointer only has to live for
/// the duration of the `curl_easy_setopt` call, but a single allocation for the whole
/// process is simpler than one per handle and is what makes handing out `&'static` sound.
pub(crate) fn discovered_cainfo() -> Option<&'static CString> {
    static RESOLVED: OnceLock<Option<CString>> = OnceLock::new();
    RESOLVED
        .get_or_init(|| {
            let info = easy::version_info();
            // SAFETY: `cainfo` is either null or libcurl's own `'static` `CURL_CA_BUNDLE`
            // string literal; `version_info` has already run global init.
            let baked = (!info.cainfo.is_null())
                .then(|| unsafe { std::ffi::CStr::from_ptr(info.cainfo) })
                .and_then(|text| text.to_str().ok());
            let env = std::env::var(CA_BUNDLE_ENV).ok();
            resolve(baked, env.as_deref(), &|path| Path::new(path).is_file())
                // `CString::new` only fails on an interior NUL, which `resolve` already
                // refuses for the env value and cannot occur in a `&'static str` literal.
                .and_then(|path| CString::new(path).ok())
        })
        .as_ref()
}
