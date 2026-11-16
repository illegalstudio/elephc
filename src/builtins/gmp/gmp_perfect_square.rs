//! Purpose:
//! Home of the PHP `gmp_perfect_square` builtin: shared-contract binding and AOT semantics.
//!
//! Called from:
//! - `crate::builtins::registry` through the inventory submission below.
//!
//! Key details:
//! - Operand coercion and php-src diagnostics live in the injected GMP prelude helper.

builtin! {
    contract: "gmp_perfect_square",
    semantics: crate::builtins::gmp::bool_semantics(),
}
