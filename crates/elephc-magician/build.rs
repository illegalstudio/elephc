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

use std::{
    env,
    path::{Path, PathBuf},
};

/// Emits package-local PCRE2 link arguments, selecting static archives on musl.
fn main() {
    println!("cargo:rerun-if-env-changed=ELEPHC_MINGW_SYSROOT");
    for path in pcre2_library_search_paths() {
        println!("cargo:rustc-link-search=native={}", path.display());
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
