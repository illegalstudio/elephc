//! Purpose:
//! Re-exports the shared PHP compatibility version selected with `--php-version`.
//!
//! Called from:
//! - `crate::cli::compile_config()` when normalizing `--php-version`.
//! - Version-sensitive standard-library preludes such as `crate::pdo_prelude`.
//!
//! Key details:
//! - The type itself lives in `elephc_builtin_contract::PhpVersion` so the shared symbol
//!   catalogs (`since`) and Magician use the same definition as the compiler.

pub use elephc_builtin_contract::PhpVersion;
