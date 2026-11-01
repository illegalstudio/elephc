//! Purpose:
//! Lowers PHP's shared internal-array-cursor builtin family.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Cursor-moving operations separate shared raw arrays before mutation and write the unique
//!   pointer back to local or global storage; boxed gradual arrays dispatch by runtime tag.

use super::*;

/// Lowers one member of PHP's `reset`/`current`/`key`/`next`/`prev`/`end` family.
pub(crate) fn lower_array_cursor(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let value = expect_operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    let operation = match name {
        "reset" => 0,
        "current" => 1,
        "key" => 2,
        "next" => 3,
        "prev" => 4,
        "end" => 5,
        _ => unreachable!("array cursor dispatcher only receives cursor-family names"),
    };
    let moves_cursor = matches!(name, "reset" | "next" | "prev" | "end");
    let kind = match value_ty {
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Mixed | PhpType::Union(_) => {
            super::super::super::load_value_to_first_int_arg(ctx, value)?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => abi::emit_load_int_immediate(ctx.emitter, "x1", operation),
                Arch::X86_64 => abi::emit_load_int_immediate(ctx.emitter, "rsi", operation),
            }
            abi::emit_call_label(ctx.emitter, "__rt_array_cursor_boxed");
            return store_if_result(ctx, inst);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} for PHP type {:?}",
                name, other
            )))
        }
    };

    if moves_cursor {
        let source_local = array_cursor_source_local_slot(ctx, value)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.load_value_to_reg(value, "x0")?,
            Arch::X86_64 => ctx.load_value_to_reg(value, "rdi")?,
        };
        abi::emit_call_label(
            ctx.emitter,
            if kind == 4 {
                "__rt_array_ensure_unique"
            } else {
                "__rt_hash_ensure_unique"
            },
        );
        ctx.store_result_value(value)?;
        if let Some(slot) = source_local {
            ctx.store_value_to_local(slot, value)?;
        }
        ctx.writeback_global_array_source(value)?;
    }

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(value, "x0")?;
            abi::emit_load_int_immediate(ctx.emitter, "x1", kind);
            abi::emit_load_int_immediate(ctx.emitter, "x2", operation);
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(value, "rdi")?;
            abi::emit_load_int_immediate(ctx.emitter, "rsi", kind);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", operation);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_cursor");
    store_if_result(ctx, inst)
}

/// Returns the local slot that supplied a raw array cursor operand, when one exists.
fn array_cursor_source_local_slot(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<LocalSlotId>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    match (inst_ref.op, inst_ref.immediate.as_ref()) {
        (Op::LoadLocal, Some(Immediate::LocalSlot(slot))) => Ok(Some(*slot)),
        _ => Ok(None),
    }
}
