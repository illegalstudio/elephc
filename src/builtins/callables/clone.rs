//! Purpose:
//! Home of PHP 8.5's `clone()` function and its object-preserving checker contract.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The function form shares shallow-clone semantics with unary `clone`, then applies
//!   `withProperties` after the clone hook.
//! - A concrete input class remains concrete in the result type. Runtime-shaped object
//!   values stay `Mixed` until the typed runtime operation validates their tag.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "clone",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CloneWith,
    ),
}

/// Validates the object and override-array operands while preserving a known class.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let object_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(
        object_ty.codegen_repr(),
        PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)
    ) {
        return Err(CompileError::new(
            cx.span,
            "clone() argument #1 must be an object",
        ));
    }

    if let Some(properties) = cx.args.get(1) {
        // A property WITHOUT a declared type is `mixed` in PHP, so an override can hand it any
        // value. The applicator is synthesized after checking and can therefore not widen the
        // slot the way an ordinary `$o->p = $value;` does; record the destination here so
        // `clone_override_storage` can do it once every body has been checked.
        cx.checker
            .clone_override_destinations
            .record(&object_ty);
        let properties_ty = cx.checker.infer_type(properties, cx.env)?;
        if !matches!(
            properties_ty.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_)
        ) {
            return Err(CompileError::new(
                cx.span,
                "clone() argument #2 must be an array",
            ));
        }
    }

    Ok(match object_ty.codegen_repr() {
        PhpType::Object(class) => PhpType::Object(class.clone()),
        _ => PhpType::Mixed,
    })
}
