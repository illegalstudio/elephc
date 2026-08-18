//! Purpose:
//! Array map target selection and callback descriptor dispatch.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Destination shape an `array_map()` lowering builds.
///
/// php-src's single-array `array_map()` PRESERVES string keys, so an associative source must
/// rebuild a hash rather than a list. The two shapes share every callback-resolution path and
/// differ only in which runtime helper receives the resolved callback.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ArrayMapTarget {
    /// Indexed source: the `__rt_array_map*` helpers build a list.
    Indexed,
    /// Associative source: `__rt_hash_map` rebuilds a hash under the source keys.
    Hash,
}

/// Lowers `array_map()` through the callback runtime helper matching the callback result type.
///
/// Associative sources take the `__rt_hash_map` path, which walks the source hash and reuses
/// each entry's key; indexed sources keep the existing list-building helpers. Callback
/// resolution — descriptor, runtime string name, callable array, or a statically bound function
/// — is identical for both and is shared verbatim.
pub(crate) fn lower_array_map(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_map", 2)?;
    let callback = expect_operand(inst, 0)?;
    let array = expect_operand(inst, 1)?;
    let source_ty = ctx.value_php_type(array)?.codegen_repr();
    let (elem_ty, target) = if matches!(source_ty, PhpType::AssocArray { .. }) {
        (
            hash_map_source_value_type(&source_ty)?,
            ArrayMapTarget::Hash,
        )
    } else {
        (
            array_map_callback_array_element_type(ctx.value_php_type(array)?)?,
            ArrayMapTarget::Indexed,
        )
    };
    match ctx.value_php_type(callback)?.codegen_repr() {
        PhpType::Callable => {
            let callback_elem_ty = array_map_descriptor_callback_result_element_type(inst)?;
            let result_elem_ty = array_map_result_element_type(inst, &callback_elem_ty)?;
            return lower_array_map_descriptor_callback(
                ctx,
                inst,
                callback,
                array,
                &elem_ty,
                &callback_elem_ty,
                &result_elem_ty,
                target,
            );
        }
        PhpType::Str => {
            let callback_elem_ty = PhpType::Mixed;
            let result_elem_ty = array_map_result_element_type(inst, &callback_elem_ty)?;
            lower_runtime_string_descriptor_callback(
                ctx,
                callback,
                Some(&PhpType::Array(Box::new(elem_ty.clone()))),
                vec![elem_ty.clone()],
                PhpType::Mixed,
                super::super::super::instruction_strict_php_profile(inst),
                "array_map",
                |ctx, wrapper_label, env_bytes| {
                    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
                    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
                    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, wrapper_label);
                    ctx.load_value_to_reg(array, array_arg_reg)?;
                    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
                    emit_array_map_runtime_call(ctx, &callback_elem_ty, env_bytes, target)?;
                    Ok(())
                },
            )?;
            finish_array_map_result(ctx, inst, target, &callback_elem_ty, &result_elem_ty)?;
            store_if_result(ctx, inst)?;
            return Ok(());
        }
        PhpType::Array(elem) if matches!(elem.codegen_repr(), PhpType::Mixed | PhpType::Str) => {
            let callback_elem_ty = array_map_descriptor_callback_result_element_type(inst)?;
            let result_elem_ty = array_map_result_element_type(inst, &callback_elem_ty)?;
            return lower_array_map_callable_array_descriptor_callback(
                ctx,
                inst,
                callback,
                array,
                &elem_ty,
                &callback_elem_ty,
                &result_elem_ty,
                target,
            );
        }
        _ => {}
    }
    if descriptor_callback_local_without_same_block_store(ctx, callback)? {
        let callback_elem_ty = array_map_descriptor_callback_result_element_type(inst)?;
        let result_elem_ty = array_map_result_element_type(inst, &callback_elem_ty)?;
        return lower_array_map_descriptor_callback(
            ctx,
            inst,
            callback,
            array,
            &elem_ty,
            &callback_elem_ty,
            &result_elem_ty,
            target,
        );
    }
    let callback_binding =
        static_sort_callback_binding(ctx, callback, "array_map callback", Some(&[elem_ty]))?;
    let callback_elem_ty = array_map_callback_result_element_type(&callback_binding.return_ty)?;
    let result_elem_ty = array_map_result_element_type(inst, &callback_elem_ty)?;
    let env_bytes = reserve_static_callback_env(ctx, callback_binding.env_source)?;
    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, &callback_binding.label);
    ctx.load_value_to_reg(array, array_arg_reg)?;
    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
    emit_array_map_runtime_call(ctx, &callback_elem_ty, env_bytes, target)?;
    if env_bytes != 0 {
        abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    }
    finish_array_map_result(ctx, inst, target, &callback_elem_ty, &result_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_map()` through a descriptor-backed callback wrapper.
pub(super) fn lower_array_map_descriptor_callback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
    array: ValueId,
    elem_ty: &PhpType,
    callback_elem_ty: &PhpType,
    result_elem_ty: &PhpType,
    target: ArrayMapTarget,
) -> Result<()> {
    let wrapper_label =
        emit_descriptor_callback_wrapper(ctx, vec![elem_ty.clone()], callback_elem_ty.clone());
    let env_bytes = reserve_descriptor_callback_env(ctx, callback)?;
    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, &wrapper_label);
    ctx.load_value_to_reg(array, array_arg_reg)?;
    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
    emit_array_map_runtime_call(ctx, callback_elem_ty, env_bytes, target)?;
    abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    finish_array_map_result(ctx, inst, target, callback_elem_ty, result_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_map()` for runtime callable-array callbacks through descriptor envs.
pub(super) fn lower_array_map_callable_array_descriptor_callback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
    array: ValueId,
    elem_ty: &PhpType,
    callback_elem_ty: &PhpType,
    result_elem_ty: &PhpType,
    target: ArrayMapTarget,
) -> Result<()> {
    let wrapper_label =
        emit_descriptor_callback_wrapper(ctx, vec![elem_ty.clone()], callback_elem_ty.clone());
    super::super::super::callables::emit_runtime_callable_array_descriptor_value(
        ctx,
        callback,
        "array_map callable array",
    )?;
    let descriptor_reg = abi::int_result_reg(ctx.emitter);
    let env_bytes = reserve_descriptor_callback_env_from_reg(ctx, descriptor_reg);
    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, &wrapper_label);
    ctx.load_value_to_reg(array, array_arg_reg)?;
    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
    emit_array_map_runtime_call(ctx, callback_elem_ty, env_bytes, target)?;
    release_descriptor_callback_env_preserving_result(ctx);
    abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    finish_array_map_result(ctx, inst, target, callback_elem_ty, result_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Releases a one-slot descriptor callback env while preserving the runtime result.
pub(super) fn release_descriptor_callback_env_preserving_result(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, result_reg);
}

/// Emits a descriptor callback wrapper next to the current EIR function body.
pub(super) fn emit_descriptor_callback_wrapper(
    ctx: &mut FunctionContext<'_>,
    visible_arg_types: Vec<PhpType>,
    return_ty: PhpType,
) -> String {
    let wrapper_label = ctx.next_global_label("array_map_descriptor_callback_wrapper");
    let done_label = ctx.next_label("array_map_descriptor_callback_after_wrapper");
    let wrapper = DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types,
        target_visible_arg_types: None,
        capture_types: Vec::new(),
        descriptor_prefix_types: Vec::new(),
        descriptor_return_type: Some(return_ty),
    };
    abi::emit_jump(ctx.emitter, &done_label);
    crate::codegen::emit_callback_wrapper(ctx.emitter, &wrapper);
    ctx.emitter.label(&done_label);
    wrapper_label
}

/// Reserves a one-slot callback environment containing the runtime callable descriptor.
pub(super) fn reserve_descriptor_callback_env(
    ctx: &mut FunctionContext<'_>,
    callback: ValueId,
) -> Result<usize> {
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    let callback_ty = ctx.load_value_to_result(callback)?;
    if callback_ty != PhpType::Callable {
        return Err(CodegenIrError::invalid_module(format!(
            "descriptor callback operand has PHP type {:?}",
            callback_ty
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp]");                            // store the runtime callable descriptor for the descriptor callback wrapper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // store the runtime callable descriptor for the descriptor callback wrapper
        }
    }
    Ok(16)
}

/// Calls a descriptor-backed array callback runtime using a callable descriptor value.
pub(super) fn lower_descriptor_callback_runtime<F>(
    ctx: &mut FunctionContext<'_>,
    callback: ValueId,
    visible_arg_types: Vec<PhpType>,
    return_ty: PhpType,
    mut emit_call: F,
) -> Result<()>
where
    F: FnMut(&mut FunctionContext<'_>, &str, usize) -> Result<()>,
{
    let wrapper_label = emit_descriptor_callback_wrapper(ctx, visible_arg_types, return_ty);
    let env_bytes = reserve_descriptor_callback_env(ctx, callback)?;
    emit_call(ctx, &wrapper_label, env_bytes)?;
    abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    Ok(())
}

/// Dispatches a runtime string callback name to a descriptor-backed array callback runtime.
pub(super) fn lower_runtime_string_descriptor_callback<F>(
    ctx: &mut FunctionContext<'_>,
    callback: ValueId,
    source_arg_ty: Option<&PhpType>,
    visible_arg_types: Vec<PhpType>,
    return_ty: PhpType,
    strict_php: bool,
    owner: &str,
    mut emit_call: F,
) -> Result<()>
where
    F: FnMut(&mut FunctionContext<'_>, &str, usize) -> Result<()>,
{
    let callback_ty = ctx.load_value_to_result(callback)?;
    if callback_ty.codegen_repr() != PhpType::Str {
        return Err(CodegenIrError::invalid_module(format!(
            "{} runtime string callback has PHP type {:?}",
            owner, callback_ty
        )));
    }

    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    let call_reg = abi::nested_call_reg(ctx.emitter);
    let specialization_ty = visible_arg_types.first().or(source_arg_ty);
    let candidate_names = ctx.runtime_callable_candidates(callback);
    let cases = runtime_string_descriptor_cases(
        ctx,
        specialization_ty,
        candidate_names.as_deref(),
        strict_php,
    )?;

    let done_label = ctx.next_label(&format!("{}_runtime_string_callback_done", owner));
    let selector = callable_dispatch::RuntimeCallableSelector::StringNameStack {
        ptr_offset: 0,
        len_offset: 8,
        call_reg,
    };
    for case in &cases {
        let next_case = ctx.next_label(&format!("{}_runtime_string_callback_next", owner));
        let matched_label = ctx.next_label("callable_string_match");
        callable_dispatch::emit_branch_if_callable_case_mismatch(
            &selector,
            case,
            &next_case,
            ctx.emitter,
            &matched_label,
            ctx.data,
        );
        let wrapper_label =
            emit_descriptor_callback_wrapper(ctx, visible_arg_types.clone(), return_ty.clone());
        let env_bytes = reserve_descriptor_callback_env_from_reg(ctx, call_reg);
        emit_call(ctx, &wrapper_label, env_bytes)?;
        abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&next_case);
    }

    emit_dynamic_string_callback_abort(ctx, owner);
    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Reserves a descriptor callback environment using a descriptor already held in a register.
pub(super) fn reserve_descriptor_callback_env_from_reg(
    ctx: &mut FunctionContext<'_>,
    descriptor_reg: &str,
) -> usize {
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("str {}, [sp]", descriptor_reg)); // store the selected runtime string descriptor for the descriptor callback wrapper
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov QWORD PTR [rsp], {}", descriptor_reg));
            // store the selected runtime string descriptor for the descriptor callback wrapper
        }
    }
    16
}

/// Emits a fatal diagnostic for runtime callback names that do not resolve to descriptors.
pub(super) fn emit_dynamic_string_callback_abort(ctx: &mut FunctionContext<'_>, owner: &str) {
    let message = format!(
        "Fatal error: {} callback string does not name a supported callable\n",
        owner
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the unresolved runtime callback diagnostic to stderr
            ctx.emitter.adrp("x1", &message_label); // load the runtime callback diagnostic page
            ctx.emitter.add_lo12("x1", "x1", &message_label); // resolve the runtime callback diagnostic address
            ctx.emitter
                .instruction(&format!("mov x2, #{}", message_len)); // pass the runtime callback diagnostic byte length to write
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the unresolved runtime callback diagnostic to Linux stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter
                .instruction(&format!("mov edx, {}", message_len)); // pass the runtime callback diagnostic byte length to write
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the fatal diagnostic before terminating
            abi::emit_exit(ctx.emitter, 1);
        }
    }
}

