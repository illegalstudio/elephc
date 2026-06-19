//! Purpose:
//! Lowers numeric arithmetic and modulo operators with PHP-compatible coercions.
//! Keeps operator-specific conversions and result register setup out of the dispatcher.
//!
//! Called from:
//! - `crate::codegen::expr::binops`
//!
//! Key details:
//! - Runtime calls and target instructions must preserve left/right evaluation order and scratch register assumptions.

use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::{abi, platform::Arch};
use super::target::{
    emit_float_binop, emit_promote_int_to_float, emit_set_bool_from_flags,
};
use super::super::{
    coerce_null_to_zero, coerce_to_int, coerce_to_string_releasing_owned, coerce_to_truthiness,
    emit_expr,
    expr_result_heap_ownership, string_result_is_owned_call_temp,
    string_result_uses_transient_concat_buffer, BinOp, Expr, PhpType,
};
use crate::codegen::context::HeapOwnership;

/// Lowers &&, ||, xor logical operators.
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
                Arch::AArch64 => emitter.instruction("cmp x0, #0"),             // test whether right-operand truthiness is zero (false)
                Arch::X86_64 => emitter.instruction("test rax, rax"),           // test whether right-operand truthiness is zero (false)
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
                Arch::AArch64 => emitter.instruction("cmp x0, #0"),             // test whether right-operand truthiness is zero (false)
                Arch::X86_64 => emitter.instruction("test rax, rax"),           // test whether right-operand truthiness is zero (false)
            }
            emit_set_bool_from_flags(emitter, "ne");
            PhpType::Bool
        }
        BinOp::Xor => {
            let left_ty = emit_expr(left, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &left_ty);
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
            let right_ty = emit_expr(right, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &right_ty);
            match emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_pop_reg(emitter, "x9");
                    emitter.instruction("eor x0, x9, x0");                      // true when exactly one operand is truthy
                }
                Arch::X86_64 => {
                    abi::emit_pop_reg(emitter, "r10");
                    emitter.instruction("xor rax, r10");                        // true when exactly one operand is truthy
                }
            }
            PhpType::Bool
        }
        _ => unreachable!(),
    }
}

/// Lowers the ** exponentiation operator using libc pow.
pub(super) fn emit_pow_binop(
    left: &Expr,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let left_ty = emit_expr(left, emitter, ctx, data);
    coerce_to_int(emitter, &left_ty);
    if left_ty != PhpType::Float {
        emit_promote_int_to_float(
            emitter,
            abi::float_result_reg(emitter),
            abi::int_result_reg(emitter),
        );
    }
    abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));
    let right_ty = emit_expr(right, emitter, ctx, data);
    coerce_to_int(emitter, &right_ty);
    if right_ty != PhpType::Float {
        emit_promote_int_to_float(
            emitter,
            abi::float_result_reg(emitter),
            abi::int_result_reg(emitter),
        );
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fmov d1, d0");                                 // move right-operand float into d1 (second pow argument)
            abi::emit_pop_float_reg(emitter, "d0");
            emitter.bl_c("pow");
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(emitter, "xmm1");
            emitter.instruction("movapd xmm2, xmm0");                           // stash right-operand float before shuffling pow argument registers
            emitter.instruction("movapd xmm0, xmm1");                           // place left-operand float into xmm0 (first pow argument)
            emitter.instruction("movapd xmm1, xmm2");                           // place right-operand float into xmm1 (second pow argument)
            emitter.instruction("call pow");                                    // invoke libc pow(xmm0, xmm1); result returned in xmm0
        }
    }
    PhpType::Float
}

/// Lowers +, -, *, /, % operators with PHP-compatible numeric coercion.
pub(super) fn emit_numeric_binop(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let left_ty = emit_expr(left, emitter, ctx, data);
    let dynamic_candidate = matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul);
    let left_stack_ty = if dynamic_candidate && left_ty == PhpType::Void {
        coerce_null_to_zero(emitter, &left_ty);
        PhpType::Int
    } else if left_ty == PhpType::TaggedScalar {
        // narrow a tagged scalar (null -> 0) before the operand is spilled as one word
        coerce_null_to_zero(emitter, &left_ty);
        PhpType::Int
    } else if dynamic_candidate {
        left_ty.clone()
    } else {
        coerce_to_int(emitter, &left_ty);
        left_ty.clone()
    };
    let use_float = left_stack_ty == PhpType::Float;
    if use_float {
        abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));
    } else {
        abi::emit_push_result_value(emitter, &left_stack_ty);
    }
    let right_ty = emit_expr(right, emitter, ctx, data);

    if should_emit_mixed_numeric_binop(op, &left_stack_ty, &right_ty) {
        return emit_mixed_numeric_binop(
            left,
            op,
            &left_stack_ty,
            right,
            &right_ty,
            emitter,
        );
    }

    coerce_to_int(emitter, &right_ty);

    if left_stack_ty == PhpType::Float || right_ty == PhpType::Float || *op == BinOp::Div {
        if right_ty != PhpType::Float {
            emit_promote_int_to_float(
                emitter,
                abi::float_result_reg(emitter),
                abi::int_result_reg(emitter),
            );
        }
        abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));
        if left_stack_ty == PhpType::Float {
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
                Arch::AArch64 => emitter.instruction("add x0, x1, x0"),         // x0 = left (x1) + right (x0)
                Arch::X86_64 => {
                    emitter.instruction(&format!("add {}, {}", left_reg, result_reg)); // left_reg += result_reg (right operand)
                    emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move the sum back into the result register
                }
            },
            BinOp::Sub => match emitter.target.arch {
                Arch::AArch64 => emitter.instruction("sub x0, x1, x0"),         // x0 = left (x1) - right (x0)
                Arch::X86_64 => {
                    emitter.instruction(&format!("sub {}, {}", left_reg, result_reg)); // left_reg -= result_reg (right operand)
                    emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move the difference back into the result register
                }
            },
            BinOp::Mul => match emitter.target.arch {
                Arch::AArch64 => emitter.instruction("mul x0, x1, x0"),         // x0 = left (x1) * right (x0)
                Arch::X86_64 => {
                    emitter.instruction(&format!("imul {}, {}", left_reg, result_reg)); // left_reg *= result_reg (right operand)
                    emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move the product back into the result register
                }
            },
            BinOp::Div => {
                emitter.instruction("sdiv x0, x1, x0");                         // x0 = left (x1) / right (x0) (signed division)
            }
            BinOp::Mod => emit_int_mod(emitter, ctx, left_reg, result_reg),
            _ => unreachable!(),
        }
        PhpType::Int
    }
}

/// Returns true when the given operator and operand types require mixed-numeric binop emission.
/// Only Add, Sub, and Mul can operate on Mixed/Union types; integerish pairs also use this path
/// to bypass normal int/float coercions when both operands fit in integers.
fn should_emit_mixed_numeric_binop(op: &BinOp, left_ty: &PhpType, right_ty: &PhpType) -> bool {
    if !matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) {
        return false;
    }
    if matches!(left_ty, PhpType::Mixed | PhpType::Union(_))
        || matches!(right_ty, PhpType::Mixed | PhpType::Union(_))
    {
        return true;
    }
    is_integerish_numeric(left_ty) && is_integerish_numeric(right_ty)
}

/// Returns true for PhpType values that represent integer-compatible PHP types:
/// Int, Bool (coerced to int), and Void (null coerced to zero).
fn is_integerish_numeric(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Int | PhpType::Bool | PhpType::Void)
}

/// Emits a mixed-numeric add/sub/mul operation.
/// At least one operand is `PhpType::Mixed` or `PhpType::Union`; the other may be a concrete
/// integer type that was previously on the expression stack. Non-Mixed operands are boxed as
/// Mixed before the runtime helper is called. The result type is always `PhpType::Mixed`.
/// Ownership: owned operands are released after the call; borrowed or static operands are not.
fn emit_mixed_numeric_binop(
    left: &Expr,
    op: &BinOp,
    left_stack_ty: &PhpType,
    right: &Expr,
    right_ty: &PhpType,
    emitter: &mut Emitter,
) -> PhpType {
    let right_was_boxed = !matches!(right_ty, PhpType::Mixed | PhpType::Union(_));
    let left_was_boxed = !matches!(left_stack_ty, PhpType::Mixed | PhpType::Union(_));
    let release_left_operand =
        left_was_boxed || mixed_numeric_operand_is_owned(left, left_stack_ty);
    let release_right_operand =
        right_was_boxed || mixed_numeric_operand_is_owned(right, right_ty);
    if right_was_boxed {
        crate::codegen::emit_box_current_value_as_mixed(emitter, right_ty);
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    load_saved_numeric_operand(emitter, left_stack_ty, 16);
    if left_was_boxed {
        crate::codegen::emit_box_current_value_as_mixed(emitter, left_stack_ty);
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(emitter, "x1");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(emitter, "rdi");
        }
    }
    abi::emit_release_temporary_stack(emitter, 16);
    let helper = match op {
        BinOp::Add => "__rt_mixed_numeric_add",
        BinOp::Sub => "__rt_mixed_numeric_sub",
        BinOp::Mul => "__rt_mixed_numeric_mul",
        _ => unreachable!(),
    };
    if release_left_operand {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    }
    if release_right_operand {
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_push_reg(emitter, "x1");
            }
            Arch::X86_64 => {
                abi::emit_push_reg(emitter, "rdi");
            }
        }
    }
    abi::emit_call_label(emitter, helper);
    release_temporary_numeric_operands(emitter, release_left_operand, release_right_operand);
    PhpType::Mixed
}

/// Returns true when a Mixed or Union typed expression result is heap-allocated and owned
/// (not borrowed, not static, not a call temporary). Used to decide whether the operand
/// needs a reference-count release after the runtime numeric helper completes.
fn mixed_numeric_operand_is_owned(expr: &Expr, ty: &PhpType) -> bool {
    matches!(ty, PhpType::Mixed | PhpType::Union(_))
        && expr_result_heap_ownership(expr) == HeapOwnership::Owned
}

/// Decrements refcounts for owned left/right operands that were pushed onto the temporary
/// stack before calling a mixed-numeric runtime helper. Skips decrement for non-owned operands.
/// The result from the helper is preserved on the stack during cleanup to avoid clobbering.
fn release_temporary_numeric_operands(
    emitter: &mut Emitter,
    release_left_operand: bool,
    release_right_operand: bool,
) {
    if !release_left_operand && !release_right_operand {
        return;
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    let mut offset = 16;
    if release_right_operand {
        abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), offset);
        abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
        offset += 16;
    }
    if release_left_operand {
        abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), offset);
        abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
    }
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
    let operand_stack_bytes =
        16 * usize::from(release_left_operand) + 16 * usize::from(release_right_operand);
    abi::emit_release_temporary_stack(emitter, operand_stack_bytes);
}

/// Loads a previously saved numeric operand from a temporary stack slot at the given offset.
/// Handles Float (fp register), Str (ptr+len register pair), and integer types (single register).
/// Void/Never types (null) are a no-op since null contributes zero to numeric operations.
fn load_saved_numeric_operand(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_load_temporary_stack_slot(
                emitter,
                abi::float_result_reg(emitter),
                offset,
            );
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_load_temporary_stack_slot(emitter, ptr_reg, offset);
            abi::emit_load_temporary_stack_slot(emitter, len_reg, offset + 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), offset);
        }
    }
}

/// Lowers the . concatenation operator using __rt_concat.
pub(super) fn emit_concat_binop(
    left: &Expr,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let left_ty = emit_expr(left, emitter, ctx, data);
    coerce_to_string_releasing_owned(
        emitter,
        ctx,
        data,
        &left_ty,
        expr_result_heap_ownership(left) == HeapOwnership::Owned,
    );
    let persisted_left = expr_result_heap_ownership(left) == HeapOwnership::NonHeap
        || string_result_uses_transient_concat_buffer(left);
    let release_left = persisted_left || string_result_is_owned_call_temp(left, ctx);
    if persisted_left {
        abi::emit_call_label(emitter, "__rt_str_persist");
    }
    let (left_ptr_reg, left_len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, left_ptr_reg, left_len_reg);
    let right_ty = emit_expr(right, emitter, ctx, data);
    coerce_to_string_releasing_owned(
        emitter,
        ctx,
        data,
        &right_ty,
        expr_result_heap_ownership(right) == HeapOwnership::Owned,
    );
    let release_right = string_result_is_owned_call_temp(right, ctx);
    let mut cleanup_operands = 0usize;
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x3, x1");                                  // save right-operand string pointer into x3
            emitter.instruction("mov x4, x2");                                  // save right-operand string length into x4
            abi::emit_pop_reg_pair(emitter, "x1", "x2");
            if release_right {
                abi::emit_push_reg_pair(emitter, "x3", "x4");
                cleanup_operands += 1;
            }
            if release_left {
                abi::emit_push_reg_pair(emitter, "x1", "x2");
                cleanup_operands += 1;
            }
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // save right-operand string pointer into rdi
            emitter.instruction("mov rsi, rdx");                                // save right-operand string length into rsi
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");
            if release_right {
                abi::emit_push_reg_pair(emitter, "rdi", "rsi");
                cleanup_operands += 1;
            }
            if release_left {
                abi::emit_push_reg_pair(emitter, "rax", "rdx");
                cleanup_operands += 1;
            }
        }
    }
    abi::emit_call_label(emitter, "__rt_concat");
    if cleanup_operands > 0 {
        emit_release_preserved_concat_operands(emitter, cleanup_operands);
    }
    PhpType::Str
}

/// Cleans up persisted (non-heap) concat operands that were preserved on the temporary stack.
/// Called after `__rt_concat` when the result has already copied the string data; the result
/// is kept alive while each operand is freed via `__rt_heap_free_safe`. The result pointer/length
/// pair is also restored to the return registers after cleanup.
fn emit_release_preserved_concat_operands(emitter: &mut Emitter, count: usize) {
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);

    // Keep the concat result live while freeing persisted operands that
    // __rt_concat has already copied into the result buffer.
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);
    for idx in 0..count {
        abi::emit_load_temporary_stack_slot(
            emitter,
            abi::int_result_reg(emitter),
            16 + idx * 16,
        );
        abi::emit_call_label(emitter, "__rt_heap_free_safe");
    }
    abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
    abi::emit_release_temporary_stack(emitter, count * 16);
}

/// Lowers &, |, ^, <<, >> bitwise operators.
pub(super) fn emit_bitwise_binop(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let left_ty = emit_expr(left, emitter, ctx, data);
    coerce_to_int(emitter, &left_ty);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    let right_ty = emit_expr(right, emitter, ctx, data);
    coerce_to_int(emitter, &right_ty);
    let left_reg = match emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "r10",
    };
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_pop_reg(emitter, left_reg);
    match op {
        BinOp::BitAnd => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("and x0, x1, x0"),             // x0 = left (x1) & right (x0)
            Arch::X86_64 => {
                emitter.instruction(&format!("and {}, {}", left_reg, result_reg)); // left_reg &= result_reg (right operand)
                emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move the AND result back into the result register
            }
        },
        BinOp::BitOr => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("orr x0, x1, x0"),             // x0 = left (x1) | right (x0)
            Arch::X86_64 => {
                emitter.instruction(&format!("or {}, {}", left_reg, result_reg)); // left_reg |= result_reg (right operand)
                emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move the OR result back into the result register
            }
        },
        BinOp::BitXor => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("eor x0, x1, x0"),             // x0 = left (x1) ^ right (x0)
            Arch::X86_64 => {
                emitter.instruction(&format!("xor {}, {}", left_reg, result_reg)); // left_reg ^= result_reg (right operand)
                emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move the XOR result back into the result register
            }
        },
        BinOp::ShiftLeft => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("lsl x0, x1, x0"),             // x0 = left (x1) << right (x0)
            Arch::X86_64 => {
                emitter.instruction("mov rcx, rax");                            // x86 shifts require count in cl -- move right operand into rcx
                emitter.instruction(&format!("shl {}, cl", left_reg));          // left_reg <<= cl (logical shift left)
                emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move the shifted value back into the result register
            }
        },
        BinOp::ShiftRight => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("asr x0, x1, x0"),             // x0 = left (x1) >> right (x0) (arithmetic shift right)
            Arch::X86_64 => {
                emitter.instruction("mov rcx, rax");                            // x86 shifts require count in cl -- move right operand into rcx
                emitter.instruction(&format!("sar {}, cl", left_reg));          // left_reg >>= cl (arithmetic shift right)
                emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move the shifted value back into the result register
            }
        },
        _ => unreachable!(),
    }
    PhpType::Int
}

/// Emits signed integer modulo (left % right) with a divisor-is-zero guard.
/// On ARM64 uses sdiv/msub; on x86_64 uses idiv. When the divisor is zero, PHP semantics
/// mandate returning zero rather than triggering a divide-by-zero trap. The left_reg holds
/// the left operand and result_reg (x0/rax) holds the right operand at entry.
fn emit_int_mod(emitter: &mut Emitter, ctx: &mut Context, left_reg: &str, result_reg: &str) {
    let skip = ctx.next_label("mod_ok");
    let zero = ctx.next_label("mod_zero");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {zero}"));                    // branch to zero-divisor guard when right operand is zero
            emitter.instruction("sdiv x2, x1, x0");                             // x2 = left / right (quotient for modulo)
            emitter.instruction("msub x0, x2, x0, x1");                         // x0 = left - quotient*right (the remainder)
            emitter.instruction(&format!("b {skip}"));                          // skip the divisor-zero case
            emitter.label(&zero);
            emitter.instruction("mov x0, #0");                                  // return 0 when the divisor was zero (PHP semantics)
            emitter.label(&skip);
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", result_reg, result_reg)); // test whether divisor is zero
            emitter.instruction(&format!("je {}", zero));                       // jump to divisor-zero case when flag set
            emitter.instruction(&format!("mov r11, {}", result_reg));           // stash divisor in r11 before overwriting rax with the dividend
            emitter.instruction(&format!("mov {}, {}", result_reg, left_reg));  // move the dividend (left operand) into rax for idiv
            emitter.instruction("cqo");                                         // sign-extend rax into rdx:rax (required by idiv)
            emitter.instruction("idiv r11");                                    // signed divide -- quotient in rax, remainder in rdx
            emitter.instruction(&format!("mov {}, rdx", result_reg));           // return the remainder in the result register
            emitter.instruction(&format!("jmp {}", skip));                      // skip the divisor-zero case
            emitter.label(&zero);
            emitter.instruction(&format!("mov {}, 0", result_reg));             // return 0 when the divisor was zero (PHP semantics)
            emitter.label(&skip);
        }
    }
}
