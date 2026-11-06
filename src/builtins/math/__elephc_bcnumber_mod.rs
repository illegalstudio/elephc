//! Purpose:
//! Binds the internal immutable Number mod decimal operation.
//!
//! Called from:
//! - The shared builtin registry and generated Number methods.
//!
//! Key details:
//! - The Number engine determines scale independently of bcscale().

builtin! {
    contract: "__elephc_bcnumber_mod",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::BcNumberMod,
    ),
}
