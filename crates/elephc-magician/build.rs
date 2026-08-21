//! Purpose:
//! Build script for native libraries used only by eval bridge unit tests.
//!
//! Called from:
//! - Cargo while compiling `elephc-magician`.
//!
//! Key details:
//! - Production eval regex support uses registered callbacks and keeps the
//!   staticlib free of direct PCRE2 link requirements.
//! - Unit tests install an equivalent host-PCRE2 provider; raw link arguments
//!   apply to this package's linked targets without propagating native-library
//!   metadata to downstream users of the rlib/staticlib.
//! - Musl test executables select the installed static PCRE2 archives so they
//!   do not acquire an unavailable glibc program interpreter.

use std::{env, path::Path};

/// Emits package-local PCRE2 link arguments, selecting static archives on musl.
fn main() {
    for path in pcre2_library_search_paths() {
        println!("cargo:rustc-link-search=native={path}");
    }
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_env == "musl" {
        println!("cargo:rustc-link-arg=-Wl,-Bstatic");
    }
    println!("cargo:rustc-link-arg=-lpcre2-posix");
    println!("cargo:rustc-link-arg=-lpcre2-8");
    if target_env == "musl" {
        // The raw PCRE2 archives follow Rust's first libc occurrence, so repeat
        // libc here to resolve their stack-protector references in link order.
        println!("cargo:rustc-link-arg=-lc");
    }
    if env::var("TARGET").as_deref() == Ok("aarch64-unknown-linux-musl") {
        println!("cargo:rustc-link-arg=-lgcc");
    }
    if target_env == "musl" {
        println!("cargo:rustc-link-arg=-Wl,-Bdynamic");
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
