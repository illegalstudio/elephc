//! Purpose:
//! Target callback wrappers, dispatch, and environment setup.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Emits a local wrapper that prepends the hidden static called-class id.
pub(super) fn emit_static_method_callback_wrapper(
    ctx: &mut FunctionContext<'_>,
    target: &StaticMethodCallbackTarget,
    visible_arg_types: &[PhpType],
) -> String {
    let wrapper_label = ctx.next_label("static_method_callback_wrapper");
    let done_label = ctx.next_label("static_method_callback_after_wrapper");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&wrapper_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            emit_static_method_callback_wrapper_aarch64(ctx, target, visible_arg_types)
        }
        Arch::X86_64 => emit_static_method_callback_wrapper_x86_64(ctx, target, visible_arg_types),
    }
    ctx.emitter.label(&done_label);
    wrapper_label
}

/// Emits the AArch64 static-method callback ABI adapter.
pub(super) fn emit_static_method_callback_wrapper_aarch64(
    ctx: &mut FunctionContext<'_>,
    target: &StaticMethodCallbackTarget,
    visible_arg_types: &[PhpType],
) {
    let env_reg = abi::int_arg_reg_name(ctx.emitter.target, callback_arg_abi_slots(visible_arg_types));
    let frame = callback_wrapper_frame(&target.param_types, visible_arg_types);
    abi::emit_reserve_temporary_stack(ctx.emitter, frame.total_bytes);
    abi::emit_store_to_sp(ctx.emitter, "x30", frame.return_address_offset);
    save_callback_visible_args(ctx, &frame, visible_arg_types);
    match target.called_class {
        StaticCallbackCalledClass::Immediate(class_id) => {
            abi::emit_load_int_immediate(ctx.emitter, "x3", class_id as i64);
        }
        StaticCallbackCalledClass::Env => {
            abi::emit_load_from_address(ctx.emitter, "x3", env_reg, 0);
        }
    }
    save_callback_hidden_arg(ctx, &frame, "x3");
    box_callback_mixed_args(ctx, &frame, &target.param_types, visible_arg_types);
    load_callback_target_args(ctx, &frame, visible_arg_types);
    emit_static_callback_dispatch(ctx, target);
    cleanup_callback_boxed_args(ctx, &frame, &target.return_ty);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x30", frame.return_address_offset);
    abi::emit_release_temporary_stack(ctx.emitter, frame.total_bytes);
    ctx.emitter.instruction("ret");                                             // return the static method result to the runtime callback helper
}

/// Emits the x86_64 static-method callback ABI adapter.
pub(super) fn emit_static_method_callback_wrapper_x86_64(
    ctx: &mut FunctionContext<'_>,
    target: &StaticMethodCallbackTarget,
    visible_arg_types: &[PhpType],
) {
    let env_reg = abi::int_arg_reg_name(ctx.emitter.target, callback_arg_abi_slots(visible_arg_types));
    let frame = callback_wrapper_frame(&target.param_types, visible_arg_types);
    ctx.emitter.instruction("push rbp");                                        // preserve the runtime helper frame pointer for the nested static method call
    ctx.emitter.instruction("mov rbp, rsp");                                    // establish a wrapper frame while shifting callback arguments
    abi::emit_reserve_temporary_stack(ctx.emitter, frame.total_bytes);
    save_callback_visible_args(ctx, &frame, visible_arg_types);
    match target.called_class {
        StaticCallbackCalledClass::Immediate(class_id) => {
            abi::emit_load_int_immediate(ctx.emitter, "rcx", class_id as i64);
        }
        StaticCallbackCalledClass::Env => {
            abi::emit_load_from_address(ctx.emitter, "rcx", env_reg, 0);
        }
    }
    save_callback_hidden_arg(ctx, &frame, "rcx");
    box_callback_mixed_args(ctx, &frame, &target.param_types, visible_arg_types);
    load_callback_target_args(ctx, &frame, visible_arg_types);
    emit_static_callback_dispatch(ctx, target);
    cleanup_callback_boxed_args(ctx, &frame, &target.return_ty);
    abi::emit_release_temporary_stack(ctx.emitter, frame.total_bytes);
    ctx.emitter.instruction("pop rbp");                                         // restore the runtime helper frame pointer before returning
    ctx.emitter.instruction("ret");                                             // return the static method result to the runtime callback helper
}

/// Emits a local wrapper that prepends the captured object receiver.
pub(super) fn emit_instance_method_callback_wrapper(
    ctx: &mut FunctionContext<'_>,
    target: &InstanceMethodCallbackTarget,
    visible_arg_types: &[PhpType],
) -> String {
    let wrapper_label = ctx.next_label("instance_method_callback_wrapper");
    let done_label = ctx.next_label("instance_method_callback_after_wrapper");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&wrapper_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            emit_instance_method_callback_wrapper_aarch64(ctx, target, visible_arg_types)
        }
        Arch::X86_64 => {
            emit_instance_method_callback_wrapper_x86_64(ctx, target, visible_arg_types)
        }
    }
    ctx.emitter.label(&done_label);
    wrapper_label
}

/// Emits the AArch64 instance-method callback ABI adapter.
pub(super) fn emit_instance_method_callback_wrapper_aarch64(
    ctx: &mut FunctionContext<'_>,
    target: &InstanceMethodCallbackTarget,
    visible_arg_types: &[PhpType],
) {
    let env_reg = abi::int_arg_reg_name(ctx.emitter.target, callback_arg_abi_slots(visible_arg_types));
    let frame = callback_wrapper_frame(&target.param_types, visible_arg_types);
    abi::emit_reserve_temporary_stack(ctx.emitter, frame.total_bytes);
    abi::emit_store_to_sp(ctx.emitter, "x30", frame.return_address_offset);
    save_callback_visible_args(ctx, &frame, visible_arg_types);
    abi::emit_load_from_address(ctx.emitter, "x3", env_reg, 0);
    save_callback_hidden_arg(ctx, &frame, "x3");
    box_callback_mixed_args(ctx, &frame, &target.param_types, visible_arg_types);
    load_callback_target_args(ctx, &frame, visible_arg_types);
    abi::emit_call_label(ctx.emitter, &target.entry_label);
    cleanup_callback_boxed_args(ctx, &frame, &target.return_ty);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x30", frame.return_address_offset);
    abi::emit_release_temporary_stack(ctx.emitter, frame.total_bytes);
    ctx.emitter.instruction("ret");                                             // return the instance method result to the runtime callback helper
}

/// Emits the x86_64 instance-method callback ABI adapter.
pub(super) fn emit_instance_method_callback_wrapper_x86_64(
    ctx: &mut FunctionContext<'_>,
    target: &InstanceMethodCallbackTarget,
    visible_arg_types: &[PhpType],
) {
    let env_reg = abi::int_arg_reg_name(ctx.emitter.target, callback_arg_abi_slots(visible_arg_types));
    let frame = callback_wrapper_frame(&target.param_types, visible_arg_types);
    ctx.emitter.instruction("push rbp");                                        // preserve the runtime helper frame pointer for the nested instance method call
    ctx.emitter.instruction("mov rbp, rsp");                                    // establish a wrapper frame while shifting callback arguments
    abi::emit_reserve_temporary_stack(ctx.emitter, frame.total_bytes);
    save_callback_visible_args(ctx, &frame, visible_arg_types);
    abi::emit_load_from_address(ctx.emitter, "rcx", env_reg, 0);
    save_callback_hidden_arg(ctx, &frame, "rcx");
    box_callback_mixed_args(ctx, &frame, &target.param_types, visible_arg_types);
    load_callback_target_args(ctx, &frame, visible_arg_types);
    abi::emit_call_label(ctx.emitter, &target.entry_label);
    cleanup_callback_boxed_args(ctx, &frame, &target.return_ty);
    abi::emit_release_temporary_stack(ctx.emitter, frame.total_bytes);
    ctx.emitter.instruction("pop rbp");                                         // restore the runtime helper frame pointer before returning
    ctx.emitter.instruction("ret");                                             // return the instance method result to the runtime callback helper
}

/// Emits either a direct static-method callback call or a late-static vtable call.
pub(super) fn emit_static_callback_dispatch(
    ctx: &mut FunctionContext<'_>,
    target: &StaticMethodCallbackTarget,
) {
    if let Some(slot) = target.dynamic_slot {
        emit_static_callback_dynamic_call(ctx, slot);
    } else {
        abi::emit_call_label(ctx.emitter, &target.entry_label);
    }
}

/// Emits an indirect static-vtable callback call for a late-bound `static::method()` wrapper.
pub(super) fn emit_static_callback_dynamic_call(ctx: &mut FunctionContext<'_>, slot: usize) {
    let hidden_called_class_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let class_id_scratch = abi::temp_int_reg(ctx.emitter.target);
    let dispatch_scratch = abi::symbol_scratch_reg(ctx.emitter);
    ctx.emitter.instruction(&format!(                                           // preserve the forwarded called-class id across static-vtable address materialization
        "mov {}, {}",
        class_id_scratch, hidden_called_class_reg
    ));
    abi::emit_symbol_address(
        ctx.emitter,
        dispatch_scratch,
        "_class_source_static_vtable_ptrs",
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(                                   // load the class-specific static-vtable pointer from the global table
                "ldr {}, [{}, {}, lsl #3]",
                dispatch_scratch, dispatch_scratch, class_id_scratch
            ));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(                                   // load the class-specific static-vtable pointer from the global table
                "mov {}, QWORD PTR [{} + {} * 8]",
                dispatch_scratch, dispatch_scratch, class_id_scratch
            ));
        }
    }
    abi::emit_load_from_address(ctx.emitter, dispatch_scratch, dispatch_scratch, slot * 8);
    abi::emit_call_reg(ctx.emitter, dispatch_scratch);
}

/// Reserves and fills the optional callback environment consumed by sort runtime helpers.
pub(super) fn reserve_static_callback_env(
    ctx: &mut FunctionContext<'_>,
    source: Option<StaticCallbackEnvSource>,
) -> Result<usize> {
    let Some(source) = source else {
        return Ok(0);
    };
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    match source {
        StaticCallbackEnvSource::Local(slot) => {
            let source_ty = ctx.load_local_to_result(slot)?;
            if source_ty != PhpType::Int {
                return Err(CodegenIrError::invalid_module(format!(
                    "hidden called-class id local has PHP type {:?}",
                    source_ty
                )));
            }
        }
        StaticCallbackEnvSource::ThisObject(slot) => {
            let source_ty = ctx.load_local_to_result(slot)?;
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
        StaticCallbackEnvSource::Value(value) => {
            let source_ty = ctx.load_value_to_result(value)?;
            if !matches!(source_ty.codegen_repr(), PhpType::Object(_)) {
                return Err(CodegenIrError::invalid_module(format!(
                    "callback environment value has PHP type {:?}",
                    source_ty
                )));
            }
        }
        StaticCallbackEnvSource::FunctionLabel(label) => {
            abi::emit_symbol_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &label,
            );
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp]");                            // store the callback environment payload for the runtime helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // store the callback environment payload for the runtime helper
        }
    }
    Ok(16)
}

/// Loads the optional callback environment argument expected by sort runtime helpers.
pub(super) fn load_static_callback_env_arg(ctx: &mut FunctionContext<'_>, env_reg: &str, env_bytes: usize) {
    if env_bytes == 0 {
        abi::emit_load_int_immediate(ctx.emitter, env_reg, 0);
    } else {
        abi::emit_temporary_stack_address(ctx.emitter, env_reg, 0);
    }
}

