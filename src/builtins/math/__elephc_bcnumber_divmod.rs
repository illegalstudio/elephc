//! Purpose:
//! Binds the internal immutable Number divmod decimal operation.
//!
//! Called from:
//! - The shared builtin registry and generated Number methods.
//!
//! Key details:
//! - The fresh result stores two strings, including the automatically scaled remainder.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "__elephc_bcnumber_divmod",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::BcNumberDivmod,
    ),
}

/// Preserves the element representation returned by the shared divmod ABI shape.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}
