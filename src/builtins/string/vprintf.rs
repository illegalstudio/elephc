//! Purpose:
//! Home of the PHP `vprintf` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts a required `format` string and a `values` array.


builtin! {
    contract: "vprintf",
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::runtime_fn_semantics(
            crate::ir::RuntimeFnId::Vprintf,
        ),
        crate::builtins::semantics::BuiltinArgumentLowering::BareArrayValues {
            arg: 1,
            sole: false,
        },
    ),
}
