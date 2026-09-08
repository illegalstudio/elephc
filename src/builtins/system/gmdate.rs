//! Purpose:
//! Home of the PHP `gmdate` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `gmdate` is a pure-data builtin whose return type
//!   (`Str`) is fully determined by its declaration. The `timestamp` parameter
//!   is optional and defaults to `null` (current time).


builtin! {
    contract: "gmdate",
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::Gmdate),
        crate::builtins::semantics::BuiltinArgumentLowering::Date,
    ),
}
