//! Purpose:
//! Home of the internal `__elephc_preg_match_compiled` builtin: matches one subject against a
//! pattern handle from `__elephc_preg_compile()`.
//!
//! Called from:
//! - The injected `preg_grep()` body in `crate::pcre_prelude` through the builtin registry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible. Unlike `preg_match()` it does not reset the
//!   `preg_last_error()` state, so a nested error an entry's `__toString()` recorded stays
//!   set, as in php-src. Returns `1`, `0`, or `-1` after recording its own match error.

builtin! {
    contract: "__elephc_preg_match_compiled",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PregMatchCompiled,
    ),
}
