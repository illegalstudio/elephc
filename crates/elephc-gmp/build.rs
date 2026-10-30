//! Purpose:
//! Adds the managed libgmp archive to `elephc-gmp`'s own test binary only.
//!
//! Called from:
//! - Cargo, automatically, before compiling any target of this crate.
//!
//! Key details:
//! - `cargo build -p elephc-gmp` never links: the staticlib/rlib keep their
//!   `__gmpz_*` references unresolved for the PHP-program linker to satisfy.
//! - `cargo test -p elephc-gmp` produces a real executable. Setting
//!   `ELEPHC_GMP_LIB_DIR` to an installed `gmp` artifact's `lib/` directory links
//!   `libgmp.a` and compiles the real operation tests behind `elephc_gmp_native`.
//!   Without it the test binary links cleanly and prints a skip message.
//! - `ELEPHC_GMP_LIB_DIR` is DIFFERENT from `src/linker/bridges.rs`'s
//!   `ELEPHC_GMP_LIB_DIR` override for a prebuilt bridge archive; it is named
//!   after the native package on purpose and only used by this script.

use std::env;
use std::path::Path;

/// Emits the custom `cfg` this build script may set.
fn declare_check_cfg() {
    println!("cargo:rustc-check-cfg=cfg(elephc_gmp_native)");
}

/// Configures libgmp for the crate's test binary when an artifact is supplied.
fn main() {
    declare_check_cfg();
    println!("cargo:rerun-if-env-changed=ELEPHC_GMP_LIB_DIR");

    let Some(lib_dir) = env::var_os("ELEPHC_GMP_LIB_DIR").filter(|value| !value.is_empty()) else {
        return;
    };
    let archive = Path::new(&lib_dir).join("libgmp.a");
    if !archive.is_file() {
        panic!(
            "ELEPHC_GMP_LIB_DIR={} does not contain libgmp.a (run: scripts/ci/gmp_lib_dir.sh <target>)",
            Path::new(&lib_dir).display()
        );
    }

    println!(
        "cargo:rustc-link-search=native={}",
        Path::new(&lib_dir).display()
    );
    // `-bundle` keeps libgmp out of this crate's staticlib output; shipped
    // programs must resolve it from the managed catalog package instead.
    println!("cargo:rustc-link-lib=static:-bundle=gmp");
    println!("cargo:rustc-cfg=elephc_gmp_native");
}
