//! Purpose:
//! Home of PHP's `unpack` builtin and its literal integer-format lowering contract.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The current AOT lowering supports literal `C`, `n`, `N`, and `V` integer formats and returns
//!   a key-preserving associative array or `false` when the input cannot satisfy the format.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "unpack",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Unpack,
    ),
}

/// Returns the associative integer-array or false union produced by supported formats.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Union(vec![
        PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Int),
        },
        PhpType::False,
    ]))
}
