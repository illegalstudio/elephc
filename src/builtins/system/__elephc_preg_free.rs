//! Purpose:
//! Home of the internal `__elephc_preg_free` builtin: releases a pattern handle returned by
//! `__elephc_preg_compile()`.
//!
//! Called from:
//! - The `finally` block of the injected `preg_grep()` body in `crate::pcre_prelude`.
//!
//! Key details:
//! - `internal: true`: never PHP-visible. It leaves the `preg_last_error()` state alone.

builtin! {
    contract: "__elephc_preg_free",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PregFree,
    ),
}
