//! Purpose:
//! Home of the PHP `get_debug_type` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry
//!   common path.
//! - Objects answer from the `_class_debug_name_entries` metadata table, which spells an
//!   anonymous class the way php-src does (`class@anonymous`, `Parent@anonymous`,
//!   `Interface@anonymous`).

builtin! {
    contract: "get_debug_type",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::GetDebugType,
    ),
}
