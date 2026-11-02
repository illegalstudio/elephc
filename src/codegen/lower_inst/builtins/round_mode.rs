//! Purpose:
//! Lowers `round($num, $precision, $mode)` — the precision-carrying forms of PHP's `round()` —
//! onto the shared `__rt_round_mode` runtime routine.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_07` for `RuntimeFnId::Round`.
//!
//! Key details:
//! - The single-argument form still lowers through
//!   `crate::codegen::lower_inst::builtins::math::lower_round()`, which emits the target's
//!   native ties-away-from-zero instruction. That is exactly `PHP_ROUND_HALF_UP` at precision
//!   zero, so both paths agree.
//! - Two- and three-argument calls go through `__rt_round_mode`, a port of php-src 8.4's
//!   `_php_math_round()`. The runtime routine — not this lowering — owns the tie-breaking and
//!   the integral-part correction; this file only materializes the ABI and the argument guard.
//! - `$mode` is validated HERE rather than in the runtime routine so the failure raises PHP's
//!   catchable `ValueError` through the ordinary codegen exception path, exactly like
//!   `str_pad()`'s `$pad_type` guard.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use crate::codegen::{CodegenIrError, Result};

use super::super::super::context::FunctionContext;
use super::super::load_value_to_first_int_arg;
use super::{ensure_arg_count_between, expect_operand, store_if_result};

/// php-src's verbatim `ValueError` wording for an unknown `round()` rounding mode.
const ROUND_MODE_MESSAGE: &str =
    "round(): Argument #3 ($mode) must be a valid rounding mode (RoundingMode::*)";

/// The lowest php-src rounding-mode integer (`PHP_ROUND_HALF_UP`).
const ROUND_MODE_MIN: i64 = 1;

/// The highest php-src rounding-mode integer (`RoundingMode::AwayFromZero`).
const ROUND_MODE_MAX: i64 = 8;

/// Lowers every arity of PHP's `round()`.
///
/// One operand keeps the native single-instruction path; two or three operands materialize
/// `(value, precision, mode)` for `__rt_round_mode` and raise `ValueError` for a `$mode`
/// outside php-src's `1..=8` enumeration.
pub(crate) fn lower_round(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "round", 1, 3)?;
    if inst.operands.len() == 1 {
        return super::math::lower_round(ctx, inst);
    }
    let value = expect_operand(inst, 0)?;
    let precision = expect_operand(inst, 1)?;
    let mode = if inst.operands.len() == 3 {
        Some(expect_operand(inst, 2)?)
    } else {
        None
    };

    // PHP evaluates every argument before validating `$mode`, and the operands are already
    // lowered SSA values here, so the guard can sit next to the ABI materialization.
    load_precision_as_int(ctx, precision)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_mode_operand(ctx, mode)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_numeric_as_float(ctx, value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_round_mode");
    store_if_result(ctx, inst)
}

/// Materializes `$mode` in the integer result register and rejects values PHP refuses.
///
/// An omitted `$mode` is `PHP_ROUND_HALF_UP`, so a two-argument call materializes the literal
/// `1` and needs no guard. A supplied mode is range-checked against php-src's `1..=8`
/// enumeration while it still sits in the integer result register.
fn emit_mode_operand(ctx: &mut FunctionContext<'_>, mode: Option<ValueId>) -> Result<()> {
    let Some(mode) = mode else {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            ROUND_MODE_MIN,
        );
        return Ok(());
    };
    load_precision_as_int(ctx, mode)?;
    let mode_reg = abi::int_result_reg(ctx.emitter);
    crate::codegen::lower_inst::exceptions::emit_value_error_unless(
        ctx,
        crate::codegen::lower_inst::exceptions::ValueGuard::SignedInRange(
            mode_reg,
            ROUND_MODE_MIN,
            ROUND_MODE_MAX,
        ),
        ROUND_MODE_MESSAGE,
    );
    Ok(())
}

/// Loads a `round()` integer operand (`$precision` or `$mode`) into the integer result register.
///
/// Mirrors PHP's `int` parameter coercion for the representations the backend can carry:
/// integers and booleans are already integral, `null` coerces to `0`, and a float or boxed
/// `Mixed` goes through the shared PHP float→int conversion.
fn load_precision_as_int(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        PhpType::Float => {
            abi::emit_float_result_to_int_result(ctx.emitter);
            Ok(())
        }
        // A nullable integer (`?int` from a helper that answers `null` for "not set") coerces
        // like `null` does: `0`.
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "round integer argument for PHP type {:?}",
            other
        ))),
    }
}

/// Loads `round()`'s `$num` operand into the floating-point result register.
///
/// Reproduces `math::load_numeric_as_float()` for the operand shapes `round()` accepts:
/// concrete floats pass through, integers and booleans convert, `null` becomes `0.0`, and
/// boxed `Mixed` goes through the shared runtime float coercion.
fn load_numeric_as_float(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Float => Ok(()),
        PhpType::Int | PhpType::Bool => {
            abi::emit_int_result_to_float_result(ctx.emitter);
            Ok(())
        }
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            abi::emit_int_result_to_float_result(ctx.emitter);
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "round for PHP type {:?}",
            other
        ))),
    }
}
