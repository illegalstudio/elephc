//! Purpose:
//! Adds native link-search/lib directives for `elephc-curl`'s own test binary,
//! and ONLY for that binary, so `cargo test -p elephc-curl` can link real
//! libcurl/libssl/libcrypto/libz static archives from an installed `elephc
//! native` package when a developer asks for it.
//!
//! Called from:
//! - Cargo, automatically, before compiling any target of this crate
//!   (including `cargo build -p elephc-curl` and `cargo test -p elephc-curl`).
//!
//! Key details:
//! - `cargo build -p elephc-curl` never needs this: the crate's `staticlib`/
//!   `rlib` outputs never invoke the system linker, so libcurl's unresolved
//!   `extern "C"` symbols stay unresolved in the archive regardless of
//!   whether these directives were emitted. The PHP-program linker supplies
//!   `libcurl.a` at final-binary link time (Task 4).
//! - `cargo test -p elephc-curl` DOES produce a real executable, so it needs
//!   every symbol resolved. The crate's own gated unit tests
//!   (`src/tests.rs`) that call the real ABI live behind
//!   `#[cfg(elephc_curl_native)]`, which this script only emits when
//!   `ELEPHC_CURL_LIB_DIR` is set. When the cfg is absent, nothing in the
//!   test binary references libcurl's symbols (the crate's own always-linked
//!   `elephc_curl_*` entry points are simply never called by anything in that
//!   build, so the linker's dead-stripping drops them) and the test binary
//!   links cleanly, running a single test that prints a clear skip message.
//! - `ELEPHC_CURL_LIB_DIR` must point at curl's OWN `lib/` directory
//!   (containing `libcurl.a`). `ELEPHC_CURL_OPENSSL_LIB_DIR` /
//!   `ELEPHC_CURL_ZLIB_LIB_DIR` point at the sibling OpenSSL/zlib `lib/`
//!   directories — these are separate `elephc native` packages with
//!   unrelated content-hashed paths, so no single directory covers all four
//!   archives. Link order mirrors libcurl's own dependency order: curl -> ssl
//!   -> crypto -> z, plus the macOS system frameworks curl's TLS/resolver
//!   backend needs (see `.superpowers/sdd/php-curl-family/task-2-report.md`'s
//!   link smoke, and this file's own empirical `SystemConfiguration` fix
//!   below, for the exact set).
//! - These three env var names are deliberately NOT `ELEPHC_CURL_LIB_DIR`
//!   alone reused for anything else: `src/linker/bridges.rs`'s `BRIDGES`
//!   table convention (`ELEPHC_<NAME>_LIB_DIR`) will likely want that exact
//!   name for a *different* purpose in Task 4 (overriding where the elephc
//!   compiler finds a prebuilt `libelephc_curl.a` bridge archive). The two
//!   never run in the same process, so there is no runtime collision, but a
//!   human reading both names side by side should not conflate them.

use std::env;
use std::path::Path;

/// Emits the custom `cfg`s this build script may set, so rustc's
/// `unexpected_cfgs` lint stays quiet regardless of which branch below runs.
fn declare_check_cfg() {
    println!("cargo:rustc-check-cfg=cfg(elephc_curl_native)");
}

/// Adds a native search path for `dir` when it exists, so a missing/typo'd
/// override directory fails at link time with a clear "symbol not found"
/// instead of a silently-ignored search path.
fn add_search_path(dir: &std::ffi::OsStr) {
    println!("cargo:rustc-link-search=native={}", Path::new(dir).display());
}

fn main() {
    declare_check_cfg();

    println!("cargo:rerun-if-env-changed=ELEPHC_CURL_LIB_DIR");
    println!("cargo:rerun-if-env-changed=ELEPHC_CURL_OPENSSL_LIB_DIR");
    println!("cargo:rerun-if-env-changed=ELEPHC_CURL_ZLIB_LIB_DIR");

    let Some(curl_lib_dir) = env::var_os("ELEPHC_CURL_LIB_DIR") else {
        // No native artifact configured. Emit nothing: `cargo build -p
        // elephc-curl` already does not need it, and `cargo test -p
        // elephc-curl` will link cleanly because the gated real tests
        // (behind `elephc_curl_native`) simply are not compiled in.
        return;
    };

    // Real native artifacts requested: wire up the link order the Task 2
    // report's C link smoke validated (curl -> ssl -> crypto -> z), plus the
    // macOS frameworks curl's OpenSSL/resolver backend needs there.
    add_search_path(&curl_lib_dir);
    println!("cargo:rustc-link-lib=static=curl");

    if let Some(ssl_lib_dir) = env::var_os("ELEPHC_CURL_OPENSSL_LIB_DIR") {
        add_search_path(&ssl_lib_dir);
    }
    println!("cargo:rustc-link-lib=static=ssl");
    println!("cargo:rustc-link-lib=static=crypto");

    if let Some(z_lib_dir) = env::var_os("ELEPHC_CURL_ZLIB_LIB_DIR") {
        add_search_path(&z_lib_dir);
    }
    println!("cargo:rustc-link-lib=static=z");

    if env::var_os("CARGO_CFG_TARGET_OS").as_deref() == Some(std::ffi::OsStr::new("macos")) {
        // `Security`/`CoreFoundation` satisfy OpenSSL's keychain-backed trust
        // store lookups; `SystemConfiguration` satisfies libcurl's own
        // `Curl_macos_init`/`SCDynamicStoreCopyProxies` system-proxy
        // detection (`lib/macos.c`) — confirmed necessary empirically: a
        // link without it fails with an undefined
        // `_SCDynamicStoreCopyProxies` symbol even though nothing in this
        // crate calls it directly. `elephc-pdo`'s `BRIDGES` entry
        // (`src/linker/bridges.rs`) needs the identical pair for the same
        // underlying reason (its libpq dependency).
        println!("cargo:rustc-link-lib=framework=Security");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=SystemConfiguration");
    }

    // Compile the crate's real, libcurl-calling unit tests in.
    println!("cargo:rustc-cfg=elephc_curl_native");
}
