//! Purpose:
//! Lowers PHP array union expressions and optimized empty-array cases.
//! Keeps operator-specific conversions and result register setup out of the dispatcher.
//!
//! Called from:
//! - `crate::codegen_support::expr::binops`
//!
//! Key details:
//! - Runtime calls and target instructions must preserve left/right evaluation order and scratch register assumptions.

use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::{abi, platform::Arch};
use super::super::{emit_expr, Expr, ExprKind, PhpType};

/// Returns true if both operands are array-like types that benefit from the
/// specialized array-union codepath (rather than the general binary operator dispatch).
///
/// Matches `PhpType::Array` and `PhpType::AssocArray` in all four pairwise combinations.
/// The `ctx` parameter provides contextual type information for both operands.
pub(super) fn is_array_union_candidate(left: &Expr, right: &Expr, ctx: &Context) -> bool {
    matches!(
        (
            super::super::super::functions::infer_contextual_type(left, ctx),
            super::super::super::functions::infer_contextual_type(right, ctx),
        ),
        (PhpType::Array(_), PhpType::Array(_))
            | (PhpType::AssocArray { .. }, PhpType::AssocArray { .. })
            | (PhpType::Array(_), PhpType::AssocArray { .. })
            | (PhpType::AssocArray { .. }, PhpType::Array(_))
    )
}

/// Lowers the `+` array union operator.
///
/// Saves the left array pointer before evaluating the right operand, then restores
/// it as the first argument to the runtime helper. The runtime helper receives arguments
/// in platform ABI order (x0/x1 on ARM64, rdi/rsi on x86_64) and returns the union result
/// in the integer result register.
///
/// # Arguments
/// * `left` - Left operand expression (evaluated first)
/// * `right` - Right operand expression (evaluated second)
/// * `emitter` - Code emitter
/// * `ctx` - Codegen context (carries variable layout, class metadata)
/// * `data` - Read-only data section for constants
///
/// # Returns
/// The `PhpType` of the union result, derived from the static types of both operands.
pub(super) fn emit_array_union_binop(
    left: &Expr,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let left_static_ty = emit_expr(left, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // save the left array pointer while evaluating the right operand
    let right_static_ty = emit_expr(right, emitter, ctx, data);
    let result_ty = array_union_result_type(left, &left_static_ty, right, &right_static_ty);
    let runtime_helper = match (&left_static_ty, &right_static_ty) {
        (PhpType::Array(_), PhpType::Array(_)) => "__rt_array_union",
        (PhpType::AssocArray { .. }, PhpType::AssocArray { .. }) => "__rt_hash_union",
        (PhpType::Array(_), PhpType::AssocArray { .. }) => "__rt_array_hash_union",
        (PhpType::AssocArray { .. }, PhpType::Array(_)) => "__rt_hash_array_union",
        _ => "__rt_array_union",
    };

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // pass the right array pointer as the second runtime argument
            abi::emit_pop_reg(emitter, "x0");                                   // restore the left array pointer as the first runtime argument
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // pass the right array pointer as the second runtime argument
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the left array pointer as the first runtime argument
        }
    }

    abi::emit_call_label(emitter, runtime_helper);                              // compute PHP array union with left-key precedence for the active storage pair

    result_ty
}

/// Determines the `PhpType` of an array union result from the static types of both operands.
///
/// This function applies PHP's type inference rules for the `+` operator:
/// - Empty array operand is discarded (left or right)
/// - Matching element types are preserved when both are homogeneous indexed arrays
/// - Mixed indexed/associative unions produce `AssocArray` with merged key/value types
/// - Dissimilar value types collapse to `Mixed`
///
/// # Arguments
/// * `left_expr` - Left operand expression (used only to detect empty array literals)
/// * `left` - Static `PhpType` of the left operand
/// * `right_expr` - Right operand expression (used only to detect empty array literals)
/// * `right` - Static `PhpType` of the right operand
///
/// # Returns
/// The inferred `PhpType` for the union expression.
fn array_union_result_type(
    left_expr: &Expr,
    left: &PhpType,
    right_expr: &Expr,
    right: &PhpType,
) -> PhpType {
    match (left, right) {
        (PhpType::Array(_), PhpType::Array(_)) if is_empty_indexed_array_literal(left_expr) => {
            right.clone()
        }
        (PhpType::Array(_), PhpType::Array(_)) if is_empty_indexed_array_literal(right_expr) => {
            left.clone()
        }
        (PhpType::Array(left_elem), PhpType::Array(right_elem)) if left_elem == right_elem => {
            PhpType::Array(left_elem.clone())
        }
        (PhpType::Array(left_elem), PhpType::Array(_)) => PhpType::Array(left_elem.clone()),
        (
            PhpType::AssocArray {
                key: left_key,
                value: left_value,
            },
            PhpType::AssocArray {
                key: right_key,
                value: right_value,
            },
        ) => {
            let key = if left_key == right_key {
                left_key.clone()
            } else {
                Box::new(PhpType::Mixed)
            };
            let value = if left_value == right_value {
                left_value.clone()
            } else {
                Box::new(PhpType::Mixed)
            };
            PhpType::AssocArray { key, value }
        }
        (PhpType::Array(left_elem), PhpType::AssocArray { key, value }) => PhpType::AssocArray {
            key: Box::new(merge_array_union_key_with_indexed(key)),
            value: Box::new(merge_array_union_value_types(left_elem, value)),
        },
        (PhpType::AssocArray { key, value }, PhpType::Array(right_elem)) => PhpType::AssocArray {
            key: Box::new(merge_array_union_key_with_indexed(key)),
            value: Box::new(merge_array_union_value_types(value, right_elem)),
        },
        _ => left.clone(),
    }
}

/// Returns true if the expression is an empty indexed array literal (`[]`).
///
/// Used by `array_union_result_type` to apply the empty-array optimization where the
/// non-empty operand's type becomes the result type.
fn is_empty_indexed_array_literal(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::ArrayLiteral(elems) if elems.is_empty())
}

/// Merges the key type of an associative array with an indexed array in a union.
///
/// In PHP array union, indexed arrays use integer keys. When an `AssocArray` with a
/// known `Int` key type is unioned with an indexed array, the key type remains `Int`;
/// otherwise it becomes `Mixed` since the union result could have non-integer keys.
fn merge_array_union_key_with_indexed(key: &PhpType) -> PhpType {
    if matches!(key, PhpType::Int) {
        PhpType::Int
    } else {
        PhpType::Mixed
    }
}

/// Merges the value types of two array operands in a union.
///
/// Returns the left type if both types match. If one side is `Never` (unreachable),
/// returns the other type. Otherwise collapses to `Mixed` since PHP arrays are
/// heterogeneous and a union can introduce values of different types.
fn merge_array_union_value_types(left: &PhpType, right: &PhpType) -> PhpType {
    if left == right {
        left.clone()
    } else if matches!(left, PhpType::Never) {
        right.clone()
    } else if matches!(right, PhpType::Never) {
        left.clone()
    } else {
        PhpType::Mixed
    }
}
