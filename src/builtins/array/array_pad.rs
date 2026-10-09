//! Purpose:
//! Home of the PHP `array_pad` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` keeps the typed rule where the typed helper applies: a list whose elements sit in
//!   8-byte slots, padded with a value of the same type, keeps its own type. Every other array
//!   (string or boxed elements, a pad value of another type, an associative, declared or boxed
//!   array) answers the boxed PHP array type, built by the boxed helper with php's keys: integer
//!   keys renumbered, string keys kept. A check hook is required both to reject a non-array first
//!   argument and to choose between the two.
//! - Arity (exactly 3 arguments) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::checker::builtins::arrays::boxed_value_may_hold_array;
use crate::types::PhpType;

builtin! {
    contract: "array_pad",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayPad,
    ),
}

/// Returns the result type of an `array_pad` call.
///
/// A list the typed helpers can pad keeps its type: elements in a layout `__rt_array_pad`,
/// `__rt_array_pad_str` or `__rt_array_pad_refcounted` stores, padded with a value of the
/// element's own type. The empty `[]` placeholder (element type `Void`) holds nothing but pad
/// copies once padded, so it takes the pad value's type when a typed helper can store it: typing
/// the result `array<never>` would make every read answer the missing-element sentinel. Any other
/// array is the boxed PHP array type, built by the boxed helper with php's keys. A non-array first
/// argument is rejected. The arguments are re-inferred here; the registry already inferred every
/// argument once for side effects, and arity (exactly 3) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let value_ty = cx.checker.infer_type(&cx.args[2], cx.env)?;
    if let PhpType::Array(elem) = &ty {
        if elem.codegen_repr() == PhpType::Void && is_indexed_pad_element_type(&value_ty) {
            return Ok(PhpType::Array(Box::new(value_ty)));
        }
        if is_indexed_pad_element_type(elem) && value_ty.codegen_repr() == elem.codegen_repr() {
            return Ok(ty);
        }
    }
    if matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        || ty.is_php_array()
        || boxed_value_may_hold_array(&ty)
    {
        return Ok(PhpType::php_array());
    }
    Err(CompileError::new(
        cx.span,
        "array_pad() first argument must be array",
    ))
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
