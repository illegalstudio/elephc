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

use std::{env, path::Path};

/// Emits native PCRE2 link directives for cargo-built test binaries and rlibs.
fn main() {
    for path in pcre2_library_search_paths() {
        println!("cargo:rustc-link-search=native={path}");
    }
    println!("cargo:rustc-link-lib=pcre2-posix");
    println!("cargo:rustc-link-lib=pcre2-8");
    if env::var("TARGET").as_deref() == Ok("aarch64-unknown-linux-musl") {
        println!("cargo:rustc-link-lib=gcc");
    }
}

/// Returns common PCRE2 library directories for local macOS/Homebrew builds.
fn pcre2_library_search_paths() -> Vec<&'static str> {
    [
        "/opt/homebrew/opt/pcre2/lib",
        "/opt/homebrew/lib",
        "/usr/local/opt/pcre2/lib",
        "/usr/local/lib",
    ]
    .into_iter()
    .filter(|path| Path::new(path).exists())
    .collect()
}
