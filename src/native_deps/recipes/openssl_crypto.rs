//! Purpose:
//! Builds the pinned OpenSSL 3.6.4 `libcrypto.a` used by the libcrypto-backed
//! cipher catalog and EVP bridge.
//!
//! Called from:
//! - `crate::native_deps::recipe::CuratedRecipes` for `openssl-crypto` revision 1.
//!
//! Key details:
//! - No legacy provider is built: PHP 8.5's default-provider catalog is exactly
//!   135 names, while the legacy provider would add 32 names PHP does not report.
//! - The shared OpenSSL recipe is reused with `no-ssl`, so only `libcrypto.a` is
//!   retained and curl's separate OpenSSL pin is unaffected.

use super::super::error::NativeError;
use super::super::recipe::RecipeRequest;

/// Builds the OpenSSL `libcrypto` static archive into the catalog staging prefix.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    super::openssl::build_crypto_only(request)
}
