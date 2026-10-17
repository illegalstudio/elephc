//! Purpose:
//! Home of the PHP `array_pad` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy rule: padding preserves the array shape, so the
//!   return type is the (array-or-assoc) first-argument type unchanged. A check hook is
//!   required both to reject a non-array first argument and to echo its type back.
//! - Arity (exactly 3 arguments) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_pad",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayPad,
    ),
}

/// Returns the (shape-preserving) array type for an `array_pad` call.
///
/// Padding keeps the array shape, so the first-argument array/assoc type is returned
/// unchanged. A non-array first argument is rejected. The first argument is re-inferred
/// here; the registry already inferred every argument once for side effects, and arity
/// (exactly 3) is pre-validated by the registry.
///
/// The empty `[]` placeholder (element type `Void`) holds nothing but pad copies once padded,
/// so it takes the pad value's element type when an indexed pad helper can store it: typing
/// the result `array<never>` would make every read answer the missing-element sentinel.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_pad() first argument must be array",
        ));
    }
    if let PhpType::Array(elem) = &ty {
        if elem.codegen_repr() == PhpType::Void {
            let pad_ty = cx.checker.infer_type(&cx.args[2], cx.env)?;
            if is_indexed_pad_element_type(&pad_ty) {
                return Ok(PhpType::Array(Box::new(pad_ty)));
            }
        }
    }
    Ok(ty)
}

/// Returns true for pad value types an indexed pad runtime helper stores: the scalar 8-byte
/// layouts (`__rt_array_pad`), 16-byte string pairs (`__rt_array_pad_str`, issue #675), and
/// every refcounted payload (`__rt_array_pad_refcounted`).
fn is_indexed_pad_element_type(ty: &PhpType) -> bool {
    let repr = ty.codegen_repr();
    matches!(
        repr,
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Callable | PhpType::Str
    ) || repr.is_refcounted()
}
