//! Purpose:
//! Static callback binding and callable-array recovery.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Callback label, return type, and optional environment source for callback runtime helpers.
pub(super) struct StaticSortCallbackBinding {
    pub(super) label: String,
    pub(super) env_source: Option<StaticCallbackEnvSource>,
    pub(super) return_ty: PhpType,
}

/// Returns a static callback binding for callback runtimes, including late-static env when needed.
pub(super) fn static_sort_callback_binding(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    owner: &str,
    visible_arg_types: Option<&[PhpType]>,
) -> Result<StaticSortCallbackBinding> {
    let callback = match static_callable_array_callback_name(ctx, value, owner)? {
        Some(callback) => callback,
        None => static_callback_name_operand(ctx, value, owner)?,
    };
    if let Some((label, param_types, return_ty)) = ctx
        .callable_function_by_name(&callback.name)
        .map(|callee| {
            (
                function_symbol(&callee.name),
                callee
                    .params
                    .iter()
                    .map(|param| param.php_type.codegen_repr())
                    .collect::<Vec<_>>(),
                callee.return_php_type.codegen_repr(),
            )
        })
    {
        let (label, env_source) = adapt_direct_callback_visible_args(
            ctx,
            label,
            &param_types,
            visible_arg_types,
            owner,
        )?;
        return Ok(StaticSortCallbackBinding {
            label,
            env_source,
            return_ty,
        });
    }
    if callback.kind == StaticCallbackOperandKind::FirstClassCallable {
        if let Some(target) =
            instance_method_sort_callback_target(ctx, &callback, owner, visible_arg_types)?
        {
            let visible_arg_types = visible_arg_types
                .expect("instance sort callback target requires known argument types");
            let label = emit_instance_method_callback_wrapper(ctx, &target, visible_arg_types);
            return Ok(StaticSortCallbackBinding {
                label,
                env_source: Some(StaticCallbackEnvSource::Value(target.receiver)),
                return_ty: target.return_ty,
            });
        }
        if let Some(target) =
            static_method_sort_callback_target(ctx, &callback.name, owner, visible_arg_types)?
        {
            let visible_arg_types = visible_arg_types
                .expect("static sort callback target requires known argument types");
            let label = emit_static_method_callback_wrapper(ctx, &target, visible_arg_types);
            return Ok(StaticSortCallbackBinding {
                label,
                env_source: target.env_source,
                return_ty: target.return_ty,
            });
        }
    }
    Err(CodegenIrError::unsupported(format!(
        "{} '{}' is not a user function or supported first-class static method",
        owner, callback.name
    )))
}

/// Adapts boxed runtime callback arguments to a direct callback's declared ABI types.
pub(super) fn adapt_direct_callback_visible_args(
    ctx: &mut FunctionContext<'_>,
    target_label: String,
    target_param_types: &[PhpType],
    visible_arg_types: Option<&[PhpType]>,
    owner: &str,
) -> Result<(String, Option<StaticCallbackEnvSource>)> {
    let Some(visible_arg_types) = visible_arg_types else {
        return Ok((target_label, None));
    };
    if visible_arg_types
        .iter()
        .map(PhpType::codegen_repr)
        .eq(target_param_types.iter().map(PhpType::codegen_repr))
    {
        return Ok((target_label, None));
    }
    if !visible_arg_types
        .iter()
        .any(|ty| matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)))
    {
        return Ok((target_label, None));
    }
    if visible_arg_types.len() != target_param_types.len() {
        return Err(CodegenIrError::unsupported(format!(
            "{} with runtime callback args {:?} for direct target params {:?}",
            owner, visible_arg_types, target_param_types
        )));
    }

    let wrapper_label = ctx.next_label("direct_callback_arg_adapter");
    let done_label = ctx.next_label("direct_callback_after_arg_adapter");
    let wrapper = DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types: visible_arg_types.to_vec(),
        target_visible_arg_types: Some(target_param_types.to_vec()),
        capture_types: Vec::new(),
        descriptor_prefix_types: Vec::new(),
        descriptor_return_type: None,
    };
    abi::emit_jump(ctx.emitter, &done_label);
    crate::codegen::emit_callback_wrapper(ctx.emitter, &wrapper);
    ctx.emitter.label(&done_label);
    Ok((
        wrapper_label,
        Some(StaticCallbackEnvSource::FunctionLabel(target_label)),
    ))
}

/// Recovers a static `[class, method]` callable array as a static-method callback name.
pub(super) fn static_callable_array_callback_name(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<Option<StaticCallbackName>> {
    let Some((array, block, limit_index)) = static_callable_array_source(ctx, value, owner)? else {
        return Ok(None);
    };
    let items = static_callable_array_items(ctx, array, block, limit_index)?;
    let [receiver, method] = items.as_slice() else {
        return Ok(None);
    };
    let Some(method_name) = static_callback_const_string(ctx, *method)? else {
        return Ok(None);
    };
    if static_callback_object_receiver(ctx, *receiver)? {
        return Ok(Some(StaticCallbackName {
            name: format!("object::{}", method_name),
            kind: StaticCallbackOperandKind::FirstClassCallable,
            receiver: Some(*receiver),
        }));
    }
    let Some(class_name) = static_callback_const_string(ctx, *receiver)? else {
        return Ok(None);
    };
    Ok(Some(StaticCallbackName {
        name: format!("{}::{}", class_name, method_name),
        kind: StaticCallbackOperandKind::FirstClassCallable,
        receiver: None,
    }))
}

/// Returns the backing array value for a same-block static callable-array operand.
pub(super) fn static_callable_array_source(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<Option<(ValueId, BlockId, u32)>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { block, index, inst } = value_ref.def else {
        return Ok(None);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    // Rooting retains the callable container without changing the callback it names.
    // Resolve a retained local at its original load, where same-block stores are authoritative.
    let unwrapped = strip_static_callback_acquire(ctx, value)?;
    if unwrapped != value && value_defining_op(ctx, unwrapped)? == Some(Op::LoadLocal) {
        return static_callable_array_source(ctx, unwrapped, owner);
    }
    let candidate = if inst_ref.op == Op::LoadLocal {
        let Some(stored) = static_callback_local_stored_value(ctx, block, index, inst_ref, owner)?
        else {
            return Ok(None);
        };
        stored
    } else {
        value
    };
    let array = strip_static_callback_acquire(ctx, candidate)?;
    if value_defining_op(ctx, array)? == Some(Op::ArrayNew) {
        let (array_block, _) = value_instruction_location(ctx, array)?;
        let limit_index = if array_block == block {
            index
        } else {
            u32::MAX
        };
        Ok(Some((array, array_block, limit_index)))
    } else {
        Ok(None)
    }
}

/// Resolves the last same-block local store before a callback local load.
pub(super) fn static_callback_local_stored_value(
    ctx: &FunctionContext<'_>,
    block: BlockId,
    load_index: u32,
    load_inst: &Instruction,
    owner: &str,
) -> Result<Option<ValueId>> {
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
    if stored.is_none() {
        stored = unique_static_callback_local_store(ctx, slot)?;
    }
    Ok(stored)
}

/// Returns the stored value for a callback local only when the function writes it once.
pub(super) fn unique_static_callback_local_store(
    ctx: &FunctionContext<'_>,
    slot: LocalSlotId,
) -> Result<Option<ValueId>> {
    let mut stored = None;
    for block in &ctx.function.blocks {
        for inst_id in &block.instructions {
            let inst_ref = ctx
                .function
                .instruction(*inst_id)
                .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
            if inst_ref.op == Op::StoreLocal
                && matches!(inst_ref.immediate, Some(Immediate::LocalSlot(candidate)) if candidate == slot)
            {
                if stored.is_some() {
                    return Ok(None);
                }
                stored = inst_ref.operands.first().copied();
            }
        }
    }
    Ok(stored)
}

/// Follows retains and identity forwarding while preserving the original callable-array producer.
pub(super) fn strip_static_callback_acquire(ctx: &FunctionContext<'_>, mut value: ValueId) -> Result<ValueId> {
    for _ in 0..ctx.function.values.len() {
        let Some(value_ref) = ctx.function.value(value) else {
            return Err(CodegenIrError::missing_entry("value", value.as_raw()));
        };
        let ValueDef::Instruction { inst, .. } = value_ref.def else { return Ok(value); };
        let Some(inst_ref) = ctx.function.instruction(inst) else {
            return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
        };
        if !matches!(inst_ref.op, Op::Acquire | Op::Move | Op::Borrow) { return Ok(value); }
        let Some(source) = inst_ref.operands.first() else { return Ok(value); };
        value = *source;
    }
    Err(CodegenIrError::invalid_module("cyclic callable-array ownership provenance"))
}

/// Returns the defining opcode for an SSA value when it comes from an instruction.
pub(super) fn value_defining_op(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<Op>> {
    let (inst, _) = match value_instruction(ctx, value)? {
        Some(location) => location,
        None => return Ok(None),
    };
    Ok(Some(inst.op))
}

/// Returns the instruction and block location that define an SSA value.
pub(super) fn value_instruction<'a>(
    ctx: &'a FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<(&'a Instruction, BlockId)>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { block, inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    Ok(Some((inst_ref, block)))
}

/// Returns the block and instruction index that define an instruction-backed SSA value.
pub(super) fn value_instruction_location(ctx: &FunctionContext<'_>, value: ValueId) -> Result<(BlockId, u32)> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { block, index, .. } = value_ref.def else {
        return Err(CodegenIrError::invalid_module(
            "static callable-array source is not instruction-backed",
        ));
    };
    Ok((block, index))
}

/// Collects item values pushed into a static callable-array literal before use.
pub(super) fn static_callable_array_items(
    ctx: &FunctionContext<'_>,
    array: ValueId,
    block: BlockId,
    limit_index: u32,
) -> Result<Vec<ValueId>> {
    let block_ref = ctx
        .function
        .block(block)
        .ok_or_else(|| CodegenIrError::missing_entry("block", block.as_raw()))?;
    let mut items = Vec::new();
    for (index, inst_id) in block_ref.instructions.iter().enumerate() {
        if index as u32 >= limit_index {
            break;
        }
        let inst_ref = ctx
            .function
            .instruction(*inst_id)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
        if inst_ref.op == Op::ArrayPush && inst_ref.operands.first().copied() == Some(array) {
            let Some(item) = inst_ref.operands.get(1).copied() else {
                return Err(CodegenIrError::invalid_module(
                    "callable array push missing value operand",
                ));
            };
            items.push(item);
        }
    }
    Ok(items)
}

/// Returns true when a callable-array receiver item is a statically typed object value.
pub(super) fn static_callback_object_receiver(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    Ok(matches!(
        ctx.value_php_type(value)?.codegen_repr(),
        PhpType::Object(_)
    ))
}

/// Returns a constant string value used by a static callable-array item.
pub(super) fn static_callback_const_string(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<String>> {
    let value = strip_static_callback_acquire(ctx, value)?;
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "callable array const_str item has no data id",
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
