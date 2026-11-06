//! Purpose:
//! Materializes method and by-reference call arguments with writeback.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.
//! - Method argument coercions use the shared cleanup plan; callers retire its block before
//!   processing reference writebacks, whose addresses include the intervening cleanup bytes.
//! - Reference arguments borrow caller locals or indexed element addresses. Managed hash-entry
//!   cells and omitted defaults use scoped owner records; scalar-to-Mixed uses writeback cells.
//! - Closures can retain default cells beyond the call. Normal return and exception unwinding
//!   retire only the caller's lease, leaving any captured lease intact.
//! - Constructor-promoted borrowed properties still use the separate persistent-cell fallback.

use super::*;

/// Loads lexical instance-call arguments using local `this`, tracking coercion owners for cleanup.
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
    let preleased_ref_cells =
        plan_preleased_ref_arg_cells(ctx, operands, visible_ref_params)?;
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
    emit_ref_arg_cell_block(
        ctx,
        &mut ref_writebacks,
        &mut ref_temp_cells,
    )?;
    let cleanup_slots = plan_call_arg_temp_cleanups(
        ctx, operands, visible_param_types, visible_ref_params, &[],
    )?;
    let cleanup_bytes = cleanup_slots.len() * CALL_ARG_TEMP_CLEANUP_BYTES;
    abi::emit_reserve_temporary_stack(ctx.emitter, cleanup_bytes);
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
                cleanup_bytes,
            )?;
            abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
        } else {
            let cleanup = cleanup_slots.iter().find(|cleanup| cleanup.param_index == index);
            let push_ty = materialize_plain_call_arg(ctx, *value, param_ty, cleanup, arg_temp_bytes)?;
            abi::emit_push_result_value(ctx.emitter, &push_ty);
        }
        arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[index + 1]);
    }
    Ok(CallArgMaterialization {
        overflow_bytes: abi::materialize_outgoing_args(ctx.emitter, &assignments),
        ref_writebacks,
        ref_temp_cells,
        preleased_ref_cells,
        cleanup_slots,
        cleanup_bytes,
        cleanup_guard_bytes: 0,
        borrowed_stack_arg_bytes: 0,
    })
}

/// Loads a register receiver and method arguments, tracking coercion owners and reference cells.
pub(super) fn materialize_method_call_args_with_receiver_reg_and_refs(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    receiver_ty: &PhpType,
    operands: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    lifetime: RefArgCellLifetime,
) -> Result<CallArgMaterialization> {
    materialize_method_call_args_with_receiver_reg_and_refs_options(
        ctx,
        receiver_reg,
        receiver_ty,
        operands,
        param_types,
        ref_params,
        lifetime,
        false,
    )
}


/// Loads receiver-register method arguments with optional guarded conversion cleanup.
fn materialize_method_call_args_with_receiver_reg_and_refs_options(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    receiver_ty: &PhpType,
    operands: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    lifetime: RefArgCellLifetime,
    _guard_conversion_temporaries: bool,
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
    let mut ref_writebacks = plan_ref_arg_writebacks(ctx, operands, param_types, ref_params)?;
    let preleased_ref_cells = plan_preleased_ref_arg_cells(ctx, operands, ref_params)?;
    // Dynamic constructors use the same reference lifetime selection as fixed construction.
    // Only classes with borrowed reference properties keep the persistent fallback.
    let mut ref_temp_cells =
        plan_ref_arg_temp_cells(ctx, operands, param_types, ref_params, &ref_writebacks, lifetime)?;
    // THE RECEIVER IS ALREADY IN A REGISTER, AND THE CELL BLOCK IS ALLOWED TO DESTROY
    // CALLER-SAVED ONES: materializing a cell's value runs `load_value_to_result` and may
    // call runtime helpers, so anything the caller left in a caller-saved register, or in
    // the integer result register, is gone by the time the receiver is staged below.
    //
    // Every caller that can reach a non-empty block therefore hands the receiver over in the
    // reserved CALLEE-SAVED nested-call register (`abi::nested_call_reg`, x19/r12), which
    // `crate::codegen::frame` reserves a save slot for. This check makes that a hard
    // contract instead of a coincidence: a future caller that passes a scratch register and
    // a by-reference argument needing a cell gets a compile-time backend error rather than a
    // constructor entered with the cell's value as `$this`. A writeback cell (a declared local
    // passed to a nullable by-reference parameter) boxes its value the same way.
    if (!ref_temp_cells.is_empty() || !ref_writebacks.is_empty())
        && receiver_reg != abi::nested_call_reg(ctx.emitter)
    {
        return Err(CodegenIrError::unsupported(format!(
            "receiver-register method call staging a by-reference cell with the receiver in \
             {receiver_reg} (needs the callee-saved nested-call register)"
        )));
    }
    emit_ref_arg_cell_block(
        ctx,
        &mut ref_writebacks,
        &mut ref_temp_cells,
    )?;
    let cleanup_slots = plan_call_arg_temp_cleanups(ctx, operands, param_types, ref_params, &[])?;
    let cleanup_bytes = cleanup_slots.len() * CALL_ARG_TEMP_CLEANUP_BYTES;
    abi::emit_reserve_temporary_stack(ctx.emitter, cleanup_bytes);
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
                cleanup_bytes,
            )?;
            abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
        } else {
            let cleanup = cleanup_slots.iter().find(|cleanup| cleanup.param_index == param_index);
            let push_ty = materialize_plain_call_arg(ctx, *value, param_ty, cleanup, arg_temp_bytes)?;
            abi::emit_push_result_value(ctx.emitter, &push_ty);
        }
        arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[param_index]);
    }
    Ok(CallArgMaterialization {
        overflow_bytes: abi::materialize_outgoing_args(ctx.emitter, &assignments),
        ref_writebacks,
        ref_temp_cells,
        preleased_ref_cells,
        cleanup_slots,
        cleanup_bytes,
        cleanup_guard_bytes: 0,
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
        // An acquired hash-entry cell already is the caller's writable Mixed storage. Looking
        // through it to the entry's earlier concrete payload type would incorrectly schedule a
        // scalar writeback against a value that has no local source slot.
        if value_is_acquired_array_element_ref_cell(ctx, *value)? {
            continue;
        }
        let source_ty = if let Some(array) = array_element_address_source(ctx, *value)? {
            match ctx.value_php_type(array)?.codegen_repr() {
                PhpType::Array(element) => element.codegen_repr(),
                PhpType::AssocArray { value, .. } => value.codegen_repr(),
                _ => {
                    return Err(CodegenIrError::invalid_module(
                        "array element address requires an array receiver",
                    ));
                }
            }
        } else {
            ctx.raw_value_php_type(*value)?.codegen_repr()
        };
        if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
            continue;
        }
        if local_ref_arg_source(ctx, *value).ok()
            .and_then(|source| ctx.local_php_type(source.slot).ok())
            .is_some_and(|storage| storage.codegen_repr() == PhpType::Mixed)
        {
            continue;
        }
        if array_element_address_source(ctx, *value)?.is_none()
            && local_ref_arg_source(ctx, *value).is_err()
        {
            // Omitted defaults have no caller location. The temporary-cell planner owns
            // their conversion and disposal, so there is no scalar/container writeback.
            continue;
        }
        reject_unsupported_mixed_ref_writeback_source(&source_ty)?;
        let source = local_ref_arg_source(ctx, *value)?;
        // A second reference to the same local shares the first one's cell.
        if let Some(existing) = writebacks
            .iter_mut()
            .find(|writeback: &&mut RefArgWriteback| writeback.source_slot == source.slot)
        {
            existing.alias_param_indexes.push(param_index);
            continue;
        }
        writebacks.push(RefArgWriteback {
            param_index,
            alias_param_indexes: Vec::new(),
            source_value: *value,
            source_slot: source.slot,
            source_ty,
            cell_offset: 0,
        });
    }
    Ok(writebacks)
}

/// Rejects concrete-to-Mixed temporary ref cells whose writeback shape is not supported yet.
///
/// Scalars (`int`, `bool`, `float`) and the heap containers whose payload is one pointer word
/// (indexed arrays, hashes, objects) are written back; the writeback verifies the callee left a
/// value of the same runtime kind (see [`emit_heap_ref_writeback`]).
pub(super) fn reject_unsupported_mixed_ref_writeback_source(source_ty: &PhpType) -> Result<()> {
    if matches!(
        source_ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    ) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "by-reference Mixed parameter writeback to PHP type {:?}",
        source_ty
    )))
}

/// Plans managed reference-cell leases and default cells required by call arguments.
/// Writebacks, locals, and borrowed indexed element addresses need no additional cell.
pub(super) fn plan_ref_arg_temp_cells(
    ctx: &FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    writebacks: &[RefArgWriteback],
    lifetime: RefArgCellLifetime,
) -> Result<Vec<RefArgTempCell>> {
    let mut cells = Vec::new();
    // Constructor-promoted properties borrow without retaining. Until their ownership
    // model adopts managed cells, they must keep the separate persistent fallback.
    if lifetime == RefArgCellLifetime::MayOutliveCall {
        return Ok(cells);
    }
    for (param_index, value) in args.iter().enumerate() {
        if !ref_params[param_index] {
            continue;
        }
        if writebacks.iter().any(|writeback| writeback.covers(param_index)) {
            continue;
        }
        if local_ref_arg_source(ctx, *value).is_ok() {
            continue;
        }
        if value_is_acquired_array_element_ref_cell(ctx, *value)? {
            continue;
        }
        if value_is_managed_array_element_ref_cell(ctx, *value)? {
            cells.push(RefArgTempCell {
                param_index,
                source_value: *value,
                cell_ty: param_types[param_index].codegen_repr(),
                cell_offset: 0,
                retain_existing: true,
            });
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
            retain_existing: false,
        });
    }
    Ok(cells)
}

/// Returns whether EIR already acquired the managed array-element cell used by this operand.
/// `Borrow` marks ledger cleanup and `Move` is a neutral forward, but both still name the same
/// acquired writable cell and must bypass scalar writeback and backend fallback-cell planning.
fn value_is_acquired_array_element_ref_cell(
    ctx: &FunctionContext<'_>,
    mut value: ValueId,
) -> Result<bool> {
    loop {
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
        match inst_ref.op {
            Op::AcquireRefCell => return Ok(true),
            Op::Borrow | Op::Move => value = expect_operand(inst_ref, 0)?,
            _ => return Ok(false),
        }
    }
}

/// Finds managed cell owners acquired during EIR argument evaluation without an EIR ledger.
///
/// A ledger-protected operand carries an outer `Borrow`, while a bare `AcquireRefCell` delegates
/// normal-path retirement to the shared ABI call cleanup. The IR peephole preserves that marker:
/// searching the whole function for a later release would conflate mutually exclusive paths and
/// owner-slot lifetimes, and could suppress the only normal-path cleanup for this call.
pub(super) fn plan_preleased_ref_arg_cells(
    ctx: &FunctionContext<'_>,
    args: &[ValueId],
    ref_params: &[bool],
) -> Result<Vec<PreleasedRefArgCell>> {
    let mut cells = Vec::new();
    for (value, is_ref) in args.iter().zip(ref_params.iter()) {
        if !is_ref {
            continue;
        }
        let Some((owner_slot, publication_order)) = preleased_ref_cell_owner(ctx, *value)? else {
            continue;
        };
        if !cells
            .iter()
            .any(|(_, cell): &(u32, PreleasedRefArgCell)| cell.owner_slot == owner_slot)
        {
            cells.push((
                publication_order,
                PreleasedRefArgCell {
                    owner_slot,
                    cell_ty: PhpType::Mixed,
                },
            ));
        }
    }
    // Named arguments are stored in ABI parameter order, which can be the reverse of source
    // evaluation and cleanup-record publication. Instruction order is the EIR source order and
    // therefore the only valid basis for the later LIFO pop.
    cells.sort_by_key(|(publication_order, _)| *publication_order);
    Ok(cells.into_iter().map(|(_, cell)| cell).collect())
}

/// Traces neutral forwarding around a bare EIR-managed argument lease.
/// `Borrow` is a deliberate stop marker: its enclosing EIR ledger owns cleanup.
fn preleased_ref_cell_owner(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<(LocalSlotId, u32)>> {
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
    if inst_ref.op == Op::Borrow {
        return Ok(None);
    }
    if inst_ref.op == Op::Move {
        return preleased_ref_cell_owner(ctx, expect_operand(inst_ref, 0)?);
    }
    if inst_ref.op != Op::AcquireRefCell {
        return Ok(None);
    }
    let Some(Immediate::LocalSlot(owner_slot)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "preleased reference-cell argument has no owner slot",
        ));
    };
    Ok(Some((owner_slot, inst.as_raw())))
}

/// Keeps the persistent fallback only when the constructed hierarchy can borrow a property cell.
/// Ordinary constructors use managed call leases, which escaping closures can retain themselves.
pub(super) fn constructor_ref_cell_lifetime(
    ctx: &FunctionContext<'_>,
    class_name: &str,
) -> RefArgCellLifetime {
    let mut current = Some(class_name);
    let mut seen = std::collections::HashSet::new();
    while let Some(name) = current {
        if !seen.insert(name) { return RefArgCellLifetime::MayOutliveCall; }
        let Some(info) = ctx.module.class_infos.get(name) else {
            return RefArgCellLifetime::MayOutliveCall;
        };
        if info.reference_properties.iter().any(|property| {
            !info.owned_reference_properties.contains(property)
        }) {
            return RefArgCellLifetime::MayOutliveCall;
        }
        // Inspect ancestors as well, since private reference properties may be shadowed.
        current = info.parent.as_deref();
    }
    RefArgCellLifetime::CallOnly
}

/// Reserves writeback slots and publishes a scoped managed owner for each default cell.
/// Each managed owner has an adjacent unwind record, linked in argument order.
pub(super) fn emit_ref_arg_cell_block(
    ctx: &mut FunctionContext<'_>,
    writebacks: &mut [RefArgWriteback],
    temp_cells: &mut [RefArgTempCell],
) -> Result<()> {
    let writeback_bytes = writebacks.len() * CALL_ARG_TEMP_CLEANUP_BYTES;
    let cell_bytes = writeback_bytes + temp_cells.len() * CALL_ARG_TEMP_CLEANUP_BYTES;
    let guard = ref_writebacks_need_result_guard(writebacks);
    let total_bytes = cell_bytes + if guard { CALL_ARG_TEMP_CLEANUP_BYTES } else { 0 };
    abi::emit_reserve_temporary_stack(ctx.emitter, total_bytes);
    if guard {
        // The result guard is linked first, so it sits beneath every cell record: the
        // writebacks retire theirs innermost first and the guard is detached last.
        let scratch = abi::tertiary_scratch_reg(ctx.emitter);
        abi::emit_load_int_immediate(ctx.emitter, scratch, 0);
        abi::emit_store_to_sp(ctx.emitter, scratch, cell_bytes);
        abi::emit_temporary_stack_address(ctx.emitter, scratch, cell_bytes);
        abi::emit_link_call_operand_owner_at_stack(ctx.emitter, scratch, false, cell_bytes + 16);
    }
    for (index, writeback) in writebacks.iter_mut().enumerate() {
        // A managed Mixed reference cell holding the boxed value: the callee may capture the
        // reference in a closure that outlives the call (`__rt_reference_cell_owner` then
        // takes its own lease), which a stack slot could not survive.
        ctx.load_value_to_result(writeback.source_value)?;
        emit_box_current_value_as_mixed(ctx.emitter, &writeback.source_ty);
        abi::emit_push_result_value(ctx.emitter, &PhpType::Mixed);
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            crate::codegen_support::runtime::reference_cells::payload_tag(&PhpType::Mixed),
        );
        abi::emit_call_label(ctx.emitter, "__rt_reference_cell_new");
        let cell_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_pop_reg(ctx.emitter, cell_reg);
        store_pushed_value_to_ref_cell(ctx, cell_reg, &PhpType::Mixed);
        writeback.cell_offset = index * CALL_ARG_TEMP_CLEANUP_BYTES;
        abi::emit_store_to_sp(ctx.emitter, cell_reg, writeback.cell_offset);
        // A throwing callee leaves no writeback: the unwind releases the cell lease instead.
        let owner_address = abi::tertiary_scratch_reg(ctx.emitter);
        abi::emit_temporary_stack_address(ctx.emitter, owner_address, writeback.cell_offset);
        abi::emit_link_call_operand_owner_at_stack(
            ctx.emitter,
            owner_address,
            false,
            writeback.cell_offset + 16,
        );
    }
    for (index, cell) in temp_cells.iter_mut().enumerate() {
        cell.cell_offset = writeback_bytes + index * CALL_ARG_TEMP_CLEANUP_BYTES;
        if cell.retain_existing {
            ctx.load_value_to_reg(cell.source_value, abi::int_result_reg(ctx.emitter))?;
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            let cell_reg = abi::symbol_scratch_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            abi::emit_pop_reg(ctx.emitter, cell_reg);
            abi::emit_store_to_sp(ctx.emitter, cell_reg, cell.cell_offset);
            let owner_address = abi::tertiary_scratch_reg(ctx.emitter);
            abi::emit_temporary_stack_address(ctx.emitter, owner_address, cell.cell_offset);
            abi::emit_link_call_operand_owner_at_stack(
                ctx.emitter,
                owner_address,
                false,
                cell.cell_offset + 16,
            );
            continue;
        }
        let source_ty = ctx.load_value_to_result(cell.source_value)?;
        coerce_ref_cell_store_value(ctx, &source_ty, &cell.cell_ty)?;
        // A container stored under another container shape is still the source itself: the
        // hash that named arguments build for an `array<mixed>` by-reference variadic went into
        // the cell unretained, so releasing the cell and then the operand freed it twice.
        let container_kept_as_is = matches!(
            (source_ty.codegen_repr(), cell.cell_ty.codegen_repr()),
            (
                PhpType::Array(_) | PhpType::AssocArray { .. },
                PhpType::Array(_) | PhpType::AssocArray { .. }
            )
        );
        if source_ty.codegen_repr() == cell.cell_ty.codegen_repr() || container_kept_as_is {
            // EIR still owns the default operand. The mutable cell needs its own retain
            // because the callee can replace it before EIR retires that original owner.
            if cell.cell_ty.codegen_repr() == PhpType::Str {
                abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            } else {
                abi::emit_incref_if_refcounted(ctx.emitter, &cell.cell_ty);
            }
        }
        abi::emit_push_result_value(ctx.emitter, &cell.cell_ty);
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            crate::codegen_support::runtime::reference_cells::payload_tag(&cell.cell_ty),
        );
        abi::emit_call_label(ctx.emitter, "__rt_reference_cell_new");
        let cell_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_pop_reg(ctx.emitter, cell_reg);
        store_pushed_value_to_ref_cell(ctx, cell_reg, &cell.cell_ty);
        abi::emit_store_to_sp(ctx.emitter, cell_reg, cell.cell_offset);
        let owner_address = abi::tertiary_scratch_reg(ctx.emitter);
        abi::emit_temporary_stack_address(ctx.emitter, owner_address, cell.cell_offset);
        abi::emit_link_call_operand_owner_at_stack(
            ctx.emitter, owner_address, false, cell.cell_offset + 16,
        );
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
        .find(|writeback| writeback.covers(param_index))
    {
        let cell_offset = arg_temp_bytes + ref_cell_base_offset + writeback.cell_offset;
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            cell_offset,
        );
        return Ok(());
    }
    if local_ref_arg_source(ctx, value).is_ok() {
        return materialize_local_ref_arg_address(ctx, value);
    }
    if let Some(cell) = temp_cells
        .iter()
        .find(|cell| cell.param_index == param_index)
    {
        let cell_offset = arg_temp_bytes + ref_cell_base_offset + cell.cell_offset;
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            cell_offset,
        );
        return Ok(());
    }
    if value_is_array_element_address(ctx, value)? {
        ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
        return Ok(());
    }
    materialize_temporary_ref_arg_cell(ctx, value, param_ty)
}

/// Allocates the persistent fallback cell required by borrowed constructor properties.
/// Ordinary calls and non-promoting constructors use managed temporary cells instead.
/// This legacy allocation must remain live until promoted properties retain their own cell
/// owners; replacing it with stack storage or releasing it on return would dangle properties.
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

/// Publishes scalar writebacks, retires default-cell leases in unwind order, and frees staging.
/// Captured cells survive because closure creation retains a separate managed lease.
pub(super) fn emit_ref_arg_writebacks(
    ctx: &mut FunctionContext<'_>,
    call_args: &CallArgMaterialization,
) -> Result<()> {
    emit_ref_arg_writebacks_guarding_result(ctx, call_args, None, false)
}

/// Stores the call result in the guard slot at `guard_offset` when it owns something: a
/// by-reference return's transferred cell and any owned refcounted value go through the guard's
/// `__rt_decref_any` cleanup (which releases a cell lease too), an owned callable descriptor
/// switches the record to the descriptor cleanup. Other results leave the slot empty.
fn emit_publish_guarded_call_result(
    ctx: &mut FunctionContext<'_>,
    result: ValueId,
    by_ref_return: bool,
    guard_offset: usize,
) -> Result<()> {
    let repr = ctx.value_php_type(result)?.codegen_repr();
    let callable = !by_ref_return && repr == PhpType::Callable;
    let owned = by_ref_return
        || (ctx.value_ownership(result)? == Ownership::Owned && (callable || repr.is_refcounted()));
    if !owned {
        return Ok(());
    }
    let reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(result, reg)?;
    abi::emit_store_to_sp(ctx.emitter, reg, guard_offset);
    if callable {
        // The record's cleanup entry follows its chain link.
        let scratch = abi::tertiary_scratch_reg(ctx.emitter);
        abi::emit_symbol_address(ctx.emitter, scratch, "__rt_cleanup_call_operand_descriptor");
        abi::emit_store_to_sp(ctx.emitter, scratch, guard_offset + 16 + 8);
    }
    Ok(())
}

/// Whether a call's writebacks can release a previous heap value, whose destructor may throw:
/// such a call reserves a result guard beneath its cell records (see
/// [`emit_ref_arg_writebacks_guarding_result`]).
fn ref_writebacks_need_result_guard(writebacks: &[RefArgWriteback]) -> bool {
    writebacks.iter().any(|writeback| {
        matches!(
            writeback.source_ty.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_)
        )
    })
}

/// [`emit_ref_arg_writebacks`] for a call whose result is already stored: an owned heap
/// result (or the cell a by-reference return transfers) is published in the call's result
/// guard while the writebacks run, so a destructor that throws when a writeback releases the
/// previous value unwinds the result too (it is not yet in the caller's own result slot).
pub(super) fn emit_ref_arg_writebacks_guarding_result(
    ctx: &mut FunctionContext<'_>,
    call_args: &CallArgMaterialization,
    result: Option<ValueId>,
    by_ref_return: bool,
) -> Result<()> {
    let cell_bytes = call_args.ref_writebacks.len() * CALL_ARG_TEMP_CLEANUP_BYTES
        + call_args.ref_temp_cells.len() * CALL_ARG_TEMP_CLEANUP_BYTES;
    let guard = ref_writebacks_need_result_guard(&call_args.ref_writebacks);
    if guard {
        if let Some(result) = result {
            emit_publish_guarded_call_result(ctx, result, by_ref_return, cell_bytes)?;
        }
    }
    for cell in call_args.ref_temp_cells.iter().rev() {
        abi::emit_unlink_call_operand_owner_at_stack(ctx.emitter, cell.cell_offset + 16);
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            cell.cell_offset,
        );
        abi::emit_call_label(ctx.emitter, "__rt_reference_cell_release");
    }
    // Writeback owner records were published before the temporary cells' ones, so they are
    // retired after them, innermost first.
    for writeback in call_args.ref_writebacks.iter().rev() {
        abi::emit_unlink_call_operand_owner_at_stack(ctx.emitter, writeback.cell_offset + 16);
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            writeback.cell_offset,
        );
        // The cell stays on the stack until its lease is released below (a heap writeback
        // releases it itself, before the old value, and leaves 0 there).
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        let cell = abi::int_result_reg(ctx.emitter);
        abi::emit_load_from_address(ctx.emitter, cell, cell, 0);
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        match writeback.source_ty.codegen_repr() {
            PhpType::Float => {
                emit_ref_writeback_tag_guard(ctx, 2);
                emit_low_payload_to_float_result(ctx);
                store_current_scalar_result_to_ref_source(ctx, writeback)?;
            }
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
                emit_heap_ref_writeback(ctx, writeback)?;
            }
            _ => {
                move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
                store_current_scalar_result_to_ref_source(ctx, writeback)?;
            }
        }
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        let released = ctx.next_label("ref_writeback_cell_released");
        abi::emit_branch_if_int_result_zero(ctx.emitter, &released);
        abi::emit_call_label(ctx.emitter, "__rt_reference_cell_release");
        ctx.emitter.label(&released);
    }
    let mut block_bytes = cell_bytes;
    if guard {
        // Every writeback is done: the result is the caller's again.
        abi::emit_unlink_call_operand_owner_at_stack(ctx.emitter, cell_bytes + 16);
        block_bytes += CALL_ARG_TEMP_CLEANUP_BYTES;
    }
    abi::emit_release_temporary_stack(ctx.emitter, block_bytes);
    for cell in call_args.preleased_ref_cells.iter().rev() {
        abi::emit_pop_call_operand_owner(ctx.emitter);
        super::local_stores::release_local_ref_cell_owner(
            ctx,
            cell.owner_slot,
            &cell.cell_ty,
        )?;
    }
    Ok(())
}

/// Fatals unless the unboxed writeback cell (tag in the first result register) carries
/// `expected_tag`: a typed local cannot hold a value of another kind, so a callee that replaced
/// the by-reference argument with one is stopped instead of corrupting the slot.
fn emit_ref_writeback_tag_guard(ctx: &mut FunctionContext<'_>, expected_tag: u8) {
    let ok = ctx.next_label("ref_writeback_kind_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{}", expected_tag));     // did the callee leave a value of the local's runtime kind?
            ctx.emitter.instruction(&format!("b.eq {}", ok));                   // same kind: the typed writeback is exact
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {}", expected_tag));     // did the callee leave a value of the local's runtime kind?
            ctx.emitter.instruction(&format!("je {}", ok));                     // same kind: the typed writeback is exact
        }
    }
    super::objects::emit_fatal_message(
        ctx,
        b"Fatal error: a by-reference argument was replaced by a value its typed variable cannot hold\n",
    );
    ctx.emitter.label(&ok);
}

/// Moves the unboxed low payload word (raw float bits) into the float result register.
fn emit_low_payload_to_float_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov d0, x1");                             // reinterpret the unboxed float bits as the float result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("movq xmm0, rdi");                          // reinterpret the unboxed float bits as the float result
        }
    }
}

/// Produces an OWNED `array<element>` for the writeback from the unboxed array payload in the
/// result register. An empty array, or one whose slots already have the element layout (its
/// header `value_type` equals the element's tag), is retained as it is. An array whose slots are
/// boxed Mixed cells — what a callee builds from Mixed values — is converted by
/// `__rt_array_unbox_slots`, which fails when an element has another kind. An `array<mixed>`
/// local instead widens raw typed slots into boxed cells with `__rt_array_to_mixed`. Every
/// other layout fatals: the typed slot cannot describe it.
fn emit_typed_array_ref_writeback_payload(ctx: &mut FunctionContext<'_>, element: &PhpType) {
    let keep = ctx.next_label("ref_writeback_keep_array");
    let done = ctx.next_label("ref_writeback_array_done");
    let converted = ctx.next_label("ref_writeback_array_converted");
    let boxed_tag = crate::codegen_support::runtime_value_tag(&PhpType::Mixed) as i64;
    let element_tag = crate::codegen_support::container_value_tag(element);
    let convertible = *element != PhpType::Mixed;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the written-back array's element count
            ctx.emitter.instruction(&format!("cbz x9, {}", keep));              // an empty array fits every element layout
            ctx.emitter.instruction("ldr x9, [x0, #-8]");                       // load the written-back array's packed kind word
            ctx.emitter.instruction("lsr x9, x9, #8");                          // shift the runtime value_type tag into the low bits
            ctx.emitter.instruction("and x9, x9, #0x7f");                       // isolate the value_type, dropping the COW bit
            ctx.emitter.instruction(&format!("cmp x9, #{}", element_tag));      // do the slots already have the element layout?
            ctx.emitter.instruction(&format!("b.eq {}", keep));                 // yes: the typed contract is exact
            if convertible {
                ctx.emitter.instruction(&format!("cmp x9, #{}", boxed_tag));    // are the slots boxed Mixed cells?
                ctx.emitter.instruction(&format!("b.ne {}_fail", converted));   // any other layout cannot be converted
                ctx.emitter.instruction(&format!("mov x1, #{}", element_tag));  // pass the element tag the typed local expects
                ctx.emitter.instruction("bl __rt_array_unbox_slots");           // build an owned raw-slot copy of the array
                ctx.emitter.instruction(&format!("cbnz x0, {}", converted));    // a non-null copy means every element matched
            } else {
                ctx.emitter.instruction(&format!("b {}_widen", converted));     // raw typed slots widen into boxed Mixed cells
            }
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                // load the written-back array's element count
            ctx.emitter.instruction("test r10, r10");                           // is the written-back array empty?
            ctx.emitter.instruction(&format!("je {}", keep));                   // an empty array fits every element layout
            ctx.emitter.instruction("mov r10, QWORD PTR [rax - 8]");            // load the written-back array's packed kind word
            ctx.emitter.instruction("shr r10, 8");                              // shift the runtime value_type tag into the low bits
            ctx.emitter.instruction("and r10, 0x7f");                           // isolate the value_type, dropping the COW bit
            ctx.emitter.instruction(&format!("cmp r10, {}", element_tag));      // do the slots already have the element layout?
            ctx.emitter.instruction(&format!("je {}", keep));                   // yes: the typed contract is exact
            if convertible {
                ctx.emitter.instruction(&format!("cmp r10, {}", boxed_tag));    // are the slots boxed Mixed cells?
                ctx.emitter.instruction(&format!("jne {}_fail", converted));    // any other layout cannot be converted
                abi::emit_load_int_immediate(ctx.emitter, "rdi", element_tag);
                ctx.emitter.instruction("call __rt_array_unbox_slots");         // build an owned raw-slot copy of the array
                ctx.emitter.instruction("test rax, rax");                       // a non-null copy means every element matched
                ctx.emitter.instruction(&format!("jne {}", converted));         // keep the converted owned copy
            } else {
                abi::emit_jump(ctx.emitter, &format!("{}_widen", converted));
            }
        }
    }
    if !convertible {
        // An `array<mixed>` local holds boxed slots: a callee that left raw typed slots (an
        // array of objects, ints, strings) is widened into an owned boxed copy. The helper
        // consumes one owner, so the borrowed cell payload is retained first.
        ctx.emitter.label(&format!("{}_widen", converted));
        abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x1, [x0, #-8]");                   // reload the packed kind word after the retain
                ctx.emitter.instruction("lsr x1, x1, #8");                      // shift the runtime value_type tag into the low bits
                ctx.emitter.instruction("and x1, x1, #0x7f");                   // pass the current slot tag to the widening helper
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the retained array to the widening helper
                ctx.emitter.instruction("mov rsi, QWORD PTR [rax - 8]");        // reload the packed kind word after the retain
                ctx.emitter.instruction("shr rsi, 8");                          // shift the runtime value_type tag into the low bits
                ctx.emitter.instruction("and rsi, 0x7f");                       // pass the current slot tag to the widening helper
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
        abi::emit_jump(ctx.emitter, &converted);
    }
    ctx.emitter.label(&format!("{}_fail", converted));
    super::objects::emit_fatal_message(
        ctx,
        b"Fatal error: a by-reference argument was replaced by an array whose elements its typed variable cannot hold\n",
    );
    ctx.emitter.label(&keep);
    abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Array(Box::new(element.clone())));
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&converted);
    ctx.emitter.label(&done);
}

/// Writes an unboxed heap payload (indexed array, hash, or object) back into its typed local.
///
/// The cell keeps its own reference until the caller decrefs it, so the local takes a fresh
/// retain of the new payload and releases the value it held before the call (retain first, so
/// a callee that left the very same container never drops it to zero). An indexed array whose
/// slots are boxed Mixed cells cannot back a typed element contract and fatals, the policy the
/// Mixed-to-typed runtime conversion already applies.
fn emit_heap_ref_writeback(ctx: &mut FunctionContext<'_>, writeback: &RefArgWriteback) -> Result<()> {
    let source_ty = writeback.source_ty.codegen_repr();
    emit_ref_writeback_tag_guard(ctx, crate::codegen_support::runtime_value_tag(&source_ty));
    move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
    match &source_ty {
        PhpType::Array(element) => {
            let element = element.codegen_repr();
            emit_typed_array_ref_writeback_payload(ctx, &element);
            if array_elements_need_writeback_guard(&element) {
                emit_array_elements_writeback_guard(ctx, &element);
            }
        }
        PhpType::Object(class_name) => {
            emit_object_writeback_class_guard(ctx, class_name);
            abi::emit_incref_if_refcounted(ctx.emitter, &source_ty);
        }
        _ => abi::emit_incref_if_refcounted(ctx.emitter, &source_ty),
    }
    // The result register holds an owned reference to the new value, and the caller pushed
    // the cell. Release the cell lease first (nothing of the old value runs yet), publish the
    // new value in the local, and only then release the old one: its destructor may throw,
    // and the local must not point to freed storage, nor the cell leak, when it does.
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_release");
    let zero = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, zero, 0);
    abi::emit_store_to_sp(ctx.emitter, zero, 16);
    ctx.load_local_to_result(writeback.source_slot)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    store_current_scalar_result_to_ref_source(ctx, writeback)?;
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_decref_if_refcounted(ctx.emitter, &source_ty);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// The fatal raised when a callee replaced a typed object argument (or an element of a typed
/// object array) with an instance of a class the local's layout cannot describe.
const FOREIGN_OBJECT_WRITEBACK_MESSAGE: &[u8] =
    b"Fatal error: a by-reference argument was replaced by an object of a class its typed variable cannot hold\n";

/// Resolves the class or interface id and kind `__rt_exception_matches` checks a writeback
/// object against, or `None` when the declared class has no static metadata.
///
/// The check runs in eval contexts too: the typed slot reads the static class's layout, so a
/// foreign instance must be stopped wherever `eval()` appears in the function. An instance of
/// a class that `eval()` itself declared is unknown to the static matcher and stops the
/// program as well, which is conservative but never reads a wrong layout.
fn writeback_class_target(ctx: &FunctionContext<'_>, class_name: &str) -> Option<(u64, i64)> {
    let normalized = class_name.trim_start_matches('\\');
    if let Some(class_info) = ctx.module.class_infos.get(normalized) {
        return Some((class_info.class_id, 0));
    }
    ctx.module
        .interface_infos
        .get(normalized)
        .map(|interface_info| (interface_info.interface_id, 1))
}

/// Fatals unless the borrowed object in the result register is an instance of `class_name`.
///
/// The writeback tag guard only proves the callee left SOME object; a typed `Foo` slot reads
/// its properties at `Foo`'s offsets, so an instance of an unrelated class must not land there.
/// The result register still holds the object afterwards.
fn emit_object_writeback_class_guard(ctx: &mut FunctionContext<'_>, class_name: &str) {
    let Some((target_id, target_kind)) = writeback_class_target(ctx, class_name) else {
        return;
    };
    let ok = ctx.next_label("ref_writeback_class_ok");
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {}
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the written-back object to the class matcher
        }
    }
    emit_writeback_class_match_call(ctx, target_id, target_kind);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", ok));               // the object is an instance of the local's class
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test eax, eax");                           // did the matcher accept the object's class?
            ctx.emitter.instruction(&format!("jne {}", ok));                    // the object is an instance of the local's class
        }
    }
    super::objects::emit_fatal_message(ctx, FOREIGN_OBJECT_WRITEBACK_MESSAGE);
    ctx.emitter.label(&ok);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
}

/// The fatal raised when a callee left an array whose nested layout the typed local cannot read.
const FOREIGN_ARRAY_WRITEBACK_MESSAGE: &[u8] =
    b"Fatal error: a by-reference argument was replaced by an array whose elements its typed variable cannot hold\n";

/// Whether the elements of an `array<element>` written back by reference need a per-element
/// check beyond the array's own slot tag: an object element's class, or a nested array's slot
/// layout (recursively).
fn array_elements_need_writeback_guard(element: &PhpType) -> bool {
    matches!(element, PhpType::Object(_) | PhpType::Array(_))
}

/// Fatals unless every element of the owned `array<element>` in the result register fits the
/// typed element contract; the result register holds the (possibly separated) array afterwards.
///
/// The slot-layout check of the outer array proves each element has the element's runtime
/// kind, not what it contains: an object element must be an instance of the element class
/// (the local reads its properties at that class's offsets), and a nested array's own slots
/// must have the nested element layout, checked recursively. Empty nested arrays fit every
/// layout. A nested array whose slots are boxed Mixed cells (the callee built it from `mixed`
/// reads) is converted into the nested layout like the outer array, and a nested array with
/// raw typed slots bound for an `array<mixed>` level is widened into boxed cells: the outer
/// array is first separated from other owners (copy-on-write), then the converted copy
/// replaces the nested array in its slot, and the same happens one level down.
fn emit_array_elements_writeback_guard(ctx: &mut FunctionContext<'_>, element: &PhpType) {
    let object_target = match element {
        PhpType::Object(class_name) => match writeback_class_target(ctx, class_name) {
            Some(target) => Some(target),
            None => return,
        },
        PhpType::Array(_) => None,
        _ => return,
    };
    let lp = ctx.next_label("ref_writeback_elements_loop");
    let next = ctx.next_label("ref_writeback_elements_next");
    let fail = ctx.next_label("ref_writeback_elements_fail");
    let done = ctx.next_label("ref_writeback_elements_done");
    // A nested array may be replaced in its slot (converted here, or separated one level
    // down), so the outer array must not be shared with another owner first.
    let nested_convertible = matches!(element, PhpType::Array(_));
    if nested_convertible {
        if ctx.emitter.target.arch == Arch::X86_64 {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the owned outer array to the COW splitter
        }
        abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    }
    // -- temporary frame: [0] owned array, [8] element index --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve the array and index spill slots
            ctx.emitter.instruction("str x0, [sp]");                            // remember the array across the element checks
            ctx.emitter.instruction("str xzr, [sp, #8]");                       // start at element 0
            ctx.emitter.label(&lp);
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // reload the element index
            ctx.emitter.instruction("ldr x10, [sp]");                           // reload the array
            ctx.emitter.instruction("ldr x11, [x10]");                          // load the element count
            ctx.emitter.instruction("cmp x9, x11");                             // has every element been checked?
            ctx.emitter.instruction(&format!("b.ge {}", done));                 // yes: the array fits the typed contract
            ctx.emitter.instruction("add x10, x10, #24");                       // skip the 24-byte array header to the slots
            ctx.emitter.instruction("ldr x0, [x10, x9, lsl #3]");               // load this element's payload pointer
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve the array and index spill slots
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // remember the array across the element checks
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 0");              // start at element 0
            ctx.emitter.label(&lp);
            ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 8]");            // reload the element index
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // reload the array
            ctx.emitter.instruction("cmp rcx, QWORD PTR [r10]");                // has every element been checked?
            ctx.emitter.instruction(&format!("jge {}", done));                  // yes: the array fits the typed contract
            ctx.emitter.instruction("mov rax, QWORD PTR [r10 + 24 + rcx * 8]"); // load this element's payload pointer
        }
    }
    if let Some((target_id, target_kind)) = object_target {
        if ctx.emitter.target.arch == Arch::X86_64 {
            ctx.emitter.instruction("mov rdi, rax");                            // pass this element's object to the class matcher
        }
        emit_writeback_class_match_call(ctx, target_id, target_kind);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cbnz x0, {}", next));         // this element is an instance of the element class
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("test eax, eax");                       // did the matcher accept this element's class?
                ctx.emitter.instruction(&format!("jne {}", next));              // this element is an instance of the element class
            }
        }
        super::objects::emit_fatal_message(ctx, FOREIGN_OBJECT_WRITEBACK_MESSAGE);
    } else if let PhpType::Array(inner) = element {
        let inner = inner.codegen_repr();
        let inner_tag = crate::codegen_support::container_value_tag(&inner);
        let boxed_tag = crate::codegen_support::runtime_value_tag(&PhpType::Mixed) as i64;
        let fits = ctx.next_label("ref_writeback_nested_fits");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x9, [x0]");                        // load the nested array's element count
                ctx.emitter.instruction(&format!("cbz x9, {}", next));          // an empty nested array fits every layout
                ctx.emitter.instruction("ldr x9, [x0, #-8]");                   // load the nested array's packed kind word
                ctx.emitter.instruction("lsr x9, x9, #8");                      // shift the runtime value_type tag into the low bits
                ctx.emitter.instruction("and x9, x9, #0x7f");                   // isolate the value_type, dropping the COW bit
                ctx.emitter.instruction(&format!("cmp x9, #{}", inner_tag));    // do the nested slots have the nested element layout?
                ctx.emitter.instruction(&format!("b.eq {}", fits));             // yes: check what they hold
                if inner == PhpType::Mixed {
                    ctx.emitter.instruction("mov x1, x9");                      // pass the nested slots' current tag to the widening helper
                    ctx.emitter.instruction("bl __rt_array_to_mixed");          // widen the slot's nested array into boxed cells (consumes it)
                    ctx.emitter.instruction("ldr x10, [sp]");                   // reload the (unique) outer array
                    ctx.emitter.instruction("ldr x9, [sp, #8]");                // reload the element index
                    ctx.emitter.instruction("add x10, x10, #24");               // skip the 24-byte array header to the slots
                    ctx.emitter.instruction("str x0, [x10, x9, lsl #3]");       // the slot now owns the widened copy
                } else if nested_convertible {
                    ctx.emitter.instruction(&format!("cmp x9, #{}", boxed_tag)); // are the nested slots boxed Mixed cells?
                    ctx.emitter.instruction(&format!("b.ne {}", fail));         // any other layout cannot be converted
                    ctx.emitter.instruction(&format!("mov x1, #{}", inner_tag)); // pass the nested element tag the local expects
                    ctx.emitter.instruction("bl __rt_array_unbox_slots");       // build an owned raw-slot copy of the nested array
                    ctx.emitter.instruction(&format!("cbz x0, {}", fail));      // a nested element of another kind cannot be held
                    ctx.emitter.instruction("str x0, [sp, #-16]!");             // keep the converted copy while the boxed one is released
                    ctx.emitter.instruction("ldr x10, [sp, #16]");              // reload the (unique) outer array
                    ctx.emitter.instruction("ldr x9, [sp, #24]");               // reload the element index
                    ctx.emitter.instruction("add x10, x10, #24");               // skip the 24-byte array header to the slots
                    ctx.emitter.instruction("ldr x0, [x10, x9, lsl #3]");       // load the boxed nested array the slot still owns
                    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
                    ctx.emitter.instruction("ldr x0, [sp], #16");               // recover the converted copy
                    ctx.emitter.instruction("ldr x10, [sp]");                   // reload the outer array
                    ctx.emitter.instruction("ldr x9, [sp, #8]");                // reload the element index
                    ctx.emitter.instruction("add x10, x10, #24");               // skip the 24-byte array header to the slots
                    ctx.emitter.instruction("str x0, [x10, x9, lsl #3]");       // the slot now owns the converted copy
                } else {
                    ctx.emitter.instruction(&format!("b {}", fail));            // no: the typed local cannot read them
                }
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("cmp QWORD PTR [rax], 0");              // is the nested array empty?
                ctx.emitter.instruction(&format!("je {}", next));               // an empty nested array fits every layout
                ctx.emitter.instruction("mov r11, QWORD PTR [rax - 8]");        // load the nested array's packed kind word
                ctx.emitter.instruction("shr r11, 8");                          // shift the runtime value_type tag into the low bits
                ctx.emitter.instruction("and r11, 0x7f");                       // isolate the value_type, dropping the COW bit
                ctx.emitter.instruction(&format!("cmp r11, {}", inner_tag));    // do the nested slots have the nested element layout?
                ctx.emitter.instruction(&format!("je {}", fits));               // yes: check what they hold
                if inner == PhpType::Mixed {
                    ctx.emitter.instruction("mov rdi, rax");                    // pass the slot's nested array to the widening helper
                    ctx.emitter.instruction("mov rsi, r11");                    // pass the nested slots' current tag
                    ctx.emitter.instruction("call __rt_array_to_mixed");        // widen the slot's nested array into boxed cells (consumes it)
                    ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");        // reload the (unique) outer array
                    ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 8]");    // reload the element index
                    ctx.emitter.instruction("mov QWORD PTR [r10 + 24 + rcx * 8], rax"); // the slot now owns the widened copy
                } else if nested_convertible {
                    ctx.emitter.instruction(&format!("cmp r11, {}", boxed_tag)); // are the nested slots boxed Mixed cells?
                    ctx.emitter.instruction(&format!("jne {}", fail));          // any other layout cannot be converted
                    abi::emit_load_int_immediate(ctx.emitter, "rdi", inner_tag);
                    ctx.emitter.instruction("call __rt_array_unbox_slots");     // build an owned raw-slot copy of the nested array
                    ctx.emitter.instruction("test rax, rax");                   // did every nested element match?
                    ctx.emitter.instruction(&format!("je {}", fail));           // a nested element of another kind cannot be held
                    ctx.emitter.instruction("sub rsp, 16");                     // keep the converted copy while the boxed one is released
                    ctx.emitter.instruction("mov QWORD PTR [rsp], rax");        // remember the converted copy
                    ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 16]");   // reload the (unique) outer array
                    ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 24]");   // reload the element index
                    ctx.emitter.instruction("mov rax, QWORD PTR [r10 + 24 + rcx * 8]"); // load the boxed nested array the slot still owns
                    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
                    ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");        // recover the converted copy
                    ctx.emitter.instruction("add rsp, 16");                     // release the conversion spill slot
                    ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");        // reload the outer array
                    ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 8]");    // reload the element index
                    ctx.emitter.instruction("mov QWORD PTR [r10 + 24 + rcx * 8], rax"); // the slot now owns the converted copy
                } else {
                    ctx.emitter.instruction(&format!("jmp {}", fail));          // no: the typed local cannot read them
                }
            }
        }
        ctx.emitter.label(&fits);
        if array_elements_need_writeback_guard(&inner) {
            emit_array_elements_writeback_guard(ctx, &inner);
            // The nested check may have separated or converted the nested array: the slot
            // owns whatever it hands back.
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("ldr x10, [sp]");                   // reload the outer array
                    ctx.emitter.instruction("ldr x9, [sp, #8]");                // reload the element index
                    ctx.emitter.instruction("add x10, x10, #24");               // skip the 24-byte array header to the slots
                    ctx.emitter.instruction("str x0, [x10, x9, lsl #3]");       // store the nested array the check returned
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");        // reload the outer array
                    ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 8]");    // reload the element index
                    ctx.emitter.instruction("mov QWORD PTR [r10 + 24 + rcx * 8], rax"); // store the nested array the check returned
                }
            }
        }
        abi::emit_jump(ctx.emitter, &next);
        ctx.emitter.label(&fail);
        super::objects::emit_fatal_message(ctx, FOREIGN_ARRAY_WRITEBACK_MESSAGE);
    }
    ctx.emitter.label(&next);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // reload the element index
            ctx.emitter.instruction("add x9, x9, #1");                          // advance to the next element
            ctx.emitter.instruction("str x9, [sp, #8]");                        // remember the advanced index
            ctx.emitter.instruction(&format!("b {}", lp));                      // check the next element
            ctx.emitter.label(&done);
            ctx.emitter.instruction("ldr x0, [sp]");                            // restore the array as the result
            ctx.emitter.instruction("add sp, sp, #16");                         // release the spill slots
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add QWORD PTR [rsp + 8], 1");              // advance to the next element
            ctx.emitter.instruction(&format!("jmp {}", lp));                    // check the next element
            ctx.emitter.label(&done);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                // restore the array as the result
            ctx.emitter.instruction("add rsp, 16");                             // release the spill slots
        }
    }
}

/// Calls `__rt_exception_matches` for the object already in the first argument register.
fn emit_writeback_class_match_call(ctx: &mut FunctionContext<'_>, target_id: u64, target_kind: i64) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), target_id as i64);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), target_kind);
    abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
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

/// Returns true when a value already holds a direct pointer to array element reference storage.
pub(super) fn value_is_array_element_address(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    Ok(array_element_address_source(ctx, value)?.is_some())
}

/// Resolves an element-slot pointer to the array whose element type describes its pointee storage.
fn array_element_address_source(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<ValueId>> {
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
    if matches!(inst_ref.op, Op::AcquireRefCell | Op::Borrow | Op::Move) {
        return array_element_address_source(ctx, expect_operand(inst_ref, 0)?);
    }
    if !matches!(inst_ref.op, Op::ArrayElemAddr | Op::LoadArrayElemRefCell) {
        return Ok(None);
    }
    Ok(Some(expect_operand(inst_ref, 0)?))
}

/// Returns whether an element-address operand names a managed hash reference cell.
///
/// `ArrayElemAddr` and a concrete indexed-array ref-cell load are borrowed interior addresses.
/// Associative, boxed, and runtime-shaped `Array(Mixed)` sources normalize to hash storage, so
/// their `LoadArrayElemRefCell` result needs a caller lease while the callee can replace its entry
/// owner.
fn value_is_managed_array_element_ref_cell(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<bool> {
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
    if inst_ref.op != Op::LoadArrayElemRefCell {
        return Ok(false);
    }
    let source = expect_operand(inst_ref, 0)?;
    Ok(match ctx.value_php_type(source)?.codegen_repr() {
        PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_) => true,
        _ => false,
    })
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
