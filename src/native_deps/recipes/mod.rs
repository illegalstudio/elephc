//! Purpose:
//! Groups reviewed package-specific native source recipes.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes`.
//!
//! Key details:
//! - Each recipe consumes only catalog constants and selected toolchain data.

pub(super) mod util;
pub mod pcre2;
pub mod zlib;
