//! Purpose:
//! Groups reviewed package-specific native source recipes.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes`.
//!
//! Key details:
//! - Each recipe consumes only catalog constants and selected toolchain data.

pub(super) mod util;
pub mod curl;
pub mod libssh2;
pub mod nghttp2;
pub mod openssl;
pub mod pcre2;
pub mod zlib;
