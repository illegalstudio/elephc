//! Purpose:
//! Internal libc locale-category setter used by the injected `setlocale()` prelude.
//!
//! Called from:
//! - The builtin registry and EIR runtime-call lowering.
//!
//! Key details:
//! - Copies the PHP locale name into a NUL-terminated stack buffer, calls libc
//!   `setlocale`, and persists the returned C string before it can be overwritten.

builtin! {
    contract: "__elephc_setlocale_set",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcSetlocaleSet,
    ),
}
