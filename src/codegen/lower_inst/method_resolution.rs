//! Purpose:
//! Resolves method targets and stores direct or dynamic call results.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;
use crate::codegen::emit::Emitter;

/// Returns whether a class's flattened method table declares the requested instance method.
pub(super) fn class_declares_method(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
) -> bool {
    let normalized = class_name.trim_start_matches('\\');
    let method_key = php_symbol_key(method_name);
    ctx.module
        .class_infos
        .get(normalized)
        .is_some_and(|class_info| class_info.methods.contains_key(&method_key))
}

/// Returns the class whose private declaration answers this call, when the receiver's own class
/// has none. See [`crate::types::private_scope`] for the rule and its measurement.
pub(super) fn lexical_private_method_scope(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
) -> Option<String> {
    crate::types::private_scope::lexical_private_method_scope(
        &ctx.module.class_infos,
        ctx.function.name.rsplit_once("::").map(|(scope, _)| scope),
        class_name,
        &php_symbol_key(method_name),
    )
}

/// Resolves method implementation class, canonical key, return type, and ABI arity.
pub(super) fn resolve_method_call_target(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
    operand_count: usize,
) -> Result<MethodCallTarget> {
    let normalized = lexical_private_method_scope(ctx, class_name, method_name)
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    let normalized = normalized.as_str();
    let class_info = ctx.module.class_infos.get(normalized).ok_or_else(|| {
        CodegenIrError::unsupported(format!("method call on unknown class {}", normalized))
    })?;
    let method_key = php_symbol_key(method_name);
    let callee_sig = class_info.methods.get(&method_key).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "method call to unknown method {}::{}",
            normalized, method_name
        ))
    })?;
    let physical_args = callee_sig.params.len() + 1;
    // php IGNORES arguments a userland function does not declare, and an override further down the
    // family may declare them. Each implementation is emitted with its own frame, so a surplus
    // operand simply occupies an argument register the base's body never reads and the override's
    // body does. Only a surplus that still fits the register-passed prefix is accepted; beyond that
    // the physical ABI spills, and a callee reading a slot the caller did not write is not the same
    // thing as ignoring an argument.
    const REGISTER_PASSED_ARGS: usize = 8;
    let tolerated_surplus = |count: usize, expected: usize| {
        count > expected && count <= REGISTER_PASSED_ARGS
    };
    let (call_sig, source_abi) = if operand_count == physical_args {
        (
            callee_sig.clone(),
            !crate::codegen_support::source_method_adapters::uses_physical_func_args_abi(
                callee_sig,
            ),
        )
    } else {
        let source = crate::codegen_support::source_method_adapters::source_visible_signature(
            callee_sig,
        )
        .map_err(CodegenIrError::unsupported)?;
        if operand_count == source.params.len() + 1 {
            (source, true)
        } else if tolerated_surplus(operand_count, physical_args) {
            (
                callee_sig.clone(),
                !crate::codegen_support::source_method_adapters::uses_physical_func_args_abi(
                    callee_sig,
                ),
            )
        } else {
            return Err(CodegenIrError::unsupported(format!(
                "method call to {}::{} with {} operands for {} physical or {} source ABI params",
                normalized,
                method_name,
                operand_count,
                physical_args,
                source.params.len() + 1
            )));
        }
    };
    if source_abi {
        let actual = class_info.methods.get(&method_key).expect("method signature exists");
        crate::codegen_support::source_method_adapters::plan_method_abi(&call_sig, actual)
            .map_err(CodegenIrError::unsupported)?;
    }
    let expected_args = call_sig.params.len() + 1;
    if operand_count < expected_args
        || (operand_count > expected_args && operand_count > REGISTER_PASSED_ARGS)
    {
        return Err(CodegenIrError::unsupported(format!(
            "method call to {}::{} has an unresolved ABI shape",
            normalized, method_name
        )));
    }
    let impl_class = class_info
        .method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| normalized.to_string());
    let dynamic_slot = class_info.vtable_slots.get(&method_key).copied();
    let has_direct_body = class_method_already_emitted(ctx, &impl_class, &method_key, false);
    if !has_direct_body && dynamic_slot.is_none() {
        return Err(CodegenIrError::unsupported(format!(
            "method call to {}::{} without an emitted EIR method body",
            impl_class, method_name
        )));
    }
    // Object-argument variance can change the instantiation without an inheritance edge.
    // Scalar arguments cannot widen in place and retain the ordinary final-class proof.
    let fixed_instantiation = crate::generics::dispatch::has_fixed_instantiation(normalized);
    // A final receiver with a fixed instantiation cannot select an override.
    // Keep the vtable fallback when this module has no directly callable EIR body.
    let dynamic_slot = if has_direct_body
        && fixed_instantiation
        && (class_info.is_final || class_info.final_methods.contains(&method_key))
    {
        None
    } else {
        dynamic_slot
    };
    // The RESULT is read the way the emitted body returns it. An inherited method's copy in the
    // receiver class can carry a narrower checker type than the body its ancestor emitted: a
    // `parent::__construct(fopen(...))` retypes the untyped property in the child's copy to a
    // resource (raw `int` storage) while `B::get()` still returns the boxed Mixed it was compiled
    // with, and re-boxing that pointer as an int printed `int(4316502720)`.
    let return_ty = ctx
        .module
        .class_infos
        .get(&impl_class)
        .and_then(|info| info.methods.get(&method_key))
        .map_or_else(|| callee_sig.return_type.clone(), |sig| sig.return_type.clone());
    Ok(MethodCallTarget {
        impl_class,
        method_key,
        dynamic_slot,
        params: call_sig
            .params
            .iter()
            .map(|(_, ty)| ty.codegen_repr())
            .collect(),
        ref_params: call_sig.ref_params.clone(),
        return_ty,
        by_ref_return: callee_sig.by_ref_return,
        source_abi,
    })
}

/// Reports whether literal guards need a runtime signature or declaring-class diagnostic.
///
/// Object-only variance preserves final literal parameters, but the actual generic
/// instantiation still supplies the class name in an argument TypeError.
pub(super) fn method_requires_runtime_literal_guards(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    target: &MethodCallTarget,
) -> bool {
    let normalized = class_name.trim_start_matches('\\');
    target.dynamic_slot.is_some()
        && (normalized.contains('<') || ctx.module.class_infos.get(normalized).is_none_or(|class| {
            !class.is_final && !class.final_methods.contains(&target.method_key)
        }))
}

/// Emits an indirect instance call through either the physical or source-ABI vtable.
pub(super) fn emit_dynamic_instance_method_call_with_abi(
    ctx: &mut FunctionContext<'_>,
    slot: usize,
    source_abi: bool,
) {
    let class_id_reg = abi::temp_int_reg(ctx.emitter.target);
    let dispatch_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_from_address(
        ctx.emitter,
        class_id_reg,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        0,
    );
    let table = if source_abi {
        "_class_source_vtable_ptrs"
    } else {
        "_class_vtable_ptrs"
    };
    abi::emit_symbol_address(ctx.emitter, dispatch_reg, table);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(                                   // load the class-specific instance-vtable pointer
                "ldr {}, [{}, {}, lsl #3]",
                dispatch_reg, dispatch_reg, class_id_reg
            ));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(                                   // load the class-specific instance-vtable pointer
                "mov {}, QWORD PTR [{} + {} * 8]",
                dispatch_reg, dispatch_reg, class_id_reg
            ));
        }
    }
    abi::emit_load_from_address(ctx.emitter, dispatch_reg, dispatch_reg, slot * 8);
    abi::emit_call_reg(ctx.emitter, dispatch_reg);
}

/// Calls a resolved target through the ABI selected from its caller-visible signature.
pub(super) fn emit_resolved_method_call(
    ctx: &mut FunctionContext<'_>,
    target: &MethodCallTarget,
) -> Result<()> {
    if let Some(slot) = target.dynamic_slot {
        emit_dynamic_instance_method_call_with_abi(ctx, slot, target.source_abi);
        return Ok(());
    }
    emit_direct_resolved_method_call(ctx, target)
}

/// Calls one statically selected implementation through its validated ABI entry.
pub(super) fn emit_direct_resolved_method_call(
    ctx: &mut FunctionContext<'_>,
    target: &MethodCallTarget,
) -> Result<()> {
    let symbol = if target.source_abi {
        let class_info = ctx.module.class_infos.get(&target.impl_class).ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "source method ABI target on unknown class {}",
                target.impl_class
            ))
        })?;
        let physical = class_info.methods.get(&target.method_key).ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "source method ABI target missing {}::{}",
                target.impl_class, target.method_key
            ))
        })?;
        if crate::codegen_support::source_method_adapters::uses_physical_func_args_abi(physical) {
            crate::codegen_support::source_method_adapters::source_method_entry_symbol(
                &target.impl_class,
                &target.method_key,
                physical,
                crate::codegen_support::source_method_adapters::MethodKind::Instance,
            )
            .map_err(CodegenIrError::unsupported)?
        } else {
            method_symbol(&target.impl_class, &target.method_key)
        }
    } else {
        method_symbol(&target.impl_class, &target.method_key)
    };
    abi::emit_call_label(ctx.emitter, &symbol);
    Ok(())
}

/// Validates an instance method's declared parameters before argument coercion.
pub(super) fn emit_method_literal_parameter_guards(
    ctx: &mut FunctionContext<'_>,
    target: &MethodCallTarget,
    operands: &[ValueId],
) -> Result<()> {
    let signature = ctx.module.class_infos.get(&target.impl_class)
        .and_then(|class| class.methods.get(&target.method_key))
        .ok_or_else(|| CodegenIrError::unsupported(format!(
            "missing declared method signature {}::{}", target.impl_class, target.method_key,
        )))?;
    let parameters = if target.source_abi
        && crate::codegen_support::source_method_adapters::uses_physical_func_args_abi(signature)
    {
        crate::codegen_support::source_method_adapters::source_visible_signature(signature)
            .map_err(CodegenIrError::unsupported)?.params
    } else {
        signature.params.clone()
    };
    super::direct_calls::emit_literal_declared_parameter_guards(
        ctx, operands, &parameters, &format!("{}::{}", target.impl_class, target.method_key),
    )
}

/// Returns true when the current EIR module includes the target class method body.
pub(super) fn class_method_already_emitted(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
    is_static: bool,
) -> bool {
    ctx.shared
        .emitted_method_contains(class_name, method_key, is_static)
}

/// Stores a call result, boxing concrete returns for generic EIR result slots.
pub(super) fn store_call_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    return_ty: &PhpType,
) -> Result<()> {
    if let Some(result) = inst.result {
        let result_ty = ctx.value_php_type(result)?;
        // A `?ptr<T>` result boxes by its declared type, so address 0 is PHP null.
        let box_ty = crate::types::mixed_box_php_type(return_ty);
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
            emit_box_call_result_as_mixed(ctx.emitter, &box_ty);
        }
        ctx.store_result_value(result)?;
    }
    Ok(())
}

/// Boxes a call's concrete result for a generic EIR slot.
///
/// The callee hands its caller an owned reference to a heap result, so the box takes that
/// reference over rather than retaining a second one: a method a narrowed receiver's
/// static type does not declare (`if ($shape instanceof Square) { $shape->grow() }`)
/// dispatches through a `Mixed` slot, and the retained copy kept the returned object
/// alive after the box was released, so it was never destructed. Strings keep the
/// copying box, since a returned string may be persistent data the box must not own.
fn emit_box_call_result_as_mixed(emitter: &mut Emitter, return_ty: &PhpType) {
    if matches!(
        return_ty,
        PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Callable
    ) {
        emit_box_current_owned_value_as_mixed(emitter, return_ty);
    } else {
        emit_box_current_value_as_mixed(emitter, return_ty);
    }
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
    // A PHP method returns its value OWNED. Boxing a container or object for a `mixed` call site
    // (every call on an untyped receiver dispatches this way) retained it for the box and kept the
    // call's own reference too: `$m->make()` on an untyped `$m` leaked what it returned, and that
    // object's destructor never ran. The box takes the call's reference instead. Strings keep the
    // ordinary path: a call's string result may live in the concat scratch buffer.
    if let Some(result) = inst.result {
        let result_ty = ctx.value_php_type(result)?;
        let return_ty = target.return_ty.codegen_repr();
        if matches!(result_ty, PhpType::Mixed | PhpType::Union(_))
            && matches!(
                return_ty,
                PhpType::Array(_)
                    | PhpType::AssocArray { .. }
                    | PhpType::Iterable
                    | PhpType::Object(_)
                    | PhpType::Callable
            )
        {
            crate::codegen::emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &return_ty,
            );
            return ctx.store_result_value(result);
        }
        // The box COPIES a string, so the call's own string stayed owned by nobody: every
        // string-returning method called on an untyped receiver leaked its result. Release it
        // after boxing, as the typed path's acquire-then-release does; the validating free skips
        // a scratch or static pointer, so a result that never left them is not freed.
        if matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) && return_ty == PhpType::Str {
            emit_box_call_string_releasing_original(ctx);
            return ctx.store_result_value(result);
        }
    }
    store_call_result(ctx, inst, &target.return_ty)
}

/// Turns a direct method call's result into the owned boxed Mixed cell a `Mixed` ladder arm
/// publishes, whatever the callee's declared return representation.
///
/// This is the by-value half of `store_method_call_result` for a site whose consumer is a
/// `Mixed` slot: a method returns its value OWNED, so an object or container is moved into the
/// box, a string is boxed and the call's own copy dropped, and a scalar is simply boxed. A callee
/// that already returns a boxed `Mixed` needs nothing.
pub(super) fn box_method_result_as_mixed(ctx: &mut FunctionContext<'_>, return_ty: &PhpType) {
    let return_ty = return_ty.codegen_repr();
    match return_ty {
        PhpType::Mixed | PhpType::Union(_) => {}
        PhpType::Void => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
        }
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Iterable
        | PhpType::Object(_)
        | PhpType::Callable => {
            crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, &return_ty);
        }
        PhpType::Str => emit_box_call_string_releasing_original(ctx),
        other => emit_box_current_value_as_mixed(ctx.emitter, &other),
    }
}

/// Boxes the string call result in the result registers and releases the call's own copy.
fn emit_box_call_string_releasing_original(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x1, [sp, #-16]!");                     // keep the call's string pointer across boxing
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction("ldr x9, [sp]");                            // recover the call's string pointer
            ctx.emitter.instruction("str x0, [sp]");                            // keep the box across the release
            ctx.emitter.instruction("mov x0, x9");                              // pass the call's string to the validating free
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
            ctx.emitter.instruction("ldr x0, [sp], #16");                       // the box is the call's Mixed result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // keep the call's string pointer across boxing
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // the string pointer is the first result word
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // recover the call's string pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // keep the box across the release
            ctx.emitter.instruction("mov rax, r10");                            // pass the call's string to the validating free
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                // the box is the call's Mixed result
            ctx.emitter.instruction("add rsp, 16");                             // discard the spill slot
        }
    }
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
