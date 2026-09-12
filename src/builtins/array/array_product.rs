//! Purpose:
//! Home of the PHP `array_product` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Integer overflow and mixed source values can select an integer or float result at runtime.
//! - The backend returns an independently owned numeric Mixed cell on every call surface.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_product",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayProduct,
    ),
}

/// Accepts concrete or dynamic arrays and returns boxed numeric storage for scalar coercion boundaries.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if ty.is_php_array() || matches!(ty.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
    ) {
        Ok(PhpType::Mixed)
    } else {
        Err(CompileError::new(cx.span, "array_product() argument must be array"))
    }
}
