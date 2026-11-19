//! Purpose:
//! Home of the internal `__elephc_preg_compile` builtin: compiles a PCRE pattern once for
//! the `preg_grep()` prelude and returns an opaque handle.
//!
//! Called from:
//! - The injected `preg_grep()` body in `crate::pcre_prelude` through the builtin registry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible. It resets the `preg_last_error()` state like the
//!   start of php-src's `preg_grep()`, and an invalid pattern yields `0` with
//!   `PREG_INTERNAL_ERROR`. Every handle must reach `__elephc_preg_free()`.

builtin! {
    contract: "__elephc_preg_compile",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PregCompile,
    ),
}
