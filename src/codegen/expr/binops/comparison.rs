//! Purpose:
//! Lowers loose equality, ordering, and spaceship operators.
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
    emit_float_compare, emit_pop_left_float_for_comparison, emit_promote_int_to_float,
    emit_set_bool_from_flags, emit_set_float_bool_from_flags,
};
use super::super::{coerce_null_to_zero, coerce_to_truthiness, emit_expr, BinOp, Expr, PhpType};

/// Converts `ty` to integer for loose comparison.
/// Handles null, bool, int, float, str, and Mixed/Union types.
/// Emits the integer result into `abi::int_result_reg(emitter)`.
/// Float values are truncated via `fcvtzs`. Strings call `__rt_atoi`.
fn coerce_to_int_for_loose_cmp(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Void => {
            emitter.instruction("mov x0, #0");                                  // coerce null into integer 0 for loose comparison
        }
        PhpType::Bool => {}
        PhpType::Int => {
            super::super::coerce_null_to_zero(emitter, ty);
        }
        PhpType::Float => {
            emitter.instruction("fcvtzs x0, d0");                               // truncate the float in d0 to signed int for loose comparison
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

/// Emits loose equality when the left operand is bool.
/// Coerces both sides to truthiness, then compares saved left truthiness against current right truthiness.
/// Uses a 16-byte temporary stack slot to preserve the left bool during right evaluation.
fn emit_bool_left_loose_equality(
    _left: &Expr,
    op: &BinOp,
    right: &Expr,
    left_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    coerce_to_truthiness(emitter, ctx, left_ty);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    let right_ty = emit_expr(right, emitter, ctx, data);
    coerce_to_truthiness(emitter, ctx, &right_ty);
    compare_saved_truthiness_with_current(op, emitter);
    PhpType::Bool
}

/// Emits loose equality when the left operand is string.
/// Pushes the left string onto the temporary stack, emits the right expression,
/// then dispatches on the right type to handle bool, void, string, numeric, and other cases.
/// Returns PhpType::Bool.
fn emit_string_left_loose_equality(
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let (left_ptr, left_len) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, left_ptr, left_len);
    let right_ty = emit_expr(right, emitter, ctx, data);
    match right_ty {
        PhpType::Bool => {
            coerce_to_truthiness(emitter, ctx, &right_ty);
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
            load_saved_left_string(emitter, 16);
            coerce_to_truthiness(emitter, ctx, &PhpType::Str);
            compare_saved_right_truthiness_with_current_left(op, emitter);
            abi::emit_release_temporary_stack(emitter, 16);
        }
        PhpType::Void => {
            pop_saved_left_string(emitter);
            emit_compare_current_string_length_to_zero(op, emitter);
        }
        PhpType::Str => {
            call_str_loose_eq_with_saved_left(op, emitter);
        }
        PhpType::Int | PhpType::Float => {
            push_current_number_as_float(emitter, &right_ty);
            load_saved_left_string(emitter, 16);
            abi::emit_call_label(emitter, "__rt_str_to_number");
            compare_parsed_string_with_saved_float(op, emitter, ctx);
            abi::emit_release_temporary_stack(emitter, 16);
        }
        _ => {
            pop_saved_left_string(emitter);
            emit_set_loose_bool_literal(op, false, emitter);
        }
    }
    PhpType::Bool
}

/// Emits loose equality when the right operand is bool but left is not.
/// Left value is already on the temporary stack (numeric). Coerces right to truthiness,
/// pops left and coerces it to truthiness, then compares saved right truthiness against current left truthiness.
fn emit_bool_right_loose_equality(
    op: &BinOp,
    left_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    coerce_to_truthiness(emitter, ctx, &PhpType::Bool);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    pop_saved_left_for_truthiness(emitter, left_ty);
    coerce_to_truthiness(emitter, ctx, left_ty);
    compare_saved_right_truthiness_with_current_left(op, emitter);
    PhpType::Bool
}

/// Emits loose equality when the right operand is string.
/// The left value (numeric) is on the temporary stack. Dispatches based on left type:
/// - void: compares current string length to zero
/// - int/float: converts string to number and compares
/// - other: discards left and returns false
fn emit_right_string_loose_equality(
    op: &BinOp,
    left_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    if *left_ty == PhpType::Void {
        discard_saved_left_numeric(emitter, left_ty);
        emit_compare_current_string_length_to_zero(op, emitter);
    } else if matches!(left_ty, PhpType::Int | PhpType::Float) {
        abi::emit_call_label(emitter, "__rt_str_to_number");
        compare_parsed_string_with_saved_left_number(op, left_ty, emitter, ctx);
    } else {
        discard_saved_left_numeric(emitter, left_ty);
        emit_set_loose_bool_literal(op, false, emitter);
    }
    PhpType::Bool
}

/// Pops the saved left truthiness value into a scratch register and compares it
/// against the current right truthiness in `abi::int_result_reg(emitter)`.
/// Sets the boolean result from flags using `loose_equality_condition(op)`.
fn compare_saved_truthiness_with_current(op: &BinOp, emitter: &mut Emitter) {
    let left_reg = match emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "r10",
    };
    abi::emit_pop_reg(emitter, left_reg);
    emitter.instruction(&format!("cmp {}, {}", left_reg, abi::int_result_reg(emitter))); // compare left truthiness against right truthiness
    emit_set_bool_from_flags(emitter, loose_equality_condition(op));
}

/// Pops the saved right truthiness value into a scratch register and compares it
/// against the current left truthiness in `abi::int_result_reg(emitter)`.
/// Sets the boolean result from flags using `loose_equality_condition(op)`.
/// The comparison order is reversed relative to `compare_saved_truthiness_with_current`.
fn compare_saved_right_truthiness_with_current_left(op: &BinOp, emitter: &mut Emitter) {
    let right_reg = match emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "r10",
    };
    abi::emit_pop_reg(emitter, right_reg);
    emitter.instruction(&format!("cmp {}, {}", abi::int_result_reg(emitter), right_reg)); // compare left truthiness against right truthiness
    emit_set_bool_from_flags(emitter, loose_equality_condition(op));
}

/// Arranges arguments on ARM64 or x86_64 ABI registers and calls `__rt_str_loose_eq`
/// with the saved left string (popped from temp stack) and current right string.
/// Inverts the result for `!=` via `invert_loose_bool_if_needed`.
fn call_str_loose_eq_with_saved_left(op: &BinOp, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x3, x1");                                  // move the right string pointer into the loose string helper argument
            emitter.instruction("mov x4, x2");                                  // move the right string length into the loose string helper argument
            abi::emit_pop_reg_pair(emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, rax");                                // preserve the right string pointer while arranging helper arguments
            emitter.instruction("mov rcx, rdx");                                // move the right string length into the fourth helper argument
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");
            emitter.instruction("mov rdx, r10");                                // move the right string pointer into the third helper argument
        }
    }
    abi::emit_call_label(emitter, "__rt_str_loose_eq");
    invert_loose_bool_if_needed(op, emitter);
}

/// Loads the saved left string from the temporary stack slot at `offset`.
/// Pointer lands in `abi::string_result_regs(emitter).0`, length in `.1`.
fn load_saved_left_string(emitter: &mut Emitter, offset: usize) {
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_load_temporary_stack_slot(emitter, ptr_reg, offset);
    abi::emit_load_temporary_stack_slot(emitter, len_reg, offset + 8);
}

/// Pops the saved left string from the temporary stack into `abi::string_result_regs(emitter)`.
fn pop_saved_left_string(emitter: &mut Emitter) {
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
}

/// Compares the current string's length register against zero.
/// Sets the boolean result via `loose_equality_condition(op)`.
fn emit_compare_current_string_length_to_zero(op: &BinOp, emitter: &mut Emitter) {
    let (_, len_reg) = abi::string_result_regs(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #0", len_reg));               // compare string length against the empty string for null loose equality
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, 0", len_reg));                // compare string length against the empty string for null loose equality
        }
    }
    emit_set_bool_from_flags(emitter, loose_equality_condition(op));
}

/// Promotes the current integer result to float if needed, then pushes the float
/// onto the temporary stack for numeric string comparison.
fn push_current_number_as_float(emitter: &mut Emitter, ty: &PhpType) {
    if *ty != PhpType::Float {
        emit_promote_int_to_float(
            emitter,
            abi::float_result_reg(emitter),
            abi::int_result_reg(emitter),
        );
    }
    abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));
}

/// Compares a numeric string (parsed into x0/d0 by `__rt_str_to_number`) against a float
/// that was saved on the temporary stack. On success (x0 != 0), pops the saved float and
/// compares it with the parsed value. On parsing failure, jumps to `false_label` and
/// sets the result to false. Uses `done_label` to skip the false branch on success.
fn compare_parsed_string_with_saved_float(
    op: &BinOp,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let false_label = ctx.next_label("loose_numeric_string_false");
    let done_label = ctx.next_label("loose_numeric_string_done");
    let saved_float_reg = match emitter.target.arch {
        Arch::AArch64 => "d1",
        Arch::X86_64 => "xmm1",
    };
    abi::emit_pop_float_reg(emitter, saved_float_reg);
    emit_branch_if_current_flag_false(emitter, &false_label);
    emit_compare_saved_float_with_parsed_string(emitter);
    emit_set_float_bool_from_flags(emitter, loose_equality_condition(op));
    abi::emit_jump(emitter, &done_label);                                       // skip the non-numeric-string false branch
    emitter.label(&false_label);
    emit_set_loose_bool_literal(op, false, emitter);
    emitter.label(&done_label);
}

/// Compares a numeric string (parsed into x0/d0) against a number that was saved on
/// the temporary stack. If left was int, it is first promoted to float. On success
/// (x0 != 0), pops the saved number and compares it with the parsed value.
/// On parsing failure, jumps to `false_label` and sets result to false.
fn compare_parsed_string_with_saved_left_number(
    op: &BinOp,
    left_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let false_label = ctx.next_label("loose_numeric_string_false");
    let done_label = ctx.next_label("loose_numeric_string_done");
    let saved_float_reg = match emitter.target.arch {
        Arch::AArch64 => "d1",
        Arch::X86_64 => "xmm1",
    };
    if *left_ty == PhpType::Float {
        abi::emit_pop_float_reg(emitter, saved_float_reg);
    } else {
        let left_int_reg = match emitter.target.arch {
            Arch::AArch64 => "x1",
            Arch::X86_64 => "r10",
        };
        abi::emit_pop_reg(emitter, left_int_reg);
        emit_promote_int_to_float(emitter, saved_float_reg, left_int_reg);
    }
    emit_branch_if_current_flag_false(emitter, &false_label);
    emit_compare_saved_float_with_parsed_string(emitter);
    emit_set_float_bool_from_flags(emitter, loose_equality_condition(op));
    abi::emit_jump(emitter, &done_label);                                       // skip the non-numeric-string false branch
    emitter.label(&false_label);
    emit_set_loose_bool_literal(op, false, emitter);
    emitter.label(&done_label);
}

/// Tests whether the string-to-number parsing result in x0/rax is zero (failure).
/// Branches to `label` when parsing failed (non-numeric string).
fn emit_branch_if_current_flag_false(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // test whether string-to-number parsing failed
            emitter.instruction(&format!("b.eq {}", label));                    // branch when the string was not numeric
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // test whether string-to-number parsing failed
            emitter.instruction(&format!("je {}", label));                      // branch when the string was not numeric
        }
    }
}

/// Issues the target-specific float comparison instruction between the saved float
/// (d1/xmm1) and the parsed numeric string result (d0/xmm0).
fn emit_compare_saved_float_with_parsed_string(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fcmp d1, d0");                                 // compare numeric operand against parsed numeric string
        }
        Arch::X86_64 => {
            emitter.instruction("ucomisd xmm1, xmm0");                          // compare numeric operand against parsed numeric string
        }
    }
}

/// Pops the saved left operand into the appropriate register for truthiness coercion.
/// Float values go to `float_result_reg`, strings are popped as a pair, other types
/// go to `int_result_reg`.
fn pop_saved_left_for_truthiness(emitter: &mut Emitter, left_ty: &PhpType) {
    match left_ty {
        PhpType::Float => {
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));
        }
        PhpType::Str => {
            pop_saved_left_string(emitter);
        }
        _ => {
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
        }
    }
}

/// Discards the saved left numeric operand from the temporary stack without using it.
/// Float values pop to `float_result_reg`, integers to `int_result_reg`.
fn discard_saved_left_numeric(emitter: &mut Emitter, left_ty: &PhpType) {
    if *left_ty == PhpType::Float {
        abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));
    } else {
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
    }
}

/// Inverts the normalized equality result (0 or 1) in x0/rax when the operator is `!=`.
fn invert_loose_bool_if_needed(op: &BinOp, emitter: &mut Emitter) {
    if *op == BinOp::NotEq {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("eor x0, x0, #1");                          // invert normalized loose equality for !=
            }
            Arch::X86_64 => {
                emitter.instruction("xor rax, 1");                              // invert normalized loose equality for !=
            }
        }
    }
}

/// Emits a boolean literal result for loose equality operators.
/// For `==`: emits `equality_value` directly; for `!=`: emits `!equality_value`.
/// Result lands in `abi::int_result_reg(emitter)`.
fn emit_set_loose_bool_literal(op: &BinOp, equality_value: bool, emitter: &mut Emitter) {
    let result = match op {
        BinOp::Eq => equality_value,
        BinOp::NotEq => !equality_value,
        _ => unreachable!(),
    };
    abi::emit_load_int_immediate(
        emitter,
        abi::int_result_reg(emitter),
        if result { 1 } else { 0 },
    );
}

/// Returns the target condition name for loose equality operators.
/// `"eq"` for `==`, `"ne"` for `!=`. Panics for other operators.
fn loose_equality_condition(op: &BinOp) -> &'static str {
    match op {
        BinOp::Eq => "eq",
        BinOp::NotEq => "ne",
        _ => unreachable!(),
    }
}

/// Emits `==` and `!=` loose equality with full PHP type coercion rules.
/// Dispatches on the left type: bool-left and string-left have specialized paths.
/// For other types, emits left, pushes it, emits right, then compares as int or float.
/// Returns PhpType::Bool with result in `abi::int_result_reg(emitter)`.
pub(super) fn emit_loose_equality_binop(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let left_ty = emit_expr(left, emitter, ctx, data);
    if left_ty == PhpType::Bool {
        return emit_bool_left_loose_equality(left, op, right, &left_ty, emitter, ctx, data);
    }
    if left_ty == PhpType::Str {
        return emit_string_left_loose_equality(op, right, emitter, ctx, data);
    }
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

    if right_ty == PhpType::Bool && matches!(left_ty, PhpType::Int | PhpType::Float | PhpType::Void) {
        return emit_bool_right_loose_equality(op, &left_ty, emitter, ctx);
    }
    if right_ty == PhpType::Str {
        return emit_right_string_loose_equality(op, &left_ty, emitter, ctx);
    }

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
        emitter.instruction(&format!("cmp {}, {}", left_reg, abi::int_result_reg(emitter))); // compare left against right in integer registers
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

/// Lowers <, >, <=, >= ordering comparisons with float/int dispatch.
pub(super) fn emit_order_compare_binop(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let left_ty = emit_expr(left, emitter, ctx, data);
    coerce_numeric_mixed_to_int(emitter, &left_ty);
    let use_float = left_ty == PhpType::Float;
    if use_float {
        abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));
    } else {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    }
    let right_ty = emit_expr(right, emitter, ctx, data);
    coerce_numeric_mixed_to_int(emitter, &right_ty);

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
        emitter.instruction(&format!("cmp {}, {}", left_reg, abi::int_result_reg(emitter))); // compare left against right in integer registers
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

/// Lowers the <=> spaceship operator returning -1, 0, or 1.
pub(super) fn emit_spaceship_binop(
    left: &Expr,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let left_ty = emit_expr(left, emitter, ctx, data);
    coerce_numeric_mixed_to_int(emitter, &left_ty);
    let use_float = left_ty == PhpType::Float;
    if use_float {
        abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));
    } else {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    }
    let right_ty = emit_expr(right, emitter, ctx, data);
    coerce_numeric_mixed_to_int(emitter, &right_ty);

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
            Arch::AArch64 => emitter.instruction("cmp x1, x0"),                 // compare left (x1) against right (x0) before computing the spaceship result
            Arch::X86_64 => emitter.instruction(&format!(                       // compare left against right in integer registers
                "cmp {}, {}",
                left_reg,
                abi::int_result_reg(emitter)
            )),
        }
    }

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cset x0, gt");                                 // set x0 to 1 when left > right, else 0
            emitter.instruction("csinv x0, x0, xzr, ge");                       // keep 1 when left >= right, invert to -1 when left < right
            if left_ty == PhpType::Float || right_ty == PhpType::Float {
                emitter.instruction("mov w1, #1");                              // candidate spaceship result for an unordered (NaN) comparison
                emitter.instruction("csel x0, x1, x0, vs");                     // PHP: NaN <=> x is 1 — pick 1 when fcmp was unordered
            }
        }
        Arch::X86_64 => {
            let greater_label = ctx.next_label("spaceship_gt");
            let less_label = ctx.next_label("spaceship_lt");
            let done_label = ctx.next_label("spaceship_done");
            if left_ty == PhpType::Float || right_ty == PhpType::Float {
                emitter.instruction(&format!("jp {}", greater_label));          // PHP: NaN <=> x is 1 — route unordered (parity) to the greater (1) branch
                emitter.instruction(&format!("ja {}", greater_label));          // floats: jump to greater branch when unordered-above
                emitter.instruction(&format!("jb {}", less_label));             // floats: jump to less branch when unordered-below
            } else {
                emitter.instruction(&format!("jg {}", greater_label));          // ints: jump to greater branch when signed greater
                emitter.instruction(&format!("jl {}", less_label));             // ints: jump to less branch when signed less
            }
            emitter.instruction("mov rax, 0");                                  // equal case: spaceship result is 0
            emitter.instruction(&format!("jmp {}", done_label));                // skip the greater/less branches
            emitter.label(&greater_label);
            emitter.instruction("mov rax, 1");                                  // greater branch: spaceship result is 1
            emitter.instruction(&format!("jmp {}", done_label));                // skip the less branch
            emitter.label(&less_label);
            emitter.instruction("mov rax, -1");                                 // less branch: spaceship result is -1
            emitter.label(&done_label);
        }
    }
    PhpType::Int
}

/// Coerces null to zero, then for Mixed/Union types calls `__rt_mixed_cast_int`
/// to normalize boxed int|bool|string before ordering comparisons.
/// Other numeric types are left as-is.
fn coerce_numeric_mixed_to_int(emitter: &mut Emitter, ty: &PhpType) {
    coerce_null_to_zero(emitter, ty);
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(emitter, "__rt_mixed_cast_int");                   // normalize boxed int|bool|string values before numeric comparisons
    }
}
