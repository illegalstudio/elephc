//! Purpose:
//! Builds the pinned OpenSSL 3.6.4 `libssl.a`/`libcrypto.a` archives used by the
//! cipher catalog and the PHP 8.6 `Openssl\Session` bridge.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes` for `openssl-crypto` revision 2.
//!
//! Key details:
//! - No legacy provider is built: PHP 8.5's default-provider catalog is exactly
//!   135 names, while the legacy provider would add 32 names PHP does not report.
//! - The shared OpenSSL recipe is reused without `no-ssl`, so `Openssl\Session`
//!   can import, export and inspect TLS sessions through libssl; curl's separate
//!   3.5.8 pin remains untouched.

use super::super::error::NativeError;
use super::super::recipe::RecipeRequest;

/// Builds the OpenSSL static archives into the catalog staging prefix.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    super::openssl::build(request)
}
