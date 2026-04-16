use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::{abi, platform::Arch};
use super::target::{
    emit_float_compare, emit_pop_left_float_for_comparison, emit_promote_int_to_float,
    emit_set_bool_from_flags, emit_set_float_bool_from_flags,
};
use super::super::{coerce_null_to_zero, emit_expr, BinOp, Expr, PhpType};

/// PHP loose comparison coerces both sides to a common type.
/// Simplified: coerce everything to int, then compare.
fn coerce_to_int_for_loose_cmp(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Void => {
            emitter.instruction("mov x0, #0");
        }
        PhpType::Bool => {}
        PhpType::Int => {
            super::super::coerce_null_to_zero(emitter, ty);
        }
        PhpType::Float => {
            emitter.instruction("fcvtzs x0, d0");
        }
        PhpType::Str => {
            abi::emit_call_label(emitter, "__rt_atoi");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_int");
        }
        _ => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        }
    }
}

pub(super) fn emit_loose_equality_binop(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let left_ty = emit_expr(left, emitter, ctx, data);
    let left_numeric = matches!(
        left_ty,
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
    );
    coerce_null_to_zero(emitter, &left_ty);
    let use_float = left_ty == PhpType::Float;
    if use_float {
        abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));
    } else {
        if !left_numeric {
            coerce_to_int_for_loose_cmp(emitter, &left_ty);
        }
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    }
    let right_ty = emit_expr(right, emitter, ctx, data);
    let right_numeric = matches!(
        right_ty,
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
    );
    coerce_null_to_zero(emitter, &right_ty);

    if left_numeric && right_numeric && (left_ty == PhpType::Float || right_ty == PhpType::Float) {
        if right_ty != PhpType::Float {
            emit_promote_int_to_float(
                emitter,
                abi::float_result_reg(emitter),
                abi::int_result_reg(emitter),
            );
        }
        emit_pop_left_float_for_comparison(emitter, &left_ty);
        emit_float_compare(emitter);
    } else {
        if !right_numeric {
            coerce_to_int_for_loose_cmp(emitter, &right_ty);
        }
        let left_reg = match emitter.target.arch {
            Arch::AArch64 => "x1",
            Arch::X86_64 => "r10",
        };
        abi::emit_pop_reg(emitter, left_reg);
        emitter.instruction(&format!("cmp {}, {}", left_reg, abi::int_result_reg(emitter)));
    }
    let cond = match op {
        BinOp::Eq => "eq",
        BinOp::NotEq => "ne",
        _ => unreachable!(),
    };
    if left_numeric && right_numeric && (left_ty == PhpType::Float || right_ty == PhpType::Float) {
        emit_set_float_bool_from_flags(emitter, cond);
    } else {
        emit_set_bool_from_flags(emitter, cond);
    }
    PhpType::Bool
}

pub(super) fn emit_order_compare_binop(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let left_ty = emit_expr(left, emitter, ctx, data);
    coerce_null_to_zero(emitter, &left_ty);
    let use_float = left_ty == PhpType::Float;
    if use_float {
        abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));
    } else {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    }
    let right_ty = emit_expr(right, emitter, ctx, data);
    coerce_null_to_zero(emitter, &right_ty);

    if left_ty == PhpType::Float || right_ty == PhpType::Float {
        if right_ty != PhpType::Float {
            emit_promote_int_to_float(
                emitter,
                abi::float_result_reg(emitter),
                abi::int_result_reg(emitter),
            );
        }
        emit_pop_left_float_for_comparison(emitter, &left_ty);
        emit_float_compare(emitter);
    } else {
        let left_reg = match emitter.target.arch {
            Arch::AArch64 => "x1",
            Arch::X86_64 => "r10",
        };
        abi::emit_pop_reg(emitter, left_reg);
        emitter.instruction(&format!("cmp {}, {}", left_reg, abi::int_result_reg(emitter)));
    }
    let cond = match op {
        BinOp::Lt => "lt",
        BinOp::Gt => "gt",
        BinOp::LtEq => "le",
        BinOp::GtEq => "ge",
        _ => unreachable!(),
    };
    if left_ty == PhpType::Float || right_ty == PhpType::Float {
        emit_set_float_bool_from_flags(emitter, cond);
    } else {
        emit_set_bool_from_flags(emitter, cond);
    }
    PhpType::Bool
}

pub(super) fn emit_spaceship_binop(
    left: &Expr,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let left_ty = emit_expr(left, emitter, ctx, data);
    coerce_null_to_zero(emitter, &left_ty);
    let use_float = left_ty == PhpType::Float;
    if use_float {
        abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));
    } else {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    }
    let right_ty = emit_expr(right, emitter, ctx, data);
    coerce_null_to_zero(emitter, &right_ty);

    if left_ty == PhpType::Float || right_ty == PhpType::Float {
        if right_ty != PhpType::Float {
            emit_promote_int_to_float(
                emitter,
                abi::float_result_reg(emitter),
                abi::int_result_reg(emitter),
            );
        }
        emit_pop_left_float_for_comparison(emitter, &left_ty);
        emit_float_compare(emitter);
    } else {
        let left_reg = match emitter.target.arch {
            Arch::AArch64 => "x1",
            Arch::X86_64 => "r10",
        };
        abi::emit_pop_reg(emitter, left_reg);
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("cmp x1, x0"),
            Arch::X86_64 => emitter.instruction(&format!(
                "cmp {}, {}",
                left_reg,
                abi::int_result_reg(emitter)
            )),
        }
    }

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cset x0, gt");
            emitter.instruction("csinv x0, x0, xzr, ge");
        }
        Arch::X86_64 => {
            let greater_label = ctx.next_label("spaceship_gt");
            let less_label = ctx.next_label("spaceship_lt");
            let done_label = ctx.next_label("spaceship_done");
            if left_ty == PhpType::Float || right_ty == PhpType::Float {
                emitter.instruction(&format!("ja {}", greater_label));
                emitter.instruction(&format!("jb {}", less_label));
            } else {
                emitter.instruction(&format!("jg {}", greater_label));
                emitter.instruction(&format!("jl {}", less_label));
            }
            emitter.instruction("mov rax, 0");
            emitter.instruction(&format!("jmp {}", done_label));
            emitter.label(&greater_label);
            emitter.instruction("mov rax, 1");
            emitter.instruction(&format!("jmp {}", done_label));
            emitter.label(&less_label);
            emitter.instruction("mov rax, -1");
            emitter.label(&done_label);
        }
    }
    PhpType::Int
}
