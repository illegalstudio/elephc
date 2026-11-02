//! Purpose:
//! Home of the PHP `spl_object_id` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook accepts definite objects and gradual values that may hold an object; returns
//!   `Int`. Gradual values keep PHP's runtime `TypeError` boundary in EIR lowering.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "spl_object_id",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::SplObjectId,
    ),
}

/// Validates that the argument is an object-capable value and returns `Int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !object_capable_argument(&ty) {
        return Err(CompileError::new(
            cx.span,
            "spl_object_id() argument must be an object",
        ));
    }
    Ok(PhpType::Int)
}

/// Returns whether a static type is an object or a gradual value that may contain one.
fn object_capable_argument(ty: &PhpType) -> bool {
    match ty {
        // A closure is an object in PHP; the lowering gives its descriptor an identity.
        PhpType::Object(_) | PhpType::Mixed | PhpType::Callable => true,
        PhpType::Union(members) => members.iter().any(object_capable_argument),
        _ => false,
    }
}
