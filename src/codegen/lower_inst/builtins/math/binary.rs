//! Purpose:
//! Lowers binary numeric PHP builtins for the EIR backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::math`.
//!
//! Key details:
//! - Preserves PHP source evaluation order before arranging libc/ABI argument
//!   registers for integer division and floating-point helpers.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, Op, ValueDef};
use crate::types::PhpType;

use super::super::super::super::context::FunctionContext;
use super::super::{expect_operand, store_if_result};


/// Lowers `intdiv()` for concrete integer-like numeric operands.
pub(crate) fn lower_intdiv(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "intdiv", 2)?;
    let zero_label = ctx.next_label("intdiv_zero");
    let overflow_label = ctx.next_label("intdiv_overflow");
    let done_label = ctx.next_label("intdiv_done");
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    // A nonzero literal divisor other than -1 can never raise intdiv()'s zero-divisor
    // ArithmeticError or its `INT_MIN / -1` overflow, so the cold checks disappear.
    if let Some(divisor) = literal_int_operand(ctx, rhs) {
        if divisor != 0 && divisor != -1 {
            return lower_intdiv_constant(ctx, lhs, divisor, inst);
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_numeric_as_int(ctx, lhs, "intdiv")?;
            abi::emit_push_reg(ctx.emitter, "x0");
            load_numeric_as_int(ctx, rhs, "intdiv")?;
            abi::emit_pop_reg(ctx.emitter, "x1");
            ctx.emitter.instruction(&format!("cbz x0, {}", zero_label));        // branch to the fatal path when the divisor is zero
            emit_intdiv_overflow_check_arm64(ctx, "x1", "x0", &overflow_label);
            ctx.emitter.instruction("sdiv x0, x1, x0");                         // divide the saved dividend by the current divisor
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the fatal path after successful integer division
        }
        Arch::X86_64 => {
            load_numeric_as_int(ctx, lhs, "intdiv")?;
            abi::emit_push_reg(ctx.emitter, "rax");
            load_numeric_as_int(ctx, rhs, "intdiv")?;
            abi::emit_pop_reg(ctx.emitter, "r11");
            ctx.emitter.instruction("test rax, rax");                           // check whether the divisor is zero
            ctx.emitter.instruction(&format!("je {}", zero_label));             // branch to the fatal path when the divisor is zero
            emit_intdiv_overflow_check_x86_64(ctx, "r11", "rax", &overflow_label);
            ctx.emitter.instruction("mov r10, rax");                            // preserve the divisor before idiv uses rax
            ctx.emitter.instruction("mov rax, r11");                            // move the saved dividend into the idiv accumulator
            ctx.emitter.instruction("cqo");                                     // sign-extend the dividend across rdx:rax
            ctx.emitter.instruction("idiv r10");                                // divide the saved dividend by the preserved divisor
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the fatal path after successful integer division
        }
    }
    ctx.emitter.label(&zero_label);
    emit_intdiv_zero_throw(ctx);
    ctx.emitter.label(&overflow_label);
    emit_intdiv_overflow_throw(ctx);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `intdiv()` with a literal divisor that cannot trigger either cold error path.
fn lower_intdiv_constant(
    ctx: &mut FunctionContext<'_>,
    lhs: crate::ir::ValueId,
    divisor: i64,
    inst: &Instruction,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_numeric_as_int(ctx, lhs, "intdiv")?;
            abi::emit_load_int_immediate(ctx.emitter, "x1", divisor);          // materialize the literal divisor after loading the dividend
            ctx.emitter.instruction("sdiv x0, x0, x1");                         // divide the dividend by the proven-safe literal divisor
        }
        Arch::X86_64 => {
            load_numeric_as_int(ctx, lhs, "intdiv")?;
            abi::emit_load_int_immediate(ctx.emitter, "r11", divisor);         // materialize the literal divisor after loading the dividend
            ctx.emitter.instruction("cqo");                                     // sign-extend the dividend across rdx:rax
            ctx.emitter.instruction("idiv r11");                                // divide by the proven-safe literal divisor
        }
    }
    store_if_result(ctx, inst)
}

/// Returns the literal integer an operand was defined with, when it is a `const_i64`.
fn literal_int_operand(ctx: &FunctionContext<'_>, value: crate::ir::ValueId) -> Option<i64> {
    let ValueDef::Instruction { inst, .. } = ctx.function.value(value)?.def else {
        return None;
    };
    let definition = ctx.function.instruction(inst)?;
    if definition.op != Op::ConstI64 {
        return None;
    }
    match definition.immediate {
        Some(Immediate::I64(literal)) => Some(literal),
        _ => None,
    }
}

/// Lowers `fdiv()` for concrete integer-like and floating operands.
pub(crate) fn lower_fdiv(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "fdiv", 2)?;
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    super::load_numeric_as_float(ctx, lhs, "fdiv")?;
    abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
    super::load_numeric_as_float(ctx, rhs, "fdiv")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_float_reg(ctx.emitter, "d1");
            ctx.emitter.instruction("fdiv d0, d1, d0");                         // compute dividend divided by divisor in the result register
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(ctx.emitter, "xmm1");
            ctx.emitter.instruction("divsd xmm1, xmm0");                        // compute dividend divided by divisor in the scratch register
            ctx.emitter.instruction("movsd xmm0, xmm1");                        // move the floating quotient into the result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `fmod()` for concrete integer-like and floating operands.
///
/// Both targets call libc `fmod`, which is IEEE-754 `remainder`-truncated: the result
/// carries the sign of the *dividend*, so `fmod(-7.5, 2.5)` is `-0.0` and `echo`s as `-0`
/// exactly like PHP. Recomputing it as `x - trunc(x / y) * y` loses that signed zero
/// (the subtraction yields `+0.0`), which is why the AArch64 path is not open-coded.
pub(crate) fn lower_fmod(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "fmod", 2)?;
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    super::load_numeric_as_float(ctx, lhs, "fmod")?;
    abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
    super::load_numeric_as_float(ctx, rhs, "fmod")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov d1, d0");                             // move the divisor into the second libc fmod argument
            abi::emit_pop_float_reg(ctx.emitter, "d0");
            ctx.emitter.bl_c("fmod");
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(ctx.emitter, "xmm1");
            ctx.emitter.instruction("movapd xmm2, xmm0");                       // preserve the divisor while ordering libc fmod arguments
            ctx.emitter.instruction("movapd xmm0, xmm1");                       // move the dividend into the first libc fmod argument
            ctx.emitter.instruction("movapd xmm1, xmm2");                       // move the divisor into the second libc fmod argument
            ctx.emitter.bl_c("fmod");
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `pow()` for concrete integer-like and floating operands.
pub(crate) fn lower_pow(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "pow", 2)?;
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    super::load_numeric_as_float(ctx, lhs, "pow")?;
    abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
    super::load_numeric_as_float(ctx, rhs, "pow")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov d1, d0");                             // move the exponent into the second libc pow argument
            abi::emit_pop_float_reg(ctx.emitter, "d0");
            ctx.emitter.bl_c("pow");
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(ctx.emitter, "xmm1");
            ctx.emitter.instruction("movapd xmm2, xmm0");                       // preserve the exponent while ordering libc pow arguments
            ctx.emitter.instruction("movapd xmm0, xmm1");                       // move the base into the first libc pow argument
            ctx.emitter.instruction("movapd xmm1, xmm2");                       // move the exponent into the second libc pow argument
            ctx.emitter.bl_c("pow");
        }
    }
    store_if_result(ctx, inst)
}

/// Emits the catchable `DivisionByZeroError` throw for `intdiv($a, 0)`.
///
/// Reference PHP 8.5.6 raises `DivisionByZeroError: Division by zero` here, catchable by
/// `DivisionByZeroError`, `ArithmeticError`, `Error`, and `Throwable`. Until this
/// increment elephc wrote a bare `Fatal error: division by zero` to stderr and exited 1,
/// so no `catch` clause could ever observe it. The shared `emit_static_exception` path
/// keeps a specific uncaught diagnostic (`Fatal error: Uncaught DivisionByZeroError:
/// Division by zero`) when no handler is active, so the uncaught case stays a fatal —
/// it just names the PHP class now.
fn emit_intdiv_zero_throw(ctx: &mut FunctionContext<'_>) {
    crate::codegen::lower_inst::exceptions::emit_division_by_zero_error(ctx, "Division by zero");
}

/// Emits the AArch64 overflow check for `intdiv(PHP_INT_MIN, -1)`.
/// Branches to `overflow_label` when `dividend == i64::MIN && divisor == -1`.
fn emit_intdiv_overflow_check_arm64(
    ctx: &mut FunctionContext<'_>,
    dividend_reg: &str,
    divisor_reg: &str,
    overflow_label: &str,
) {
    let int_min_label = ctx.next_label("intdiv_int_min");
    let neg_one = abi::symbol_scratch_reg(ctx.emitter);
    ctx.emitter.instruction(&format!("mov {}, #-1", neg_one));                  // materialize -1 for comparison
    ctx.emitter.instruction(&format!("cmp {}, {}", divisor_reg, neg_one));      // check whether the divisor is -1
    ctx.emitter.instruction(&format!("b.ne {}", int_min_label));                // skip overflow check when the divisor is not -1
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, scratch, i64::MIN);
    ctx.emitter.instruction(&format!("cmp {}, {}", dividend_reg, scratch));     // check whether the dividend is PHP_INT_MIN
    ctx.emitter.instruction(&format!("b.eq {}", overflow_label));               // branch to the overflow throw when INT_MIN / -1
    ctx.emitter.label(&int_min_label);
}

/// Emits the x86_64 overflow check for `intdiv(PHP_INT_MIN, -1)`.
/// Branches to `overflow_label` when `dividend == i64::MIN && divisor == -1`.
fn emit_intdiv_overflow_check_x86_64(
    ctx: &mut FunctionContext<'_>,
    dividend_reg: &str,
    divisor_reg: &str,
    overflow_label: &str,
) {
    let not_neg_one_label = ctx.next_label("intdiv_not_neg_one");
    ctx.emitter.instruction(&format!("cmp {}, -1", divisor_reg));               // check whether the divisor is -1
    ctx.emitter.instruction(&format!("jne {}", not_neg_one_label));             // skip overflow check when the divisor is not -1
    ctx.emitter.instruction(&format!("mov r10, 0x8000000000000000"));           // materialize i64::MIN (PHP_INT_MIN)
    ctx.emitter.instruction(&format!("cmp {}, r10", dividend_reg));             // check whether the dividend is PHP_INT_MIN
    ctx.emitter.instruction(&format!("je {}", overflow_label));                 // branch to the overflow throw when INT_MIN / -1
    ctx.emitter.label(&not_neg_one_label);
}

/// Emits a catchable `ArithmeticError` throw for `intdiv(PHP_INT_MIN, -1)`.
fn emit_intdiv_overflow_throw(ctx: &mut FunctionContext<'_>) {
    crate::codegen::lower_inst::exceptions::emit_arithmetic_error(
        ctx, "Division of PHP_INT_MIN by -1 is not an integer",
    );
}

/// Loads a numeric operand and normalizes values into the integer result register.
fn load_numeric_as_int(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    name: &str,
) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        PhpType::Float => {
            abi::emit_float_result_to_int_result(ctx.emitter);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}
