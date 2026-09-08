//! Purpose:
//! Emits checked local/UTC mktime calls and boxes their PHP int|false result.
//!
//! Called from:
//! - The typed Mktime/Gmmktime runtime-function dispatch.
//!
//! Key details:
//! - Nullable optional fields travel as a bit mask; all inputs remain evaluated once.
//! - The C ABI returns two integer words: timestamp and validity, on both architectures.

use super::*;
use crate::codegen_support::emit_box_current_value_as_mixed;

/// Calls the checked timelib bridge with a nullable-field mask and an owned boxed result.
pub(super) fn lower_checked_mktime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    utc: bool,
) -> Result<()> {
    let name = if utc { "gmmktime" } else { "mktime" };
    let given = inst.operands.len();
    if given == 0 || given > 6 {
        let message = if given == 0 {
            format!("{name}() expects at least 1 argument, 0 given")
        } else {
            format!("{name}() expects at most 6 arguments, {given} given")
        };
        let location = ctx.module.source_path.clone()
            .map(|file| (file, inst.span.map_or(0, |span| span.line)));
        exceptions::emit_argument_count_error(ctx, &message, location);
        return Ok(());
    }
    super::ensure_arg_count(inst, name, 6)?;
    system::emit_scratch_reserve(ctx, 16);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    system::emit_store_result_to_scratch(ctx, 0);
    for index in 1..6 {
        accumulate_null_mask(ctx, expect_operand(inst, index)?, index)?;
    }
    system::marshal_integer_args(ctx, inst, &system::MKTIME_ARG_LABELS)?;
    if ctx.emitter.target.arch == Arch::AArch64 {
        let mask_arg = abi::int_arg_reg_name(ctx.emitter.target, 6);
        system::emit_load_scratch_to_reg(ctx, mask_arg, 0);
    }
    // On System V x86_64 the seventh C argument is already at caller rsp + 0.
    ctx.emitter.bl_c(if utc { "elephc_tz_gmmktime_checked" } else { "elephc_tz_mktime_checked" });
    let invalid = ctx.next_label("mktime.invalid");
    let done = ctx.next_label("mktime.boxed");
    let branch = match ctx.emitter.target.arch {
        Arch::AArch64 => format!("cbz x1, {invalid}"),
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdx, rdx");                           // test the second INTEGER-class C return word
            format!("jz {invalid}")
        }
    };
    ctx.emitter.instruction(&branch);                                           // distinguish failure from the valid integer timestamp -1
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&invalid);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&done);
    system::emit_scratch_release(ctx, 16);
    store_if_result(ctx, inst)
}

/// Records one optional null field without confusing a supplied integer zero with null.
fn accumulate_null_mask(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    index: usize,
) -> Result<()> {
    let tag_reg = match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(value)?;
            Some(crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter))
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            Some(abi::int_result_reg(ctx.emitter))
        }
        PhpType::Void | PhpType::Never => None,
        _ => return Ok(()),
    };
    let not_null = ctx.next_label("mktime.field.present");
    if let Some(tag_reg) = tag_reg {
        let compare = match ctx.emitter.target.arch {
            Arch::AArch64 => format!("cmp {tag_reg}, #8"),
            Arch::X86_64 => format!("cmp {tag_reg}, 8"),
        };
        let branch = match ctx.emitter.target.arch {
            Arch::AArch64 => format!("b.ne {not_null}"),
            Arch::X86_64 => format!("jne {not_null}"),
        };
        ctx.emitter.instruction(&compare);                                      // nullable scalar and Mixed cells share PHP's null tag
        ctx.emitter.instruction(&branch);                                       // preserve explicit non-null values, including zero
    }
    let (mask_reg, bit_reg, stack_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x9", "x10", "sp"),
        Arch::X86_64 => ("r10", "r11", "rsp"),
    };
    system::emit_load_scratch_to_reg(ctx, mask_reg, 0);
    abi::emit_load_int_immediate(ctx.emitter, bit_reg, 1 << index);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("orr x9, x9, x10"),            // mark this field for default completion in timelib
        Arch::X86_64 => ctx.emitter.instruction("or r10, r11"),                 // mark this field for default completion in timelib
    }
    abi::emit_store_to_address(ctx.emitter, mask_reg, stack_reg, 0);
    ctx.emitter.label(&not_null);
    Ok(())
}
