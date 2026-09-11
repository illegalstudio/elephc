//! Purpose:
//! Binds mb_parse_str to shared query parsing and native output-reference publication.
//!
//! Called from:
//! - Compiler builtin checking and typed EIR runtime-call lowering.
//!
//! Key details:
//! - The neutral contract owns arity, the write-only result parameter, and the boolean return.
//! - The V5 host supplies live Core configuration and preserves the output reference identity.

builtin! {
    contract: "mb_parse_str",
    check: crate::builtins::mbstring::capture_check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbParseStr),
}
