//! Purpose:
//! Adds native link directives for `elephc-tidy`'s own test binary when
//! `ELEPHC_TIDY_LIBTIDY_LIB_DIR` points at the managed `libtidy` artifact.
//!
//! Called from:
//! - Cargo, automatically, before compiling any target of this crate.
//!
//! Key details:
//! - `cargo build -p elephc-tidy` never needs this: the staticlib/rlib outputs
//!   do not invoke the system linker, so the unresolved libtidy symbols stay
//!   unresolved until the final PHP-program link supplies `libtidy.a`.
//! - `cargo test -p elephc-tidy` does link an executable, so the libtidy-calling
//!   tests live behind `cfg(elephc_tidy_native)`; without the env variable they
//!   are not compiled and the single skip test links cleanly.
//! - `ELEPHC_TIDY_LIB_DIR` is a different variable: `src/linker/bridges.rs`
//!   reads it as the override for a prebuilt `libelephc_tidy.a` bridge archive.

use std::env;
use std::path::Path;

/// Declares the custom cfg and, when configured, links the managed archive.
fn main() {
    println!("cargo:rustc-check-cfg=cfg(elephc_tidy_native)");
    println!("cargo:rerun-if-env-changed=ELEPHC_TIDY_LIBTIDY_LIB_DIR");

    let Some(lib_dir) = env::var_os("ELEPHC_TIDY_LIBTIDY_LIB_DIR") else {
        return;
    };

    println!(
        "cargo:rustc-link-search=native={}",
        Path::new(&lib_dir).display()
    );
    // `-bundle`: link the test binary against the archive without copying its
    // members into `libelephc_tidy.a`; compiled programs keep reaching libtidy
    // through the managed catalog artifact.
    println!("cargo:rustc-link-lib=static:-bundle=tidy");
    println!("cargo:rustc-cfg=elephc_tidy_native");
}
