//! Purpose:
//! Static callback name and source-instruction recovery.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Static callback operand metadata recovered from a literal-producing EIR instruction.
pub(super) struct StaticCallbackName {
    pub(super) name: String,
    pub(super) kind: StaticCallbackOperandKind,
    pub(super) receiver: Option<ValueId>,
}

/// Classifies whether a static callback came from a PHP string or `foo(...)` syntax.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StaticCallbackOperandKind {
    StringLiteral,
    FirstClassCallable,
}

/// Returns a static callback name from a string literal or `foo(...)` descriptor instruction.
pub(super) fn static_callback_name_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<StaticCallbackName> {
    let inst_ref = static_callback_source_instruction(ctx, value, owner)?;
    let receiver = inst_ref.operands.first().copied();
    let kind = match inst_ref.op {
        Op::ConstStr => StaticCallbackOperandKind::StringLiteral,
        Op::FirstClassCallableNew => StaticCallbackOperandKind::FirstClassCallable,
        _ => unreachable!("callback source instruction was validated earlier"),
    };
    let data = match inst_ref.immediate.as_ref() {
        Some(Immediate::Data(data))
        | Some(Immediate::ProfiledData { data, .. }) => data,
        _ => {
            return Err(CodegenIrError::invalid_module(format!(
                "{} string literal has no data id",
                owner
            )));
        }
    };
    let name = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    Ok(StaticCallbackName {
        name,
        kind,
        receiver,
    })
}

/// Returns the literal callback-producing instruction for a callback operand.
pub(super) fn static_callback_source_instruction<'a>(
    ctx: &'a FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<&'a Instruction> {
    let value = strip_static_callback_acquire(ctx, value)?;
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { block, index, inst } = value_ref.def else {
        return Err(CodegenIrError::unsupported(format!(
            "{} with non-static callback operand",
            owner
        )));
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if inst_ref.op == Op::LoadLocal {
        return static_callback_local_source_instruction(ctx, block, index, inst_ref, owner);
    }
    require_static_callback_source(inst_ref, owner)
}

/// Returns whether a callback operand can use the static callback binding path.
pub(super) fn static_callback_operand_is_recoverable(ctx: &FunctionContext<'_>, value: ValueId) -> bool {
    static_callback_source_instruction(ctx, value, "static callback probe").is_ok()
}

/// Returns true when a callback operand is a dynamic callable local, such as a parameter.
pub(super) fn descriptor_callback_local_without_same_block_store(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<bool> {
    let value = strip_static_callback_acquire(ctx, value)?;
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { block, index, inst } = value_ref.def else {
        return Ok(false);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if inst_ref.op != Op::LoadLocal {
        return Ok(false);
    }
    let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "array_map callback load_local has no local slot",
        ));
    };
    if same_block_store_before(ctx, block, index, slot)? {
        return Ok(false);
    }
    Ok(ctx.local_php_type(slot)? == PhpType::Callable)
}

/// Returns true when the selected local slot is stored earlier in the same EIR block.
pub(super) fn same_block_store_before(
    ctx: &FunctionContext<'_>,
    block: BlockId,
    load_index: u32,
    slot: LocalSlotId,
) -> Result<bool> {
    let block_ref = ctx
        .function
        .block(block)
        .ok_or_else(|| CodegenIrError::missing_entry("block", block.as_raw()))?;
    for (index, inst_id) in block_ref.instructions.iter().enumerate() {
        if index as u32 >= load_index {
            break;
        }
        let inst_ref = ctx
            .function
            .instruction(*inst_id)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
        if inst_ref.op == Op::StoreLocal
            && matches!(inst_ref.immediate, Some(Immediate::LocalSlot(candidate)) if candidate == slot)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Resolves a local callback load to the last same-block store before that load.
pub(super) fn static_callback_local_source_instruction<'a>(
    ctx: &'a FunctionContext<'_>,
    block: BlockId,
    load_index: u32,
    load_inst: &Instruction,
    owner: &str,
) -> Result<&'a Instruction> {
    let Some(Immediate::LocalSlot(slot)) = load_inst.immediate else {
        return Err(CodegenIrError::invalid_module(format!(
            "{} load_local callback has no local slot",
            owner
        )));
    };
    let block_ref = ctx
        .function
        .block(block)
        .ok_or_else(|| CodegenIrError::missing_entry("block", block.as_raw()))?;
    let mut stored = None;
    for (index, inst_id) in block_ref.instructions.iter().enumerate() {
        if index as u32 >= load_index {
            break;
        }
        let inst_ref = ctx
            .function
            .instruction(*inst_id)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
        if inst_ref.op == Op::StoreLocal
            && matches!(inst_ref.immediate, Some(Immediate::LocalSlot(candidate)) if candidate == slot)
        {
            stored = inst_ref.operands.first().copied();
        }
    }
    let Some(stored) = stored else {
        return Err(CodegenIrError::unsupported(format!(
            "{} with local callback operand that has no prior same-block store",
            owner
        )));
    };
    let Some(value_ref) = ctx.function.value(stored) else {
        return Err(CodegenIrError::missing_entry("value", stored.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(format!(
            "{} with local callback operand from non-instruction value",
            owner
        )));
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    require_static_callback_source(inst_ref, owner)
}

/// Verifies an instruction directly materializes a callback identity supported by the runtime.
pub(super) fn require_static_callback_source<'a>(
    inst: &'a Instruction,
    owner: &str,
) -> Result<&'a Instruction> {
    if matches!(inst.op, Op::ConstStr | Op::FirstClassCallableNew) {
        Ok(inst)
    } else {
        Err(CodegenIrError::unsupported(format!(
            "{} with non-static callback operand",
            owner
        )))
    }
}
