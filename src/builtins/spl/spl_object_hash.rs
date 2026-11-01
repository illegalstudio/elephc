//! Purpose:
//! Home of the PHP `spl_object_hash` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook accepts definite objects and gradual values that may hold an object; returns
//!   `Str`. Gradual values keep PHP's runtime `TypeError` boundary in EIR lowering.
//! - The hash is derived from the object's heap pointer stringified via `__rt_itoa`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "spl_object_hash",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::SplObjectHash,
    ),
}

/// Validates that the argument is an object-capable value and returns `Str`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !object_capable_argument(&ty) {
        return Err(CompileError::new(
            cx.span,
            "spl_object_hash() argument must be an object",
        ));
    }
    Ok(PhpType::Str)
}

/// Returns whether a static type is an object or a gradual value that may contain one.
fn object_capable_argument(ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(_) | PhpType::Mixed => true,
        PhpType::Union(members) => members.iter().any(object_capable_argument),
        _ => false,
    }
}
