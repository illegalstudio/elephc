//! Purpose:
//! Lowers static, lexical-instance, late-bound, and Fiber static method calls.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers a direct static-method call on a named class receiver.
pub(super) fn lower_static_method_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let target = method_name_data(ctx, inst)?.to_string();
    let (receiver_label, method_name) = parse_static_method_target(&target)?;
    let receiver = resolve_static_method_receiver(ctx, receiver_label)?;
    if is_static_fiber_get_current_call(&receiver, method_name) {
        return lower_static_fiber_get_current(ctx, inst);
    }
    if is_static_fiber_suspend_call(&receiver, method_name) {
        return lower_static_fiber_suspend(ctx, inst);
    }
    if let Some(()) =
        enums::try_lower_enum_static_method(ctx, receiver.as_str(), method_name, inst)?
    {
        return Ok(());
    }
    let called_class_id = resolve_static_called_class_arg(ctx, receiver_label, &receiver)?;
    if let Some(intrinsic) = runtime_backed_static_intrinsic(receiver.as_str(), method_name) {
        return lower_static_runtime_intrinsic(
            ctx,
            inst,
            receiver.as_str(),
            method_name,
            &called_class_id,
            intrinsic,
        );
    }
    let late_bound_static = is_late_bound_static_receiver(receiver_label);
    let Some(receiver_info) = ctx.module.class_infos.get(receiver.as_str()) else {
        if builtins::has_eval_context(ctx) {
            return builtins::lower_eval_static_method_call(
                ctx,
                inst,
                receiver.as_str(),
                method_name,
            );
        }
        return Err(CodegenIrError::unsupported(format!(
            "static method call on unknown class {}",
            receiver
        )));
    };
    let method_key = php_symbol_key(method_name);
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(receiver.as_str());
    let impl_info = ctx.module.class_infos.get(impl_class).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "static method implementation on unknown class {}",
            impl_class
        ))
    })?;
    let Some(callee_sig) = impl_info.static_methods.get(&method_key) else {
        if is_lexical_instance_static_receiver(receiver_label)
            && receiver_info.methods.contains_key(&method_key)
        {
            return lower_lexical_instance_static_method_call(
                ctx,
                inst,
                receiver.as_str(),
                method_name,
            );
        }
        return Err(CodegenIrError::unsupported(format!(
            "static method call to unknown method {}",
            target
        )));
    };
    if inst.operands.len() != callee_sig.params.len() {
        return Err(CodegenIrError::unsupported(format!(
            "static method call to {} with {} operands for {} params",
            target,
            inst.operands.len(),
            callee_sig.params.len()
        )));
    }
    let param_types = callee_sig
        .params
        .iter()
        .map(|(_, ty)| ty.codegen_repr())
        .collect::<Vec<_>>();
    let dynamic_static_slot = if late_bound_static {
        receiver_info.static_vtable_slots.get(&method_key).copied()
    } else {
        None
    };
    let eval_done_label = if late_bound_static && ctx.module.required_runtime_features.eval_bridge {
        let no_override_label = ctx.next_label("eval_late_static_no_override");
        let done_label = ctx.next_label("eval_late_static_done");
        builtins::lower_eval_native_frame_static_method_call(
            ctx,
            inst,
            receiver.as_str(),
            method_name,
            &no_override_label,
            &done_label,
        )?;
        ctx.emitter.label(&no_override_label);
        Some(done_label)
    } else {
        None
    };
    let call_args = materialize_static_method_call_args_with_refs(
        ctx,
        &called_class_id,
        &inst.operands,
        &param_types,
        &callee_sig.ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    if let Some(slot) = dynamic_static_slot {
        emit_dynamic_static_method_call(ctx, slot);
    } else {
        abi::emit_call_label(ctx.emitter, &static_method_symbol(impl_class, &method_key));
    }
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &callee_sig.return_type)?;
    emit_call_arg_temp_cleanups(ctx, &call_args, inst.result)?;
    emit_ref_arg_writebacks(ctx, &call_args)?;
    if let Some(done_label) = eval_done_label {
        ctx.emitter.label(&done_label);
    }
    Ok(())
}

/// Lowers a direct static-method call against a class declared by a previous eval fragment.
pub(super) fn lower_eval_static_method_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let target = method_name_data(ctx, inst)?.to_string();
    let (receiver_label, method_name) = parse_static_method_target(&target)?;
    let receiver = resolve_static_method_receiver(ctx, receiver_label)?;
    builtins::lower_eval_static_method_call(ctx, inst, receiver.as_str(), method_name)
}

/// Lowers `self::method()` or `parent::method()` when it targets an instance method.
pub(super) fn lower_lexical_instance_static_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver: &str,
    method_name: &str,
) -> Result<()> {
    let this_slot = ctx.local_slot_by_name("this").ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "lexical instance method static call without this in {}",
            ctx.function.name
        ))
    })?;
    let mut target =
        resolve_method_call_target(ctx, receiver, method_name, inst.operands.len() + 1)?;
    target.dynamic_slot = None;
    let receiver_ty = PhpType::Object(receiver.to_string());
    let mut param_types = Vec::with_capacity(target.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(target.params.iter().map(|param| param.codegen_repr()));
    let mut ref_params = Vec::with_capacity(target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(target.ref_params.iter().copied());
    // `parent::__construct(...)` reaches this lowering, and a parent constructor may PROMOTE
    // a by-reference parameter into a property that borrows the argument's cell for the whole
    // life of the object — so a constructor target keeps its heap cell while every other
    // method takes the caller-stack one (see `RefArgCellLifetime`).
    let ref_cell_lifetime = if method_name.eq_ignore_ascii_case("__construct") {
        RefArgCellLifetime::MayOutliveCall
    } else {
        RefArgCellLifetime::CallOnly
    };
    let call_args = materialize_method_call_args_with_receiver_local_and_refs(
        ctx,
        this_slot,
        &receiver_ty,
        &inst.operands,
        &param_types,
        &ref_params,
        ref_cell_lifetime,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(
        ctx.emitter,
        &method_symbol(&target.impl_class, &target.method_key),
    );
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_method_call_result(ctx, inst, &target)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Emits an indirect static-vtable call for a late-bound `static::method()` receiver.
pub(super) fn emit_dynamic_static_method_call(ctx: &mut FunctionContext<'_>, slot: usize) {
    let hidden_called_class_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let class_id_scratch = abi::temp_int_reg(ctx.emitter.target);
    let dispatch_scratch = abi::symbol_scratch_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(
                "mov {}, {}",
                class_id_scratch, hidden_called_class_reg
            ));                                                                 // preserve the forwarded called-class id across static-vtable address materialization
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(
                "mov {}, {}",
                class_id_scratch, hidden_called_class_reg
            ));                                                                 // preserve the forwarded called-class id across static-vtable address materialization
        }
    }
    abi::emit_symbol_address(ctx.emitter, dispatch_scratch, "_class_static_vtable_ptrs");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(
                "ldr {}, [{}, {}, lsl #3]",
                dispatch_scratch, dispatch_scratch, class_id_scratch
            ));                                                                 // load the class-specific static-vtable pointer from the global table
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(
                "mov {}, QWORD PTR [{} + {} * 8]",
                dispatch_scratch, dispatch_scratch, class_id_scratch
            ));                                                                 // load the class-specific static-vtable pointer from the global table
        }
    }
    abi::emit_load_from_address(ctx.emitter, dispatch_scratch, dispatch_scratch, slot * 8);
    abi::emit_call_reg(ctx.emitter, dispatch_scratch);
}

/// Lowers static `Fiber::suspend($value = null)` through the shared runtime helper.
pub(super) fn lower_static_fiber_suspend(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = fiber_single_optional_arg(ctx, &inst.operands, "Fiber::suspend")?;
    emit_optional_mixed_arg(ctx, value)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter)); // preserve the boxed suspend value for target-specific argument loading
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0)); // pass the boxed suspend value as runtime helper argument 1
    abi::emit_call_label(ctx.emitter, "__rt_fiber_suspend");
    store_if_result(ctx, inst)
}

/// Lowers static `Fiber::getCurrent()` through the shared runtime helper.
pub(super) fn lower_static_fiber_get_current(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if !inst.operands.is_empty() {
        return Err(CodegenIrError::unsupported(
            "Fiber::getCurrent with EIR arguments",
        ));
    }
    abi::emit_call_label(ctx.emitter, "__rt_fiber_get_current");
    store_if_result(ctx, inst)
}

/// Returns true when a static method call targets PHP's built-in `Fiber::getCurrent`.
pub(super) fn is_static_fiber_get_current_call(receiver: &str, method_name: &str) -> bool {
    php_symbol_key(receiver.trim_start_matches('\\')) == "fiber"
        && php_symbol_key(method_name) == "getcurrent"
}

/// Returns true when a static method call targets PHP's built-in `Fiber::suspend`.
pub(super) fn is_static_fiber_suspend_call(receiver: &str, method_name: &str) -> bool {
    php_symbol_key(receiver.trim_start_matches('\\')) == "fiber"
        && php_symbol_key(method_name) == "suspend"
}

/// Resolves the hidden called-class id argument for a static method call.
pub(super) fn resolve_static_called_class_arg(
    ctx: &FunctionContext<'_>,
    receiver_label: &str,
    receiver: &str,
) -> Result<CalledClassIdArg> {
    let receiver_label = receiver_label.trim_start_matches('\\');
    if matches!(receiver_label, "self" | "parent" | "static") {
        if let Some(slot) = ctx.local_slot_by_name(CALLED_CLASS_ID_PARAM) {
            return Ok(CalledClassIdArg::Local(slot));
        }
        if let Some(slot) = ctx.local_slot_by_name("this") {
            return Ok(CalledClassIdArg::ThisObject(slot));
        }
    }
    let class_info = ctx.module.class_infos.get(receiver).ok_or_else(|| {
        CodegenIrError::unsupported(format!("static method call on unknown class {}", receiver))
    })?;
    Ok(CalledClassIdArg::Immediate(class_info.class_id))
}

/// Resolves lexical `self` and `parent` receivers for static method calls.
pub(super) fn resolve_static_method_receiver(ctx: &FunctionContext<'_>, receiver: &str) -> Result<String> {
    let receiver = receiver.trim_start_matches('\\');
    match receiver {
        "self" => current_method_class(ctx).map(str::to_string),
        "parent" => {
            let class_name = current_method_class(ctx)?;
            ctx.module
                .class_infos
                .get(class_name)
                .and_then(|class| class.parent.clone())
                .ok_or_else(|| {
                    CodegenIrError::unsupported(format!(
                        "parent static method call outside class with parent for {}",
                        ctx.function.name
                    ))
                })
        }
        "static" => current_method_class(ctx).map(str::to_string),
        _ => Ok(receiver.to_string()),
    }
}

/// Returns true for the late-bound static receiver spelling.
pub(super) fn is_late_bound_static_receiver(receiver: &str) -> bool {
    receiver.trim_start_matches('\\') == "static"
}

/// Returns true when PHP static-call syntax should bind an instance method lexically.
pub(super) fn is_lexical_instance_static_receiver(receiver: &str) -> bool {
    matches!(receiver.trim_start_matches('\\'), "self" | "parent")
}

/// Returns the class name encoded in the current EIR class-method function name.
pub(super) fn current_method_class<'a>(ctx: &'a FunctionContext<'_>) -> Result<&'a str> {
    ctx.function
        .name
        .rsplit_once("::")
        .map(|(class_name, _)| class_name)
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "lexical static method receiver outside class method {}",
                ctx.function.name
            ))
        })
}

/// Splits an EIR static-method call label into class receiver and method name.
pub(super) fn parse_static_method_target(target: &str) -> Result<(&str, &str)> {
    target.rsplit_once("::").ok_or_else(|| {
        CodegenIrError::invalid_module(format!("invalid static method target '{}'", target))
    })
}

