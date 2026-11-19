//! Purpose:
//! Home of the PHP `gmp_powm_sec` builtin: shared-contract binding and AOT semantics.
//!
//! Called from:
//! - `crate::builtins::registry` through the inventory submission below.
//!
//! Key details:
//! - Operand coercion and php-src 8.6 diagnostics live in the injected GMP prelude helper.

builtin! {
    contract: "gmp_powm_sec",
    semantics: crate::builtins::gmp::object_semantics(),
}
