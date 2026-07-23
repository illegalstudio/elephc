//! Purpose:
//! Groups reviewed package-specific native source recipes.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes`.
//!
//! Key details:
//! - Each recipe consumes only catalog constants and selected toolchain data.

pub mod pcre2;
