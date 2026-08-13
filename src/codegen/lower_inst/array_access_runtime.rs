//! Purpose:
//! Lowers ArrayAccess and typed runtime fallback dispatch.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers high-level runtime fallback casts that Phase 04 can identify by type.
pub(super) fn lower_runtime_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if let Some(Immediate::RuntimeCall(target)) = inst.immediate {
        return runtime_calls::lower(ctx, inst, target);
    }
    if inst.operands.len() == 3 && matches!(inst.immediate, Some(Immediate::Data(_))) {
        return lower_property_array_runtime_set(ctx, inst);
    }
    if let Some(()) = try_lower_array_access_runtime_call(ctx, inst)? {
        return Ok(());
    }
    if inst.operands.len() == 3 {
        if inst.result_php_type.codegen_repr() != PhpType::Void {
            return lower_mixed_array_runtime_get(ctx, inst, false);
        }
        return lower_mixed_array_runtime_set(ctx, inst);
    }
    if inst.operands.len() == 2 {
        return lower_binary_runtime_call(ctx, inst);
    }
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::unsupported(format!(
            "runtime_call with {} operands returning PHP type {:?}",
            inst.operands.len(),
            inst.result_php_type
        )));
    }
    let value = expect_operand(inst, 0)?;
    let source_ty = ctx.value_php_type(value)?.codegen_repr();
    if let (PhpType::Object(class_name), PhpType::Str) =
        (&source_ty, inst.result_php_type.codegen_repr())
    {
        let normalized = class_name.trim_start_matches('\\');
        if !object_class_has_tostring(ctx, normalized) {
            emit_missing_tostring_fatal(ctx, normalized);
            return Ok(());
        }
        emit_object_tostring_call(ctx, value, normalized)?;
        return store_if_result(ctx, inst);
    }
    if inst.result_php_type.codegen_repr() == PhpType::Iterable
        && matches!(
            source_ty,
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) | PhpType::Iterable
        )
    {
        ctx.load_value_to_result(value)?;
        return store_if_result(ctx, inst);
    }
    if inst.result_php_type.codegen_repr() == PhpType::TaggedScalar {
        match source_ty {
            PhpType::Int | PhpType::Bool | PhpType::Callable => {
                ctx.load_value_to_result(value)?;
                crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
                return store_if_result(ctx, inst);
            }
            PhpType::Void | PhpType::Never => {
                crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
                return store_if_result(ctx, inst);
            }
            other => {
                return Err(CodegenIrError::unsupported(format!(
                    "runtime_call from PHP type {:?} to PHP type TaggedScalar",
                    other
                )))
            }
        }
    }
    if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
        let result_ty = inst.result_php_type.codegen_repr();
        load_value_to_first_int_arg(ctx, value)?;
        match result_ty {
            PhpType::Str => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string"),
            PhpType::Float => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float"),
            PhpType::Int => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int"),
            PhpType::Bool => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool"),
            PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed => {
                lower_mixed_to_mixed_indexed_array(ctx)?;
            }
            PhpType::AssocArray { value, .. } if value.codegen_repr() == PhpType::Mixed => {
                lower_mixed_to_mixed_assoc_array(ctx)?;
            }
            PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Callable
            | PhpType::Iterable
            | PhpType::Object(_) => {
                emit_unbox_mixed_to_owned_refcounted_result(ctx, &result_ty);
            }
            other => {
                return Err(CodegenIrError::unsupported(format!(
                    "runtime_call from PHP type {:?} to PHP type {:?}",
                    source_ty, other
                )))
            }
        }
        return store_if_result(ctx, inst);
    }
    Err(CodegenIrError::unsupported(format!(
        "runtime_call from PHP type {:?} to PHP type {:?}",
        source_ty, inst.result_php_type
    )))
}

/// Lowers generic EIR runtime calls that represent PHP `ArrayAccess` object indexing.
///
/// Subscript reads carry a trailing warn-on-missing flag that only the boxed-`Mixed`
/// runtime reader consumes, so operand count alone no longer separates a read from
/// `offsetSet`. Reads are identified structurally instead: subscript writes lower
/// through `emit_void` and carry no result value, while reads always produce one.
/// The flag operand is stripped before dispatch so `offsetGet` keeps its
/// single-argument PHP signature.
pub(super) fn try_lower_array_access_runtime_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<Option<()>> {
    let Some(receiver) = inst.operands.first().copied() else {
        return Ok(None);
    };
    let receiver_ty = ctx.raw_value_php_type(receiver)?;
    let Some(dispatch) = array_access_runtime_dispatch(ctx, &receiver_ty) else {
        return Ok(None);
    };
    // A result value marks a subscript read: `$obj[$k] = v` and `$obj[] = v` lower
    // through `emit_void`. Keying off the result PHP type instead would misread a
    // read whose declared `offsetGet` return type has no runtime representation.
    let is_read = inst.result.is_some();
    let method_name = match inst.operands.len() {
        2 if is_read => "offsetGet",
        2 => "append",
        3 if is_read => "offsetGet",
        3 => "offsetSet",
        _ => return Ok(None),
    };
    // Drop the read's warn-on-missing operand before argument materialization:
    // `offsetGet($offset)` takes one argument, and the shared method-call
    // lowerers resolve arity straight from `inst.operands`.
    let read_without_warning_flag;
    let inst = if is_read && inst.operands.len() == 3 {
        read_without_warning_flag = Instruction {
            operands: inst.operands[..2].to_vec(),
            ..inst.clone()
        };
        &read_without_warning_flag
    } else {
        inst
    };
    match dispatch {
        ArrayAccessRuntimeDispatch::Concrete(class_name) => {
            let concrete_method =
                if method_name == "append" && is_spl_doubly_linked_list_family(&class_name) {
                    "push"
                } else {
                    method_name
                };
            if let Some(intrinsic) = runtime_backed_instance_intrinsic(&class_name, concrete_method)
            {
                lower_instance_runtime_intrinsic(
                    ctx,
                    inst,
                    &class_name,
                    concrete_method,
                    intrinsic,
                )?;
            } else {
                lower_runtime_object_method_call(ctx, inst, &class_name, concrete_method)?;
            }
        }
        ArrayAccessRuntimeDispatch::Interface {
            boxed_receiver: false,
        } => {
            lower_interface_method_call(ctx, inst, "ArrayAccess", method_name)?;
        }
        ArrayAccessRuntimeDispatch::Interface {
            boxed_receiver: true,
        } => {
            lower_boxed_array_access_interface_call(ctx, inst, method_name)?;
        }
    }
    Ok(Some(()))
}

/// Returns true when a concrete class uses the SPL doubly-linked-list append helper.
pub(super) fn is_spl_doubly_linked_list_family(class_name: &str) -> bool {
    matches!(class_name, "SplDoublyLinkedList" | "SplStack" | "SplQueue")
}

/// Selects the ArrayAccess runtime dispatch strategy for a receiver type.
pub(super) fn array_access_runtime_dispatch(
    ctx: &FunctionContext<'_>,
    receiver_ty: &PhpType,
) -> Option<ArrayAccessRuntimeDispatch> {
    match receiver_ty {
        PhpType::Object(class_name) => {
            let normalized = class_name.trim_start_matches('\\');
            if interface_satisfies_interface(ctx, normalized, "ArrayAccess") {
                return Some(ArrayAccessRuntimeDispatch::Interface {
                    boxed_receiver: false,
                });
            }
            if class_implements_interface(ctx, normalized, "ArrayAccess") {
                return Some(ArrayAccessRuntimeDispatch::Concrete(normalized.to_string()));
            }
            None
        }
        PhpType::Union(members) if union_satisfies_array_access(ctx, members) => {
            Some(ArrayAccessRuntimeDispatch::Interface {
                boxed_receiver: true,
            })
        }
        _ => None,
    }
}

/// Returns true when all non-null union arms are ArrayAccess-compatible objects.
pub(super) fn union_satisfies_array_access(ctx: &FunctionContext<'_>, members: &[PhpType]) -> bool {
    let mut saw_object = false;
    for member in members {
        match member {
            PhpType::Void | PhpType::Never => {}
            PhpType::Object(class_name) => {
                if !object_name_satisfies_interface(ctx, class_name, "ArrayAccess") {
                    return false;
                }
                saw_object = true;
            }
            _ => return false,
        }
    }
    saw_object
}

/// Returns true when a class or interface name satisfies the requested interface.
pub(super) fn object_name_satisfies_interface(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    interface_name: &str,
) -> bool {
    let normalized = class_name.trim_start_matches('\\');
    interface_satisfies_interface(ctx, normalized, interface_name)
        || class_implements_interface(ctx, normalized, interface_name)
}

/// Lowers ArrayAccess on a boxed union receiver through runtime interface metadata.
pub(super) fn lower_boxed_array_access_interface_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    method_name: &str,
) -> Result<()> {
    let (interface_name, method_key, callee_sig) =
        resolve_interface_call_signature(ctx, "ArrayAccess", method_name, inst.operands.len())?;
    let receiver = expect_operand(inst, 0)?;
    let receiver_ty = PhpType::Object(interface_name.clone());
    let mut param_types = Vec::with_capacity(callee_sig.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(callee_sig.params.iter().map(|(_, ty)| ty.codegen_repr()));
    let mut ref_params = Vec::with_capacity(callee_sig.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(callee_sig.ref_params.iter().copied());

    ctx.load_value_to_result(receiver)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, mixed_unbox_low_payload_reg(ctx));
    abi::emit_pop_reg(ctx.emitter, receiver_reg);
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &receiver_ty,
        &inst.operands,
        &param_types,
        &ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    let return_ty =
        iterators::emit_interface_dispatch_call(ctx, &interface_name, &method_key, None)?;
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &return_ty)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Emits the concrete method body backing a PHP object runtime fallback.
pub(in crate::codegen) fn lower_runtime_object_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    method_name: &str,
) -> Result<()> {
    let target = resolve_method_call_target(ctx, class_name, method_name, inst.operands.len())?;
    let mut param_types = Vec::with_capacity(target.params.len() + 1);
    param_types.push(PhpType::Object(class_name.to_string()));
    param_types.extend(target.params.iter().map(|param| param.codegen_repr()));
    let mut ref_params = Vec::with_capacity(target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(target.ref_params.iter().copied());
    let call_args = materialize_direct_call_args_with_refs_and_options(
        ctx,
        &inst.operands,
        &param_types,
        &ref_params,
        true,
        RefArgCellLifetime::CallOnly,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(
        ctx.emitter,
        &method_symbol(&target.impl_class, &target.method_key),
    );
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_runtime_object_call_result(ctx, inst, &target.return_ty)?;
    emit_call_arg_temp_cleanups(ctx, &call_args, inst.result)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Stores an object fallback call result, casting boxed Mixed values when the access type is known.
pub(super) fn store_runtime_object_call_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    return_ty: &PhpType,
) -> Result<()> {
    if return_ty.codegen_repr() != PhpType::Mixed {
        return store_call_result(ctx, inst, return_ty);
    }
    let Some(result) = inst.result else {
        return Ok(());
    };
    let result_ty = ctx.value_php_type(result)?.codegen_repr();
    if matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) {
        ctx.store_result_value(result)?;
        return Ok(());
    }
    cast_loaded_mixed_pointer_to_result(ctx, &result_ty)?;
    ctx.store_result_value(result)
}

/// Returns true when a class implements an interface, following parent classes if needed.
pub(super) fn class_implements_interface(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    interface_name: &str,
) -> bool {
    let interface_key = php_symbol_key(interface_name.trim_start_matches('\\'));
    let mut current = Some(class_name.trim_start_matches('\\'));
    while let Some(candidate) = current {
        let Some(info) = ctx.module.class_infos.get(candidate) else {
            return false;
        };
        if info.interfaces.iter().any(|interface| {
            let interface = interface.trim_start_matches('\\');
            php_symbol_key(interface) == interface_key
                || interface_satisfies_interface(ctx, interface, interface_name)
        }) {
            return true;
        }
        current = info.parent.as_deref();
    }
    false
}

/// Returns true when an interface is or extends the requested ancestor.
pub(super) fn interface_satisfies_interface(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    ancestor_name: &str,
) -> bool {
    if php_symbol_key(interface_name.trim_start_matches('\\'))
        == php_symbol_key(ancestor_name.trim_start_matches('\\'))
    {
        return true;
    }
    let Some(interface_info) = ctx
        .module
        .interface_infos
        .get(interface_name.trim_start_matches('\\'))
    else {
        return false;
    };
    interface_info.parents.iter().any(|parent| {
        let parent = parent.trim_start_matches('\\');
        php_symbol_key(parent) == php_symbol_key(ancestor_name.trim_start_matches('\\'))
            || interface_satisfies_interface(ctx, parent, ancestor_name)
    })
}

/// Converts an untyped boxed Mixed payload into indexed-array storage with Mixed slots.
pub(super) fn lower_mixed_to_mixed_indexed_array(ctx: &mut FunctionContext<'_>) -> Result<()> {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed indexed-array payload to the Mixed conversion helper
            ctx.emitter.instruction("ldr x1, [x0, #-8]");                       // load indexed-array metadata before Mixed-slot conversion
            ctx.emitter.instruction("lsr x1, x1, #8");                          // move the runtime value_type tag into the low bits
            ctx.emitter.instruction("and x1, x1, #0x7f");                       // isolate the indexed-array value_type tag
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");            // load indexed-array metadata before Mixed-slot conversion
            ctx.emitter.instruction("shr rsi, 8");                              // move the runtime value_type tag into the low bits
            ctx.emitter.instruction("and rsi, 0x7f");                           // isolate the indexed-array value_type tag
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
    Ok(())
}

/// Converts an untyped boxed Mixed payload into associative-array storage with Mixed values.
pub(super) fn lower_mixed_to_mixed_assoc_array(ctx: &mut FunctionContext<'_>) -> Result<()> {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed associative-array payload to the Mixed conversion helper
        }
        Arch::X86_64 => {}
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
    abi::emit_incref_if_refcounted(
        ctx.emitter,
        &PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
    );
    Ok(())
}
