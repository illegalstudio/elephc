//! Purpose:
//! Home of the PHP `gmp_testbit` builtin: shared-contract binding and AOT semantics.
//!
//! Called from:
//! - `crate::builtins::registry` through the inventory submission below.
//!
//! Key details:
//! - Operand coercion and php-src diagnostics live in the injected GMP prelude helper.

builtin! {
    contract: "gmp_testbit",
    semantics: crate::builtins::gmp::bool_semantics(),
}
