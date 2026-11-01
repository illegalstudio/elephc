//! Purpose:
//! Pure-Rust ZIP archive bridge for elephc's `ext/zip` surface.
//!
//! Called from:
//! - Compiled PHP programs through the `elephc_zip_*` C ABI declared by `src/zip_prelude`.
//! - `cargo test -p elephc-zip` for the archive-engine unit tests.
//!
//! Key details:
//! - Handles are monotonic IDs backed by a process registry; no pointers cross the ABI.
//! - Archive mutations are applied in memory and serialized atomically on close.
//! - Every PHP-visible string crosses as hex when it may contain arbitrary bytes.

mod abi;
mod clock;
mod compress;
mod consts;
mod error;
mod extract;
mod metadata;
mod model;
mod timeutil;
mod write;

#[cfg(test)]
mod tests;

pub use abi::*;

/// Returns the libzip compatibility version exposed as `ZipArchive::LIBZIP_VERSION`.
pub fn engine_version() -> &'static str {
    "1.11.4"
}
