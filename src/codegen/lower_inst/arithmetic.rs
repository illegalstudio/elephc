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
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    load_integer_operand(ctx, lhs, result_reg, inst)?;
    load_integer_operand(ctx, rhs, rhs_reg, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(
                &format!("{} {}, {}, {}", aarch64_mnemonic, result_reg, result_reg, rhs_reg)
            );                                                                  // compute the integer arithmetic result from both SSA operands
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(
                &format!("{} {}, {}", x86_64_mnemonic, result_reg, rhs_reg)
            );                                                                  // update the integer result register with the arithmetic operand
        }
    }
    store_if_result(ctx, inst)
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
            ctx.emitter.instruction(&format!("add {}, {}", bits_reg, bits_reg));// shift out the sign bit so -0.0 tests equal to +0.0 (NaN stays non-zero)
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
    let op = expect_mixed_numeric_op(inst)?;
    let lhs_ty = ctx.value_php_type(lhs)?;
    if op == MixedNumericOp::UnaryPlus {
        return lower_mixed_unary_plus(ctx, inst, lhs, &lhs_ty);
    }
    let rhs = expect_operand(inst, 1)?;
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

/// Lowers PHP unary plus for operands whose runtime value can change the result or throw.
fn lower_mixed_unary_plus(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operand: ValueId,
    operand_ty: &PhpType,
) -> Result<()> {
    match operand_ty {
        PhpType::Str => {
            ctx.load_value_to_result(operand)?;
            emit_unary_plus_string(ctx, inst, None, None);
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            super::exceptions::emit_type_error(ctx, "Unsupported operand types: array * int");
        }
        PhpType::Object(class_name) => {
            super::exceptions::emit_type_error(
                ctx,
                &format!(
                    "Unsupported operand types: {} * int",
                    class_name.trim_start_matches('\\')
                ),
            );
        }
        PhpType::Resource(_) => {
            super::exceptions::emit_type_error(ctx, "Unsupported operand types: resource * int");
        }
        PhpType::Callable => {
            super::exceptions::emit_type_error(ctx, "Unsupported operand types: Closure * int");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_dynamic_unary_plus(ctx, inst, operand, operand_ty, false)?;
        }
        PhpType::Iterable => {
            emit_dynamic_unary_plus(ctx, inst, operand, operand_ty, true)?;
        }
        other => {
            return Err(CodegenIrError::invalid_module(format!(
                "unary plus runtime lowering received PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Converts one string operand with PHP's numeric-string grammar or throws its exact TypeError.
fn emit_unary_plus_string(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    boxed_temp: Option<bool>,
    done: Option<&str>,
) {
    let numeric = ctx.next_label("unary_plus_string_numeric");
    let leading_numeric = ctx.next_label("unary_plus_string_leading_numeric");
    let invalid = ctx.next_label("unary_plus_string_invalid");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    abi::emit_call_label(ctx.emitter, "__rt_cstr");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(ctx.emitter, "__rt_php_num_scan");
            ctx.emitter.instruction(&format!("cbnz x1, {}", numeric));
            ctx.emitter.instruction("ldrb w9, [x0]");                          // an empty clipped run means the string has no numeric prefix
            ctx.emitter.instruction(&format!("cbnz w9, {}", leading_numeric));
            abi::emit_jump(ctx.emitter, &invalid);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                           // pass the C-string scratch pointer to the PHP numeric scanner
            abi::emit_call_label(ctx.emitter, "__rt_php_num_scan");
            ctx.emitter.instruction("test rdx, rdx");
            ctx.emitter.instruction(&format!("jnz {}", numeric));
            ctx.emitter.instruction("cmp BYTE PTR [rax], 0");                  // an empty clipped run means the string has no numeric prefix
            ctx.emitter.instruction(&format!("jne {}", leading_numeric));
            abi::emit_jump(ctx.emitter, &invalid);
        }
    }

    ctx.emitter.label(&leading_numeric);
    emit_unary_plus_non_numeric_warning(ctx, inst);
    abi::emit_jump(ctx.emitter, &numeric);

    ctx.emitter.label(&invalid);
    abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
    if let Some(is_temp) = boxed_temp {
        cleanup_dynamic_unary_plus_operand(ctx, is_temp);
    }
    super::exceptions::emit_type_error(ctx, "Unsupported operand types: string * int");

    ctx.emitter.label(&numeric);
    abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x3, xzr"),              // a zero delta converts the numeric string without changing its value
        Arch::X86_64 => ctx.emitter.instruction("xor ecx, ecx"),               // a zero delta converts the numeric string without changing its value
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_inc_dec");
    if let Some(is_temp) = boxed_temp {
        finish_dynamic_unary_plus_result(ctx, is_temp);
    }
    if let Some(done) = done {
        abi::emit_jump(ctx.emitter, done);
    }
}

/// Dispatches unary plus by the runtime tag of a boxed Mixed or Iterable operand.
fn emit_dynamic_unary_plus(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operand: ValueId,
    operand_ty: &PhpType,
    boxed_temp: bool,
) -> Result<()> {
    let scalar = ctx.next_label("unary_plus_scalar");
    let string = ctx.next_label("unary_plus_string");
    let boolean = ctx.next_label("unary_plus_bool");
    let null = ctx.next_label("unary_plus_null");
    let array = ctx.next_label("unary_plus_array");
    let object = ctx.next_label("unary_plus_object");
    let resource = ctx.next_label("unary_plus_resource");
    let closure = ctx.next_label("unary_plus_closure");
    let done = ctx.next_label("unary_plus_done");

    materialize_value_as_mixed(ctx, operand, operand_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_unary_plus_tag_branch(ctx, 0, &scalar);
    emit_unary_plus_tag_branch(ctx, 2, &scalar);
    emit_unary_plus_tag_branch(ctx, 1, &string);
    emit_unary_plus_tag_branch(ctx, 3, &boolean);
    emit_unary_plus_tag_branch(ctx, 4, &array);
    emit_unary_plus_tag_branch(ctx, 5, &array);
    emit_unary_plus_tag_branch(ctx, 6, &object);
    emit_unary_plus_tag_branch(ctx, 8, &null);
    emit_unary_plus_tag_branch(ctx, 9, &resource);
    emit_unary_plus_tag_branch(ctx, 10, &closure);
    cleanup_dynamic_unary_plus_operand(ctx, boxed_temp);
    super::exceptions::emit_type_error(ctx, "Unsupported operand types: unknown * int");

    ctx.emitter.label(&scalar);
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rsi, rdx");                               // adapt unboxed high payload to the Mixed boxer ABI
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    finish_dynamic_unary_plus_result(ctx, boxed_temp);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&boolean);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #0");                             // unary plus converts bool to PHP int
            ctx.emitter.instruction("mov x2, xzr");                            // integer boxes have no high payload
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, 0");                             // unary plus converts bool to PHP int
            ctx.emitter.instruction("xor esi, esi");                           // integer boxes have no high payload
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    finish_dynamic_unary_plus_result(ctx, boxed_temp);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&null);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #0");                             // unary plus converts null to integer zero
            ctx.emitter.instruction("mov x1, xzr");
            ctx.emitter.instruction("mov x2, xzr");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, 0");                             // unary plus converts null to integer zero
            ctx.emitter.instruction("xor edi, edi");
            ctx.emitter.instruction("xor esi, esi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    finish_dynamic_unary_plus_result(ctx, boxed_temp);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&string);
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rax, rdi");                               // move the unboxed pointer into the string-result register
    }
    emit_unary_plus_string(ctx, inst, Some(boxed_temp), Some(&done));

    ctx.emitter.label(&array);
    cleanup_dynamic_unary_plus_operand(ctx, boxed_temp);
    super::exceptions::emit_type_error(ctx, "Unsupported operand types: array * int");

    ctx.emitter.label(&resource);
    cleanup_dynamic_unary_plus_operand(ctx, boxed_temp);
    super::exceptions::emit_type_error(ctx, "Unsupported operand types: resource * int");

    ctx.emitter.label(&closure);
    cleanup_dynamic_unary_plus_operand(ctx, boxed_temp);
    super::exceptions::emit_type_error(ctx, "Unsupported operand types: Closure * int");

    ctx.emitter.label(&object);
    emit_dynamic_unary_plus_object_error(ctx, boxed_temp);
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits PHP's suppression-aware warning for a string with a trailing nonnumeric suffix.
fn emit_unary_plus_non_numeric_warning(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    let masked = ctx.next_label("unary_plus_warning_masked");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_rt_error_reporting", 0);
            ctx.emitter.instruction("tst x9, #2");                              // E_WARNING must be enabled in the active mask
            ctx.emitter.instruction(&format!("b.eq {}", masked));
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_rt_error_reporting", 0);
            ctx.emitter.instruction("test r10, 2");                             // E_WARNING must be enabled in the active mask
            ctx.emitter.instruction(&format!("jz {}", masked));
        }
    }
    emit_unary_plus_warning_fragment(ctx, b"\nWarning: A non-numeric value encountered");
    if let Some(span) = inst.span.filter(|span| span.line > 0) {
        let source = ctx.module.source_path.as_deref().unwrap_or("Unknown");
        emit_unary_plus_warning_fragment(ctx, format!(" in {source} on line ").as_bytes());
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            i64::from(span.line),
        );
        abi::emit_call_label(ctx.emitter, "__rt_itoa");
        if ctx.emitter.target.arch == Arch::X86_64 {
            ctx.emitter.instruction("mov rdi, rax");                           // pass the rendered line pointer to the diagnostic writer
            ctx.emitter.instruction("mov rsi, rdx");                           // pass the rendered line length to the diagnostic writer
        }
        abi::emit_call_label(ctx.emitter, "__rt_diag_write");
    }
    emit_unary_plus_warning_fragment(ctx, b"\n");
    ctx.emitter.label(&masked);
}

/// Writes one static unary-plus warning fragment through the diagnostic runtime.
fn emit_unary_plus_warning_fragment(ctx: &mut FunctionContext<'_>, fragment: &[u8]) {
    let (label, len) = ctx.data.add_string(fragment);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_write");
}

/// Emits a branch when the unboxed Mixed tag matches one unary-plus runtime case.
fn emit_unary_plus_tag_branch(ctx: &mut FunctionContext<'_>, tag: u8, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{}", tag));
            ctx.emitter.instruction(&format!("b.eq {}", label));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {}", tag));
            ctx.emitter.instruction(&format!("je {}", label));
        }
    }
}

/// Releases the saved operand slot, including a temporary box created for Iterable values.
fn cleanup_dynamic_unary_plus_operand(ctx: &mut FunctionContext<'_>, boxed_temp: bool) {
    if boxed_temp {
        decref_mixed_temp_at(ctx, 0);
    }
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Preserves a newly boxed result while releasing the saved unary-plus operand slot.
fn finish_dynamic_unary_plus_result(ctx: &mut FunctionContext<'_>, boxed_temp: bool) {
    if boxed_temp {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        decref_mixed_temp_at(ctx, 16);
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    }
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Builds `<runtime class> * int` and throws the catchable unary-plus TypeError.
fn emit_dynamic_unary_plus_object_error(ctx: &mut FunctionContext<'_>, boxed_temp: bool) {
    let (name_ptr, name_len) = abi::string_result_regs(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x1]");                           // load the runtime class id from the object payload
            abi::emit_symbol_address(ctx.emitter, "x10", "_class_name_entries");
            ctx.emitter.instruction("lsl x11, x9, #4");                        // scale the class id to its 16-byte metadata row
            ctx.emitter.instruction("add x10, x10, x11");
            ctx.emitter.instruction(&format!("ldr {}, [x10]", name_ptr));
            ctx.emitter.instruction(&format!("ldr {}, [x10, #8]", name_len));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rdi]");                // load the runtime class id from the object payload
            abi::emit_symbol_address(ctx.emitter, "r10", "_class_name_entries");
            ctx.emitter.instruction("shl r9, 4");                              // scale the class id to its 16-byte metadata row
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [r10 + r9]", name_ptr));
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [r10 + r9 + 8]", name_len));
        }
    }
    emit_unary_plus_message_prefix(ctx, "Unsupported operand types: ");
    emit_unary_plus_message_suffix(ctx, " * int");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    abi::emit_push_reg_pair(ctx.emitter, name_ptr, name_len);
    if boxed_temp {
        decref_mixed_temp_at(ctx, 16);
    }
    abi::emit_pop_reg_pair(ctx.emitter, name_ptr, name_len);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    super::exceptions::emit_type_error_from_string_result(ctx);
}

/// Prepends one static fragment to the current runtime-built unary-plus message.
fn emit_unary_plus_message_prefix(ctx: &mut FunctionContext<'_>, prefix: &str) {
    let (text_ptr, text_len) = abi::string_result_regs(ctx.emitter);
    let (right_ptr, right_len) = unary_plus_concat_right_regs(ctx);
    let (prefix_label, prefix_len) = ctx.data.add_string(prefix.as_bytes());
    ctx.emitter.instruction(&format!("mov {}, {}", right_ptr, text_ptr));
    ctx.emitter.instruction(&format!("mov {}, {}", right_len, text_len));
    abi::emit_symbol_address(ctx.emitter, text_ptr, &prefix_label);
    abi::emit_load_int_immediate(ctx.emitter, text_len, prefix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
}

/// Appends one static fragment to the current runtime-built unary-plus message.
fn emit_unary_plus_message_suffix(ctx: &mut FunctionContext<'_>, suffix: &str) {
    let (right_ptr, right_len) = unary_plus_concat_right_regs(ctx);
    let (suffix_label, suffix_len) = ctx.data.add_string(suffix.as_bytes());
    abi::emit_symbol_address(ctx.emitter, right_ptr, &suffix_label);
    abi::emit_load_int_immediate(ctx.emitter, right_len, suffix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
}

/// Returns the target registers consumed by `__rt_concat` for its right operand.
fn unary_plus_concat_right_regs(ctx: &FunctionContext<'_>) -> (&'static str, &'static str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ("x3", "x4"),
        Arch::X86_64 => ("rdi", "rsi"),
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
        MixedNumericOp::UnaryPlus => unreachable!("unary plus is lowered before helper selection"),
    }
}
