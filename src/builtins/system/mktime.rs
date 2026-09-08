//! Purpose:
//! Home of the PHP `mktime` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Optional civil fields use a single post-argument clock snapshot.

/// PHP date-format tokens in the six civil-argument positions; hour has no default.
pub(crate) const MKTIME_COMPONENT_FORMATS: [&str; 6] = ["G", "i", "s", "n", "j", "Y"];

/// Selects shared nullable-field preparation before the typed local/UTC runtime call.
pub(crate) const fn mktime_semantics(utc: bool) -> crate::builtins::semantics::BuiltinSemantics {
    let target = if utc { crate::ir::RuntimeFnId::Gmmktime } else { crate::ir::RuntimeFnId::Mktime };
    let mut semantics = crate::builtins::semantics::runtime_fn_semantics(target);
    semantics.argument_lowering = crate::builtins::semantics::BuiltinArgumentLowering::Mktime { utc };
    semantics
}

builtin! {
    contract: "mktime",
    semantics: mktime_semantics(false),
}
