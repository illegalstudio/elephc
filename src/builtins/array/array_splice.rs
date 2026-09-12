//! Purpose:
//! Home of the PHP `array_splice` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The signature matches reference PHP 8.4 exactly:
//!   `array_splice(array &$array, int $offset, ?int $length = null, mixed $replacement = [])`.
//!   4 params, `array` by-ref, arity 2-4. The `ref` marker is mandatory because it makes
//!   by-reference mutation lower correctly (ir_lower reads `ref_params` from the registry sig).
//! - A declared PHP array preserves its non-null array contract while using boxed storage.
//!   Other `Mixed`/`Union` sources keep the opaque result; concrete arrays keep their type.
//!   All remaining arguments are inferred for side effects.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_splice",
    check: check,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::ArraySplice),
        crate::builtins::semantics::BuiltinArgumentLowering::ArraySplice,
    ),
}

/// Returns the result type for an `array_splice` call.
///
/// Arity (2 to 4 args) is pre-validated by the registry. The first argument is re-inferred
/// to drive the return type; remaining arguments are inferred for side effects. Declared PHP
/// arrays retain their boxed array contract. Other `Mixed`/`Union` arguments yield `Mixed`;
/// concrete arrays retain their type, and non-array arguments are rejected.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    for arg in &cx.args[1..] {
        cx.checker.infer_type(arg, cx.env)?;
    }
    if ty.is_php_array() {
        return Ok(ty);
    }
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        return Ok(PhpType::Mixed);
    }
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() first argument must be array", cx.name),
        ));
    }
    Ok(ty)
}
