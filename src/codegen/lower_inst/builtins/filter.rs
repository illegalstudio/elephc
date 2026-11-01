//! Purpose:
//! Lowers the literal-dispatched `filter_var()` synthetic builtins
//! (`filter_var$int[_nof]`, `filter_var$float[_nof]`, `filter_var$bool[_nof]`,
//! `filter_var$default[_nof]`, `filter_var$ip[4|6][_nof]`) produced by
//! `crate::ir_lower::expr::filter` into target-aware EIR backend code,
//! dispatching on the value operand's concrete PHP type and calling the
//! dedicated `__rt_filter_*` runtime parsers for string input.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Every path here returns a boxed `Mixed` value (the checker types
//!   `filter_var()` as `Mixed`): int/float/bool/string results box their
//!   concrete type; a failure boxes `Bool(false)` or, when the `_nof` variant
//!   is selected, `Void` (PHP `null`).
//! - `Bool(false)` input ALWAYS fails `VALIDATE_INT`/`VALIDATE_FLOAT` (a
//!   php-verified quirk — `Bool(true)` always succeeds as `1`/`1.0`) but is
//!   ALWAYS valid for `VALIDATE_BOOL(EAN)` (never a failure, since a bool is
//!   already unambiguous).
//! - `Array`/`AssocArray` input always fails (php-verified: without
//!   `FILTER_REQUIRE_ARRAY`/`FILTER_FORCE_ARRAY`, which the checker keeps loud,
//!   `filter_var()` on an array is always `false`/`null`, never a mis-validated
//!   passthrough).
//! - `Mixed` input unboxes via `__rt_mixed_unbox` and re-dispatches through the
//!   SAME per-type emission as the concretely-typed cases for tags
//!   int/string/float/bool/null; any other tag (array/hash/object/...) fails.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::emit_box_current_value_as_mixed;
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::{expect_operand, store_if_result};
use crate::codegen::{CodegenIrError, Result};

/// IEEE-754 bit pattern of `+2.0^63` — the exclusive upper bound for a float
/// value to fit `PHP_INT_MAX`. Any float `>= this` (and any float `< -this`)
/// cannot be represented as a PHP int (`(float) PHP_INT_MAX` itself rounds up
/// to exactly this value and is php-verified to fail `FILTER_VALIDATE_INT`).
const FLOAT_INT_UPPER_BOUND_BITS: i64 = 0x43E0_0000_0000_0000;
/// IEEE-754 bit pattern of `-2.0^63` (`PHP_INT_MIN` exactly, the inclusive lower bound).
const FLOAT_INT_LOWER_BOUND_BITS: i64 = 0xC3E0_0000_0000_0000_u64 as i64;
/// IEEE-754 bit pattern of `+infinity`, used to detect a NaN/Infinite float by
/// comparing `|value|` against it.
const FLOAT_INFINITY_BITS: i64 = 0x7FF0_0000_0000_0000;

/// Lowers `filter_var$int`/`filter_var$int_nof` (`FILTER_VALIDATE_INT`).
pub(super) fn lower_filter_var_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    null_on_failure: bool,
) -> Result<()> {
    super::ensure_arg_count(inst, "filter_var", 1)?;
    let value = expect_operand(inst, 0)?;
    emit_filter_var_int_result(ctx, value, null_on_failure)?;
    store_if_result(ctx, inst)
}

/// Emits the boxed `Mixed` `FILTER_VALIDATE_INT` result for `value` into the integer
/// result register, without storing it. Shared by `lower_filter_var_int` and the
/// range-constrained variant.
fn emit_filter_var_int_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    null_on_failure: bool,
) -> Result<()> {
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Int => {
            ctx.load_value_to_result(value)?;
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
        }
        PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            emit_int_from_loaded_bool(ctx, null_on_failure);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            emit_int_from_loaded_float(ctx, null_on_failure);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_filter_validate_int");
            emit_int_from_validate_int_result(ctx, null_on_failure);
        }
        PhpType::Void => emit_filter_failure(ctx, null_on_failure),
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            emit_filter_failure(ctx, null_on_failure)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_dispatch(ctx, value, null_on_failure, MixedFilterKind::Int)?
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "filter_var(FILTER_VALIDATE_INT) for PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Lowers `filter_var$int_range`/`filter_var$int_range_nof`: `FILTER_VALIDATE_INT`
/// with a compile-time-constant `min_range`/`max_range` constraint. Operands are the
/// value plus the (already-resolved, `PHP_INT_MIN`/`PHP_INT_MAX`-defaulted) bounds.
///
/// Produces the base `FILTER_VALIDATE_INT` boxed result, then hands it to
/// `__rt_filter_int_range`, which returns the result unchanged when it is a non-int
/// failure passthrough or an int within `[min, max]`, and otherwise releases it and
/// returns the range-failure value (`false`, or `null` for the `_nof` variant).
pub(super) fn lower_filter_var_int_range(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    null_on_failure: bool,
) -> Result<()> {
    super::ensure_arg_count(inst, "filter_var", 3)?;
    let value = expect_operand(inst, 0)?;
    let min = expect_operand(inst, 1)?;
    let max = expect_operand(inst, 2)?;
    emit_filter_var_int_result(ctx, value, null_on_failure)?;
    // Result box is in the integer result register (arg0 of `__rt_filter_int_range`);
    // load the bounds into the remaining argument registers (a `ConstI64` operand loads
    // as a direct frame-slot read, so the result register is preserved), then the fail
    // mode immediate.
    let fail_is_null = i64::from(null_on_failure);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(min, "x1")?;
            ctx.load_value_to_reg(max, "x2")?;
            abi::emit_load_int_immediate(ctx.emitter, "x3", fail_is_null);
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(min, "rdi")?;
            ctx.load_value_to_reg(max, "rsi")?;
            abi::emit_load_int_immediate(ctx.emitter, "rdx", fail_is_null);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_filter_int_range");
    store_if_result(ctx, inst)
}

/// Lowers `filter_var$float`/`filter_var$float_nof` (`FILTER_VALIDATE_FLOAT`).
pub(super) fn lower_filter_var_float(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    null_on_failure: bool,
) -> Result<()> {
    super::ensure_arg_count(inst, "filter_var", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Int => {
            ctx.load_value_to_result(value)?;
            emit_int_to_float_result(ctx);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Float);
        }
        PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            emit_float_from_loaded_bool(ctx, null_on_failure);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            emit_float_from_loaded_float(ctx, null_on_failure);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_filter_validate_float");
            emit_float_from_validate_float_result(ctx, null_on_failure);
        }
        PhpType::Void => emit_filter_failure(ctx, null_on_failure),
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            emit_filter_failure(ctx, null_on_failure)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_dispatch(ctx, value, null_on_failure, MixedFilterKind::Float)?
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "filter_var(FILTER_VALIDATE_FLOAT) for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `filter_var$bool`/`filter_var$bool_nof` (`FILTER_VALIDATE_BOOL(EAN)`).
pub(super) fn lower_filter_var_bool(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    null_on_failure: bool,
) -> Result<()> {
    super::ensure_arg_count(inst, "filter_var", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Bool => {
            // A bool input is always valid for the BOOL filter (never a failure).
            ctx.load_value_to_result(value)?;
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
        }
        PhpType::Int => {
            ctx.load_value_to_result(value)?;
            emit_bool_from_loaded_int(ctx, null_on_failure);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            emit_bool_from_loaded_float(ctx, null_on_failure);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_filter_validate_bool_str");
            emit_bool_from_validate_bool_result(ctx, null_on_failure);
        }
        PhpType::Void => {
            // null is treated like "" for the BOOL filter: a valid false, not a failure.
            emit_static_bool_mixed(ctx, false);
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            emit_filter_failure(ctx, null_on_failure)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_dispatch(ctx, value, null_on_failure, MixedFilterKind::Bool)?
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "filter_var(FILTER_VALIDATE_BOOL) for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `filter_var$default`/`filter_var$default_nof` (`FILTER_DEFAULT` ==
/// `FILTER_UNSAFE_RAW`): a passthrough that stringifies scalar/null input and
/// fails only on array input.
pub(super) fn lower_filter_var_default(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    null_on_failure: bool,
) -> Result<()> {
    super::ensure_arg_count(inst, "filter_var", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
        }
        PhpType::Int => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_ftoa");
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
        }
        PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            super::super::strings::lower_loaded_bool_to_string(ctx)?;
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
        }
        PhpType::Void => {
            emit_empty_string_result(ctx);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            emit_filter_failure(ctx, null_on_failure)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_default(ctx, value, null_on_failure)?
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "filter_var(FILTER_DEFAULT) for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `filter_var$ip`/`filter_var$ip_nof` (`FILTER_VALIDATE_IP` with
/// neither/both of `FILTER_FLAG_IPV4`/`FILTER_FLAG_IPV6` set — accepts either
/// address family).
pub(super) fn lower_filter_var_ip(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    null_on_failure: bool,
) -> Result<()> {
    lower_filter_var_ip_family(ctx, inst, IpFamily::Either, null_on_failure)
}

/// Lowers `filter_var$ip4`/`filter_var$ip4_nof` (`FILTER_VALIDATE_IP` with
/// `FILTER_FLAG_IPV4` — restricts to IPv4 literals).
pub(super) fn lower_filter_var_ip4(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    null_on_failure: bool,
) -> Result<()> {
    lower_filter_var_ip_family(ctx, inst, IpFamily::V4, null_on_failure)
}

/// Lowers `filter_var$ip6`/`filter_var$ip6_nof` (`FILTER_VALIDATE_IP` with
/// `FILTER_FLAG_IPV6` — restricts to IPv6 literals).
pub(super) fn lower_filter_var_ip6(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    null_on_failure: bool,
) -> Result<()> {
    lower_filter_var_ip_family(ctx, inst, IpFamily::V6, null_on_failure)
}

/// Which address family(ies) `FILTER_VALIDATE_IP` accepts, per the
/// `FILTER_FLAG_IPV4`/`FILTER_FLAG_IPV6` flags (php-verified matrix: either
/// flag alone restricts to that family, both or neither accept either).
#[derive(Clone, Copy)]
enum IpFamily {
    V4,
    V6,
    Either,
}

/// Shared `FILTER_VALIDATE_IP` lowering: only string (and Mixed-unboxed-string)
/// input can ever succeed — every other concrete type (int/float/bool/null/
/// array/assoc) is unconditionally NOT a valid IP literal (php-verified:
/// `filter_var(123, FILTER_VALIDATE_IP)` and friends are always `false`; unlike
/// `FILTER_VALIDATE_INT/FLOAT/BOOL`, there is no bool/int/float passthrough
/// special case here).
fn lower_filter_var_ip_family(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    family: IpFamily,
    null_on_failure: bool,
) -> Result<()> {
    super::ensure_arg_count(inst, "filter_var", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Str => emit_ip_validate_call(ctx, IpStringSource::Str(value), family, null_on_failure)?,
        PhpType::Int
        | PhpType::Float
        | PhpType::Bool
        | PhpType::Void
        | PhpType::Array(_)
        | PhpType::AssocArray { .. } => emit_filter_failure(ctx, null_on_failure),
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_ip_dispatch(ctx, value, family, null_on_failure)?
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "filter_var(FILTER_VALIDATE_IP) for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Where the candidate IP literal comes from: an already-Str-typed operand
/// (load directly), or a Mixed operand that must be unboxed and tag-checked
/// as a string on every (re-)load, since the validator calls below clobber
/// the scratch registers holding the previous load.
#[derive(Clone, Copy)]
enum IpStringSource {
    Str(ValueId),
    MixedString(ValueId),
}

/// Loads the candidate IP literal into the ABI string-result registers,
/// unboxing from a Mixed cell first when the source requires it.
fn load_ip_string_operand(ctx: &mut FunctionContext<'_>, source: IpStringSource) -> Result<()> {
    match source {
        IpStringSource::Str(value) => {
            ctx.load_value_to_result(value)?;
        }
        IpStringSource::MixedString(value) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");                  // x0/rax=tag, x1/rdi=lo, x2/rdx=hi
            move_mixed_string_payload_to_string_result(ctx);
        }
    }
    Ok(())
}

/// Calls the `__rt_filter_validate_ip4`/`ip6` runtime validators per `family`
/// (trying v4 then v6 for the `Either` case) and boxes the result: the
/// original string unmodified on success (php-verified: `FILTER_VALIDATE_IP`
/// never normalizes/trims a valid literal), or the filter failure otherwise.
fn emit_ip_validate_call(
    ctx: &mut FunctionContext<'_>,
    source: IpStringSource,
    family: IpFamily,
    null_on_failure: bool,
) -> Result<()> {
    let fail_label = ctx.next_label("filter_ip_fail");
    let done_label = ctx.next_label("filter_ip_done");

    match family {
        IpFamily::V4 => {
            load_ip_string_operand(ctx, source)?;
            abi::emit_call_label(ctx.emitter, "__rt_filter_validate_ip4");
            emit_branch_if_int_reg_zero(ctx, &fail_label);
        }
        IpFamily::V6 => {
            load_ip_string_operand(ctx, source)?;
            abi::emit_call_label(ctx.emitter, "__rt_filter_validate_ip6");
            emit_branch_if_int_reg_zero(ctx, &fail_label);
        }
        IpFamily::Either => {
            let v6_label = ctx.next_label("filter_ip_either_v6");
            let success_label = ctx.next_label("filter_ip_either_success");
            load_ip_string_operand(ctx, source)?;
            abi::emit_call_label(ctx.emitter, "__rt_filter_validate_ip4");
            emit_branch_if_int_reg_zero(ctx, &v6_label);
            abi::emit_jump(ctx.emitter, &success_label);
            ctx.emitter.label(&v6_label);
            load_ip_string_operand(ctx, source)?;                                   // reload: the v4 attempt clobbered the string registers
            abi::emit_call_label(ctx.emitter, "__rt_filter_validate_ip6");
            emit_branch_if_int_reg_zero(ctx, &fail_label);
            ctx.emitter.label(&success_label);
        }
    }

    // Success: reload the original literal (the validator call clobbered the
    // registers holding it) and box it unmodified.
    load_ip_string_operand(ctx, source)?;
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&fail_label);
    emit_filter_failure(ctx, null_on_failure);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Unboxes a Mixed value and dispatches `FILTER_VALIDATE_IP`: only the string
/// tag can succeed, every other tag (int/float/bool/null/array/hash/object)
/// fails unconditionally, mirroring the concretely-typed dispatch above.
fn emit_mixed_ip_dispatch(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    family: IpFamily,
    null_on_failure: bool,
) -> Result<()> {
    let str_label = ctx.next_label("filter_ip_mixed_str");
    let fail_label = ctx.next_label("filter_ip_mixed_fail");
    let done_label = ctx.next_label("filter_ip_mixed_done");

    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");                          // x0/rax=tag, x1/rdi=lo, x2/rdx=hi
    emit_branch_on_mixed_tag(ctx, 1, &str_label);
    abi::emit_jump(ctx.emitter, &fail_label);

    ctx.emitter.label(&str_label);
    emit_ip_validate_call(ctx, IpStringSource::MixedString(value), family, null_on_failure)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&fail_label);
    emit_filter_failure(ctx, null_on_failure);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Which filter kind a Mixed-tag dispatch resumes into after unboxing.
#[derive(Clone, Copy)]
enum MixedFilterKind {
    Int,
    Float,
    Bool,
}

/// Emits a boxed `false`/`null` failure result per `null_on_failure`.
fn emit_filter_failure(ctx: &mut FunctionContext<'_>, null_on_failure: bool) {
    if null_on_failure {
        emit_void_result(ctx);
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
    } else {
        emit_bool_immediate_result(ctx, false);
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    }
}

/// Emits a statically-known boxed `Bool` result (used for the BOOL filter's
/// null-input special case, which is always a valid `false`, never a failure).
fn emit_static_bool_mixed(ctx: &mut FunctionContext<'_>, value: bool) {
    emit_bool_immediate_result(ctx, value);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
}

/// Loads the PHP void/null sentinel into the integer result register (mirrors
/// the sentinel used across the codebase for a boxed `Void` payload).
fn emit_void_result(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
}

/// Loads a boolean immediate into the integer result register.
fn emit_bool_immediate_result(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), i64::from(value));
}

/// Sets the string result registers to an empty string (used for `null`'s
/// `FILTER_DEFAULT` passthrough, which stringifies to `""`).
fn emit_empty_string_result(ctx: &mut FunctionContext<'_>) {
    let len_reg = abi::string_result_regs(ctx.emitter).1;
    abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
}

/// Converts a loaded int (in the int result register) to the float result register.
fn emit_int_to_float_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("scvtf d0, x0"),               // convert the loaded PHP int to a double
        Arch::X86_64 => ctx.emitter.instruction("cvtsi2sd xmm0, rax"),          // convert the loaded PHP int to a double
    };
}

// -- Bool(loaded)-input handlers --------------------------------------------

/// `FILTER_VALIDATE_INT` on a loaded bool: `true` -> `1`, `false` always fails.
fn emit_int_from_loaded_bool(ctx: &mut FunctionContext<'_>, null_on_failure: bool) {
    let false_label = ctx.next_label("filter_int_bool_false");
    let done_label = ctx.next_label("filter_int_bool_done");
    emit_branch_if_int_reg_zero(ctx, &false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&false_label);
    emit_filter_failure(ctx, null_on_failure);
    ctx.emitter.label(&done_label);
}

/// `FILTER_VALIDATE_FLOAT` on a loaded bool: `true` -> `1.0`, `false` always fails.
fn emit_float_from_loaded_bool(ctx: &mut FunctionContext<'_>, null_on_failure: bool) {
    let false_label = ctx.next_label("filter_float_bool_false");
    let done_label = ctx.next_label("filter_float_bool_done");
    emit_branch_if_int_reg_zero(ctx, &false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    emit_int_to_float_result(ctx);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Float);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&false_label);
    emit_filter_failure(ctx, null_on_failure);
    ctx.emitter.label(&done_label);
}

/// `FILTER_VALIDATE_BOOL(EAN)` on a loaded int: only `0`/`1` are valid.
fn emit_bool_from_loaded_int(ctx: &mut FunctionContext<'_>, null_on_failure: bool) {
    let zero_label = ctx.next_label("filter_bool_int_zero");
    let one_label = ctx.next_label("filter_bool_int_one");
    let fail_label = ctx.next_label("filter_bool_int_fail");
    let done_label = ctx.next_label("filter_bool_int_done");
    emit_branch_if_int_reg_zero(ctx, &zero_label);
    emit_branch_if_int_reg_equals_one(ctx, &one_label);
    abi::emit_jump(ctx.emitter, &fail_label);
    ctx.emitter.label(&zero_label);
    emit_static_bool_mixed(ctx, false);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&one_label);
    emit_static_bool_mixed(ctx, true);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&fail_label);
    emit_filter_failure(ctx, null_on_failure);
    ctx.emitter.label(&done_label);
}

/// Branches to `label` when the loaded int result register is zero.
fn emit_branch_if_int_reg_zero(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbz x0, {}", label)),
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("je {}", label));
        }
    };
}

/// Branches to `label` when the loaded int result register equals exactly 1.
fn emit_branch_if_int_reg_equals_one(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");
            ctx.emitter.instruction(&format!("b.eq {}", label));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");
            ctx.emitter.instruction(&format!("je {}", label));
        }
    };
}

// -- Float(loaded)-input handlers -------------------------------------------

/// `FILTER_VALIDATE_FLOAT` on a loaded float: reject NaN/Infinite, else succeed as-is.
fn emit_float_from_loaded_float(ctx: &mut FunctionContext<'_>, null_on_failure: bool) {
    let fail_label = ctx.next_label("filter_float_flt_fail");
    let done_label = ctx.next_label("filter_float_flt_done");
    emit_branch_if_float_not_finite(ctx, &fail_label);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Float);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&fail_label);
    emit_filter_failure(ctx, null_on_failure);
    ctx.emitter.label(&done_label);
}

/// `FILTER_VALIDATE_INT` on a loaded float: reject NaN/Infinite/fractional/out-of-range.
fn emit_int_from_loaded_float(ctx: &mut FunctionContext<'_>, null_on_failure: bool) {
    let fail_label = ctx.next_label("filter_int_flt_fail");
    let done_label = ctx.next_label("filter_int_flt_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d0, d0");                             // NaN is unordered against itself
            ctx.emitter.instruction(&format!("b.vs {}", fail_label));           // NaN -> fail
            ctx.emitter.instruction("frintz d1, d0");                           // d1 = value truncated toward zero
            ctx.emitter.instruction("fcmp d0, d1");                             // does the value have a fractional part?
            ctx.emitter.instruction(&format!("b.ne {}", fail_label));           // fractional part -> fail
            abi::emit_load_int_immediate(ctx.emitter, "x5", FLOAT_INT_LOWER_BOUND_BITS);
            ctx.emitter.instruction("fmov d2, x5");                             // d2 = PHP_INT_MIN as a double (inclusive lower bound)
            ctx.emitter.instruction("fcmp d0, d2");                             // is the value below PHP_INT_MIN?
            ctx.emitter.instruction(&format!("b.lt {}", fail_label));           // below range -> fail
            abi::emit_load_int_immediate(ctx.emitter, "x5", FLOAT_INT_UPPER_BOUND_BITS);
            ctx.emitter.instruction("fmov d3, x5");                             // d3 = 2^63 as a double (exclusive upper bound)
            ctx.emitter.instruction("fcmp d0, d3");                             // is the value at or above 2^63?
            ctx.emitter.instruction(&format!("b.ge {}", fail_label));           // at/above the upper bound -> fail
            ctx.emitter.instruction("fcvtzs x0, d0");                           // convert the already-truncated, in-range value to i64
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("ucomisd xmm0, xmm0");                      // NaN is unordered against itself
            ctx.emitter.instruction(&format!("jp {}", fail_label));             // NaN -> fail
            ctx.emitter.instruction("roundsd xmm1, xmm0, 3");                   // xmm1 = value truncated toward zero (mode 3 = truncate)
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // does the value have a fractional part?
            ctx.emitter.instruction(&format!("jne {}", fail_label));            // fractional part -> fail
            abi::emit_load_int_immediate(ctx.emitter, "rax", FLOAT_INT_LOWER_BOUND_BITS);
            ctx.emitter.instruction("movq xmm2, rax");                          // xmm2 = PHP_INT_MIN as a double (inclusive lower bound)
            ctx.emitter.instruction("ucomisd xmm0, xmm2");                      // is the value below PHP_INT_MIN?
            ctx.emitter.instruction(&format!("jb {}", fail_label));             // below range -> fail
            abi::emit_load_int_immediate(ctx.emitter, "rax", FLOAT_INT_UPPER_BOUND_BITS);
            ctx.emitter.instruction("movq xmm2, rax");                          // xmm2 = 2^63 as a double (exclusive upper bound)
            ctx.emitter.instruction("ucomisd xmm0, xmm2");                      // is the value at or above 2^63?
            ctx.emitter.instruction(&format!("jae {}", fail_label));            // at/above the upper bound -> fail
            ctx.emitter.instruction("cvttsd2si rax, xmm0");                     // convert the already-truncated, in-range value to i64
        }
    }
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&fail_label);
    emit_filter_failure(ctx, null_on_failure);
    ctx.emitter.label(&done_label);
}

/// `FILTER_VALIDATE_BOOL(EAN)` on a loaded float: only `0.0`/`1.0` are valid.
fn emit_bool_from_loaded_float(ctx: &mut FunctionContext<'_>, null_on_failure: bool) {
    let zero_label = ctx.next_label("filter_bool_flt_zero");
    let one_label = ctx.next_label("filter_bool_flt_one");
    let fail_label = ctx.next_label("filter_bool_flt_fail");
    let done_label = ctx.next_label("filter_bool_flt_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov d1, xzr");                            // d1 = 0.0
            ctx.emitter.instruction("fcmp d0, d1");
            ctx.emitter.instruction(&format!("b.eq {}", zero_label));           // value == 0.0
            ctx.emitter.instruction("fmov d1, #1.0");                           // d1 = 1.0
            ctx.emitter.instruction("fcmp d0, d1");
            ctx.emitter.instruction(&format!("b.eq {}", one_label));            // value == 1.0
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xorpd xmm1, xmm1");                        // xmm1 = 0.0
            ctx.emitter.instruction("ucomisd xmm0, xmm1");
            ctx.emitter.instruction(&format!("je {}", zero_label));             // value == 0.0
            abi::emit_load_int_immediate(ctx.emitter, "rax", 0x3FF0_0000_0000_0000_i64);
            ctx.emitter.instruction("movq xmm1, rax");                          // xmm1 = 1.0
            ctx.emitter.instruction("ucomisd xmm0, xmm1");
            ctx.emitter.instruction(&format!("je {}", one_label));              // value == 1.0
        }
    }
    abi::emit_jump(ctx.emitter, &fail_label);
    ctx.emitter.label(&zero_label);
    emit_static_bool_mixed(ctx, false);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&one_label);
    emit_static_bool_mixed(ctx, true);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&fail_label);
    emit_filter_failure(ctx, null_on_failure);
    ctx.emitter.label(&done_label);
}

/// Branches to `label` when the loaded float result register is NaN or +-infinite.
fn emit_branch_if_float_not_finite(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d0, d0");                             // NaN is unordered against itself
            ctx.emitter.instruction(&format!("b.vs {}", label));                // NaN -> not finite
            ctx.emitter.instruction("fabs d1, d0");                             // d1 = |value|
            abi::emit_load_int_immediate(ctx.emitter, "x5", FLOAT_INFINITY_BITS);
            ctx.emitter.instruction("fmov d2, x5");                             // d2 = +infinity
            ctx.emitter.instruction("fcmp d1, d2");
            ctx.emitter.instruction(&format!("b.eq {}", label));                // |value| == infinity -> not finite
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("ucomisd xmm0, xmm0");                      // NaN is unordered against itself
            ctx.emitter.instruction(&format!("jp {}", label));                  // NaN -> not finite
            ctx.emitter.instruction("movq xmm1, xmm0");                         // xmm1 = value
            abi::emit_load_int_immediate(ctx.emitter, "rax", 0x7FFF_FFFF_FFFF_FFFF_i64);
            ctx.emitter.instruction("movq xmm2, rax");                          // xmm2 = sign-bit mask
            ctx.emitter.instruction("andpd xmm1, xmm2");                        // xmm1 = |value|
            abi::emit_load_int_immediate(ctx.emitter, "rax", FLOAT_INFINITY_BITS);
            ctx.emitter.instruction("movq xmm2, rax");                          // xmm2 = +infinity
            ctx.emitter.instruction("ucomisd xmm1, xmm2");
            ctx.emitter.instruction(&format!("je {}", label));                  // |value| == infinity -> not finite
        }
    }
}

// -- String(validated)-input result handlers --------------------------------

/// Interprets `__rt_filter_validate_int`'s result (x0/rax=success, x1/rdx=value).
fn emit_int_from_validate_int_result(ctx: &mut FunctionContext<'_>, null_on_failure: bool) {
    let fail_label = ctx.next_label("filter_int_str_fail");
    let done_label = ctx.next_label("filter_int_str_done");
    emit_branch_if_int_reg_zero(ctx, &fail_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),                 // move the parsed value into the int result register
        Arch::X86_64 => ctx.emitter.instruction("mov rax, rdx"),                // move the parsed value into the int result register
    };
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&fail_label);
    emit_filter_failure(ctx, null_on_failure);
    ctx.emitter.label(&done_label);
}

/// Interprets `__rt_filter_validate_float`'s result (x0/rax=success, d0/xmm0=value).
fn emit_float_from_validate_float_result(ctx: &mut FunctionContext<'_>, null_on_failure: bool) {
    let fail_label = ctx.next_label("filter_float_str_fail");
    let done_label = ctx.next_label("filter_float_str_done");
    emit_branch_if_int_reg_zero(ctx, &fail_label);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Float);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&fail_label);
    emit_filter_failure(ctx, null_on_failure);
    ctx.emitter.label(&done_label);
}

/// Interprets `__rt_filter_validate_bool_str`'s 0/1/2 result register.
fn emit_bool_from_validate_bool_result(ctx: &mut FunctionContext<'_>, null_on_failure: bool) {
    let false_label = ctx.next_label("filter_bool_str_false");
    let true_label = ctx.next_label("filter_bool_str_true");
    let done_label = ctx.next_label("filter_bool_str_done");
    emit_branch_if_int_reg_zero(ctx, &false_label);
    emit_branch_if_int_reg_equals_one(ctx, &true_label);
    // result register == 2: no token matched.
    emit_filter_failure(ctx, null_on_failure);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&false_label);
    emit_static_bool_mixed(ctx, false);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&true_label);
    emit_static_bool_mixed(ctx, true);
    ctx.emitter.label(&done_label);
}

// -- Mixed-input dispatch ----------------------------------------------------

/// Unboxes a Mixed value and re-dispatches through the same per-type emission
/// used by the concretely-typed branches, for `FILTER_VALIDATE_INT/FLOAT/BOOL`.
fn emit_mixed_dispatch(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    null_on_failure: bool,
    kind: MixedFilterKind,
) -> Result<()> {
    let int_label = ctx.next_label("filter_mixed_int");
    let str_label = ctx.next_label("filter_mixed_str");
    let float_label = ctx.next_label("filter_mixed_float");
    let bool_label = ctx.next_label("filter_mixed_bool");
    let fail_label = ctx.next_label("filter_mixed_fail");
    let done_label = ctx.next_label("filter_mixed_done");

    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");                      // x0/rax=tag, x1/rdi=lo, x2/rdx=hi
    emit_branch_on_mixed_tag(ctx, 0, &int_label);
    emit_branch_on_mixed_tag(ctx, 1, &str_label);
    emit_branch_on_mixed_tag(ctx, 2, &float_label);
    emit_branch_on_mixed_tag(ctx, 3, &bool_label);
    abi::emit_jump(ctx.emitter, &fail_label);                                   // null (8) and every other (array/hash/object) tag fails

    ctx.emitter.label(&int_label);
    move_mixed_scalar_lo_to_int_result(ctx);
    match kind {
        MixedFilterKind::Int => emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int),
        MixedFilterKind::Float => {
            emit_int_to_float_result(ctx);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Float);
        }
        MixedFilterKind::Bool => emit_bool_from_loaded_int(ctx, null_on_failure),
    }
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&str_label);
    move_mixed_string_payload_to_string_result(ctx);
    match kind {
        MixedFilterKind::Int => {
            abi::emit_call_label(ctx.emitter, "__rt_filter_validate_int");
            emit_int_from_validate_int_result(ctx, null_on_failure);
        }
        MixedFilterKind::Float => {
            abi::emit_call_label(ctx.emitter, "__rt_filter_validate_float");
            emit_float_from_validate_float_result(ctx, null_on_failure);
        }
        MixedFilterKind::Bool => {
            abi::emit_call_label(ctx.emitter, "__rt_filter_validate_bool_str");
            emit_bool_from_validate_bool_result(ctx, null_on_failure);
        }
    }
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&float_label);
    move_mixed_scalar_lo_to_float_result(ctx);
    match kind {
        MixedFilterKind::Int => emit_int_from_loaded_float(ctx, null_on_failure),
        MixedFilterKind::Float => emit_float_from_loaded_float(ctx, null_on_failure),
        MixedFilterKind::Bool => emit_bool_from_loaded_float(ctx, null_on_failure),
    }
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&bool_label);
    move_mixed_scalar_lo_to_int_result(ctx);
    match kind {
        MixedFilterKind::Int => emit_int_from_loaded_bool(ctx, null_on_failure),
        MixedFilterKind::Float => emit_float_from_loaded_bool(ctx, null_on_failure),
        MixedFilterKind::Bool => emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool),
    }
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&fail_label);
    if matches!(kind, MixedFilterKind::Bool) {
        // A Mixed null unboxes to tag 8: like the concrete Void case, the BOOL
        // filter treats it as "" (a valid false), never a failure. Every other
        // unhandled tag (array/hash/object) IS a genuine failure for all three
        // kinds; disambiguating null from those here keeps this one path exact.
        emit_mixed_bool_null_or_nonscalar_fail(ctx, null_on_failure);
    } else {
        emit_filter_failure(ctx, null_on_failure);
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Distinguishes an unboxed Mixed null (tag 8, valid `false` for the BOOL
/// filter) from every other non-scalar tag (array/hash/object, a real failure).
fn emit_mixed_bool_null_or_nonscalar_fail(ctx: &mut FunctionContext<'_>, null_on_failure: bool) {
    let real_fail_label = ctx.next_label("filter_mixed_bool_real_fail");
    let done_label = ctx.next_label("filter_mixed_bool_null_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #8");                              // is the unboxed tag PHP null?
            ctx.emitter.instruction(&format!("b.ne {}", real_fail_label));      // any other tag is a genuine failure
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 8");                              // is the unboxed tag PHP null?
            ctx.emitter.instruction(&format!("jne {}", real_fail_label));       // any other tag is a genuine failure
        }
    }
    emit_static_bool_mixed(ctx, false);                                        // null behaves like "" for the BOOL filter: valid false
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&real_fail_label);
    emit_filter_failure(ctx, null_on_failure);
    ctx.emitter.label(&done_label);
}

/// Unboxes a Mixed value and re-dispatches for `FILTER_DEFAULT`/`FILTER_UNSAFE_RAW`.
fn emit_mixed_default(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    null_on_failure: bool,
) -> Result<()> {
    let array_label = ctx.next_label("filter_mixed_default_array");
    let scalar_label = ctx.next_label("filter_mixed_default_scalar");
    let done_label = ctx.next_label("filter_mixed_default_done");

    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");                      // x0/rax=tag (payload registers unused here)
    emit_branch_on_mixed_tag(ctx, 4, &array_label);
    emit_branch_on_mixed_tag(ctx, 5, &array_label);
    abi::emit_jump(ctx.emitter, &scalar_label);

    ctx.emitter.label(&array_label);
    emit_filter_failure(ctx, null_on_failure);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&scalar_label);
    // Every non-array tag (int/string/float/bool/null/object/...) stringifies
    // exactly like a normal PHP `(string)` cast for FILTER_DEFAULT passthrough,
    // so the ORIGINAL boxed Mixed pointer (reloaded, since __rt_mixed_unbox
    // consumed the register holding it) can go straight through the existing
    // general-purpose Mixed-to-string runtime cast.
    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Moves an unboxed Mixed scalar low payload word into the int result register.
fn move_mixed_scalar_lo_to_int_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),                 // x1 = unboxed low payload word
        Arch::X86_64 => ctx.emitter.instruction("mov rax, rdi"),                // rdi = unboxed low payload word
    };
}

/// Moves an unboxed Mixed scalar low payload word (float bits) into the float result register.
fn move_mixed_scalar_lo_to_float_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("fmov d0, x1"),                // x1 = unboxed float payload bits
        Arch::X86_64 => ctx.emitter.instruction("movq xmm0, rdi"),              // rdi = unboxed float payload bits
    };
}

/// Moves an unboxed Mixed string payload into the normal string result registers.
fn move_mixed_string_payload_to_string_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {}                                                     // x1=ptr, x2=len already match the string-result convention
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed string pointer into the string result register
        }
    };
}

/// Branches when the unboxed Mixed tag (in x0/rax) equals the requested runtime tag.
fn emit_branch_on_mixed_tag(ctx: &mut FunctionContext<'_>, tag: u8, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{}", tag));              // compare the unboxed Mixed tag against this filter case
            ctx.emitter.instruction(&format!("b.eq {}", label));                // branch when the Mixed tag matches this filter case
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {}", tag));              // compare the unboxed Mixed tag against this filter case
            ctx.emitter.instruction(&format!("je {}", label));                  // branch when the Mixed tag matches this filter case
        }
    }
}
