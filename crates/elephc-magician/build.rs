//! Purpose:
//! Build script for native libraries used by the eval bridge crate.
//!
//! Called from:
//! - Cargo while compiling `elephc-magician`.
//!
//! Key details:
//! - Eval regex support calls PCRE2's POSIX wrapper directly, so Rust test
//!   binaries that link the rlib need the same native libraries as generated
//!   elephc binaries.

use std::{
    env,
    path::{Path, PathBuf},
};

/// Emits native PCRE2 link directives for cargo-built test binaries and rlibs.
fn main() {
    println!("cargo:rerun-if-env-changed=ELEPHC_MINGW_SYSROOT");
    for path in pcre2_library_search_paths() {
        println!("cargo:rustc-link-search=native={}", path.display());
    }
    println!("cargo:rustc-link-lib=pcre2-posix");
    println!("cargo:rustc-link-lib=pcre2-8");
    if env::var("TARGET").as_deref() == Ok("x86_64-pc-windows-gnu") {
        println!("cargo:rustc-link-lib=iconv");
    }
    if env::var("TARGET").as_deref() == Ok("aarch64-unknown-linux-musl") {
        println!("cargo:rustc-link-lib=gcc");
    }
}

/// Returns target-compatible PCRE2 library directories from the MinGW sysroot
/// and common local macOS/Homebrew installations.
fn pcre2_library_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if env::var("TARGET").as_deref() == Ok("x86_64-pc-windows-gnu") {
        if let Some(sysroot) = env::var_os("ELEPHC_MINGW_SYSROOT") {
            let sysroot = PathBuf::from(sysroot);
            for directory in [sysroot.join("lib"), sysroot.join("lib64")] {
                if directory.is_dir() {
                    paths.push(directory);
                }
            }
        }
    }
    paths.extend([
        "/opt/homebrew/opt/pcre2/lib",
        "/opt/homebrew/lib",
        "/usr/local/opt/pcre2/lib",
        "/usr/local/lib",
    ]
    .into_iter()
    .filter(|path| Path::new(path).exists())
    .map(PathBuf::from));
    paths
}
