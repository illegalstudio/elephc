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
    // A `false`-declared parameter must refuse a boxed mixed argument before the ordinary
    // bool coercion folds `0`/`""`/`[]` into `false`; php throws the argument `TypeError`
    // naming what was given. The guards run in source order, before materialization.
    for (index, (param, operand)) in callee.params.iter().zip(&inst.operands).enumerate() {
        if param.php_type != PhpType::False {
            continue;
        }
        let operand_ty = ctx.value_php_type(*operand)?;
        if matches!(operand_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            let base_name = function_name.split('<').next().unwrap_or(&function_name);
            emit_false_param_mixed_guard(
                ctx,
                *operand,
                &format!(
                    "{base_name}(): Argument #{} (${}) must be of type false, ",
                    index + 1,
                    param.name
                ),
            )?;
        }
    }
    let borrowed_stack_mixed_args =
        plan_borrowed_stack_mixed_args(ctx, callee, &inst.operands, &param_types, &ref_params)?;
    let call_args = materialize_direct_call_args_with_refs_and_borrowed_options(
        ctx,
        &inst.operands,
        &param_types,
        &ref_params,
        true,
        &borrowed_stack_mixed_args,
        RefArgCellLifetime::for_callee(ctx, &function_name),
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &function_symbol(&function_name));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    if let Some(result) = inst.result {
        ctx.store_runtime_return_ownership(result);
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
    // User callees own their Mixed parameter shadows, including an aliased resource return.
    emit_call_arg_temp_cleanups(ctx, &call_args, None)?;
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
    // See `reference_arguments::omits_only_hidden_collector`: an internal dispatcher may leave
    // out the trailing hidden collector, which is staged empty below.
    let omitted_collector = args.len() != param_types.len()
        && super::reference_arguments::omits_only_hidden_collector(args.len(), param_types)
        && !ref_params.last().copied().unwrap_or(false);
    if args.len() != param_types.len() && !omitted_collector {
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
    let preleased_ref_cells = plan_preleased_ref_arg_cells(ctx, args, ref_params)?;
    let mut ref_temp_cells = plan_ref_arg_temp_cells(
        ctx,
        args,
        param_types,
        ref_params,
        &ref_writebacks,
        ref_cell_lifetime,
    )?;
    emit_ref_arg_cell_block(
        ctx,
        &mut ref_writebacks,
        &mut ref_temp_cells,
    )?;
    let abi_param_types = abi_param_types_for_refs(param_types, ref_params);
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &abi_param_types, 0);
    let borrowed_stack_arg_bytes = borrowed_stack_mixed_args.len() * BORROWED_MIXED_ARG_CELL_BYTES;
    if borrowed_stack_arg_bytes > 0 {
        abi::emit_reserve_temporary_stack(ctx.emitter, borrowed_stack_arg_bytes);
    }
    let mut cleanup_slots = if track_mixed_temp_cleanups {
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
    if omitted_collector {
        cleanup_slots.push(CallArgTempCleanup {
            param_index: args.len(),
            offset: cleanup_slots.len() * CALL_ARG_TEMP_CLEANUP_BYTES,
            ty: PhpType::Array(Box::new(PhpType::Mixed)),
        });
    }
    let cleanup_bytes = cleanup_slots.len() * CALL_ARG_TEMP_CLEANUP_BYTES;
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
    if omitted_collector {
        let index = args.len();
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 4);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 8);
        abi::emit_call_label(ctx.emitter, "__rt_array_new");
        let result_reg = abi::int_result_reg(ctx.emitter).to_string();
        crate::codegen::emit_array_value_type_stamp(ctx.emitter, &result_reg, &PhpType::Mixed);
        if let Some(cleanup) = cleanup_slots.iter().find(|cleanup| cleanup.param_index == index) {
            save_call_arg_temp_cleanup(ctx, cleanup, arg_temp_bytes);
        }
        abi::emit_push_result_value(ctx.emitter, &param_types[index]);
    }
    Ok(CallArgMaterialization {
        overflow_bytes: abi::materialize_outgoing_args(ctx.emitter, &assignments),
        ref_writebacks,
        ref_temp_cells,
        preleased_ref_cells,
        cleanup_slots,
        cleanup_bytes,
        cleanup_guard_bytes: 0,
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
    // `func_args::method_families` gives a whole interface family the hidden `func_get_args()`
    // collector when one member reads its surplus arguments, static methods included, and a
    // static interface call (`$subscriber::getSubscribedEvents()`) passes only the source
    // arguments. The collector is staged empty, which is what a call with no surplus passes.
    let omitted_collector = args.len() + 1 == param_types.len()
        && param_types.last() == Some(&PhpType::Array(Box::new(PhpType::Mixed)))
        && !ref_params.last().copied().unwrap_or(false);
    if args.len() != param_types.len() && !omitted_collector {
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
    let preleased_ref_cells = plan_preleased_ref_arg_cells(ctx, args, ref_params)?;
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
    emit_ref_arg_cell_block(
        ctx,
        &mut ref_writebacks,
        &mut ref_temp_cells,
    )?;
    let mut cleanup_slots = plan_call_arg_temp_cleanups(ctx, args, param_types, ref_params, &[])?;
    if omitted_collector {
        cleanup_slots.push(CallArgTempCleanup {
            param_index: args.len(),
            offset: cleanup_slots.len() * CALL_ARG_TEMP_CLEANUP_BYTES,
            ty: PhpType::Array(Box::new(PhpType::Mixed)),
        });
    }
    let cleanup_bytes = cleanup_slots.len() * CALL_ARG_TEMP_CLEANUP_BYTES;
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
    if omitted_collector {
        let index = args.len();
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 4);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 8);
        abi::emit_call_label(ctx.emitter, "__rt_array_new");
        let result_reg = abi::int_result_reg(ctx.emitter).to_string();
        crate::codegen::emit_array_value_type_stamp(ctx.emitter, &result_reg, &PhpType::Mixed);
        if let Some(cleanup) = cleanup_slots.iter().find(|cleanup| cleanup.param_index == index) {
            super::save_call_arg_temp_cleanup(ctx, cleanup, arg_temp_bytes);
        }
        abi::emit_push_result_value(ctx.emitter, &param_types[index]);
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
            materialize_this_object_called_class_id(ctx, *slot)?;
        }
    }
    Ok(())
}

/// Loads the runtime class id from `$this`, including boxed gradual local storage.
pub(super) fn materialize_this_object_called_class_id(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
) -> Result<()> {
    let source_ty = ctx.load_local_to_result(slot)?;
    match source_ty.codegen_repr() {
        PhpType::Object(_) => {
            let result_reg = abi::int_result_reg(ctx.emitter).to_string();
            abi::emit_load_from_address(ctx.emitter, &result_reg, &result_reg, 0);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("ldr x0, [x1]");                    // load the class id from the boxed object's payload
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov rax, QWORD PTR [rdi]");        // load the class id from the boxed object's payload
                }
            }
            Ok(())
        }
        other => Err(CodegenIrError::invalid_module(format!(
            "this local has PHP type {:?} for forwarded called-class id",
            other
        ))),
    }
}

/// Materializes one ordinary (non-by-reference, non-borrowed-cell) call argument.
///
/// The `__rt_incref` is what stops a widening conversion from rewriting the CALLER's array.
/// `__rt_array_to_mixed` consumes an owner slot — it splits through
/// `__rt_array_ensure_unique`, which only clones when the refcount says the array is shared —
/// so a borrowed array reached it looking unique and had its element slots rewritten in place,
/// and an owned operand that was in fact shared lost its reference to the clone. Making it
/// visibly shared first forces the clone; the reserved cleanup slot then releases that clone
/// once the callee returns.
///
/// The planner reserves that slot for exactly the widening arguments, so the presence
/// of a cleanup is the same decision as the incref and the two cannot drift: an incref with no
/// cleanup would leak the clone, and a cleanup with no incref would release the caller's array.
pub(super) fn materialize_plain_call_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    param_ty: &PhpType,
    cleanup: Option<&CallArgTempCleanup>,
    arg_temp_bytes: usize,
) -> Result<PhpType> {
    // A boxed value handed to a `callable` parameter is the descriptor INSIDE the box (or the
    // descriptor its string / array spelling resolves to), never the box: the callee calls
    // through the pointer it receives. A `mixed` receiver's candidate call reached
    // `getResolver(callable $callable)` with the box, and `$callable(...)` / `new
    // ReflectionFunction($callable)` read garbage from it -- Symfony's runtime died on it.
    if param_ty.codegen_repr() == PhpType::Callable
        && matches!(ctx.raw_value_php_type(value)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
    {
        emit_boxed_argument_as_callable(ctx, value)?;
        return Ok(PhpType::Callable);
    }
    ctx.load_value_to_result(value)?;
    let source_ty = ctx.raw_value_php_type(value)?;
    if cleanup.is_some() && super::call_cleanup::argument_widens_typed_array(&source_ty, param_ty) {
        abi::emit_call_label(ctx.emitter, "__rt_incref");
    }
    let push_ty = materialize_direct_call_arg_for_param(ctx, &source_ty, param_ty)?;
    if let Some(cleanup) = cleanup {
        // A detached string travels as a pointer/length pair; the cleanup record owns its pointer.
        let string_pair_on_aarch64 =
            cleanup.ty == PhpType::Str && ctx.emitter.target.arch == Arch::AArch64;
        if string_pair_on_aarch64 {
            ctx.emitter.instruction("mov x0, x1");                              // stage the detached string pointer in the owner-record register
        }
        save_call_arg_temp_cleanup(ctx, cleanup, arg_temp_bytes);
        if string_pair_on_aarch64 {
            ctx.emitter.instruction("mov x1, x0");                              // restore the string pointer half of the argument pair
        }
    }
    Ok(push_ty)
}

/// Refuses a boxed mixed/union argument bound to a `false`-declared parameter.
///
/// The ordinary argument coercion for a bool-repr parameter is the truthiness cast, which
/// would fold `0`, `""` and `[]` into `false`; php accepts ONLY the exact `false` bool for a
/// `false` parameter and throws the argument `TypeError` naming the runtime type of what was
/// given. The check runs BEFORE argument materialization, which re-loads every operand on its
/// own, and throws for every case php throws — the accepted case falls through and the
/// ordinary coercion stores `false` exactly as php stores it.
pub(super) fn emit_false_param_mixed_guard(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    message_prefix: &str,
) -> Result<()> {
    emit_literal_param_mixed_guard(ctx, value, message_prefix, false)
}

/// Rejects non-literal boxed arguments before either false or null ABI conversion.
fn emit_literal_param_mixed_guard(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    message_prefix: &str,
    null_only: bool,
) -> Result<()> {
    load_value_to_first_int_arg(ctx, value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let tag_reg = abi::int_result_reg(ctx.emitter);
    let accepted = ctx.next_label("literal_param_ok");
    let null_tag = crate::codegen_support::sentinels::TAGGED_SCALAR_TAG_NULL;
    if null_only {
        super::enums::emit_mixed_tag_branch(ctx, tag_reg, null_tag, &accepted);
    }
    let bool_arm = ctx.next_label("false_param_bool");
    let mut refuse_arms: Vec<(u8, String, String)> = Vec::new();
    for (tag, name) in super::objects::RUNTIME_TAGS
        .iter()
        .filter(|(tag, _)| *tag != super::objects::BOOL_TAG && (!null_only || i64::from(*tag) != null_tag))
    {
        refuse_arms.push((
            *tag,
            format!("{message_prefix}{name} given"),
            ctx.next_label(&format!("false_param_reject_{name}")),
        ));
    }
    for (tag, _, label) in &refuse_arms {
        super::enums::emit_mixed_tag_branch(ctx, tag_reg, i64::from(*tag), label);
    }
    super::enums::emit_mixed_tag_branch(ctx, tag_reg, i64::from(super::objects::BOOL_TAG), &bool_arm);
    // A tag outside the table never reaches the unbox helper (tag 7 is a nested box it peels),
    // so this fall-through is a defensive refusal rather than a live path.
    super::exceptions::emit_type_error(ctx, &format!("{message_prefix}mixed given"));
    for (tag, message, label) in refuse_arms {
        ctx.emitter.label(&label);
        if tag == super::objects::OBJECT_TAG {
            let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
            let result_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_reg_move(ctx.emitter, result_reg, payload_reg);
            super::runtime_class_messages::emit_runtime_class_name_to_string_result(
                ctx, result_reg, "object",
            );
            super::runtime_class_messages::emit_concat_static_prefix(ctx, message_prefix);
            super::runtime_class_messages::emit_concat_static_suffix(ctx, " given");
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            super::exceptions::emit_type_error_from_string_result(ctx);
        } else {
            super::exceptions::emit_type_error(ctx, &message);
        }
    }
    ctx.emitter.label(&bool_arm);
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, result_reg, payload_reg);
    if null_only {
        let false_arm = ctx.next_label("null_param_false");
        abi::emit_branch_if_int_result_zero(ctx.emitter, &false_arm);
        super::exceptions::emit_type_error(ctx, &format!("{message_prefix}true given"));
        ctx.emitter.label(&false_arm);
        super::exceptions::emit_type_error(ctx, &format!("{message_prefix}false given"));
    } else {
        abi::emit_branch_if_int_result_zero(ctx.emitter, &accepted);
        super::exceptions::emit_type_error(ctx, &format!("{message_prefix}true given"));
    }
    ctx.emitter.label(&accepted);
    Ok(())
}

/// Checks false and null declarations before ABI materialization erases their literal type.
pub(super) fn emit_literal_declared_parameter_guards(
    ctx: &mut FunctionContext<'_>,
    operands: &[ValueId],
    parameters: &[(String, PhpType)],
    callable_name: &str,
) -> Result<()> {
    for (index, ((name, declared), value)) in parameters.iter().zip(operands).enumerate() {
        if !matches!(declared, PhpType::False | PhpType::Void) {
            continue;
        }
        let source = ctx.value_php_type(*value)?;
        let literal = if declared == &PhpType::Void { "null" } else { "false" };
        let prefix = format!(
            "{callable_name}(): Argument #{} (${name}) must be of type {literal}, ",
            index + 1,
        );
        if matches!(source.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            emit_literal_param_mixed_guard(ctx, *value, &prefix, declared == &PhpType::Void)?;
        } else if source == PhpType::Bool && declared == &PhpType::False {
            ctx.load_value_to_result(*value)?;
            let accepted = ctx.next_label("false_parameter_accepted");
            abi::emit_branch_if_int_result_zero(ctx.emitter, &accepted);
            super::exceptions::emit_type_error(ctx, &format!("{prefix}true given"));
            ctx.emitter.label(&accepted);
        }
    }
    Ok(())
}

/// Loads the callable descriptor a boxed argument holds into the integer result register.
///
/// An existing descriptor is passed borrowed (the box keeps it alive for the call); php null stays
/// the callable null sentinel, as a `?callable` parameter expects.
fn emit_boxed_argument_as_callable(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let null_label = ctx.next_label("call_arg_callable_null");
    let done_label = ctx.next_label("call_arg_callable_done");
    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #8");                              // is the boxed argument php null?
            ctx.emitter.instruction(&format!("b.eq {}", null_label));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 8");                              // is the boxed argument php null?
            ctx.emitter.instruction(&format!("je {}", null_label));
        }
    }
    super::callables::emit_runtime_mixed_callable_descriptor_value(
        ctx,
        value,
        "callable argument",
        false,
        crate::strict_php::is_enabled(),
    )?;
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        crate::codegen_support::sentinels::NULL_SENTINEL,
    );
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Converts the loaded call operand to the ABI shape required by the callee parameter.
/// Literal false/null parameter guards must run before this conversion.
pub(super) fn materialize_direct_call_arg_for_param(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
    param_ty: &PhpType,
) -> Result<PhpType> {
    match param_ty.codegen_repr() {
        PhpType::Void if matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) => {
            // The declared null guard ran before materialization; pass the null word, not its box.
            let result = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, result, 0);
            Ok(PhpType::Void)
        }
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
        // A declared `iterable` parameter takes the array or Traversable payload, never the
        // box. Handing the box over is what a `mixed` receiver's candidate call did in an eval
        // program: `$this->fillCells($row)` from Symfony's table generator closure reached
        // `fillCells(iterable $row)` with a pointer its `foreach` read as null.
        PhpType::Iterable
            if matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) =>
        {
            super::reference_arguments::emit_mixed_iterable_unbox_guard(ctx);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("mov x0, x1");                      // pass the unboxed iterable payload to the typed parameter
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov rax, rdi");                    // pass the unboxed iterable payload to the typed parameter
                }
            }
            Ok(PhpType::Iterable)
        }
        PhpType::Mixed if source_ty.codegen_repr() != PhpType::Mixed => {
            emit_box_current_value_as_mixed(ctx.emitter, source_ty);
            Ok(PhpType::Mixed)
        }
        PhpType::Array(param_elem) if param_elem.codegen_repr() == PhpType::Mixed => {
            if let PhpType::AssocArray { .. } = source_ty.codegen_repr() {
                // A declared PHP `array` parameter accepts both indexed and associative
                // storage. Keep hash keys intact: `array_values()` would silently turn every
                // string key into an integer position before the user function can observe it.
                // A hash whose keys ARE 0..n-1 in order is the same PHP value as a list, though,
                // and is handed over packed: not every `array<mixed>` consumer dispatches on the
                // heap kind, and `in_array()`, `max()`, `reset()` and callback `array_filter()`
                // read a hash's buckets as packed slots. A gradual container parameter
                // (`kernel.enabled_locales`) reaches Symfony's listeners exactly this way.
                // The conversion works on a reference of its own, which the planned cleanup
                // releases after the call (see `direct_call_arg_temp_cleanup_type`).
                abi::emit_incref_if_refcounted(ctx.emitter, &source_ty.codegen_repr());
                match ctx.emitter.target.arch {
                    Arch::AArch64 => {
                        abi::emit_load_int_immediate(ctx.emitter, "x1", 7);
                        abi::emit_load_int_immediate(ctx.emitter, "x2", 1);
                    }
                    Arch::X86_64 => {
                        ctx.emitter.instruction("mov rdi, rax");                // pass the hash to the list-packing helper
                        abi::emit_load_int_immediate(ctx.emitter, "rsi", 7);
                        abi::emit_load_int_immediate(ctx.emitter, "rdx", 1);
                    }
                }
                abi::emit_call_label(ctx.emitter, "__rt_array_unbox_values");
                return Ok(PhpType::Array(Box::new(PhpType::Mixed)));
            }
            if let PhpType::Array(source_elem) = source_ty.codegen_repr() {
                let source_elem = source_elem.codegen_repr();
                if source_elem != PhpType::Mixed {
                    emit_loaded_indexed_array_to_mixed(ctx, &source_elem);
                }
                return Ok(PhpType::Array(Box::new(PhpType::Mixed)));
            }
            if matches!(source_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                let indexed = ctx.next_label("call_arg_mixed_array_indexed");
                let associative = ctx.next_label("call_arg_mixed_array_associative");
                let done = ctx.next_label("call_arg_mixed_array_done");
                match ctx.emitter.target.arch {
                    Arch::AArch64 => {
                        ctx.emitter.instruction("cmp x0, #4");                  // tag 4 is indexed PHP array storage
                        ctx.emitter.instruction(&format!("b.eq {}", indexed));
                        ctx.emitter.instruction("cmp x0, #5");                  // tag 5 is associative PHP array storage
                        ctx.emitter.instruction(&format!("b.eq {}", associative));
                    }
                    Arch::X86_64 => {
                        ctx.emitter.instruction("cmp rax, 4");                  // tag 4 is indexed PHP array storage
                        ctx.emitter.instruction(&format!("je {}", indexed));
                        ctx.emitter.instruction("cmp rax, 5");                  // tag 5 is associative PHP array storage
                        ctx.emitter.instruction(&format!("je {}", associative));
                    }
                }
                exceptions::emit_type_error(ctx, "Argument must be of type array, non-array given");

                ctx.emitter.label(&indexed);
                match ctx.emitter.target.arch {
                    Arch::AArch64 => {
                        ctx.emitter.instruction("mov x0, x1");                  // pass the borrowed indexed-array payload to the normalizing clone helper
                    }
                    Arch::X86_64 => {
                        ctx.emitter.instruction("mov rax, rdi");                // pass the borrowed indexed-array payload to the normalizing clone helper
                    }
                }
                let result_reg = abi::int_result_reg(ctx.emitter).to_string();
                callable_invoker_args::emit_clone_indexed_array_for_invoker_with_runtime_tag(
                    &result_reg,
                    ctx.emitter,
                );
                abi::emit_jump(ctx.emitter, &done);

                ctx.emitter.label(&associative);
                match ctx.emitter.target.arch {
                    Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),     // pass the borrowed associative payload to the hash clone helper
                    Arch::X86_64 => ctx.emitter.instruction("mov rax, rdi"),    // pass the borrowed associative payload to the hash clone helper
                }
                callable_invoker_args::emit_clone_assoc_array_for_invoker_with_value_type(
                    &result_reg,
                    &PhpType::Mixed,
                    ctx.emitter,
                );
                ctx.emitter.label(&done);
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
        PhpType::Str => {
            emit_string_result_as_tagged_scalar(ctx);
            Ok(PhpType::TaggedScalar)
        }
        // An object, an array or a float reaching a declared `?int` is not a missing lowering:
        // it is the argument PHP itself refuses, with a `TypeError`, before the callee runs. The
        // shape that produces one is a runtime-selected class — `new $class($a, $b)` lowers to a
        // ladder with one arm per candidate class in the program, and an arm whose constructor
        // happens to declare `?int` where the site passes an object is simply never taken.
        // Refusing to COMPILE that arm let one unrelated class veto the whole construction site:
        // Symfony's `Router::getMatcher` could not be compiled because `NumericNode::__construct`
        // exists somewhere in the same program.
        other => {
            exceptions::emit_type_error(
                ctx,
                &format!("Argument must be of type ?int, {} given", php_type_label(&other)),
            );
            Ok(PhpType::TaggedScalar)
        }
    }
}

/// Spells a representation the way PHP names it in a `TypeError`.
fn php_type_label(ty: &PhpType) -> String {
    match ty {
        PhpType::Object(class_name) if !class_name.is_empty() => class_name.clone(),
        PhpType::Object(_) => "object".to_string(),
        PhpType::Array(_) | PhpType::AssocArray { .. } => "array".to_string(),
        PhpType::Float => "float".to_string(),
        PhpType::Iterable => "iterable".to_string(),
        PhpType::Resource(_) => "resource".to_string(),
        other => format!("{:?}", other),
    }
}

/// Coerces a loaded PHP string into nullable-int ABI storage or throws a catchable TypeError.
fn emit_string_result_as_tagged_scalar(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    let invalid_label = ctx.next_label("call_arg_string_to_nullable_int_invalid");
    let done_label = ctx.next_label("call_arg_string_to_nullable_int_done");
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    abi::emit_call_label(ctx.emitter, "__rt_str_looks_like_int_for_coercion");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz x0, {}", invalid_label));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter
                .instruction(&format!("jz {}", invalid_label));
        }
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, ptr_reg, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, len_reg, 8);
    abi::emit_call_label(ctx.emitter, "__rt_str_to_int");
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&invalid_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    exceptions::emit_type_error(
        ctx,
        "Argument must be of type ?int, non-numeric string given",
    );
    ctx.emitter.label(&done_label);
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
