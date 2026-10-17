//! Purpose:
//! Binds the internal immutable Number ceil decimal operation.
//!
//! Called from:
//! - The shared builtin registry and generated Number methods.
//!
//! Key details:
//! - The Number engine determines scale independently of bcscale().

builtin! {
    contract: "__elephc_bcnumber_ceil",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::BcNumberCeil,
    ),
}
