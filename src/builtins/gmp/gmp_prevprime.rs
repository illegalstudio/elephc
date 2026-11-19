//! Purpose:
//! Home of the PHP `gmp_prevprime` builtin: shared-contract binding and AOT semantics.
//!
//! Called from:
//! - `crate::builtins::registry` through the inventory submission below.
//!
//! Key details:
//! - The optional `$definitely_prime` output parameter is write-only; the custom lowering
//!   stores the prelude helper's flag back into the caller's variable storage.
//! - Operand coercion and php-src 8.6 diagnostics live in the injected GMP prelude helper.

builtin! {
    contract: "gmp_prevprime",
    semantics: crate::builtins::gmp::prevprime_semantics(),
}
