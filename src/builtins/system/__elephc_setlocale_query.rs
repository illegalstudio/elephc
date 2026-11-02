//! Purpose:
//! Internal libc locale-category query used by the injected `setlocale()` prelude.
//!
//! Called from:
//! - The builtin registry and EIR runtime-call lowering.
//!
//! Key details:
//! - Calls `setlocale(category, NULL)` and persists the returned C string before the
//!   next locale change can overwrite libc's static result buffer.

builtin! {
    contract: "__elephc_setlocale_query",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcSetlocaleQuery,
    ),
}
