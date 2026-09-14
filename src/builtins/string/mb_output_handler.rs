//! Purpose:
//! Binds mb_output_handler to the shared argument, response, and output conversion contracts.
//!
//! Called from:
//! - The compiler builtin registry and typed EIR call lowering.
//!
//! Key details:
//! - The shared engine owns phase handling, codec state, MIME selection, and protected headers.

builtin! {
    contract: "mb_output_handler",
    check: crate::builtins::mbstring::check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbOutputHandler),
}
