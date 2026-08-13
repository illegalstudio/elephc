//! Purpose:
//! Materializes method and by-reference call arguments with writeback.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.
//! - EVERY BY-REFERENCE ARGUMENT NEEDS AN ADDRESS, and there are four sources for one:
//!   the caller local's own storage, an array element's slot, a caller-side stack cell that
//!   is WRITTEN BACK into a scalar local afterwards (a scalar local passed to a `mixed`
//!   by-reference parameter), and — for an argument with no caller variable at all, i.e. an
//!   OMITTED optional by-reference argument — a caller-side stack cell that is simply
//!   discarded. The last two share one pushed cell block, planned before any argument is
//!   staged and released once after the call.
//! - THE DISCARDED CELL USED TO BE A HEAP ALLOCATION THAT NOTHING FREED. `f($x)` against
//!   `f($x, int &$out = null)` leaked 16 bytes PER CALL — unbounded in a loop, and PHP's
//!   documented `while ($info = curl_multi_info_read($mh))` loop is exactly that shape.
//!   Moving it into the existing cell block makes the release automatic; the heap path is
//!   kept only as a defensive fallback for a caller that plans no cells at all.

use super::*;

/// Loads method call arguments for lexical `self::`/`parent::` instance calls using local `this`.
pub(super) fn materialize_method_call_args_with_receiver_local_and_refs(
    ctx: &mut FunctionContext<'_>,
    receiver_slot: LocalSlotId,
    receiver_ty: &PhpType,
    operands: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    lifetime: RefArgCellLifetime,
) -> Result<CallArgMaterialization> {
    if operands.len() + 1 != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "lexical instance call materialization received {} operands for {} params",
            operands.len(),
            param_types.len()
        )));
    }
    if ref_params.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "lexical instance call materialization received {} ref flags for {} params",
            ref_params.len(),
            param_types.len()
        )));
    }
    let visible_param_types = &param_types[1..];
    let visible_ref_params = &ref_params[1..];
    let mut ref_writebacks =
        plan_ref_arg_writebacks(ctx, operands, visible_param_types, visible_ref_params)?;
    let mut ref_temp_cells = plan_ref_arg_temp_cells(
        ctx,
        operands,
        visible_param_types,
        visible_ref_params,
        &ref_writebacks,
        lifetime,
    )?;
    emit_ref_arg_cell_block(ctx, &mut ref_writebacks, &mut ref_temp_cells)?;
    let abi_param_types = abi_param_types_for_refs(param_types, ref_params);
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &abi_param_types, 0);
    ctx.load_local_to_result(receiver_slot)?;
    abi::emit_push_result_value(ctx.emitter, receiver_ty);
    let mut arg_temp_bytes = call_arg_temp_slot_size(&abi_param_types[0]);
    for (index, (value, param_ty)) in operands.iter().zip(visible_param_types.iter()).enumerate() {
        if visible_ref_params[index] {
            materialize_ref_arg_address(
                ctx,
                *value,
                index,
                param_ty,
                arg_temp_bytes,
                &ref_writebacks,
                &ref_temp_cells,
                0,
            )?;
            abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
        } else {
            ctx.load_value_to_result(*value)?;
            let source_ty = ctx.raw_value_php_type(*value)?;
            let push_ty = materialize_direct_call_arg_for_param(ctx, &source_ty, param_ty)?;
            abi::emit_push_result_value(ctx.emitter, &push_ty);
        }
        arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[index + 1]);
    }
    Ok(CallArgMaterialization {
        overflow_bytes: abi::materialize_outgoing_args(ctx.emitter, &assignments),
        ref_writebacks,
        ref_temp_cells,
        cleanup_slots: Vec::new(),
        cleanup_bytes: 0,
        borrowed_stack_arg_bytes: 0,
    })
}

/// Loads method call arguments with by-reference parameter support for local operands.
pub(super) fn materialize_method_call_args_with_receiver_reg_and_refs(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    receiver_ty: &PhpType,
    operands: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
) -> Result<CallArgMaterialization> {
    if operands.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "method call materialization received {} operands for {} params",
            operands.len(),
            param_types.len()
        )));
    }
    if ref_params.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "method call materialization received {} ref flags for {} params",
            ref_params.len(),
            param_types.len()
        )));
    }
    let ref_writebacks = plan_ref_arg_writebacks(ctx, operands, param_types, ref_params)?;
    if !ref_writebacks.is_empty() {
        return Err(CodegenIrError::unsupported(
            "receiver-register method call with scalar-to-mixed by-reference writebacks",
        ));
    }
    // `RefArgCellLifetime::CallOnly` is not a guess: PHP forbids calling `__construct()` as
    // an ordinary instance method, so no callee reachable through a receiver REGISTER can
    // promote a by-reference parameter into a property.
    let mut ref_temp_cells = plan_ref_arg_temp_cells(
        ctx,
        operands,
        param_types,
        ref_params,
        &ref_writebacks,
        RefArgCellLifetime::CallOnly,
    )?;
    // The receiver is already in a register here, so the cell block is pushed BEFORE it is
    // staged — the same order the receiver-local variant uses.
    let mut no_writebacks: Vec<RefArgWriteback> = Vec::new();
    emit_ref_arg_cell_block(ctx, &mut no_writebacks, &mut ref_temp_cells)?;
    let abi_param_types = abi_param_types_for_refs(param_types, ref_params);
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &abi_param_types, 0);
    move_reg_to_int_result(ctx, receiver_reg);
    abi::emit_push_result_value(ctx.emitter, receiver_ty);
    let mut arg_temp_bytes = call_arg_temp_slot_size(&abi_param_types[0]);
    for (index, (value, param_ty)) in operands
        .iter()
        .skip(1)
        .zip(param_types.iter().skip(1))
        .enumerate()
    {
        let param_index = index + 1;
        if ref_params[param_index] {
            materialize_ref_arg_address(
                ctx,
                *value,
                param_index,
                &param_types[param_index],
                arg_temp_bytes,
                &ref_writebacks,
                &ref_temp_cells,
                0,
            )?;
            abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
        } else {
            ctx.load_value_to_result(*value)?;
            let source_ty = ctx.raw_value_php_type(*value)?;
            let push_ty = materialize_direct_call_arg_for_param(ctx, &source_ty, param_ty)?;
            abi::emit_push_result_value(ctx.emitter, &push_ty);
        }
        arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[param_index]);
    }
    Ok(CallArgMaterialization {
        overflow_bytes: abi::materialize_outgoing_args(ctx.emitter, &assignments),
        ref_writebacks,
        ref_temp_cells,
        cleanup_slots: Vec::new(),
        cleanup_bytes: 0,
        borrowed_stack_arg_bytes: 0,
    })
}

/// Converts declared parameter types to the ABI-visible shape for by-reference args.
pub(super) fn abi_param_types_for_refs(param_types: &[PhpType], ref_params: &[bool]) -> Vec<PhpType> {
    param_types
        .iter()
        .zip(ref_params.iter())
        .map(|(ty, is_ref)| {
            if *is_ref {
                PhpType::Int
            } else {
                ty.codegen_repr()
            }
        })
        .collect()
}

/// Returns the temporary stack slot size used by outgoing-argument staging.
pub(super) fn call_arg_temp_slot_size(ty: &PhpType) -> usize {
    if matches!(ty.codegen_repr(), PhpType::Void | PhpType::Never) {
        0
    } else {
        16
    }
}

/// Plans caller-side Mixed cells needed for scalar locals passed to by-reference Mixed params.
pub(super) fn plan_ref_arg_writebacks(
    ctx: &FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
) -> Result<Vec<RefArgWriteback>> {
    let mut writebacks = Vec::new();
    for (param_index, value) in args.iter().enumerate() {
        if !ref_params[param_index] || param_types[param_index].codegen_repr() != PhpType::Mixed {
            continue;
        }
        let source_ty = ctx.raw_value_php_type(*value)?.codegen_repr();
        if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
            continue;
        }
        reject_unsupported_mixed_ref_writeback_source(&source_ty)?;
        let source = local_ref_arg_source(ctx, *value)?;
        writebacks.push(RefArgWriteback {
            param_index,
            source_value: *value,
            source_slot: source.slot,
            source_ty,
            cell_offset: 0,
        });
    }
    Ok(writebacks)
}

/// Rejects scalar-to-Mixed temporary ref cells whose writeback shape is not supported yet.
pub(super) fn reject_unsupported_mixed_ref_writeback_source(source_ty: &PhpType) -> Result<()> {
    if matches!(source_ty.codegen_repr(), PhpType::Int | PhpType::Bool) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "by-reference Mixed parameter writeback to PHP type {:?}",
        source_ty
    )))
}

/// Plans caller-side stack cells for by-reference arguments that have NO caller variable
/// behind them — the OMITTED optional by-reference argument, above all.
///
/// The predicates below mirror [`materialize_ref_arg_address`]'s own order exactly: an
/// argument that already has a writeback cell, a local slot, or an array-element address
/// needs no cell of its own. Everything else would otherwise reach the heap fallback, whose
/// allocation nothing frees.
pub(super) fn plan_ref_arg_temp_cells(
    ctx: &FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    writebacks: &[RefArgWriteback],
    lifetime: RefArgCellLifetime,
) -> Result<Vec<RefArgTempCell>> {
    let mut cells = Vec::new();
    // A callee that may KEEP the reference needs storage that outlives this frame, so it
    // keeps the heap cell (see `RefArgCellLifetime`). Planning a stack cell there would hand
    // a constructor-promoted property a pointer into a frame that is gone by its first use.
    if lifetime == RefArgCellLifetime::MayOutliveCall {
        return Ok(cells);
    }
    for (param_index, value) in args.iter().enumerate() {
        if !ref_params[param_index] {
            continue;
        }
        if writebacks
            .iter()
            .any(|writeback| writeback.param_index == param_index)
        {
            continue;
        }
        if local_ref_arg_source(ctx, *value).is_ok() {
            continue;
        }
        if value_is_array_element_address(ctx, *value)? {
            continue;
        }
        cells.push(RefArgTempCell {
            param_index,
            source_value: *value,
            cell_ty: param_types[param_index].codegen_repr(),
            cell_offset: 0,
        });
    }
    Ok(cells)
}

/// Emits the caller-side by-reference cell block: the Mixed writeback cells first, then the
/// discarded cells, as one contiguous run of 16-byte stack slots.
///
/// Offsets are assigned across the WHOLE block (the last cell pushed sits at the current
/// stack pointer), which is what lets [`materialize_ref_arg_address`] address either kind
/// the same way and [`emit_ref_arg_writebacks`] release them in one step.
pub(super) fn emit_ref_arg_cell_block(
    ctx: &mut FunctionContext<'_>,
    writebacks: &mut [RefArgWriteback],
    temp_cells: &mut [RefArgTempCell],
) -> Result<()> {
    let total = writebacks.len() + temp_cells.len();
    for (index, writeback) in writebacks.iter_mut().enumerate() {
        ctx.load_value_to_result(writeback.source_value)?;
        emit_box_current_value_as_mixed(ctx.emitter, &writeback.source_ty);
        abi::emit_push_result_value(ctx.emitter, &PhpType::Mixed);
        writeback.cell_offset = (total - index - 1) * 16;
    }
    let pushed = writebacks.len();
    for (index, cell) in temp_cells.iter_mut().enumerate() {
        let source_ty = ctx.load_value_to_result(cell.source_value)?;
        coerce_ref_cell_store_value(ctx, &source_ty, &cell.cell_ty)?;
        abi::emit_push_result_value(ctx.emitter, &cell.cell_ty);
        // A push writes ONE word for every representation except `Str`/`TaggedScalar`, so
        // the cell's second word is whatever the stack happened to hold. The heap path this
        // replaces zeroed it, and a callee that reads the cell as a two-word value (a
        // string, a tagged scalar) must not see garbage there.
        if !matches!(cell.cell_ty.codegen_repr(), PhpType::Str | PhpType::TaggedScalar) {
            let scratch = abi::symbol_scratch_reg(ctx.emitter);
            abi::emit_temporary_stack_address(ctx.emitter, scratch, 0);
            abi::emit_store_zero_to_address(ctx.emitter, scratch, 8);
        }
        cell.cell_offset = (total - pushed - index - 1) * 16;
    }
    Ok(())
}

/// Loads the address that should be passed for a by-reference argument.
pub(super) fn materialize_ref_arg_address(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    param_index: usize,
    param_ty: &PhpType,
    arg_temp_bytes: usize,
    writebacks: &[RefArgWriteback],
    temp_cells: &[RefArgTempCell],
    ref_cell_base_offset: usize,
) -> Result<()> {
    if let Some(writeback) = writebacks
        .iter()
        .find(|writeback| writeback.param_index == param_index)
    {
        let cell_offset = arg_temp_bytes + ref_cell_base_offset + writeback.cell_offset;
        abi::emit_temporary_stack_address(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            cell_offset,
        );
        return Ok(());
    }
    if local_ref_arg_source(ctx, value).is_ok() {
        return materialize_local_ref_arg_address(ctx, value);
    }
    if value_is_array_element_address(ctx, value)? {
        ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
        return Ok(());
    }
    if let Some(cell) = temp_cells
        .iter()
        .find(|cell| cell.param_index == param_index)
    {
        let cell_offset = arg_temp_bytes + ref_cell_base_offset + cell.cell_offset;
        abi::emit_temporary_stack_address(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            cell_offset,
        );
        return Ok(());
    }
    materialize_temporary_ref_arg_cell(ctx, value, param_ty)
}

/// Allocates a heap ref-cell for a by-reference argument that is not a local variable.
///
/// DEFENSIVE FALLBACK ONLY. Every call path that stages by-reference arguments now plans a
/// stack cell for exactly this case ([`plan_ref_arg_temp_cells`]), because the allocation
/// below is never freed — one leaked 16-byte block per call, which an omitted optional
/// by-reference argument in a loop turns into unbounded growth. A caller that reaches this
/// function has planned no cells at all.
pub(super) fn materialize_temporary_ref_arg_cell(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    param_ty: &PhpType,
) -> Result<()> {
    let source_ty = ctx.load_value_to_result(value)?;
    let target_ty = param_ty.codegen_repr();
    coerce_ref_cell_store_value(ctx, &source_ty, &target_ty)?;
    abi::emit_push_result_value(ctx.emitter, &target_ty);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    let cell_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_pop_reg(ctx.emitter, cell_reg);
    store_pushed_value_to_ref_cell(ctx, cell_reg, &target_ty);
    move_reg_to_int_result(ctx, cell_reg);
    Ok(())
}

/// Stores the pushed argument value into a freshly allocated by-reference cell.
pub(super) fn store_pushed_value_to_ref_cell(ctx: &mut FunctionContext<'_>, cell_reg: &str, val_ty: &PhpType) {
    let temp_reg = if cell_reg == abi::temp_int_reg(ctx.emitter.target) {
        abi::symbol_scratch_reg(ctx.emitter)
    } else {
        abi::temp_int_reg(ctx.emitter.target)
    };
    match val_ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
            abi::emit_store_to_address(ctx.emitter, ptr_reg, cell_reg, 0);
            abi::emit_store_to_address(ctx.emitter, len_reg, cell_reg, 8);
        }
        PhpType::TaggedScalar => {
            let tag_reg = crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, abi::int_result_reg(ctx.emitter), tag_reg);
            abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), cell_reg, 0);
            abi::emit_store_to_address(ctx.emitter, tag_reg, cell_reg, 8);
        }
        PhpType::Float => {
            abi::emit_pop_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
            abi::emit_store_to_address(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                cell_reg,
                0,
            );
        }
        _ => {
            abi::emit_pop_reg(ctx.emitter, temp_reg);
            abi::emit_store_to_address(ctx.emitter, temp_reg, cell_reg, 0);
            abi::emit_store_zero_to_address(ctx.emitter, cell_reg, 8);
        }
    }
}

/// Writes temporary Mixed by-reference cells back into the original caller locals, releases
/// whatever a discarded cell ended up holding, and frees the whole cell block.
///
/// One function for both kinds because they share one pushed block: releasing them
/// separately would need two stack adjustments and two chances to get the order wrong.
pub(super) fn emit_ref_arg_writebacks(
    ctx: &mut FunctionContext<'_>,
    call_args: &CallArgMaterialization,
) -> Result<()> {
    for writeback in &call_args.ref_writebacks {
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            writeback.cell_offset,
        );
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
        store_current_scalar_result_to_ref_source(ctx, writeback)?;
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    }
    for cell in &call_args.ref_temp_cells {
        // A discarded cell has no caller variable to write back to, but a REFCOUNTED one
        // still holds a value the caller owns — the default the caller materialized, or
        // whatever the callee left in its place, which `store_ref_cell` retained on the way
        // in. Releasing it is the same ownership rule the writeback loop above applies to
        // its own cells. `emit_decref_if_refcounted` is the canonical dispatcher and is
        // deliberately a no-op for scalars AND for `Str`, whose ownership is not refcounted
        // in this runtime; a string left in a discarded cell therefore keeps whatever
        // behaviour the surrounding string-return path already has.
        let cell_ty = cell.cell_ty.codegen_repr();
        if !matches!(
            cell_ty,
            PhpType::Mixed
                | PhpType::Union(_)
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_)
                | PhpType::Iterable
                | PhpType::Callable
        ) {
            continue;
        }
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            cell.cell_offset,
        );
        abi::emit_decref_if_refcounted(ctx.emitter, &cell_ty);
    }
    let block_cells = call_args.ref_writebacks.len() + call_args.ref_temp_cells.len();
    abi::emit_release_temporary_stack(ctx.emitter, block_cells * 16);
    Ok(())
}

/// Returns the low payload register produced by `__rt_mixed_unbox` on the active target.
pub(super) fn mixed_unbox_low_payload_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    }
}

/// Unboxes a boxed Mixed/Union payload and retains it for an owned concrete heap result.
pub(super) fn emit_unbox_mixed_to_owned_refcounted_result(ctx: &mut FunctionContext<'_>, result_ty: &PhpType) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
    abi::emit_incref_if_refcounted(ctx.emitter, result_ty);
}

/// Unboxes a guarded Mixed value into an owned concrete heap representation.
///
/// Flow-sensitive checking proves the value has the requested type before this op is emitted;
/// the runtime helper extracts its payload and this result takes its own reference so retaining
/// stores and later cleanup have a balanced ownership ledger.
pub(super) fn lower_mixed_unbox(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    load_value_to_first_int_arg(ctx, value)?;
    let result_ty = inst.result_php_type.codegen_repr();
    emit_unbox_mixed_to_owned_refcounted_result(ctx, &result_ty);
    store_if_result(ctx, inst)
}

/// Stores an unboxed scalar Mixed payload back through the original by-reference source.
pub(super) fn store_current_scalar_result_to_ref_source(
    ctx: &mut FunctionContext<'_>,
    writeback: &RefArgWriteback,
) -> Result<()> {
    ctx.store_current_result_to_local(writeback.source_slot)
}

/// Loads a local variable's address for a by-reference method-call argument.
pub(super) fn materialize_local_ref_arg_address(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let source = local_ref_arg_source(ctx, value)?;
    ctx.materialize_local_storage_address(source.slot, abi::int_result_reg(ctx.emitter))
}

/// Returns true when a value already holds a direct pointer to an array element slot.
pub(super) fn value_is_array_element_address(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(false);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    Ok(inst_ref.op == Op::ArrayElemAddr)
}

/// Describes a local operand used as a by-reference call argument.
struct LocalRefArgSource {
    slot: LocalSlotId,
}

/// Resolves an EIR value back to a local slot and whether it already stores a ref-cell pointer.
fn local_ref_arg_source(ctx: &FunctionContext<'_>, value: ValueId) -> Result<LocalRefArgSource> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(
            "by-reference method call argument from non-local value",
        ));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    match inst_ref.op {
        Op::LoadLocal | Op::LoadRefCell => {}
        _ => {
            return Err(CodegenIrError::unsupported(format!(
                "by-reference method call argument from opcode {}",
                inst_ref.op.name()
            )))
        }
    };
    let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "by-reference load argument has no local slot",
        ));
    };
    Ok(LocalRefArgSource { slot })
}

/// Resolves an EIR value back to a `load_local` source slot for by-reference calls.
pub(super) fn local_slot_for_loaded_value(ctx: &FunctionContext<'_>, value: ValueId) -> Result<LocalSlotId> {
    local_ref_arg_source(ctx, value).map(|source| source.slot)
}

/// Returns true when a local slot stores a ref-cell pointer instead of a raw value.
pub(super) fn local_slot_stores_ref_cell_pointer(ctx: &FunctionContext<'_>, slot: LocalSlotId) -> bool {
    ctx.local_stores_ref_cell_pointer(slot)
}

/// Moves a scratch integer register into the canonical integer result register.
pub(super) fn move_reg_to_int_result(ctx: &mut FunctionContext<'_>, source_reg: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    if source_reg == result_reg {
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov {}, {}", result_reg, source_reg)); // move the unboxed receiver pointer into the normal argument staging register
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov {}, {}", result_reg, source_reg)); // move the unboxed receiver pointer into the normal argument staging register
        }
    }
}
