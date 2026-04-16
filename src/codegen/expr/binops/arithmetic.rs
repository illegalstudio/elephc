use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::{abi, platform::Arch};
use super::target::{
    emit_float_binop, emit_promote_int_to_float, emit_set_bool_from_flags,
};
use super::super::{
    coerce_null_to_zero, coerce_to_string, coerce_to_truthiness, emit_expr,
    expr_result_heap_ownership, BinOp, Expr, HeapOwnership, PhpType,
};

pub(super) fn emit_logical_binop(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    match op {
        BinOp::And => {
            let end_label = ctx.next_label("and_end");
            let left_ty = emit_expr(left, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &left_ty);
            abi::emit_branch_if_int_result_zero(emitter, &end_label);
            let right_ty = emit_expr(right, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &right_ty);
            match emitter.target.arch {
                Arch::AArch64 => emitter.instruction("cmp x0, #0"),
                Arch::X86_64 => emitter.instruction("test rax, rax"),
            }
            emit_set_bool_from_flags(emitter, "ne");
            emitter.label(&end_label);
            PhpType::Bool
        }
        BinOp::Or => {
            let end_label = ctx.next_label("or_end");
            let left_ty = emit_expr(left, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &left_ty);
            abi::emit_branch_if_int_result_nonzero(emitter, &end_label);
            let right_ty = emit_expr(right, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &right_ty);
            emitter.label(&end_label);
            match emitter.target.arch {
                Arch::AArch64 => emitter.instruction("cmp x0, #0"),
                Arch::X86_64 => emitter.instruction("test rax, rax"),
            }
            emit_set_bool_from_flags(emitter, "ne");
            PhpType::Bool
        }
        _ => unreachable!(),
    }
}

pub(super) fn emit_pow_binop(
    left: &Expr,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let left_ty = emit_expr(left, emitter, ctx, data);
    coerce_null_to_zero(emitter, &left_ty);
    if left_ty != PhpType::Float {
        emit_promote_int_to_float(
            emitter,
            abi::float_result_reg(emitter),
            abi::int_result_reg(emitter),
        );
    }
    abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));
    let right_ty = emit_expr(right, emitter, ctx, data);
    coerce_null_to_zero(emitter, &right_ty);
    if right_ty != PhpType::Float {
        emit_promote_int_to_float(
            emitter,
            abi::float_result_reg(emitter),
            abi::int_result_reg(emitter),
        );
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fmov d1, d0");
            abi::emit_pop_float_reg(emitter, "d0");
            emitter.bl_c("pow");
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(emitter, "xmm1");
            emitter.instruction("movapd xmm2, xmm0");
            emitter.instruction("movapd xmm0, xmm1");
            emitter.instruction("movapd xmm1, xmm2");
            emitter.instruction("call pow");
        }
    }
    PhpType::Float
}

pub(super) fn emit_numeric_binop(
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

    if left_ty == PhpType::Float || right_ty == PhpType::Float || *op == BinOp::Div {
        if right_ty != PhpType::Float {
            emit_promote_int_to_float(
                emitter,
                abi::float_result_reg(emitter),
                abi::int_result_reg(emitter),
            );
        }
        abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));
        if left_ty == PhpType::Float {
            let left_float_reg = match emitter.target.arch {
                Arch::AArch64 => "d1",
                Arch::X86_64 => "xmm1",
            };
            abi::emit_load_temporary_stack_slot(emitter, left_float_reg, 16);
        } else {
            let left_int_reg = abi::symbol_scratch_reg(emitter);
            let left_float_reg = match emitter.target.arch {
                Arch::AArch64 => "d1",
                Arch::X86_64 => "xmm1",
            };
            abi::emit_load_temporary_stack_slot(emitter, left_int_reg, 16);
            emit_promote_int_to_float(emitter, left_float_reg, left_int_reg);
        }
        abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));
        emit_float_binop(emitter, op);
        abi::emit_release_temporary_stack(emitter, 16);
        PhpType::Float
    } else {
        let left_reg = match emitter.target.arch {
            Arch::AArch64 => "x1",
            Arch::X86_64 => "r10",
        };
        let result_reg = abi::int_result_reg(emitter);
        abi::emit_pop_reg(emitter, left_reg);
        match op {
            BinOp::Add => match emitter.target.arch {
                Arch::AArch64 => emitter.instruction("add x0, x1, x0"),
                Arch::X86_64 => {
                    emitter.instruction(&format!("add {}, {}", left_reg, result_reg));
                    emitter.instruction(&format!("mov {}, {}", result_reg, left_reg));
                }
            },
            BinOp::Sub => match emitter.target.arch {
                Arch::AArch64 => emitter.instruction("sub x0, x1, x0"),
                Arch::X86_64 => {
                    emitter.instruction(&format!("sub {}, {}", left_reg, result_reg));
                    emitter.instruction(&format!("mov {}, {}", result_reg, left_reg));
                }
            },
            BinOp::Mul => match emitter.target.arch {
                Arch::AArch64 => emitter.instruction("mul x0, x1, x0"),
                Arch::X86_64 => {
                    emitter.instruction(&format!("imul {}, {}", left_reg, result_reg));
                    emitter.instruction(&format!("mov {}, {}", result_reg, left_reg));
                }
            },
            BinOp::Div => {
                emitter.instruction("sdiv x0, x1, x0");
            }
            BinOp::Mod => emit_int_mod(emitter, ctx, left_reg, result_reg),
            _ => unreachable!(),
        }
        PhpType::Int
    }
}

pub(super) fn emit_concat_binop(
    left: &Expr,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let left_ty = emit_expr(left, emitter, ctx, data);
    coerce_to_string(emitter, ctx, data, &left_ty);
    if expr_result_heap_ownership(left) == HeapOwnership::NonHeap {
        abi::emit_call_label(emitter, "__rt_str_persist");
    }
    let (left_ptr_reg, left_len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, left_ptr_reg, left_len_reg);
    let right_ty = emit_expr(right, emitter, ctx, data);
    coerce_to_string(emitter, ctx, data, &right_ty);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x3, x1");
            emitter.instruction("mov x4, x2");
            abi::emit_pop_reg_pair(emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");
            emitter.instruction("mov rsi, rdx");
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(emitter, "__rt_concat");
    PhpType::Str
}

pub(super) fn emit_bitwise_binop(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let left_ty = emit_expr(left, emitter, ctx, data);
    coerce_null_to_zero(emitter, &left_ty);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    let right_ty = emit_expr(right, emitter, ctx, data);
    coerce_null_to_zero(emitter, &right_ty);
    let left_reg = match emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "r10",
    };
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_pop_reg(emitter, left_reg);
    match op {
        BinOp::BitAnd => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("and x0, x1, x0"),
            Arch::X86_64 => {
                emitter.instruction(&format!("and {}, {}", left_reg, result_reg));
                emitter.instruction(&format!("mov {}, {}", result_reg, left_reg));
            }
        },
        BinOp::BitOr => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("orr x0, x1, x0"),
            Arch::X86_64 => {
                emitter.instruction(&format!("or {}, {}", left_reg, result_reg));
                emitter.instruction(&format!("mov {}, {}", result_reg, left_reg));
            }
        },
        BinOp::BitXor => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("eor x0, x1, x0"),
            Arch::X86_64 => {
                emitter.instruction(&format!("xor {}, {}", left_reg, result_reg));
                emitter.instruction(&format!("mov {}, {}", result_reg, left_reg));
            }
        },
        BinOp::ShiftLeft => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("lsl x0, x1, x0"),
            Arch::X86_64 => {
                emitter.instruction("mov rcx, rax");
                emitter.instruction(&format!("shl {}, cl", left_reg));
                emitter.instruction(&format!("mov {}, {}", result_reg, left_reg));
            }
        },
        BinOp::ShiftRight => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("asr x0, x1, x0"),
            Arch::X86_64 => {
                emitter.instruction("mov rcx, rax");
                emitter.instruction(&format!("sar {}, cl", left_reg));
                emitter.instruction(&format!("mov {}, {}", result_reg, left_reg));
            }
        },
        _ => unreachable!(),
    }
    PhpType::Int
}

fn emit_int_mod(emitter: &mut Emitter, ctx: &mut Context, left_reg: &str, result_reg: &str) {
    let skip = ctx.next_label("mod_ok");
    let zero = ctx.next_label("mod_zero");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {zero}"));
            emitter.instruction("sdiv x2, x1, x0");
            emitter.instruction("msub x0, x2, x0, x1");
            emitter.instruction(&format!("b {skip}"));
            emitter.label(&zero);
            emitter.instruction("mov x0, #0");
            emitter.label(&skip);
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", result_reg, result_reg));
            emitter.instruction(&format!("je {}", zero));
            emitter.instruction(&format!("mov r11, {}", result_reg));
            emitter.instruction(&format!("mov {}, {}", result_reg, left_reg));
            emitter.instruction("cqo");
            emitter.instruction("idiv r11");
            emitter.instruction(&format!("mov {}, rdx", result_reg));
            emitter.instruction(&format!("jmp {}", skip));
            emitter.label(&zero);
            emitter.instruction(&format!("mov {}, 0", result_reg));
            emitter.label(&skip);
        }
    }
}
