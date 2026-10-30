//! Purpose:
//! Home of the PHP `gmp_scan1` builtin: shared-contract binding and AOT semantics.
//!
//! Called from:
//! - `crate::builtins::registry` through the inventory submission below.
//!
//! Key details:
//! - Operand coercion and php-src diagnostics live in the injected GMP prelude helper.

builtin! {
    contract: "gmp_scan1",
    semantics: crate::builtins::gmp::int_semantics(),
}
