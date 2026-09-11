//! Purpose:
//! Binds mb_ereg_search_pos to the shared mbregex request operation.
//!
//! Called from:
//! - The AOT builtin registry and typed EIR lowering.
//!
//! Key details:
//! - Shared contracts own PHP signatures; the bridge owns state, captures, and errors.

builtin! {
    contract: "mb_ereg_search_pos",
    check: crate::builtins::mbstring::check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbEregSearchPos),
}
