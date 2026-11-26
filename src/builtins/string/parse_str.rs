//! Purpose:
//! Binds the internal byte-preserving parser used by the `parse_str()` prelude.
//!
//! Called from:
//! - Compiler builtin checking and typed EIR runtime-call lowering.
//!
//! Key details:
//! - The neutral contract owns arity and the write-only result parameter.
//! - The bridge reuses the mbstring query engine without encoding conversion.

builtin! {
    contract: "__elephc_parse_str",
    check: crate::builtins::mbstring::capture_check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::ParseStr),
}
