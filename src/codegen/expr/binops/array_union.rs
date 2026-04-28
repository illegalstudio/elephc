use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::{abi, platform::Arch};
use super::super::{emit_expr, Expr, ExprKind, PhpType};

pub(super) fn is_array_union_candidate(left: &Expr, right: &Expr, ctx: &Context) -> bool {
    matches!(
        (
            super::super::super::functions::infer_contextual_type(left, ctx),
            super::super::super::functions::infer_contextual_type(right, ctx),
        ),
        (PhpType::Array(_), PhpType::Array(_))
            | (PhpType::AssocArray { .. }, PhpType::AssocArray { .. })
    )
}

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
    let use_hash_union = matches!(left_static_ty, PhpType::AssocArray { .. });

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

    if use_hash_union {
        abi::emit_call_label(emitter, "__rt_hash_union");                       // compute PHP associative-array union with left-key precedence
    } else {
        abi::emit_call_label(emitter, "__rt_array_union");                      // compute PHP indexed-array union with numeric-key precedence
    }

    result_ty
}

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
        _ => left.clone(),
    }
}

fn is_empty_indexed_array_literal(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::ArrayLiteral(elems) if elems.is_empty())
}
