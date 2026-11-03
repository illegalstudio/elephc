//! Purpose:
//! Adds native link-search/lib directives for `elephc-zip`'s own test binary, and ONLY for
//! that binary, so `cargo test -p elephc-zip` can link the managed libzip 1.11.4 archive and
//! its bzip2/LZMA/zlib/OpenSSL dependency chain from installed `elephc native` packages.
//!
//! Called from:
//! - Cargo, automatically, before compiling any target of this crate (including
//!   `cargo build -p elephc-zip` and `cargo test -p elephc-zip`).
//!
//! Key details:
//! - `cargo build -p elephc-zip` never needs this: the crate's `staticlib`/`rlib` outputs
//!   never invoke the system linker, so the unresolved `zip*` symbols stay unresolved in the
//!   archive. The PHP-program linker supplies the managed `libzip` package at final-binary
//!   link time instead.
//! - `cargo test -p elephc-zip` produces a real executable. The tests live behind
//!   `#[cfg(elephc_zip_native)]`, which this script only emits when
//!   `ELEPHC_ZIP_NATIVE_LIB_DIRS` is set to the dependency `lib/` directories in link order
//!   (`COLON`-separated: libzip, bzip2, xz, zlib, openssl).
//! - `ELEPHC_ZIP_LIB_DIR` remains the production override for a prebuilt `libelephc_zip.a`
//!   bridge archive (`src/linker/bridges.rs`), never the native artifacts.

use std::env;
use std::path::PathBuf;

/// Emits the custom `cfg` this build script may set, so `unexpected_cfgs` stays quiet.
fn declare_check_cfg() {
    println!("cargo:rustc-check-cfg=cfg(elephc_zip_native)");
}

/// Configures the managed libzip chain when the test artifact directories are supplied.
fn main() {
    declare_check_cfg();
    println!("cargo:rerun-if-env-changed=ELEPHC_ZIP_NATIVE_LIB_DIRS");

    let Some(dirs) = env::var_os("ELEPHC_ZIP_NATIVE_LIB_DIRS") else {
        return;
    };
    if dirs.is_empty() {
        return;
    }
    for dir in env::split_paths(&dirs) {
        println!("cargo:rustc-link-search=native={}", PathBuf::from(dir).display());
    }
    // Static link order: libzip references bzip2, LZMA, zlib and libcrypto; libssl references
    // libcrypto. `-bundle` keeps these archives out of the crate's own staticlib output.
    for library in ["zip", "bz2", "lzma", "z", "ssl", "crypto"] {
        println!("cargo:rustc-link-lib=static:-bundle={library}");
    }
    if env::var_os("CARGO_CFG_TARGET_OS").as_deref() == Some(std::ffi::OsStr::new("linux")) {
        // OpenSSL's static archives pull in dlopen/dlsym on Linux.
        println!("cargo:rustc-link-lib=dylib=dl");
    }

    println!("cargo:rustc-cfg=elephc_zip_native");
}
