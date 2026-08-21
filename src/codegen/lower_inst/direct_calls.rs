//! Purpose:
//! Lowers direct calls and materializes ordinary ABI arguments.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers a direct call to a module-local user function.
pub(super) fn lower_direct_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let function_name = ctx.function_name_data(expect_data(inst)?)?.to_string();
    let callee = ctx
        .callable_function_by_name(&function_name)
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!("call to unknown function {}", function_name))
        })?;
    // A by-reference-returning callee hands back a single-word reference-cell pointer in the
    // integer result register (see `Terminator::Return`); capture the flag before the mutable
    // call-materialization borrows so the result is stored single-word, not split by type.
    let callee_by_ref_return = callee.flags.by_ref_return;
    if inst.operands.len() != callee.params.len() {
        return Err(CodegenIrError::unsupported(format!(
            "call to {} with {} args for {} params",
            function_name,
            inst.operands.len(),
            callee.params.len()
        )));
    }
    let param_types = callee
        .params
        .iter()
        .map(|param| param.php_type.codegen_repr())
        .collect::<Vec<_>>();
    let ref_params = callee
        .params
        .iter()
        .map(|param| param.by_ref)
        .collect::<Vec<_>>();
    let borrowed_stack_mixed_args =
        plan_borrowed_stack_mixed_args(ctx, callee, &inst.operands, &param_types, &ref_params)?;
    let call_args = materialize_direct_call_args_with_refs_and_borrowed_options(
        ctx,
        &inst.operands,
        &param_types,
        &ref_params,
        true,
        &borrowed_stack_mixed_args,
        RefArgCellLifetime::CallOnly,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &function_symbol(&function_name));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    if let Some(result) = inst.result {
        if ctx.value_php_type(result)? == PhpType::Void {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
        }
        if callee_by_ref_return {
            ctx.store_int_result_value(result)?;
        } else {
            ctx.store_result_value(result)?;
        }
    }
    emit_call_arg_temp_cleanups(ctx, &call_args, inst.result)?;
    emit_borrowed_stack_mixed_arg_release(ctx, &call_args);
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Loads SSA operands into ABI argument registers and caller-stack slots for a direct call.
pub(in crate::codegen) fn materialize_direct_call_args(
    ctx: &mut FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
) -> Result<usize> {
    let ref_params = vec![false; param_types.len()];
    let materialized = materialize_direct_call_args_with_refs(ctx, args, param_types, &ref_params)?;
    Ok(materialized.overflow_bytes)
}

/// Loads SSA operands into ABI argument slots, preserving by-reference locals.
pub(super) fn materialize_direct_call_args_with_refs(
    ctx: &mut FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
) -> Result<CallArgMaterialization> {
    materialize_direct_call_args_with_refs_and_options(
        ctx,
        args,
        param_types,
        ref_params,
        false,
        RefArgCellLifetime::CallOnly,
    )
}

/// Loads SSA operands into ABI argument slots with optional caller-temp cleanup tracking.
pub(super) fn materialize_direct_call_args_with_refs_and_options(
    ctx: &mut FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    track_mixed_temp_cleanups: bool,
    ref_cell_lifetime: RefArgCellLifetime,
) -> Result<CallArgMaterialization> {
    materialize_direct_call_args_with_refs_and_borrowed_options(
        ctx,
        args,
        param_types,
        ref_params,
        track_mixed_temp_cleanups,
        &[],
        ref_cell_lifetime,
    )
}

/// Loads SSA operands into ABI argument slots with optional borrowed Mixed stack cells.
pub(super) fn materialize_direct_call_args_with_refs_and_borrowed_options(
    ctx: &mut FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    track_mixed_temp_cleanups: bool,
    borrowed_stack_mixed_args: &[BorrowedStackMixedArg],
    ref_cell_lifetime: RefArgCellLifetime,
) -> Result<CallArgMaterialization> {
    if args.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "direct call materialization received {} args for {} params",
            args.len(),
            param_types.len()
        )));
    }
    if ref_params.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "direct call materialization received {} ref flags for {} params",
            ref_params.len(),
            param_types.len()
        )));
    }
    let mut ref_writebacks = plan_ref_arg_writebacks(ctx, args, param_types, ref_params)?;
    let mut ref_temp_cells = plan_ref_arg_temp_cells(
        ctx,
        args,
        param_types,
        ref_params,
        &ref_writebacks,
        ref_cell_lifetime,
    )?;
    emit_ref_arg_cell_block(ctx, &mut ref_writebacks, &mut ref_temp_cells)?;
    let abi_param_types = abi_param_types_for_refs(param_types, ref_params);
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &abi_param_types, 0);
    let borrowed_stack_arg_bytes = borrowed_stack_mixed_args.len() * BORROWED_MIXED_ARG_CELL_BYTES;
    if borrowed_stack_arg_bytes > 0 {
        abi::emit_reserve_temporary_stack(ctx.emitter, borrowed_stack_arg_bytes);
    }
    let cleanup_slots = if track_mixed_temp_cleanups {
        plan_call_arg_temp_cleanups(
            ctx,
            args,
            param_types,
            ref_params,
            borrowed_stack_mixed_args,
        )?
    } else {
        Vec::new()
    };
    let cleanup_bytes = cleanup_slots.len() * 16;
    if cleanup_bytes > 0 {
        abi::emit_reserve_temporary_stack(ctx.emitter, cleanup_bytes);
    }
    let ref_cell_base_offset = borrowed_stack_arg_bytes + cleanup_bytes;
    let borrowed_cell_base_offset = cleanup_bytes;
    let mut arg_temp_bytes = 0usize;
    for (index, (value, param_ty)) in args.iter().zip(param_types.iter()).enumerate() {
        if ref_params[index] {
            materialize_ref_arg_address(
                ctx,
                *value,
                index,
                param_ty,
                arg_temp_bytes,
                &ref_writebacks,
                &ref_temp_cells,
                ref_cell_base_offset,
            )?;
            abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
        } else if let Some(borrowed) = borrowed_stack_mixed_args
            .iter()
            .find(|borrowed| borrowed.param_index == index)
        {
            ctx.load_value_to_result(*value)?;
            emit_borrowed_stack_mixed_arg_cell(
                ctx,
                borrowed,
                borrowed_cell_base_offset + arg_temp_bytes,
            );
            abi::emit_push_result_value(ctx.emitter, &PhpType::Mixed);
        } else {
            let cleanup = cleanup_slots
                .iter()
                .find(|cleanup| cleanup.param_index == index);
            let push_ty =
                materialize_plain_call_arg(ctx, *value, param_ty, cleanup, arg_temp_bytes)?;
            abi::emit_push_result_value(ctx.emitter, &push_ty);
        }
        arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[index]);
    }
    Ok(CallArgMaterialization {
        overflow_bytes: abi::materialize_outgoing_args(ctx.emitter, &assignments),
        ref_writebacks,
        ref_temp_cells,
        cleanup_slots,
        cleanup_bytes,
        borrowed_stack_arg_bytes,
    })
}

/// Loads hidden and visible static-method arguments, preserving by-reference locals.
pub(super) fn materialize_static_method_call_args_with_refs(
    ctx: &mut FunctionContext<'_>,
    called_class_id: &CalledClassIdArg,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
) -> Result<CallArgMaterialization> {
    if args.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "static method call materialization received {} args for {} visible params",
            args.len(),
            param_types.len()
        )));
    }
    if ref_params.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "static method call materialization received {} ref flags for {} visible params",
            ref_params.len(),
            param_types.len()
        )));
    }
    let mut ref_writebacks = plan_ref_arg_writebacks(ctx, args, param_types, ref_params)?;
    // `CallOnly`: PHP refuses `Foo::__construct()` as a static call, so nothing reachable
    // here can promote a by-reference parameter into a property.
    let mut ref_temp_cells = plan_ref_arg_temp_cells(
        ctx,
        args,
        param_types,
        ref_params,
        &ref_writebacks,
        RefArgCellLifetime::CallOnly,
    )?;
    emit_ref_arg_cell_block(ctx, &mut ref_writebacks, &mut ref_temp_cells)?;
    let cleanup_slots = plan_call_arg_temp_cleanups(ctx, args, param_types, ref_params, &[])?;
    let cleanup_bytes = cleanup_slots.len() * 16;
    if cleanup_bytes > 0 {
        abi::emit_reserve_temporary_stack(ctx.emitter, cleanup_bytes);
    }
    let visible_abi_param_types = abi_param_types_for_refs(param_types, ref_params);
    let mut abi_param_types = Vec::with_capacity(visible_abi_param_types.len() + 1);
    abi_param_types.push(PhpType::Int);
    abi_param_types.extend_from_slice(&visible_abi_param_types);
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &abi_param_types, 0);
    materialize_called_class_id(ctx, called_class_id)?;
    abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
    let mut arg_temp_bytes = call_arg_temp_slot_size(&PhpType::Int);
    for (index, (value, param_ty)) in args.iter().zip(param_types.iter()).enumerate() {
        if ref_params[index] {
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
            let cleanup = cleanup_slots
                .iter()
                .find(|cleanup| cleanup.param_index == index);
            let push_ty =
                materialize_plain_call_arg(ctx, *value, param_ty, cleanup, arg_temp_bytes)?;
            abi::emit_push_result_value(ctx.emitter, &push_ty);
        }
        arg_temp_bytes += call_arg_temp_slot_size(&visible_abi_param_types[index]);
    }
    Ok(CallArgMaterialization {
        overflow_bytes: abi::materialize_outgoing_args(ctx.emitter, &assignments),
        ref_writebacks,
        ref_temp_cells,
        cleanup_slots,
        cleanup_bytes,
        borrowed_stack_arg_bytes: 0,
    })
}

/// Materializes the hidden called-class id into the integer result register.
pub(super) fn materialize_called_class_id(
    ctx: &mut FunctionContext<'_>,
    called_class_id: &CalledClassIdArg,
) -> Result<()> {
    match called_class_id {
        CalledClassIdArg::Immediate(class_id) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                *class_id as i64,
            );
        }
        CalledClassIdArg::Local(slot) => {
            let source_ty = ctx.load_local_to_result(*slot)?;
            if source_ty != PhpType::Int {
                return Err(CodegenIrError::invalid_module(format!(
                    "hidden called-class id local has PHP type {:?}",
                    source_ty
                )));
            }
        }
        CalledClassIdArg::ThisObject(slot) => {
            let source_ty = ctx.load_local_to_result(*slot)?;
            if !matches!(source_ty.codegen_repr(), PhpType::Object(_)) {
                return Err(CodegenIrError::invalid_module(format!(
                    "this local has PHP type {:?} for forwarded called-class id",
                    source_ty
                )));
            }
            abi::emit_load_from_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                abi::int_result_reg(ctx.emitter),
                0,
            );
        }
    }
    Ok(())
}

/// Materializes one ordinary (non-by-reference, non-borrowed-cell) call argument.
///
/// The `__rt_incref` is what stops a widening conversion from rewriting the CALLER's array.
/// `__rt_array_to_mixed` consumes an owner slot — it splits through
/// `__rt_array_ensure_unique`, which only clones when the refcount says the array is shared —
/// so a borrowed array reached it looking unique and had its element slots rewritten in place.
/// Making it visibly shared first forces the clone; the reserved cleanup slot then releases
/// that clone once the callee returns.
///
/// The planner reserves that slot for exactly the borrowed widening arguments, so the presence
/// of a cleanup is the same decision as the incref and the two cannot drift: an incref with no
/// cleanup would leak the clone, and a cleanup with no incref would release the caller's array.
fn materialize_plain_call_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    param_ty: &PhpType,
    cleanup: Option<&CallArgTempCleanup>,
    arg_temp_bytes: usize,
) -> Result<PhpType> {
    ctx.load_value_to_result(value)?;
    let source_ty = ctx.raw_value_php_type(value)?;
    if cleanup.is_some() && super::call_cleanup::argument_widens_typed_array(&source_ty, param_ty) {
        abi::emit_call_label(ctx.emitter, "__rt_incref");
    }
    let push_ty = materialize_direct_call_arg_for_param(ctx, &source_ty, param_ty)?;
    if let Some(cleanup) = cleanup {
        save_call_arg_temp_cleanup(ctx, cleanup, arg_temp_bytes);
    }
    Ok(push_ty)
}

/// Converts the loaded call operand to the ABI shape required by the callee parameter.
pub(super) fn materialize_direct_call_arg_for_param(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
    param_ty: &PhpType,
) -> Result<PhpType> {
    match param_ty.codegen_repr() {
        PhpType::TaggedScalar => coerce_loaded_value_to_tagged_scalar(ctx, source_ty),
        PhpType::Int if matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(PhpType::Int)
        }
        PhpType::Bool if matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            Ok(PhpType::Bool)
        }
        PhpType::Float
            if matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) =>
        {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            Ok(PhpType::Float)
        }
        PhpType::Str if matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            Ok(PhpType::Str)
        }
        PhpType::Object(name)
            if matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) =>
        {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("mov x0, x1");                      // pass the unboxed object payload to the typed parameter
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov rax, rdi");                    // pass the unboxed object payload to the typed parameter
                }
            }
            Ok(PhpType::Object(name))
        }
        PhpType::Mixed if source_ty.codegen_repr() != PhpType::Mixed => {
            emit_box_current_value_as_mixed(ctx.emitter, source_ty);
            Ok(PhpType::Mixed)
        }
        PhpType::Array(param_elem) if param_elem.codegen_repr() == PhpType::Mixed => {
            if let PhpType::Array(source_elem) = source_ty.codegen_repr() {
                let source_elem = source_elem.codegen_repr();
                if source_elem != PhpType::Mixed {
                    emit_loaded_indexed_array_to_mixed(ctx, &source_elem);
                }
                return Ok(PhpType::Array(Box::new(PhpType::Mixed)));
            }
            Ok(PhpType::Array(param_elem))
        }
        target_ty => Ok(target_ty),
    }
}

/// Converts the currently loaded result registers into the inline nullable-int shape.
pub(in crate::codegen) fn coerce_loaded_value_to_tagged_scalar(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
) -> Result<PhpType> {
    match source_ty.codegen_repr() {
        PhpType::TaggedScalar => Ok(PhpType::TaggedScalar),
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
            Ok(PhpType::TaggedScalar)
        }
        PhpType::Void | PhpType::Never => {
            crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
            Ok(PhpType::TaggedScalar)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_result_as_tagged_scalar(ctx);
            Ok(PhpType::TaggedScalar)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "conversion from PHP type {:?} to PHP type TaggedScalar",
            other
        ))),
    }
}

/// Reorders `__rt_mixed_unbox` output into the inline tagged-scalar result registers.
pub(super) fn emit_mixed_result_as_tagged_scalar(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, x0");                              // preserve the unboxed Mixed tag before moving the payload
            ctx.emitter.instruction("mov x0, x1");                              // place the unboxed payload into the tagged-scalar payload register
            ctx.emitter.instruction("mov x1, x9");                              // place the unboxed Mixed tag into the tagged-scalar tag register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // preserve the unboxed Mixed tag before moving the payload
            ctx.emitter.instruction("mov rax, rdi");                            // place the unboxed payload into the tagged-scalar payload register
            ctx.emitter.instruction("mov rdx, r10");                            // place the unboxed Mixed tag into the tagged-scalar tag register
        }
    }
}
