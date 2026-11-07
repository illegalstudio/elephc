//! Purpose:
//! Lowers integer, float and boxed arithmetic, bitwise operations and shifts.
//! Keeps scalar operands and temporary numeric cells consistent with the runtime ABI.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - The lowering preserves PHP scalar semantics and keeps all target
//!   register choices behind ABI helpers where shared helpers exist.
//! - Only exact integer constants may use the borrowed stack-cell RHS path.
//! - Checked result cells use frame storage only after a complete nonescaping lifetime proof.
//! - `IOverflows` is a boolean when emitted alone. Next to its wrapping arithmetic and a
//!   `cond_br`, both are skipped and the branch consumes the overflow flags directly.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Function, Immediate, Instruction, IrHeapKind, IrType, MixedNumericOp, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{
    expect_operand, require_float, require_integer_like, secondary_float_reg, store_if_result,
};
use crate::codegen::{CodegenIrError, Result};

#[cfg(test)]
#[path = "arithmetic/borrowed_integer_rhs_tests.rs"]
mod borrowed_integer_rhs_tests;

pub(super) mod min_max;
pub(super) mod stack_cell;

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
    let home = inst.result.and_then(|result| ctx.allocation.register_of(result));
    // The x86_64 two-operand form cannot write the result into the register that
    // holds the right operand. That fallback computes into the result register,
    // so the value still needs its usual store-to-home copy.
    let mut wrote_home = false;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // Three-operand forms read both register-allocated operands in place.
            let lhs_reg = integer_operand_register(ctx, lhs, result_reg, inst)?;
            let rhs_reg = integer_operand_register(ctx, rhs, scratch_reg, inst)?;
            let destination = home.unwrap_or(result_reg);
            ctx.emitter.instruction(
                &format!("{} {}, {}, {}", aarch64_mnemonic, destination, lhs_reg, rhs_reg)
            );                                                                  // compute the result directly in its allocated home
            wrote_home = home.is_some();
        }
        Arch::X86_64 => {
            let rhs_reg = integer_operand_register(ctx, rhs, scratch_reg, inst)?;
            if let Some(destination) = home.filter(|destination| *destination != rhs_reg) {
                if ctx.allocation.register_of(lhs) != Some(destination) {
                    load_integer_operand(ctx, lhs, destination, inst)?;
                }
                ctx.emitter.instruction(
                    &format!("{} {}, {}", x86_64_mnemonic, destination, rhs_reg)
                );                                                              // update the allocated home with the arithmetic operand
                wrote_home = true;
            } else {
                load_integer_operand(ctx, lhs, result_reg, inst)?;
                ctx.emitter.instruction(
                    &format!("{} {}, {}", x86_64_mnemonic, result_reg, rhs_reg)
                );                                                              // update the integer result register with the arithmetic operand
            }
        }
    }
    if !wrote_home {
        store_if_result(ctx, inst)?;
    }
    Ok(())
}

/// How a fused overflow branch reads the flags of the arithmetic just emitted.
#[derive(Clone, Copy)]
pub(in crate::codegen) enum OverflowBranch {
    /// ARM `VS` or x86 `OF`, set by signed add, subtract, and x86 `imul`.
    Signed,
    /// ARM signed high-half compare is not equal when the product does not fit.
    NotEqual,
}

/// Lowers `IOverflows` to a boolean when the branch could not absorb it.
///
/// The wrapping arithmetic, if any, has already stored its result. This recomputes the
/// same operation only to read the signed overflow condition.
pub(super) fn lower_ioverflows(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let op = overflow_op(inst)?;
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let left = abi::int_result_reg(ctx.emitter);
    let right = abi::secondary_scratch_reg(ctx.emitter);
    let tmp = abi::tertiary_scratch_reg(ctx.emitter);
    load_integer_operand(ctx, lhs, left, inst)?;
    load_integer_operand(ctx, rhs, right, inst)?;
    ctx.emitter.comment("-- recompute signed overflow as a boolean --");
    match (ctx.emitter.target.arch, op) {
        (Arch::AArch64, MixedNumericOp::Add) => {
            ctx.emitter.instruction(&format!("adds {tmp}, {left}, {right}"));   // set VS from the signed sum without keeping the bits
            ctx.emitter.instruction(&format!("cset {left}, vs"));               // 1 when the signed sum does not fit in an integer
        }
        (Arch::AArch64, MixedNumericOp::Sub) => {
            ctx.emitter.instruction(&format!("subs {tmp}, {left}, {right}"));   // set VS from the signed difference without keeping the bits
            ctx.emitter.instruction(&format!("cset {left}, vs"));               // 1 when the signed difference does not fit in an integer
        }
        (Arch::AArch64, MixedNumericOp::Mul) => {
            let high = abi::symbol_scratch_reg(ctx.emitter);
            ctx.emitter.instruction(&format!("smulh {high}, {left}, {right}")); // recover the high half of the signed product
            ctx.emitter.instruction(&format!("mul {tmp}, {left}, {right}"));    // recover the low half of the signed product
            ctx.emitter.instruction(&format!("cmp {high}, {tmp}, asr #63"));    // a fitting product has only sign extension in its high half
            ctx.emitter.instruction(&format!("cset {left}, ne"));               // 1 when the signed product does not fit in an integer
        }
        (Arch::X86_64, MixedNumericOp::Add) => {
            ctx.emitter.instruction(&format!("mov {tmp}, {left}"));             // keep the left integer while the flag-setting add runs
            ctx.emitter.instruction(&format!("add {tmp}, {right}"));            // set OF from the signed sum
            ctx.emitter.instruction("seto al");                                 // capture signed overflow in the result's low byte
            ctx.emitter.instruction("movzx eax, al");                           // widen the overflow boolean to the result register
        }
        (Arch::X86_64, MixedNumericOp::Sub) => {
            ctx.emitter.instruction(&format!("mov {tmp}, {left}"));             // keep the left integer while the flag-setting subtract runs
            ctx.emitter.instruction(&format!("sub {tmp}, {right}"));            // set OF from the signed difference
            ctx.emitter.instruction("seto al");                                 // capture signed overflow in the result's low byte
            ctx.emitter.instruction("movzx eax, al");                           // widen the overflow boolean to the result register
        }
        (Arch::X86_64, MixedNumericOp::Mul) => {
            ctx.emitter.instruction(&format!("mov {tmp}, {left}"));             // keep the left integer while the flag-setting multiply runs
            ctx.emitter.instruction(&format!("imul {tmp}, {right}"));           // set OF when the signed product does not fit
            ctx.emitter.instruction("seto al");                                 // capture signed overflow in the result's low byte
            ctx.emitter.instruction("movzx eax, al");                           // widen the overflow boolean to the result register
        }
        _ => {
            return Err(CodegenIrError::unsupported(
                "ioverflows accepts add, subtract and multiply only",
            ));
        }
    }
    store_if_result(ctx, inst)
}

/// Emits the wrapping arithmetic into its allocated register and reports which flags mean overflow.
///
/// The following branch must be the next flag-reading instruction. Moves used to place the
/// result preserve the overflow flags on both supported targets.
pub(in crate::codegen) fn emit_fused_overflow(
    ctx: &mut FunctionContext<'_>,
    arith: crate::ir::InstId,
) -> Result<OverflowBranch> {
    let inst = ctx
        .function
        .instruction(arith)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", arith.as_raw()))?;
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("fused overflow arithmetic has no result")
    })?;
    // A spilled result is stored with a flag-preserving move after the arithmetic, so the
    // following branch still sees the overflow flags. A register home is written directly.
    let home = ctx.allocation.register_of(result);
    let destination = home.unwrap_or_else(|| abi::int_result_reg(ctx.emitter));
    let op = match inst.op {
        Op::IAdd => MixedNumericOp::Add,
        Op::ISub => MixedNumericOp::Sub,
        Op::IMul => MixedNumericOp::Mul,
        _ => {
            return Err(CodegenIrError::invalid_module(
                "fused overflow arithmetic must be add, subtract or multiply",
            ));
        }
    };
    let lhs = expect_operand(&inst, 0)?;
    let rhs = expect_operand(&inst, 1)?;
    ctx.emitter.comment("-- signed arithmetic with an adjacent overflow branch --");
    let kind = match ctx.emitter.target.arch {
        Arch::AArch64 => emit_fused_overflow_arm(ctx, &inst, op, lhs, rhs, destination)?,
        Arch::X86_64 => emit_fused_overflow_x86(ctx, &inst, op, lhs, rhs, destination)?,
    };
    if home.is_none() {
        store_if_result(ctx, &inst)?;
    }
    Ok(kind)
}

/// Emits a three-operand ARM arithmetic operation directly into `home`.
fn emit_fused_overflow_arm(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    op: MixedNumericOp,
    lhs: ValueId,
    rhs: ValueId,
    home: &str,
) -> Result<OverflowBranch> {
    let lhs_reg = integer_operand_register(ctx, lhs, abi::int_result_reg(ctx.emitter), inst)?;
    let rhs_reg = integer_operand_register(ctx, rhs, abi::secondary_scratch_reg(ctx.emitter), inst)?;
    match op {
        MixedNumericOp::Add => {
            ctx.emitter.instruction(&format!("adds {home}, {lhs_reg}, {rhs_reg}")); // signed sum in its register, VS set on overflow
            Ok(OverflowBranch::Signed)
        }
        MixedNumericOp::Sub => {
            ctx.emitter.instruction(&format!("subs {home}, {lhs_reg}, {rhs_reg}")); // signed difference in its register, VS set on overflow
            Ok(OverflowBranch::Signed)
        }
        MixedNumericOp::Mul => {
            let high = abi::symbol_scratch_reg(ctx.emitter);
            ctx.emitter.instruction(&format!("smulh {high}, {lhs_reg}, {rhs_reg}")); // high half while both factors are still intact
            ctx.emitter.instruction(&format!("mul {home}, {lhs_reg}, {rhs_reg}")); // low half of the signed product in its register
            ctx.emitter.instruction(&format!("cmp {high}, {home}, asr #63"));   // a fitting product has only sign extension above
            Ok(OverflowBranch::NotEqual)
        }
        _ => Err(CodegenIrError::unsupported(
            "fused overflow accepts add, subtract and multiply only",
        )),
    }
}

/// Emits a two-operand x86 arithmetic operation into `home`, preserving overflow flags.
fn emit_fused_overflow_x86(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    op: MixedNumericOp,
    lhs: ValueId,
    rhs: ValueId,
    home: &str,
) -> Result<OverflowBranch> {
    let rhs_reg = integer_operand_register(ctx, rhs, abi::secondary_scratch_reg(ctx.emitter), inst)?;
    let mnemonic = match op {
        MixedNumericOp::Add => "add",
        MixedNumericOp::Sub => "sub",
        MixedNumericOp::Mul => "imul",
        _ => {
            return Err(CodegenIrError::unsupported(
                "fused overflow accepts add, subtract and multiply only",
            ));
        }
    };
    if home != rhs_reg {
        if ctx.allocation.register_of(lhs) != Some(home) {
            load_integer_operand(ctx, lhs, home, inst)?;
        }
        ctx.emitter.instruction(&format!("{mnemonic} {home}, {rhs_reg}"));      // update the allocated register and set OF on signed overflow
    } else {
        let tmp = abi::int_result_reg(ctx.emitter);
        load_integer_operand(ctx, lhs, tmp, inst)?;
        ctx.emitter.instruction(&format!("{mnemonic} {tmp}, {rhs_reg}"));       // compute into a temporary so the right operand stays intact
        ctx.emitter.instruction(&format!("mov {home}, {tmp}"));                 // publish the result without clearing the overflow flag
    }
    Ok(OverflowBranch::Signed)
}

/// Returns the add, subtract, or multiply operation selected by an `IOverflows` immediate.
fn overflow_op(inst: &Instruction) -> Result<MixedNumericOp> {
    match inst.immediate {
        Some(Immediate::MixedNumericOp(op @ (MixedNumericOp::Add | MixedNumericOp::Sub | MixedNumericOp::Mul))) => {
            Ok(op)
        }
        _ => Err(CodegenIrError::invalid_module(
            "ioverflows requires an add, subtract or multiply immediate",
        )),
    }
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
    if let Some(offset) = inst.result.and_then(|value| ctx.numeric_cell_offsets.get(&value).copied()) {
        return stack_cell::lower(ctx, inst, offset);
    }
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

/// Lowers PHP `/` for two integer operands with the exact-result rule.
///
/// PHP keeps an integer when the division is exact and the quotient fits; otherwise it
/// returns a double. A zero divisor raises a catchable `DivisionByZeroError`, and
/// `PHP_INT_MIN / -1` overflows the integer domain, so both cases are guarded before the
/// hardware divide. The result is a fresh boxed Mixed cell (int or float), matching the
/// representation of the other checked integer operations.
pub(super) fn lower_int_checked_div(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let lhs_reg = abi::int_result_reg(ctx.emitter);
    let scratch_reg = abi::secondary_scratch_reg(ctx.emitter);
    load_integer_operand(ctx, lhs, lhs_reg, inst)?;
    load_integer_operand(ctx, rhs, scratch_reg, inst)?;
    let zero_label = ctx.next_label("checked_div_zero");
    let float_label = ctx.next_label("checked_div_float");
    let int_label = ctx.next_label("checked_div_int");
    let done_label = ctx.next_label("checked_div_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {scratch_reg}, {zero_label}")); // a zero divisor raises DivisionByZeroError
            abi::emit_load_int_immediate(ctx.emitter, "x9", i64::MIN);
            ctx.emitter.instruction(&format!("cmp {lhs_reg}, x9"));             // is the dividend PHP_INT_MIN?
            ctx.emitter.instruction(&format!("b.ne {int_label}"));              // every other dividend is safe to divide
            ctx.emitter.instruction(&format!("cmp {scratch_reg}, #-1"));        // does INT_MIN divide by -1?
            ctx.emitter.instruction(&format!("b.eq {float_label}"));            // that quotient overflows the integer domain
            ctx.emitter.label(&int_label);
            ctx.emitter.instruction(&format!("mov x1, {scratch_reg}"));         // place the divisor in the ABI argument register
            ctx.emitter.instruction(&format!("sdiv x9, {lhs_reg}, x1"));        // truncate toward zero like PHP integer division
            ctx.emitter.instruction(&format!("msub x11, x9, x1, {lhs_reg}"));   // remainder = dividend - quotient * divisor
            ctx.emitter.instruction(&format!("cbnz x11, {float_label}"));       // a non-zero remainder yields a double
            ctx.emitter.instruction("mov x1, x9");                              // integer payload for the boxed quotient
            ctx.emitter.instruction("mov x2, xzr");                             // integer payloads do not use a high word
            ctx.emitter.instruction("mov x0, #0");                              // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            abi::emit_jump(ctx.emitter, &done_label);
            ctx.emitter.label(&float_label);
            ctx.emitter.instruction(&format!("mov x1, {scratch_reg}"));         // reload the divisor for the double conversion
            ctx.emitter.instruction(&format!("scvtf d0, {lhs_reg}"));           // convert the dividend to double
            ctx.emitter.instruction("scvtf d1, x1");                            // convert the divisor to double
            ctx.emitter.instruction("fdiv d0, d0, d1");                         // build PHP's promoted float quotient
            ctx.emitter.instruction("fmov x1, d0");                             // float payload for the boxed quotient
            ctx.emitter.instruction("mov x2, xzr");                             // float payloads do not use a high word
            ctx.emitter.instruction("mov x0, #2");                              // runtime tag 2 = double
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            abi::emit_jump(ctx.emitter, &done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov r8, {lhs_reg}"));             // preserve the dividend for the promoted float path
            ctx.emitter.instruction(&format!("test {scratch_reg}, {scratch_reg}")); // a zero divisor raises DivisionByZeroError
            ctx.emitter.instruction(&format!("jz {zero_label}"));               // a zero divisor raises DivisionByZeroError
            abi::emit_load_int_immediate(ctx.emitter, "r11", i64::MIN);
            ctx.emitter.instruction(&format!("cmp {lhs_reg}, r11"));            // is the dividend INT64_MIN?
            ctx.emitter.instruction(&format!("jne {int_label}"));               // every other dividend is safe to divide
            ctx.emitter.instruction(&format!("cmp {scratch_reg}, -1"));         // does INT64_MIN divide by -1?
            ctx.emitter.instruction(&format!("je {float_label}"));              // that quotient would trap the hardware divide
            ctx.emitter.label(&int_label);
            ctx.emitter.instruction(&format!("mov rsi, {scratch_reg}"));        // place the divisor in the second SysV argument register
            ctx.emitter.instruction("cqo");                                     // sign-extend the dividend before signed division
            ctx.emitter.instruction("idiv rsi");                                // quotient in rax, remainder in rdx
            ctx.emitter.instruction("test rdx, rdx");                           // is the division exact?
            ctx.emitter.instruction(&format!("jnz {float_label}"));             // a non-zero remainder yields a double
            ctx.emitter.instruction("mov rdi, rax");                            // integer payload for the boxed quotient
            ctx.emitter.instruction("xor esi, esi");                            // integer payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            abi::emit_jump(ctx.emitter, &done_label);
            ctx.emitter.label(&float_label);
            ctx.emitter.instruction(&format!("mov rsi, {scratch_reg}"));        // reload the divisor for the double conversion
            ctx.emitter.instruction("cvtsi2sd xmm0, r8");                       // convert the original dividend to double
            ctx.emitter.instruction("cvtsi2sd xmm1, rsi");                      // convert the divisor to double
            ctx.emitter.instruction("divsd xmm0, xmm1");                        // build PHP's promoted float quotient
            ctx.emitter.instruction("movq rdi, xmm0");                          // float payload for the boxed quotient
            ctx.emitter.instruction("xor esi, esi");                            // float payloads do not use a high word
            ctx.emitter.instruction("mov eax, 2");                              // runtime tag 2 = double
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            abi::emit_jump(ctx.emitter, &done_label);
        }
    }
    ctx.emitter.label(&zero_label);
    super::exceptions::emit_division_by_zero_error(ctx, DIVISION_BY_ZERO_MESSAGE);
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
    if let Some(offset) = inst.result.and_then(|value| ctx.numeric_cell_offsets.get(&value).copied()) {
        return stack_cell::lower_mixed(ctx, inst, offset);
    }
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let op = expect_mixed_numeric_op(inst)?;
    let lhs_ty = ctx.value_php_type(lhs)?;
    let rhs_ty = ctx.value_php_type(rhs)?;
    if matches!(op, MixedNumericOp::Add | MixedNumericOp::Sub | MixedNumericOp::Mul)
        && is_mixed_like(&lhs_ty)
        && ctx.value_ir_type(lhs)? == IrType::Heap(IrHeapKind::Mixed)
    {
        let rhs_reg = match ctx.emitter.target.arch {
            Arch::AArch64 => "x1",
            Arch::X86_64 => "rdi",
        };
        if let Some(integer) = integer_rhs_constant(ctx.function, rhs) {
            ctx.load_value_to_result(lhs)?;
            abi::emit_load_int_immediate(ctx.emitter, rhs_reg, integer);
            assert!(abi::emit_mixed_numeric_integer_rhs_call(ctx.emitter, op));
            return store_if_result(ctx, inst);
        }
        // A proven scalar counter is already an integer payload. Lend it to the same
        // read-only integer-rhs helper instead of allocating a Mixed box for it.
        if ctx.value_ir_type(rhs)? == IrType::I64
            && matches!(rhs_ty.codegen_repr(), PhpType::Int | PhpType::Bool)
        {
            ctx.load_value_to_result(lhs)?;
            ctx.load_value_to_reg(rhs, rhs_reg)?;
            assert!(abi::emit_mixed_numeric_integer_rhs_call(ctx.emitter, op));
            return store_if_result(ctx, inst);
        }
    }
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

/// Requires an actual PHP integer constant, excluding bool/null values sharing I64 storage.
fn integer_rhs_constant(function: &Function, value: ValueId) -> Option<i64> {
    let metadata = function.value(value)?;
    if metadata.php_type != PhpType::Int || metadata.ir_type != IrType::I64 {
        return None;
    }
    let ValueDef::Instruction { inst, .. } = metadata.def else { return None };
    let definition = function.instruction(inst)?;
    match (&definition.op, &definition.immediate) {
        (Op::ConstI64, Some(Immediate::I64(integer))) => Some(*integer),
        _ => None,
    }
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
