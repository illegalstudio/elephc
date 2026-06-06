//! Purpose:
//! Lowers simple PCRE-style regex builtins for the EIR backend.
//! Bridges already-evaluated EIR operands to the shared target-aware regex runtime helpers.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - `preg_match()` captures currently support direct local `$matches` variables.
//! - `preg_replace_callback()` supports static string callbacks and no-capture
//!   closure descriptors whose ABI matches the regex callback runtime.
//! - `preg_split()` forces boxed Mixed element slots so dynamic flags cannot mismatch layout.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::names::function_symbol;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;

const PREG_SPLIT_FORCE_MIXED_RESULT: i64 = 1 << 30;

/// Lowers `preg_match(pattern, subject)` through the shared regex runtime helper.
pub(super) fn lower_preg_match(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count_between(inst, "preg_match", 2, 3)?;
    let pattern = super::expect_operand(inst, 0)?;
    let subject = super::expect_operand(inst, 1)?;
    let matches_slot = inst
        .operands
        .get(2)
        .copied()
        .map(|value| matches_local_slot(ctx, value))
        .transpose()?;
    load_pattern_and_subject(ctx, pattern, subject)?;
    if let Some(slot) = matches_slot {
        abi::emit_call_label(ctx.emitter, "__rt_preg_match_capture");
        store_matches_array(ctx, slot)?;
    } else {
        abi::emit_call_label(ctx.emitter, "__rt_preg_match");
    }
    super::store_if_result(ctx, inst)
}

/// Lowers `preg_match_all(pattern, subject)` through the shared regex runtime helper.
pub(super) fn lower_preg_match_all(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "preg_match_all", 2)?;
    let pattern = super::expect_operand(inst, 0)?;
    let subject = super::expect_operand(inst, 1)?;
    load_pattern_and_subject(ctx, pattern, subject)?;
    abi::emit_call_label(ctx.emitter, "__rt_preg_match_all");
    super::store_if_result(ctx, inst)
}

/// Lowers `preg_replace(pattern, replacement, subject)` through the regex replacement helper.
pub(super) fn lower_preg_replace(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "preg_replace", 3)?;
    let pattern = super::expect_operand(inst, 0)?;
    let replacement = super::expect_operand(inst, 1)?;
    let subject = super::expect_operand(inst, 2)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg(ctx, pattern, "x1", "x2", "preg_replace pattern")?;
            load_string_arg(ctx, replacement, "x3", "x4", "preg_replace replacement")?;
            load_string_arg(ctx, subject, "x5", "x6", "preg_replace subject")?;
        }
        Arch::X86_64 => {
            load_string_arg(ctx, pattern, "rdi", "rsi", "preg_replace pattern")?;
            load_string_arg(ctx, replacement, "rdx", "rcx", "preg_replace replacement")?;
            load_string_arg(ctx, subject, "r8", "r9", "preg_replace subject")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_preg_replace");
    super::store_if_result(ctx, inst)
}

/// Lowers `preg_replace_callback(pattern, callback, subject)` through supported direct callbacks.
pub(super) fn lower_preg_replace_callback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "preg_replace_callback", 3)?;
    let pattern = super::expect_operand(inst, 0)?;
    let callback = super::expect_operand(inst, 1)?;
    let subject = super::expect_operand(inst, 2)?;
    let callback_target = preg_replace_callback_target(ctx, callback)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg(ctx, pattern, "x1", "x2", "preg_replace_callback pattern")?;
            abi::emit_symbol_address(ctx.emitter, "x3", &callback_target.entry_label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", 0);
            load_string_arg(ctx, subject, "x5", "x6", "preg_replace_callback subject")?;
        }
        Arch::X86_64 => {
            load_string_arg(ctx, pattern, "rdi", "rsi", "preg_replace_callback pattern")?;
            abi::emit_symbol_address(ctx.emitter, "rdx", &callback_target.entry_label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", 0);
            load_string_arg(ctx, subject, "r8", "r9", "preg_replace_callback subject")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_preg_replace_callback");
    super::store_if_result(ctx, inst)
}

/// Runtime callback target passed to `__rt_preg_replace_callback`.
struct PregReplaceCallbackTarget {
    entry_label: String,
}

/// Resolves a regex replacement callback to a direct entry with no environment.
fn preg_replace_callback_target(
    ctx: &FunctionContext<'_>,
    callback: ValueId,
) -> Result<PregReplaceCallbackTarget> {
    if let Some(entry_label) = static_string_callback_entry(ctx, callback)? {
        return Ok(PregReplaceCallbackTarget { entry_label });
    }
    if let Some(entry_label) = closure_callback_entry(ctx, callback)? {
        return Ok(PregReplaceCallbackTarget { entry_label });
    }
    Err(CodegenIrError::unsupported(
        "preg_replace_callback callback with non-literal string",
    ))
}

/// Resolves a literal string callback to a module-local function entry.
fn static_string_callback_entry(
    ctx: &FunctionContext<'_>,
    callback: ValueId,
) -> Result<Option<String>> {
    let Some(callback_name) = maybe_const_string_operand(ctx, callback)? else {
        return Ok(None);
    };
    let function_name = ctx
        .callable_function_by_name(&callback_name)
        .map(|function| function.name.to_string())
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "preg_replace_callback static callback {}",
                callback_name
            ))
        })?;
    Ok(Some(function_symbol(&function_name)))
}

/// Resolves a no-capture closure descriptor to its direct EIR closure entry.
fn closure_callback_entry(
    ctx: &FunctionContext<'_>,
    callback: ValueId,
) -> Result<Option<String>> {
    let Some(inst) = value_source_instruction(ctx, callback)? else {
        return Ok(None);
    };
    if inst.op != Op::ClosureNew {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = inst.immediate else {
        return Err(CodegenIrError::invalid_module(
            "preg_replace_callback closure descriptor has no data id",
        ));
    };
    let closure_name = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    let closure = ctx
        .module
        .closures
        .iter()
        .find(|function| function.name == *closure_name)
        .ok_or_else(|| CodegenIrError::missing_entry("closure", data.as_raw()))?;
    if closure.params.len() != 1 || closure.return_php_type.codegen_repr() != PhpType::Str {
        return Err(CodegenIrError::unsupported(format!(
            "preg_replace_callback closure {} with unsupported signature",
            closure_name
        )));
    }
    Ok(Some(function_symbol(closure_name)))
}

/// Lowers `preg_split(pattern, subject, limit?, flags?)` through the regex split helper.
pub(super) fn lower_preg_split(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count_between(inst, "preg_split", 2, 4)?;
    let pattern = super::expect_operand(inst, 0)?;
    let subject = super::expect_operand(inst, 1)?;
    let limit = inst.operands.get(2).copied();
    let flags = inst.operands.get(3).copied();
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg(ctx, pattern, "x1", "x2", "preg_split pattern")?;
            load_string_arg(ctx, subject, "x3", "x4", "preg_split subject")?;
            load_limit_arg(ctx, limit, "x5")?;
            load_flags_arg(ctx, flags, "x6")?;
            ctx.emitter.instruction(&format!("orr x6, x6, #{}", PREG_SPLIT_FORCE_MIXED_RESULT)); // force boxed-Mixed split slots for EIR result layout
        }
        Arch::X86_64 => {
            load_string_arg(ctx, pattern, "rdi", "rsi", "preg_split pattern")?;
            load_string_arg(ctx, subject, "rdx", "rcx", "preg_split subject")?;
            load_limit_arg(ctx, limit, "r8")?;
            load_flags_arg(ctx, flags, "r9")?;
            ctx.emitter.instruction(&format!("or r9, {}", PREG_SPLIT_FORCE_MIXED_RESULT)); // force boxed-Mixed split slots for EIR result layout
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_preg_split");
    super::store_if_result(ctx, inst)
}

/// Loads pattern and subject string operands into the regex runtime ABI registers.
fn load_pattern_and_subject(
    ctx: &mut FunctionContext<'_>,
    pattern: ValueId,
    subject: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg(ctx, pattern, "x1", "x2", "preg pattern")?;
            load_string_arg(ctx, subject, "x3", "x4", "preg subject")
        }
        Arch::X86_64 => {
            load_string_arg(ctx, pattern, "rdi", "rsi", "preg pattern")?;
            load_string_arg(ctx, subject, "rdx", "rcx", "preg subject")
        }
    }
}

/// Returns the local slot represented by a `preg_match()` `$matches` operand.
fn matches_local_slot(ctx: &FunctionContext<'_>, value: ValueId) -> Result<LocalSlotId> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(
            "preg_match matches argument that is not a local load",
        ));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::LoadLocal {
        return Err(CodegenIrError::unsupported(
            "preg_match matches argument that is not a local variable",
        ));
    }
    let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "preg_match matches load missing local slot",
        ));
    };
    Ok(slot)
}

/// Stores the runtime-built matches array into a local slot without clobbering the match flag.
fn store_matches_array(ctx: &mut FunctionContext<'_>, slot: LocalSlotId) -> Result<()> {
    let offset = ctx.local_offset(slot)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::store_at_offset(ctx.emitter, "x1", offset);
        }
        Arch::X86_64 => {
            abi::store_at_offset(ctx.emitter, "rdx", offset);
        }
    }
    Ok(())
}

/// Returns a string literal value when `value` is defined by a `ConstStr` instruction.
fn maybe_const_string_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<String>> {
    let Some(inst_ref) = value_source_instruction(ctx, value)? else {
        return Ok(None);
    };
    if inst_ref.op != Op::ConstStr {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "preg_replace_callback callback string literal has no data id",
        ));
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .map(Some)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Returns the instruction that defines an SSA value, when it has one.
fn value_source_instruction<'a>(
    ctx: &'a FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<&'a Instruction>> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    ctx
        .function
        .instruction(inst)
        .map(Some)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))
}

/// Loads a string operand into an explicit pointer/length register pair.
fn load_string_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    ptr_reg: &str,
    len_reg: &str,
    context: &str,
) -> Result<()> {
    require_string(ctx.value_php_type(value)?, context)?;
    ctx.load_string_value_to_regs(value, ptr_reg, len_reg)
}

/// Loads the optional `preg_split()` limit, using PHP's default `-1`.
fn load_limit_arg(
    ctx: &mut FunctionContext<'_>,
    limit: Option<ValueId>,
    reg: &str,
) -> Result<()> {
    let Some(limit) = limit else {
        abi::emit_load_int_immediate(ctx.emitter, reg, -1);
        return Ok(());
    };
    require_integer_like(ctx.load_value_to_reg(limit, reg)?, "preg_split limit")
}

/// Loads the optional `preg_split()` flags, using PHP's default `0`.
fn load_flags_arg(
    ctx: &mut FunctionContext<'_>,
    flags: Option<ValueId>,
    reg: &str,
) -> Result<()> {
    let Some(flags) = flags else {
        abi::emit_load_int_immediate(ctx.emitter, reg, 0);
        return Ok(());
    };
    require_integer_like(ctx.load_value_to_reg(flags, reg)?, "preg_split flags")
}

/// Verifies that a regex string operand is statically string-shaped.
fn require_string(ty: PhpType, context: &str) -> Result<()> {
    if ty == PhpType::Str {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        context,
        ty
    )))
}

/// Verifies that a regex integer option is statically integer-like.
fn require_integer_like(ty: PhpType, context: &str) -> Result<()> {
    if matches!(ty, PhpType::Int | PhpType::Bool) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        context,
        ty
    )))
}
