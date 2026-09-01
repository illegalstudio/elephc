//! Purpose:
//! Resolves method targets and stores direct or dynamic call results.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Resolves method implementation class, canonical key, return type, and ABI arity.
pub(super) fn resolve_method_call_target(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
    operand_count: usize,
) -> Result<MethodCallTarget> {
    let normalized = class_name.trim_start_matches('\\');
    let receiver_info = ctx.module.class_infos.get(normalized).ok_or_else(|| {
        CodegenIrError::unsupported(format!("method call on unknown class {}", normalized))
    })?;
    let method_key = php_symbol_key(method_name);
    let mut method_owner = normalized;
    let mut class_info = receiver_info;
    while !class_info.methods.contains_key(&method_key) {
        let Some(parent_name) = class_info.parent.as_deref() else {
            return Err(CodegenIrError::unsupported(format!(
                "method call to unknown method {}::{}",
                normalized, method_name
            )));
        };
        method_owner = parent_name;
        class_info = ctx.module.class_infos.get(parent_name).ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "method call parent metadata missing for {}",
                parent_name
            ))
        })?;
    }
    let callee_sig = class_info.methods.get(&method_key).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "method call to unknown method {}::{}",
            normalized, method_name
        ))
    })?;
    let expected_args = callee_sig.params.len() + 1;
    if operand_count != expected_args {
        return Err(CodegenIrError::unsupported(format!(
            "method call to {}::{} with {} operands for {} ABI params",
            normalized, method_name, operand_count, expected_args
        )));
    }
    let impl_class = class_info
        .method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| method_owner.to_string());
    let dynamic_slot = receiver_info.vtable_slots.get(&method_key).copied();
    let has_direct_body = class_method_already_emitted(ctx, &impl_class, &method_key, false);
    if !has_direct_body && dynamic_slot.is_none() {
        return Err(CodegenIrError::unsupported(format!(
            "method call to {}::{} without an emitted EIR method body",
            impl_class, method_name
        )));
    }
    let dynamic_slot = if class_info.final_methods.contains(&method_key) {
        None
    } else {
        dynamic_slot
    };
    Ok(MethodCallTarget {
        impl_class,
        method_key,
        dynamic_slot,
        params: callee_sig
            .params
            .iter()
            .map(|(_, ty)| ty.codegen_repr())
            .collect(),
        ref_params: callee_sig.ref_params.clone(),
        return_ty: callee_sig.return_type.clone(),
        by_ref_return: callee_sig.by_ref_return,
    })
}

/// Emits a runtime vtable dispatch for an instance method whose concrete override is late-bound.
pub(super) fn emit_dynamic_instance_method_call(ctx: &mut FunctionContext<'_>, slot: usize) {
    let class_id_reg = abi::temp_int_reg(ctx.emitter.target);
    let dispatch_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_from_address(
        ctx.emitter,
        class_id_reg,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        0,
    );
    abi::emit_symbol_address(ctx.emitter, dispatch_reg, "_class_vtable_ptrs");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(
                "ldr {}, [{}, {}, lsl #3]",
                dispatch_reg, dispatch_reg, class_id_reg
            ));                                                                 // load the class-specific instance-vtable pointer
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(
                "mov {}, QWORD PTR [{} + {} * 8]",
                dispatch_reg, dispatch_reg, class_id_reg
            ));                                                                 // load the class-specific instance-vtable pointer
        }
    }
    abi::emit_load_from_address(ctx.emitter, dispatch_reg, dispatch_reg, slot * 8);
    abi::emit_call_reg(ctx.emitter, dispatch_reg);
}

/// Returns true when the current EIR module includes the target class method body.
pub(super) fn class_method_already_emitted(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
    is_static: bool,
) -> bool {
    ctx.module.class_methods.iter().any(|function| {
        function.flags.is_static == is_static
            && function
                .name
                .rsplit_once("::")
                .is_some_and(|(candidate_class, candidate_method)| {
                    candidate_class == class_name && php_symbol_key(candidate_method) == method_key
                })
    })
}

/// Stores a call result, boxing concrete returns for generic EIR result slots.
pub(super) fn store_call_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    return_ty: &PhpType,
) -> Result<()> {
    if let Some(result) = inst.result {
        let result_ty = ctx.value_php_type(result)?;
        let return_ty = return_ty.codegen_repr();
        if return_ty == PhpType::Void || result_ty == PhpType::Void {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            if matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) {
                emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
            }
            ctx.store_result_value(result)?;
            return Ok(());
        }
        if matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) && return_ty != PhpType::Mixed {
            emit_box_current_value_as_mixed(ctx.emitter, &return_ty);
        }
        ctx.store_result_value(result)?;
    }
    Ok(())
}

/// Stores a resolved method call's result, honoring by-reference returns.
///
/// A by-reference-returning method hands back a single-word reference-cell pointer in the
/// integer result register (the method body's `Terminator::Return` placed it there), so the
/// result is stored single-word rather than split by the declared `Str`/`Float` return type.
pub(super) fn store_method_call_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: &MethodCallTarget,
) -> Result<()> {
    if target.by_ref_return {
        if let Some(result) = inst.result {
            ctx.store_int_result_value(result)?;
        }
        return Ok(());
    }
    store_call_result(ctx, inst, &target.return_ty)
}

/// Stores a dynamically dispatched Mixed-receiver result without retaining an owned return twice.
pub(super) fn store_mixed_method_call_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: &MethodCallTarget,
) -> Result<()> {
    if target.by_ref_return {
        return store_method_call_result(ctx, inst, target);
    }
    let Some(result) = inst.result else {
        return Ok(());
    };
    let result_ty = ctx.value_php_type(result)?.codegen_repr();
    let return_ty = target.return_ty.codegen_repr();
    if matches!(result_ty, PhpType::Mixed | PhpType::Union(_))
        && return_ty != PhpType::Mixed
        && return_ty.is_refcounted()
    {
        // Generated methods return an owned refcounted result (including acquired `$this`
        // aliases). Move that owner into the fresh Mixed cell. The ordinary boxer retains its
        // input and is reserved for borrowed results; using it here leaks one callee acquisition
        // per dynamic call.
        emit_box_current_owned_value_as_mixed(ctx.emitter, &return_ty);
        ctx.store_result_value(result)?;
        return Ok(());
    }
    store_call_result(ctx, inst, &target.return_ty)
}

/// Resolves an instruction data immediate as a method name.
pub(super) fn method_name_data<'a>(ctx: &'a FunctionContext<'_>, inst: &Instruction) -> Result<&'a str> {
    let data = expect_data(inst)?;
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}
