//! Purpose:
//! The `elephc_zip` bridge: PHP's `ext/zip` surface as a thin FFI layer over libzip 1.11.4
//! (the C library from the native catalog's `libzip` package), linked into compiled programs
//! that use `ZipArchive` or the deprecated procedural API.
//!
//! Called from:
//! - Compiled PHP programs through the `elephc_zip_*` entry points (`crate::abi`), declared
//!   by the compiler's zip prelude as an `extern "elephc_zip"` block.
//! - `cargo test -p elephc-zip` for the engine tests (they need the managed native archives,
//!   see `build.rs`).
//!
//! Key details:
//! - `ffi` declares libzip's public C API; `archive` owns the `zip_t` pointer lifecycle.
//! - Handles are integer IDs backed by a process registry; no libzip pointer crosses the ABI.
//! - Payloads and comments cross as explicit byte slices, hex-encoded where PHP must see
//!   arbitrary binary data.

mod archive;
mod abi;
mod clock;
mod consts;
mod error;
mod extract;
mod ffi;

#[cfg(all(test, elephc_zip_native))]
mod tests;

#[cfg(all(test, not(elephc_zip_native)))]
mod skipped_tests {
    //! Purpose:
    //! The single test that runs when no native libzip artifact is configured, so a plain
    //! `cargo test -p elephc-zip` reports why the engine tests did not run.
    //!
    //! Called from:
    //! - `cargo test -p elephc-zip` without `ELEPHC_ZIP_NATIVE_LIB_DIRS`.
    //!
    //! Key details:
    //! - Mirrors the native-package skip path used by the curl and xml bridges.

    /// Prints the skip instruction instead of silently passing an empty suite.
    #[test]
    fn native_libzip_tests_are_skipped() {
        eprintln!(
            "SKIP: ELEPHC_ZIP_NATIVE_LIB_DIRS is not set; run elephc native add libzip and point it at the libzip/zlib/bzip2/xz/openssl lib/ directories"
        );
    }
}

pub use abi::*;
