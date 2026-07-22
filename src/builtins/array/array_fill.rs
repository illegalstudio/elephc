//! Purpose:
//! Home of the PHP `array_fill` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` computes the actual return type based on the `start_index` argument:
//!   a literal-zero start produces an indexed array; any other start produces an
//!   associative array with Int keys and Mixed values.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "array_fill",
    area: Array,
    params: [start_index: Mixed, count: Mixed, value: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayFill,
    ),
    summary: "Fill an array with values.",
    php_manual: "https://www.php.net/manual/en/function.array-fill.php",
}

/// Computes the return array type based on whether `start_index` is a literal zero.
///
/// The registry's `check_arity` handles arity enforcement (exactly 3 arguments).
/// A non-literal-zero start builds a keyed assoc array (Int → Mixed); a literal-zero
/// start builds an indexed array preserving the value type. This mirrors the codegen
/// emitter's branch logic so static types stay consistent with runtime behavior.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.infer_type(&cx.args[1], cx.env)?;
    let val_ty = cx.checker.infer_type(&cx.args[2], cx.env)?;
    // `Void` is also the storage marker used for an empty indexed-array element,
    // so an `array<void>` result would let a later append replace the null element
    // type instead of widening the existing payload. Null fills therefore use
    // boxed Mixed slots, which preserve the stored null across later writes.
    let val_ty = if val_ty.codegen_repr() == PhpType::Void {
        PhpType::Mixed
    } else {
        val_ty
    };
    let start_is_literal_zero =
        matches!(cx.args[0].kind, crate::parser::ast::ExprKind::IntLiteral(0));
    if !start_is_literal_zero {
        Ok(PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Mixed),
        })
    } else {
        Ok(PhpType::Array(Box::new(val_ty)))
    }
}
