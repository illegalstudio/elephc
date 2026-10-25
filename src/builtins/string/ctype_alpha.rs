//! Purpose:
//! Home of the PHP `ctype_alpha` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `ctype_alpha` is a pure-data builtin whose return type
//!   (`Bool`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.


builtin! {
    contract: "ctype_alpha",
    // php-src takes `mixed`: an int is a character code or its digits, every other non-string
    // is false. The operand keeps its own type so the backend can tell them apart.
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::CtypeAlpha),
        crate::builtins::semantics::BuiltinArgumentLowering::PreserveValues,
    ),
}
