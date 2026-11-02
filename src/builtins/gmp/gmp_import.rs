//! Purpose:
//! Home of the PHP `gmp_import` builtin: shared-contract binding and AOT semantics.
//!
//! Called from:
//! - `crate::builtins::registry` through the inventory submission below.
//!
//! Key details:
//! - Operand coercion and php-src diagnostics live in the injected GMP prelude helper.

builtin! {
    contract: "gmp_import",
    semantics: crate::builtins::gmp::object_semantics(),
}
