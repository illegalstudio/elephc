//! Purpose:
//! Lowers integer arithmetic, bitwise, shift, and integer-to-float division EIR
//! opcodes for the Phase 04 stack-slot backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - The lowering preserves PHP scalar semantics and keeps all target
//!   register choices behind ABI helpers where shared helpers exist.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Immediate, Instruction, MixedNumericOp, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{
    expect_operand, require_float, require_integer_like, secondary_float_reg, store_if_result,
};
use crate::codegen::{CodegenIrError, Result};

/// Lowers a two-operand integer arithmetic or bitwise instruction.
pub(super) fn lower_int_binop(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    aarch64_mnemonic: &str,
    x86_64_mnemonic: &str,
) -> Result<()> {
    if super::strength_reduction::try_lower_constant_multiply(ctx, inst)? { return Ok(()); }
    if super::immediate_operands::try_int_binop(ctx, inst)? { return Ok(()); }
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let scratch_reg = abi::secondary_scratch_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // Three-operand forms read both register-allocated operands in place.
            let lhs_reg = integer_operand_register(ctx, lhs, result_reg, inst)?;
            let rhs_reg = integer_operand_register(ctx, rhs, scratch_reg, inst)?;
            ctx.emitter.instruction(
                &format!("{} {}, {}, {}", aarch64_mnemonic, result_reg, lhs_reg, rhs_reg)
            );                                                                  // compute the integer arithmetic result from both SSA operands
        }
        Arch::X86_64 => {
            load_integer_operand(ctx, lhs, result_reg, inst)?;
            let rhs_reg = integer_operand_register(ctx, rhs, scratch_reg, inst)?;
            ctx.emitter.instruction(
                &format!("{} {}, {}", x86_64_mnemonic, result_reg, rhs_reg)
            );                                                                  // update the integer result register with the arithmetic operand
        }
    }
    store_if_result(ctx, inst)
}

/// Returns a register holding an integer operand that the consuming instruction only reads.
///
/// An `int`/`bool` value with an allocated register home is used in place; anything else is
/// loaded into `fallback` with the usual null-to-zero coercion.
pub(super) fn integer_operand_register(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    fallback: &str,
    inst: &Instruction,
) -> Result<String> {
    if let Some(home) = ctx.allocation.register_of(value) {
        if matches!(ctx.value_php_type(value)?, PhpType::Int | PhpType::Bool) {
            return Ok(home.to_string());
        }
    }
    load_integer_operand(ctx, value, fallback, inst)?;
    Ok(fallback.to_string())
}

/// Lowers a checked integer binary operation that may overflow to float.
///
/// Loads both I64 operands into ABI argument registers, calls the target runtime
/// helper (e.g. `__rt_int_add_checked`), and stores the boxed Mixed result.
/// The helper returns a `Heap(Mixed)` pointer in the integer result register.
pub(super) fn lower_int_checked_binop(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    helper: &str,
) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let lhs_reg = abi::int_result_reg(ctx.emitter);
    let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    load_integer_operand(ctx, lhs, lhs_reg, inst)?;
    load_integer_operand(ctx, rhs, rhs_reg, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // AArch64 ABI: x0 = first arg, x1 = second arg.
            // lhs is already in x0 (int_result_reg), but rhs is in x10 (secondary_scratch_reg).
            // Move rhs to x1 to match the helper's expected calling convention.
            ctx.emitter.instruction("mov x1, x10");                             // place the right integer operand in the ABI argument register x1
            abi::emit_call_label(ctx.emitter, helper);
        }
        Arch::X86_64 => {
            // x86_64 SysV ABI: rdi = first arg, rsi = second arg.
            // Move lhs to rdi, rhs to rsi before the call.
            ctx.emitter.instruction(&format!("mov rdi, {}", lhs_reg));          // place the left integer operand in the first SysV argument register
            ctx.emitter.instruction(&format!("mov rsi, {}", rhs_reg));          // place the right integer operand in the second SysV argument register
            abi::emit_call_label(ctx.emitter, helper);
        }
    }
    store_if_result(ctx, inst)
}

/// The php-src wording for a zero divisor in `%` / `%=`.
const MODULO_BY_ZERO_MESSAGE: &str = "Modulo by zero";
/// The php-src wording for a zero divisor in `/` / `/=`.
const DIVISION_BY_ZERO_MESSAGE: &str = "Division by zero";
/// The php-src wording for `<<` / `>>` with a negative shift count.
const NEGATIVE_SHIFT_MESSAGE: &str = "Bit shift by negative number";

/// Lowers a signed integer modulo operation with PHP's zero-divisor and overflow guards.
///
/// Reference PHP 8.4 raises a catchable `DivisionByZeroError("Modulo by zero")` for `$x % 0`
/// instead of producing a value, and evaluates `PHP_INT_MIN % -1` to `0`. The x86_64 `idiv`
/// instruction traps with `#DE` (SIGFPE) on that second case, so `-1` divisors are answered
/// without ever reaching the divide unit. AArch64's `sdiv`/`msub` pair already wraps to `0`
/// there, matching PHP.
pub(super) fn lower_int_mod(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    load_integer_operand(ctx, lhs, result_reg, inst)?;
    load_integer_operand(ctx, rhs, rhs_reg, inst)?;
    let zero_label = ctx.next_label("mod_zero");
    let done_label = ctx.next_label("mod_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let quotient_reg = abi::tertiary_scratch_reg(ctx.emitter);
            ctx.emitter.instruction(
                &format!("cbz {}, {}", rhs_reg, zero_label)
            );                                                                  // branch to the zero-divisor throw when the modulo divisor is zero
            ctx.emitter.instruction(
                &format!("sdiv {}, {}, {}", quotient_reg, result_reg, rhs_reg)
            );                                                                  // compute signed quotient for the modulo operation
            ctx.emitter.instruction(
                &format!("msub {}, {}, {}, {}", result_reg, quotient_reg, rhs_reg, result_reg)
            );                                                                  // compute left - quotient * right as the remainder
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the zero-divisor throw after a normal remainder
        }
        Arch::X86_64 => {
            let neg_one_label = ctx.next_label("mod_neg_one");
            ctx.emitter.instruction(&format!("test {}, {}", rhs_reg, rhs_reg)); // test whether the modulo divisor is zero
            ctx.emitter.instruction(&format!("je {}", zero_label));             // branch to the zero-divisor throw when the modulo divisor is zero
            ctx.emitter.instruction(&format!("cmp {}, -1", rhs_reg));           // test whether the modulo divisor is -1
            ctx.emitter.instruction(&format!("je {}", neg_one_label));          // PHP_INT_MIN % -1 would raise #DE, and every x % -1 is zero anyway
            ctx.emitter.instruction("cqo");                                     // sign-extend the dividend before signed division
            ctx.emitter.instruction(&format!("idiv {}", rhs_reg));              // divide signed integers with quotient in rax and remainder in rdx
            ctx.emitter.instruction(&format!("mov {}, rdx", result_reg));       // move the signed remainder into the integer result register
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the guard blocks after a normal remainder
            ctx.emitter.label(&neg_one_label);
            ctx.emitter.instruction(&format!("mov {}, 0", result_reg));         // every integer modulo -1 is zero, exactly like PHP
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the zero-divisor throw after the -1 shortcut
        }
    }
    ctx.emitter.label(&zero_label);
    super::exceptions::emit_division_by_zero_error(ctx, MODULO_BY_ZERO_MESSAGE);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers PHP `/` for integer operands by promoting both sides to floating point.
///
/// Reference PHP 8.4 raises a catchable `DivisionByZeroError("Division by zero")` for a zero
/// divisor, so the hardware quotient (`INF` / `NaN`) is never observable. The guard runs before
/// the promotion for both supported targets.
pub(super) fn lower_int_div_to_float(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let lhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    let rhs_reg = abi::tertiary_scratch_reg(ctx.emitter);
    load_integer_operand(ctx, lhs, lhs_reg, inst)?;
    load_integer_operand(ctx, rhs, rhs_reg, inst)?;
    let zero_label = ctx.next_label("div_zero");
    let done_label = ctx.next_label("div_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(
                &format!("cbz {}, {}", rhs_reg, zero_label)
            );                                                                  // branch to the zero-divisor throw when the divisor is zero
            ctx.emitter.instruction(&format!("scvtf d0, {}", lhs_reg));         // promote the integer dividend into the float result register
            ctx.emitter.instruction(&format!("scvtf d1, {}", rhs_reg));         // promote the integer divisor into a float scratch register
            ctx.emitter.instruction("fdiv d0, d0, d1");                         // divide promoted operands as PHP floating-point division
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the zero-divisor throw after a normal quotient
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", rhs_reg, rhs_reg)); // test whether the divisor is zero
            ctx.emitter.instruction(&format!("je {}", zero_label));             // branch to the zero-divisor throw when the divisor is zero
            ctx.emitter.instruction(&format!("cvtsi2sd xmm0, {}", lhs_reg));    // promote the integer dividend into the float result register
            ctx.emitter.instruction(&format!("cvtsi2sd xmm1, {}", rhs_reg));    // promote the integer divisor into a float scratch register
            ctx.emitter.instruction("divsd xmm0, xmm1");                        // divide promoted operands as PHP floating-point division
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the zero-divisor throw after a normal quotient
        }
    }
    ctx.emitter.label(&zero_label);
    super::exceptions::emit_division_by_zero_error(ctx, DIVISION_BY_ZERO_MESSAGE);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers PHP `/` for floating-point operands with the PHP zero-divisor guard.
///
/// Reference PHP 8.4 raises `DivisionByZeroError` for `1.0 / 0`, `1 / 0.0`, and `0.0 / 0.0`
/// alike — the IEEE result (`INF` / `NaN`) is never observable through the `/` operator. Only
/// `fdiv()` returns it. Both `+0.0` and `-0.0` divisors throw and a `NaN` divisor does not, so
/// AArch64 uses `fcmp`'s zero form (unordered leaves `eq` clear) and x86_64 shifts the sign bit
/// out of the raw bit pattern, which is zero for `±0.0` only.
pub(super) fn lower_float_div(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let lhs_reg = secondary_float_reg(ctx.emitter.target.arch);
    let rhs_reg = abi::float_result_reg(ctx.emitter);
    require_float(ctx.load_value_to_reg(lhs, lhs_reg)?, inst)?;
    require_float(ctx.load_value_to_reg(rhs, rhs_reg)?, inst)?;
    let zero_label = ctx.next_label("fdiv_zero");
    let done_label = ctx.next_label("fdiv_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d0, #0.0");                           // compare the divisor with zero; NaN stays unordered and divides normally
            ctx.emitter.instruction(&format!("b.eq {}", zero_label));           // branch to the zero-divisor throw for both +0.0 and -0.0
            ctx.emitter.instruction("fdiv d0, d1, d0");                         // divide the dividend by the divisor into the float result register
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the zero-divisor throw after a normal quotient
        }
        Arch::X86_64 => {
            let bits_reg = abi::secondary_scratch_reg(ctx.emitter);
            ctx.emitter.instruction(&format!("movq {}, xmm0", bits_reg));       // raw IEEE-754 bits of the divisor
            ctx.emitter.instruction(&format!("add {bits_reg}, {bits_reg}"));    // shift out the sign bit so -0.0 tests equal to +0.0 (NaN stays non-zero)
            ctx.emitter.instruction(&format!("jz {}", zero_label));             // branch to the zero-divisor throw for both +0.0 and -0.0
            ctx.emitter.instruction("divsd xmm1, xmm0");                        // divide the dividend by the divisor in the float scratch register
            ctx.emitter.instruction("movsd xmm0, xmm1");                        // move the quotient into the float result register
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the zero-divisor throw after a normal quotient
        }
    }
    ctx.emitter.label(&zero_label);
    super::exceptions::emit_division_by_zero_error(ctx, DIVISION_BY_ZERO_MESSAGE);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers a single-operand integer instruction.
pub(super) fn lower_int_unary(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    aarch64_mnemonic: &str,
    x86_64_mnemonic: &str,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    load_integer_operand(ctx, value, result_reg, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(
                &format!("{} {}, {}", aarch64_mnemonic, result_reg, result_reg)
            );                                                                  // apply the integer unary operation to the loaded operand
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(
                &format!("{} {}", x86_64_mnemonic, result_reg)
            );                                                                  // apply the integer unary operation to the loaded operand
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers a variable-count signed integer shift operation with PHP's shift-count rules.
///
/// Raw AArch64 (`lsl`/`asr`) and x86_64 (`shl`/`sar`) register shifts mask the count to its low
/// six bits, which is *not* what PHP does. Reference PHP 8.4:
/// - a negative shift count raises a catchable `ArithmeticError("Bit shift by negative number")`;
/// - `<<` by 64 or more yields `0`;
/// - `>>` by 64 or more yields `0` for a non-negative value and `-1` for a negative one, i.e. the
///   arithmetic shift saturates at a full sign fill.
///
/// `left` selects `<<` (logical left shift, saturating to zero) from `>>` (arithmetic right
/// shift, saturating to the sign fill). Both branches are emitted identically on both targets.
pub(super) fn lower_int_shift(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    left: bool,
) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    load_integer_operand(ctx, lhs, result_reg, inst)?;
    load_integer_operand(ctx, rhs, rhs_reg, inst)?;
    let negative_label = ctx.next_label("shift_negative");
    let saturate_label = ctx.next_label("shift_saturate");
    let done_label = ctx.next_label("shift_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let mnemonic = if left { "lsl" } else { "asr" };
            ctx.emitter.instruction(
                &format!("tbnz {}, #63, {}", rhs_reg, negative_label)
            );                                                                  // a negative shift count is an ArithmeticError in PHP
            ctx.emitter.instruction(&format!("cmp {}, #64", rhs_reg));          // is the shift count outside the 64-bit window?
            ctx.emitter.instruction(&format!("b.hs {}", saturate_label));       // PHP saturates instead of masking the count to 6 bits
            ctx.emitter.instruction(
                &format!("{} {}, {}, {}", mnemonic, result_reg, result_reg, rhs_reg)
            );                                                                  // shift the integer operand by the EIR count operand
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the saturation and throw blocks after a normal shift
            ctx.emitter.label(&saturate_label);
            if left {
                ctx.emitter.instruction(&format!("mov {}, #0", result_reg));    // every bit is shifted out, so PHP yields 0
            } else {
                ctx.emitter.instruction(
                    &format!("asr {}, {}, #63", result_reg, result_reg)
                );                                                              // PHP fills with the sign bit: 0 for non-negative, -1 for negative
            }
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the throw block after saturating
        }
        Arch::X86_64 => {
            let mnemonic = if left { "shl" } else { "sar" };
            ctx.emitter.instruction(&format!("test {}, {}", rhs_reg, rhs_reg)); // inspect the sign of the shift count
            ctx.emitter.instruction(&format!("js {}", negative_label));         // a negative shift count is an ArithmeticError in PHP
            ctx.emitter.instruction(&format!("cmp {}, 64", rhs_reg));           // is the shift count outside the 64-bit window?
            ctx.emitter.instruction(&format!("jge {}", saturate_label));        // PHP saturates instead of masking the count to 6 bits
            ctx.emitter.instruction(&format!("mov rcx, {}", rhs_reg));          // move the variable shift count into x86_64's required cl register
            ctx.emitter.instruction(
                &format!("{} {}, cl", mnemonic, result_reg)
            );                                                                  // shift the integer operand by the low count byte
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the saturation and throw blocks after a normal shift
            ctx.emitter.label(&saturate_label);
            if left {
                ctx.emitter.instruction(&format!("mov {}, 0", result_reg));     // every bit is shifted out, so PHP yields 0
            } else {
                ctx.emitter.instruction(&format!("sar {}, 63", result_reg));    // PHP fills with the sign bit: 0 for non-negative, -1 for negative
            }
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the throw block after saturating
        }
    }
    ctx.emitter.label(&negative_label);
    super::exceptions::emit_arithmetic_error(ctx, NEGATIVE_SHIFT_MESSAGE);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Loads an integer arithmetic operand, coercing PHP null to integer zero.
pub(super) fn load_integer_operand(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    reg: &str,
    inst: &Instruction,
) -> Result<()> {
    match ctx.value_php_type(value)? {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, reg, 0);
            Ok(())
        }
        _ => {
            require_integer_like(ctx.load_value_to_reg(value, reg)?, inst)?;
            Ok(())
        }
    }
}

/// Lowers a dynamic mixed numeric add/sub/mul through the boxed-Mixed runtime helpers.
pub(super) fn lower_mixed_numeric_binop(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let op = expect_mixed_numeric_op(inst)?;
    let lhs_ty = ctx.value_php_type(lhs)?;
    let rhs_ty = ctx.value_php_type(rhs)?;
    let left_box_temp = !is_mixed_like(&lhs_ty);
    let right_box_temp = !is_mixed_like(&rhs_ty);

    materialize_value_as_mixed(ctx, lhs, &lhs_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    materialize_value_as_mixed(ctx, rhs, &rhs_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
        }
    }
    abi::emit_call_label(ctx.emitter, mixed_numeric_helper(op));
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if left_box_temp {
        decref_mixed_temp_at(ctx, 32);
    }
    if right_box_temp {
        decref_mixed_temp_at(ctx, 16);
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    store_if_result(ctx, inst)
}

/// Returns true when a PHP type is already represented as a boxed Mixed pointer.
fn is_mixed_like(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Mixed)
}

/// Loads an SSA value as a boxed Mixed pointer in the integer result register.
fn materialize_value_as_mixed(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    ty: &PhpType,
) -> Result<()> {
    let ty = ty.codegen_repr();
    if is_mixed_like(&ty) {
        ctx.load_value_to_result(value)?;
        return Ok(());
    }
    match ty {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        _ => {
            ctx.load_value_to_result(value)?;
        }
    }
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &ty);
    Ok(())
}

/// Releases a temporary Mixed box saved on the temporary stack.
fn decref_mixed_temp_at(ctx: &mut FunctionContext<'_>, offset: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", offset);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", offset);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
}

/// Returns the mixed numeric operation immediate attached to the EIR instruction.
fn expect_mixed_numeric_op(inst: &Instruction) -> Result<MixedNumericOp> {
    match inst.immediate {
        Some(Immediate::MixedNumericOp(op)) => Ok(op),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing mixed numeric op immediate",
            inst.op.name()
        ))),
    }
}

/// Maps a mixed numeric operation to the target-aware runtime helper label.
fn mixed_numeric_helper(op: MixedNumericOp) -> &'static str {
    match op {
        MixedNumericOp::Add => "__rt_mixed_numeric_add",
        MixedNumericOp::Sub => "__rt_mixed_numeric_sub",
        MixedNumericOp::Mul => "__rt_mixed_numeric_mul",
        MixedNumericOp::Pow => "__rt_mixed_numeric_pow",
    }
}
