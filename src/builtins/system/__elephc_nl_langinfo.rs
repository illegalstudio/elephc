//! Purpose:
//! Internal libc `nl_langinfo()` query used by the injected `nl_langinfo()` prelude.
//!
//! Called from:
//! - The builtin registry and EIR runtime-call lowering.
//!
//! Key details:
//! - Calls libc `nl_langinfo(item)` and persists the returned C string before another
//!   locale change can overwrite libc's static result buffer.

builtin! {
    contract: "__elephc_nl_langinfo",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcNlLanginfo,
    ),
}
