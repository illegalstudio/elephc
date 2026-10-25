//! Purpose:
//! Declares the internal tokenizer ParseError location setter.
//!
//! Called from:
//! - The tokenizer prelude before throwing an invalid scanned source.
//!
//! Key details:
//! - Typed runtime lowering preserves the shared Throwable layout on every target.

builtin! {
    contract: "__elephc_tokenizer_error_location",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::TokenizerErrorLocation,
    ),
}
