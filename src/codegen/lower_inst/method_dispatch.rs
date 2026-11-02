//! Purpose:
//! Lowers direct and boxed-Mixed instance method dispatch.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Reports whether the receiver's static class or interface declares the method, or a class
/// handles undeclared calls through `__call`.
///
/// A narrowed receiver keeps its declared type in EIR, so a method only its narrowed class has
/// is absent here and must dispatch on the runtime class instead.
fn static_receiver_declares_method(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
) -> bool {
    let normalized = class_name.trim_start_matches('\\');
    let method_key = php_symbol_key(method_name);
    // `$object->staticMethod()` is a valid call, so a static method counts as declared too.
    if let Some(interface_info) = ctx.module.interface_infos.get(normalized) {
        return interface_info.methods.contains_key(&method_key)
            || interface_info.static_methods.contains_key(&method_key);
    }
    let Some(mut class_info) = ctx.module.class_infos.get(normalized) else {
        return true;
    };
    // A private method an ancestor declares is not copied into a subclass's table, yet a call
    // from that ancestor's own code reaches it on a subclass receiver (Symfony's
    // `CompiledUrlMatcherTrait::match()` calls its private `doMatch()` on a
    // `RedirectableCompiledUrlMatcher` once `$this` is narrowed), so the ancestors count too.
    let mut seen = std::collections::HashSet::new();
    loop {
        if class_info.methods.contains_key(&method_key)
            || class_info.static_methods.contains_key(&method_key)
            || class_info.methods.contains_key(&php_symbol_key("__call"))
        {
            return true;
        }
        let Some(parent) = class_info.parent.as_deref() else {
            return false;
        };
        let parent = parent.trim_start_matches('\\');
        if !seen.insert(parent.to_string()) {
            return false;
        }
        match ctx.module.class_infos.get(parent) {
            Some(info) => class_info = info,
            None => return false,
        }
    }
}

/// Lowers a direct instance-method call on a statically known object receiver.
///
/// Overridable methods check literal `false` and `null` parameters on the runtime
/// implementation. Generic final declarations also select the runtime instantiation's
/// diagnostic name, even though object-only variance preserves literal parameter types.
pub(super) fn lower_method_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    // The shared untyped-receiver helper holds only the native dispatch: the site already asked
    // the eval owner (and kept its scope), so the helper must not probe again.
    if inside_mixed_method_call_helper(ctx) {
        return lower_native_method_call(ctx, inst);
    }
    // The shared owner-call helper holds only the interpreter-declared slow path.
    if builtins::inside_owner_call_helper(ctx) {
        return builtins::lower_owner_call_helper_body(ctx, inst);
    }
    if !ctx.module.required_runtime_features.eval_bridge {
        return lower_native_method_call(ctx, inst);
    }
    let object = expect_operand(inst, 0)?;
    let method_name = method_name_data(ctx, inst)?.to_string();
    // `Class::method` is a call pinned to that implementation: nothing to probe or dispatch on.
    if method_name.contains("::") {
        return lower_native_method_call(ctx, inst);
    }
    let class_name = objects::nullable_object_receiver_class(ctx, object)?
        .map(|(name, _)| name);
    let statically_bound = class_name.as_ref()
        .and_then(|name| ctx.module.class_infos.get(name.trim_start_matches('\\')))
        .is_some_and(|info| {
            let key = php_symbol_key(&method_name);
            info.final_methods.contains(&key)
                || matches!(info.method_visibilities.get(&key),
                    Some(crate::parser::ast::Visibility::Private))
        });
    if statically_bound {
        return lower_native_method_call(ctx, inst);
    }
    // Dynamic ownership belongs to the receiver, not to the caller's eval state.
    // Probe before the typed, nullable, interface and Mixed native dispatch split.
    let native_label = ctx.next_label("method_native_dispatch");
    let done_label = ctx.next_label("method_dynamic_dispatch_done");
    builtins::lower_eval_owned_method_call(
        ctx, inst, object, &method_name, &native_label, &done_label,
    )?;
    ctx.emitter.label(&native_label);
    lower_native_method_call(ctx, inst)?;
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Dispatches a receiver after dynamic ownership has been ruled out.
fn lower_native_method_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let method_name = method_name_data(ctx, inst)?.to_string();
    if let Some((scope_class, plain_method)) = method_name.split_once("::") {
        return lower_scoped_direct_method_call(ctx, inst, scope_class, plain_method);
    }
    if builtins::has_eval_context(ctx)
        && boxed_reflection_receiver_needs_eval_bridge(ctx, object, &method_name)?
    {
        return builtins::lower_eval_method_call(ctx, inst, object, &method_name);
    }
    if let Some((class_name, true)) = objects::nullable_object_receiver_class(ctx, object)? {
        if !class_name.is_empty() {
            if !static_receiver_declares_method(ctx, &class_name, &method_name) {
                // A narrowed nullable receiver (`if ($this->shape instanceof Square)`): dispatch
                // on the runtime class, which still reports a call on null.
                return lower_mixed_method_call(ctx, inst, object, &method_name);
            }
            return lower_nullable_receiver_method_call(ctx, inst, object, &class_name, &method_name);
        }
    }
    let object_ty = ctx.value_php_type(object)?.codegen_repr();
    if matches!(&object_ty, PhpType::Object(name) if name.is_empty()) {
        return lower_mixed_method_call(ctx, inst, object, &method_name);
    }
    if matches!(object_ty, PhpType::Mixed | PhpType::Union(_)) {
        if let Some(state) = fiber_state_predicate_method(&method_name) {
            return lower_mixed_maybe_fiber_state_predicate(ctx, inst, object, &method_name, state);
        }
        if let Some(method) = mixed_fiber_method(&method_name) {
            return lower_mixed_maybe_fiber_method(ctx, inst, object, &method_name, method);
        }
        return lower_mixed_method_call(ctx, inst, object, &method_name);
    }
    let PhpType::Object(class_name) = object_ty else {
        return Err(CodegenIrError::unsupported(format!(
            "method call receiver for PHP type {:?}",
            object_ty
        )));
    };
    guard_static_method_receiver(ctx, object, &method_name)?;
    if builtins::has_eval_context(ctx)
        && reflection_function_callable_metadata_method(&class_name, &method_name)
    {
        return builtins::lower_eval_method_call(ctx, inst, object, &method_name);
    }
    if builtins::has_eval_context(ctx) && reflection_class_runtime_metadata_method(&class_name) {
        if !class_reflector_names_a_class(&class_name) {
            return builtins::lower_eval_method_call(ctx, inst, object, &method_name);
        }
        let native_label = ctx.next_label("reflector_native_dispatch");
        let done_label = ctx.next_label("reflector_dispatch_done");
        emit_reflector_is_complete_probe(ctx, object, &class_name, &native_label)?;
        builtins::lower_eval_method_call(ctx, inst, object, &method_name)?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&native_label);
        lower_typed_native_method_call(ctx, inst, object, class_name, &method_name)?;
        ctx.emitter.label(&done_label);
        return Ok(());
    }
    lower_typed_native_method_call(ctx, inst, object, class_name, &method_name)
}

/// Dispatches a statically typed object receiver once the eval bridge has been ruled out.
fn lower_typed_native_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    class_name: String,
    method_name: &str,
) -> Result<()> {
    let method_name = method_name.to_string();
    if let Some(state) = fiber_state_predicate(&class_name, &method_name) {
        return lower_fiber_state_predicate(ctx, inst, object, state);
    }
    if let Some(intrinsic) = generator_intrinsic(&class_name, &method_name) {
        return lower_generator_intrinsic(ctx, inst, intrinsic);
    }
    if let Some(intrinsic) = callback_filter_intrinsic(&class_name, &method_name) {
        return lower_callback_filter_accept_intrinsic(ctx, inst, intrinsic);
    }
    if is_fiber_start_call(&class_name, &method_name) {
        return lower_fiber_start(ctx, inst, object);
    }
    if is_fiber_resume_call(&class_name, &method_name) {
        return lower_fiber_resume(ctx, inst, object);
    }
    if is_fiber_throw_call(&class_name, &method_name) {
        return lower_fiber_throw(ctx, inst, object);
    }
    if is_fiber_get_return_call(&class_name, &method_name) {
        return lower_fiber_noarg_runtime_method(ctx, inst, object, "__rt_fiber_get_return");
    }
    if let Some(intrinsic) = runtime_backed_instance_intrinsic(&class_name, &method_name) {
        return lower_instance_runtime_intrinsic(ctx, inst, &class_name, &method_name, intrinsic);
    }
    if is_throwable_standard_method_call(ctx, &class_name, &method_name) {
        return lower_throwable_standard_method(ctx, inst, object, &method_name);
    }
    if !static_receiver_declares_method(ctx, &class_name, &method_name) {
        // The checker accepted a method the receiver's static type does not declare, so the
        // receiver was narrowed (`if ($shape instanceof Square)`). Lowering keeps the slot's
        // declared type, so dispatch on the runtime class, as for an untyped receiver.
        return lower_mixed_method_call(ctx, inst, object, &method_name);
    }
    if ctx
        .module
        .interface_infos
        .contains_key(class_name.trim_start_matches('\\'))
    {
        return lower_interface_method_call(ctx, inst, &class_name, &method_name);
    }
    let normalized_class = class_name.trim_start_matches('\\');
    if normalized_class.is_empty() {
        return lower_narrowed_interface_method_call(ctx, inst, "", &method_name);
    }
    if !ctx.module.class_infos.contains_key(normalized_class)
        && !ctx.module.interface_infos.contains_key(normalized_class)
        && !ctx.module.extern_class_infos.contains_key(normalized_class)
        && !ctx.module.packed_class_infos.contains_key(normalized_class)
    {
        exceptions::emit_error(ctx, &format!("Class \"{}\" not found", normalized_class));
        return Ok(());
    }
    if !class_declares_method(ctx, &class_name, &method_name)
        && !narrowed_interface_candidates(ctx, normalized_class, &method_name, inst.operands.len())?
            .is_empty()
    {
        return lower_narrowed_interface_method_call(
            ctx,
            inst,
            normalized_class,
            &method_name,
        );
    }
    // Neither the class nor a descendant declares the method, and nothing intercepts it: php
    // throws when the call runs. Doctrine's `EntityManager::__construct()` guards
    // `$conn->getEventManager()` with `method_exists()`, which DBAL 4's `Connection` fails, and
    // refusing the call here interpreted the entity manager.
    if !class_declares_method(ctx, &class_name, &method_name)
        && !class_declares_method(ctx, &class_name, "__call")
        && super::method_resolution::lexical_private_method_scope(ctx, &class_name, &method_name).is_none()
        && ctx.module.class_infos.contains_key(normalized_class)
    {
        exceptions::emit_error(
            ctx,
            &format!("Call to undefined method {}::{}()", normalized_class, method_name),
        );
        return Ok(());
    }
    let target = resolve_method_call_target(ctx, &class_name, &method_name, inst.operands.len())?;
    if super::method_resolution::method_requires_runtime_literal_guards(ctx, &class_name, &target) {
        // An override may widen `false` or `null`. The vtable selects that body, so the
        // guard has to read the runtime class rather than the static base signature.
        let receiver_reg = abi::nested_call_reg(ctx.emitter);
        ctx.load_value_to_reg(object, receiver_reg)?;
        super::method_intrinsics::emit_hierarchy_literal_parameter_guards(
            ctx,
            inst,
            &class_name,
            &method_name,
            receiver_reg,
        )?;
    } else {
        super::method_resolution::emit_method_literal_parameter_guards(
            ctx, &target, &inst.operands[1..],
        )?;
    }
    emit_instance_call_to_target(ctx, inst, class_name, &method_name, target)
}

/// Lowers `Class::method` on an object receiver as a NON-virtual call of that class's method.
///
/// This is `ReflectionMethod::invoke()` of an ancestor's method on a descendant receiver: php
/// binds the reflected declaring class's body, so an override in the receiver's class must not
/// run. The receiver pointer is passed as `$this` unchanged.
fn lower_scoped_direct_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    scope_class: &str,
    method_name: &str,
) -> Result<()> {
    let mut target = resolve_method_call_target(ctx, scope_class, method_name, inst.operands.len())?;
    target.dynamic_slot = None;
    // A non-virtual call binds this exact body, so its own signature is the one to guard.
    super::method_resolution::emit_method_literal_parameter_guards(
        ctx, &target, &inst.operands[1..],
    )?;
    emit_instance_call_to_target(ctx, inst, scope_class.to_string(), method_name, target)
}

/// Materializes the arguments of an instance call and emits it against a resolved target.
fn emit_instance_call_to_target(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: String,
    method_name: &str,
    target: MethodCallTarget,
) -> Result<()> {
    let method_name = method_name.to_string();
    let mut param_types = Vec::with_capacity(inst.operands.len());
    param_types.push(PhpType::Object(class_name));
    param_types.extend(target.params.iter().map(|param| param.codegen_repr()));
    let mut ref_params = Vec::with_capacity(inst.operands.len());
    ref_params.push(false);
    ref_params.extend(target.ref_params.iter().copied());
    // Operands past the resolved signature are arguments php would IGNORE here and an override
    // further down the family declares. `resolve_method_call_target` has already limited them to
    // the register-passed prefix; each keeps its own type so the argument register is written with
    // the value the override reads, and the base's body simply never looks at it.
    while param_types.len() < inst.operands.len() {
        let surplus = inst.operands[param_types.len()];
        param_types.push(ctx.value_php_type(surplus)?.codegen_repr());
        ref_params.push(false);
    }
    let call_args = materialize_direct_call_args_with_refs_and_options(
        ctx,
        &inst.operands,
        &param_types,
        &ref_params,
        true,
        crate::codegen::lower_inst::RefArgCellLifetime::for_callee(ctx, &method_name),
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    emit_resolved_method_call(ctx, &target)?;
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_method_call_result(ctx, inst, &target)?;
    // User callees return an owner independent of the caller's materialized Mixed arguments.
    emit_call_arg_temp_cleanups(ctx, &call_args, None)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Returns whether a ReflectionFunction method depends on retained callable metadata.
fn reflection_function_callable_metadata_method(class_name: &str, method_name: &str) -> bool {
    class_name.trim_start_matches('\\') == "ReflectionFunction"
        && matches!(
            php_symbol_key(method_name).as_str(),
            "isanonymous"
                | "isstatic"
                | "isclosure"
                | "getclosurethis"
                | "getclosurescopeclass"
                | "getclosurecalledclass"
                | "getclosureusedvariables"
        )
}

/// Returns whether a reflector class's `__name` slot names a class-like (not an attribute).
fn class_reflector_names_a_class(class_name: &str) -> bool {
    matches!(
        class_name.trim_start_matches('\\'),
        "ReflectionClass" | "ReflectionObject" | "ReflectionEnum"
    )
}

/// Branches to `native_label` when a class reflector was fully materialized by compiled code.
///
/// Every call on a `ReflectionClass`/`ReflectionObject`/`ReflectionEnum` receiver in a function
/// that holds an eval context went to the interpreter, because a reflector the bridge built, or a
/// shallow one, leaves slots empty that the native bodies read (the bridge fills the name arrays,
/// never the member objects). A reflector the full materializer built has them all, and says so
/// in `__complete`. Sending it through the interpreter cost a bridge crossing per call and used
/// the interpreter's view of the class -- `(new ReflectionClass(Suit::class))->isEnum()` answered
/// false there.
fn emit_reflector_is_complete_probe(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    class_name: &str,
    native_label: &str,
) -> Result<()> {
    let flag_offset = ctx
        .module
        .class_infos
        .get(class_name.trim_start_matches('\\'))
        .and_then(|info| info.property_offsets.get("__complete").copied())
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, object_reg)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, flag_offset);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, native_label);
    Ok(())
}

/// Returns true when runtime evaluation must own a reflection metadata method call.
fn reflection_class_runtime_metadata_method(class_name: &str) -> bool {
    matches!(
        class_name.trim_start_matches('\\'),
        "ReflectionAttribute" | "ReflectionClass" | "ReflectionObject" | "ReflectionEnum"
    )
}

/// Returns whether a boxed receiver carries a reflector whose metadata only the eval bridge holds.
///
/// The routing below reads one statically known class, so a receiver a narrowing left as a union
/// never reached it: `?ReflectionClass` from a nullable return, or the `ReflectionClass|
/// ReflectionMethod` a reassignment produces, is answered first by the nullable and boxed-receiver
/// paths. What those reach is the synthesized reflection body, which reads generated AOT class
/// metadata — empty for a class only the interpreter declared — so the same reflector that
/// reported its name and `hasMethod()` correctly from its own slots listed no methods at all and
/// threw "does not exist" from `getMethod()`. Requiring every arm to be a built-in reflector keeps
/// this to reflection receivers, and the bridge hands back a call it does not own to native
/// dispatch, so the arms it does not answer for keep their behaviour.
fn boxed_reflection_receiver_needs_eval_bridge(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    method_name: &str,
) -> Result<bool> {
    let PhpType::Union(members) = objects::raw_value_php_type(ctx, object)? else {
        return Ok(false);
    };
    let mut routed = false;
    for member in members {
        match member {
            PhpType::Void | PhpType::False => {}
            PhpType::Object(class_name) => {
                if !builtin_reflection_class_name(&class_name) {
                    return Ok(false);
                }
                routed |= reflection_class_runtime_metadata_method(&class_name)
                    || reflection_function_callable_metadata_method(&class_name, method_name);
            }
            _ => return Ok(false),
        }
    }
    Ok(routed)
}

/// Returns whether a class name is one of the reflection classes the compiler itself declares.
fn builtin_reflection_class_name(class_name: &str) -> bool {
    matches!(
        class_name.trim_start_matches('\\'),
        "ReflectionAttribute"
            | "ReflectionClass"
            | "ReflectionClassConstant"
            | "ReflectionEnum"
            | "ReflectionEnumBackedCase"
            | "ReflectionEnumUnitCase"
            | "ReflectionFunction"
            | "ReflectionFunctionAbstract"
            | "ReflectionIntersectionType"
            | "ReflectionMethod"
            | "ReflectionNamedType"
            | "ReflectionObject"
            | "ReflectionParameter"
            | "ReflectionProperty"
            | "ReflectionReference"
            | "ReflectionType"
            | "ReflectionUnionType"
    )
}

/// Rejects the raw null-container representation before a static object method dispatch.
pub(super) fn guard_static_method_receiver(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    let receiver_reg = abi::symbol_scratch_reg(ctx.emitter);
    let scratch_reg = abi::secondary_scratch_reg(ctx.emitter);
    let null_label = ctx.next_label("static_method_receiver_null");
    let done_label = ctx.next_label("static_method_receiver_checked");
    ctx.load_value_to_reg(object, receiver_reg)?;
    crate::codegen::sentinels::emit_branch_if_null_container(
        ctx.emitter,
        receiver_reg,
        scratch_reg,
        &null_label,
    );
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    if crate::codegen::shared_null_method_trace::emit_shared_null_method_trace(
        ctx,
        method_name,
        Some(object),
    )? {
        // The shared helper ran both traces (raw, then plain); only the error is left.
        super::method_intrinsics::emit_method_call_on_null_error(ctx, method_name, false);
    } else {
        emit_aot_raw_null_method_receiver_trace(ctx, object, method_name)?;
        emit_method_call_on_null_fatal(ctx, method_name);
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the raw receiver word for a statically dispatched null-method fatal.
///
/// The value is only observed on the error path and is passed after the ordinary
/// diagnostic fields, so it cannot affect ABI argument materialization or a
/// successful method call.
fn emit_aot_raw_null_method_receiver_trace(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    if !ctx.module.required_runtime_features.eval_bridge {
        return Ok(());
    }
    let (function_label, function_len) = ctx.data.add_string(ctx.function.name.as_bytes());
    let (method_label, method_len) = ctx.data.add_string(method_name.as_bytes());
    let function_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_symbol_address(ctx.emitter, function_ptr_arg, &function_label);
    let function_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_int_immediate(ctx.emitter, function_len_arg, function_len as i64);
    let method_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, method_ptr_arg, &method_label);
    let method_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_load_int_immediate(ctx.emitter, method_len_arg, method_len as i64);
    let line_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    let line = ctx
        .current_instruction_span()
        .map(|span| span.source_line() as i64)
        .unwrap_or_default();
    abi::emit_load_int_immediate(ctx.emitter, line_arg, line);
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    ctx.load_value_to_reg(object, receiver_arg)?;
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_trace_aot_raw_null_method_receiver");
    abi::emit_call_label(ctx.emitter, &symbol);
    Ok(())
}

/// Lowers an instance-method call whose receiver is boxed as `Mixed`.
pub(super) fn lower_mixed_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    let candidates = mixed_method_candidates(ctx, method_name, inst.operands.len())?;
    if candidates.is_empty() {
        if builtins::has_eval_context(ctx) {
            return builtins::lower_eval_method_call(ctx, inst, object, method_name);
        }
        emit_method_call_on_null_fatal(ctx, method_name);
        return Ok(());
    }
    if builtins::has_eval_context(ctx)
        && (mixed_method_call_needs_eval_callable_adapter(ctx, inst, &candidates)?
            || mixed_method_call_needs_eval_ref_adapter(ctx, inst, &candidates)?)
    {
        return builtins::lower_eval_method_call(ctx, inst, object, method_name);
    }
    if try_lower_outlined_mixed_method_call(ctx, inst, &candidates)? {
        return Ok(());
    }

    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let non_object_label = ctx.next_label("mixed_method_non_object");
    let no_match_label = ctx.next_label("mixed_method_no_match");
    let done_label = ctx.next_label("mixed_method_done");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_method_{}",
                label_fragment(&candidate.class_name)
            ))
        })
        .collect::<Vec<_>>();

    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Object(_)) {
        ctx.load_value_to_reg(object, receiver_reg)?;
    } else {
        ctx.load_value_to_result(object)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        emit_mixed_method_object_payload_or_fatal(ctx, receiver_reg, &non_object_label);
    }
    emit_mixed_method_class_dispatch(
        ctx,
        receiver_reg,
        &candidates,
        &match_labels,
        &no_match_label,
    );

    // Most arms are the same code for every class (argument conversion, then a call through the
    // shared vtable slot); an arm identical to an earlier one becomes a jump to it.
    let mut arms = super::arm_dedup::ArmDeduper::default();
    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let body_start = ctx.emitter.checkpoint();
        // A reflector that arrives boxed is still a reflector: this arm is only reached once the
        // receiver's runtime class matched, so it takes the same eval-bridge route a statically
        // typed reflection receiver takes. The synthesized body it used to run answers from
        // generated AOT class metadata, which holds nothing about a class only the interpreter
        // declared, so `getMethods()` listed nothing and `getMethod()` threw "does not exist" for
        // a method PHP finds. Every other candidate class keeps its direct call.
        if builtins::has_eval_context(ctx)
            && (reflection_class_runtime_metadata_method(&candidate.class_name)
                || reflection_function_callable_metadata_method(
                    &candidate.class_name,
                    method_name,
                ))
        {
            builtins::lower_eval_method_call(ctx, inst, object, method_name)?;
        } else {
            lower_mixed_method_candidate_call(ctx, inst, receiver_reg, candidate, method_name)?;
        }
        abi::emit_jump(ctx.emitter, &done_label);
        arms.finish_arm(ctx.emitter, body_start, label);
    }

    ctx.emitter.label(&no_match_label);
    if builtins::has_eval_context(ctx) {
        builtins::lower_eval_method_call(ctx, inst, object, method_name)?;
        abi::emit_jump(ctx.emitter, &done_label);
    } else {
        // The receiver IS an object (a non-object took `non_object_label`), just of a class none
        // of the candidates covers: php names the class and the method, not a null receiver.
        emit_missing_method_error(ctx, receiver_reg, method_name);
    }

    ctx.emitter.label(&non_object_label);
    emit_method_call_on_null_fatal(ctx, method_name);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Returns whether a gradual callable argument needs the eval context carried by its object cell.
fn mixed_method_call_needs_eval_callable_adapter(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    candidates: &[MixedMethodCandidate],
) -> Result<bool> {
    for (operand_index, operand) in inst.operands.iter().enumerate().skip(1) {
        let source_ty = ctx.raw_value_php_type(*operand)?.codegen_repr();
        if !matches!(
            source_ty,
            PhpType::Mixed | PhpType::Union(_) | PhpType::Object(_)
        ) {
            continue;
        }
        let param_index = operand_index - 1;
        if candidates.iter().any(|candidate| {
            candidate
                .target
                .params
                .get(param_index)
                .is_some_and(|param| param.codegen_repr() == PhpType::Callable)
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Returns whether direct dispatch would pass a promoted Mixed reference cell to a typed ref ABI.
fn mixed_method_call_needs_eval_ref_adapter(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    candidates: &[MixedMethodCandidate],
) -> Result<bool> {
    for (operand_index, operand) in inst.operands.iter().enumerate().skip(1) {
        let Ok(slot) = local_slot_for_loaded_value(ctx, *operand) else {
            continue;
        };
        if !local_slot_stores_ref_cell_pointer(ctx, slot)
            && ctx.local_php_type(slot)?.codegen_repr() != PhpType::Mixed
        {
            continue;
        }
        let param_index = operand_index - 1;
        // Only a typed by-reference parameter (`int &$x`) mismatches a Mixed cell. A `mixed &`
        // parameter takes the promoted cell as its own, so direct dispatch hands it the caller's
        // durable cell; the eval bridge instead passes a frame-address marker, which a callee
        // that keeps the reference in a closure (`PDOStatement::bindParam()`) would capture
        // dangling.
        if candidates.iter().any(|candidate| {
            candidate
                .target
                .ref_params
                .get(param_index)
                .copied()
                .unwrap_or(false)
                && candidate
                    .target
                    .params
                    .get(param_index)
                    .is_none_or(|param| param.codegen_repr() != PhpType::Mixed)
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Emits one concrete class branch for a `Mixed` receiver method call.
pub(super) fn lower_mixed_method_candidate_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver_reg: &str,
    candidate: &MixedMethodCandidate,
    method_name: &str,
) -> Result<()> {
    // Built-in Throwables implement the standard Throwable surface through compact intrinsics,
    // not through class vtable slots — those slots stay empty for builtins. Dispatching this
    // candidate dynamically would load a null slot and branch to it, so route it to the same
    // intrinsic the direct-receiver path uses. The receiver payload is already unboxed in
    // `receiver_reg`, which is exactly what `_from_reg` expects.
    if is_throwable_standard_method_call(ctx, &candidate.class_name, method_name) {
        return lower_throwable_standard_method_from_reg(ctx, inst, receiver_reg, method_name);
    }
    abi::emit_push_reg(ctx.emitter, receiver_reg);
    super::method_resolution::emit_method_literal_parameter_guards(
        ctx, &candidate.target, &inst.operands[1..],
    )?;
    abi::emit_pop_reg(ctx.emitter, receiver_reg);
    let receiver_ty = PhpType::Object(candidate.class_name.clone());
    let mut param_types = Vec::with_capacity(candidate.target.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(
        candidate
            .target
            .params
            .iter()
            .map(|param| param.codegen_repr()),
    );
    let mut ref_params = Vec::with_capacity(candidate.target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(candidate.target.ref_params.iter().copied());
    guard_mixed_method_candidate_object_arguments(ctx, inst, candidate)?;
    // Arguments past this candidate's parameters are not passed, and those past its fixed
    // parameters are packed into a trailing variadic (see `mixed_method_candidates`).
    let fixed_abi = if candidate.variadic_last {
        candidate.target.params.len()
    } else {
        candidate.target.params.len() + 1
    };
    // A site lowered WITH a signature naming the variadic already packed it: exactly one operand
    // in the tail position, an array. That operand is the parameter itself.
    let site_packed_tail = candidate.variadic_last
        && inst.operands.len() == fixed_abi + 1
        && matches!(
            ctx.value_php_type(inst.operands[fixed_abi])?.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. }
        );
    let (passed, variadic_tail) = if site_packed_tail || !candidate.variadic_last {
        (inst.operands.len().min(candidate.target.params.len() + 1), None)
    } else {
        (
            inst.operands.len().min(fixed_abi),
            Some(inst.operands.get(fixed_abi..).unwrap_or(&[])),
        )
    };
    let call_args = materialize_method_call_args_with_receiver_reg_refs_defaults_and_tail(
        ctx,
        receiver_reg,
        &receiver_ty,
        &inst.operands[..passed],
        &param_types,
        &ref_params,
        crate::codegen::lower_inst::RefArgCellLifetime::for_callee(ctx, method_name),
        &candidate.defaults,
        variadic_tail,
        // A site that packed the tail itself wrote the count; otherwise every operand after the
        // receiver is an argument the caller passed.
        (candidate.tail_counts_args && variadic_tail.is_some())
            .then(|| inst.operands.len().saturating_sub(1)),
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    emit_resolved_method_call(ctx, &candidate.target)?;
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_method_call_result(ctx, inst, &candidate.target)?;
    // Candidate dispatch creates the same caller-owned coercion cells as concrete dispatch.
    emit_call_arg_temp_cleanups(ctx, &call_args, None)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Returns true when `class_name` is `ancestor` or reaches it through `parent`.
pub(super) fn class_is_same_or_subclass(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    ancestor: &str,
) -> bool {
    let ancestor = ancestor.trim_start_matches('\\');
    let mut current = class_name.trim_start_matches('\\');
    let mut visited = std::collections::HashSet::new();
    while visited.insert(current) {
        if current == ancestor {
            return true;
        }
        match ctx
            .module
            .class_infos
            .get(current)
            .and_then(|info| info.parent.as_deref())
        {
            Some(parent) => current = parent.trim_start_matches('\\'),
            None => return false,
        }
    }
    false
}

/// Validates boxed arguments against the concrete candidate's nominal object parameters.
///
/// A `Mixed` receiver has no single signature during IR lowering, so its arguments cannot be
/// narrowed before runtime class dispatch chooses a candidate. This guard supplies the missing
/// candidate-specific PHP check before ABI materialization exposes an object payload.
fn guard_mixed_method_candidate_object_arguments(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    candidate: &MixedMethodCandidate,
) -> Result<()> {
    for (param_index, value) in inst.operands.iter().skip(1).enumerate() {
        let Some(param_ty) = candidate.target.params.get(param_index) else {
            continue;
        };
        let PhpType::Object(target_name) = param_ty.codegen_repr() else {
            continue;
        };
        let source_ty = ctx.raw_value_php_type(*value)?.codegen_repr();
        if !matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
            continue;
        }
        emit_mixed_method_candidate_object_argument_guard(ctx, *value, &target_name)?;
    }
    Ok(())
}

/// Rejects a non-matching boxed `Mixed` value before it crosses an object parameter ABI.
fn emit_mixed_method_candidate_object_argument_guard(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    target_name: &str,
) -> Result<()> {
    let target_name = target_name.trim_start_matches('\\');
    let object_label = ctx.next_label("mixed_method_object_argument_object");
    let wrong_tag_label = ctx.next_label("mixed_method_object_argument_wrong_tag");
    let accepted_label = ctx.next_label("mixed_method_object_argument_accepted");
    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                            // runtime tag 6 is the only object ABI payload
            ctx.emitter.instruction(&format!("b.eq {}", object_label));
            ctx.emitter.instruction(&format!("b {}", wrong_tag_label));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                            // runtime tag 6 is the only object ABI payload
            ctx.emitter.instruction(&format!("je {}", object_label));
            ctx.emitter.instruction(&format!("jmp {}", wrong_tag_label));
        }
    }
    super::builtins::arrays::union_type_guard::emit_mixed_wrong_tag_type_error_dispatch(
        ctx,
        &wrong_tag_label,
        &|given| {
            let expected = if target_name.is_empty() { "object" } else { target_name };
            format!("Argument must be of type {}, {} given", expected, given)
        },
    );

    ctx.emitter.label(&object_label);
    // A bare `object` parameter is `Object("")`: it names no class, so ANY object satisfies it.
    // Classifying the empty name found nothing and fell through to the unconditional TypeError,
    // which read `Argument must be of type , object given` -- for an object. Symfony's console
    // passes its commands through such parameters, and every command failed on it.
    if target_name.is_empty() {
        ctx.emitter.label(&accepted_label);
        return Ok(());
    }
    // An object the interpreter built, of a class only it declares or of an interpreted subclass,
    // is answered by the bridge; the native matcher sees only compiled hierarchies. Laravel's
    // interpreted `Illuminate\Http\Request` reached `handleRequest(Request $request)` this way.
    if ctx.module.required_runtime_features.eval_bridge {
        let native_fallback = ctx.next_label("mixed_method_object_argument_native");
        super::builtins::emit_eval_object_is_a_named_fallback(
            ctx,
            value,
            target_name,
            &accepted_label,
            &native_fallback,
            // A bridge-owned object the bridge rules out reaches the matcher below, which fails
            // on its carrier header and raises the TypeError.
            &native_fallback,
        )?;
        ctx.emitter.label(&native_fallback);
        ctx.load_value_to_result(value)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    }
    if let Some((target_id, target_kind)) = objects::classify_named_target(ctx, target_name) {
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),           // matcher takes the unboxed object payload in its first argument register
            Arch::X86_64 => {}                                                 // the unboxed object payload already uses the first argument register
        }
        objects::emit_match_call(ctx, target_id, target_kind, "__rt_exception_matches");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cbnz x0, {}", accepted_label));
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("test rax, rax");
                ctx.emitter.instruction(&format!("jne {}", accepted_label));
            }
        }
    }
    exceptions::emit_type_error(
        ctx,
        &format!("Argument must be of type {}, object given", target_name),
    );
    ctx.emitter.label(&accepted_label);
    Ok(())
}

/// Lowers an interface receiver call accepted through an `instanceof` capability narrowing.
pub(super) fn lower_narrowed_interface_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    interface_name: &str,
    method_name: &str,
) -> Result<()> {
    lower_narrowed_interface_method_call_with_failure(
        ctx,
        inst,
        interface_name,
        method_name,
        None,
    )
}

/// Lowers an interface receiver call and raises the supplied TypeError when no implementation
/// matches the runtime class.
pub(super) fn lower_narrowed_interface_method_call_or_type_error(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    interface_name: &str,
    method_name: &str,
    type_error: &str,
) -> Result<()> {
    lower_narrowed_interface_method_call_with_failure(
        ctx,
        inst,
        interface_name,
        method_name,
        Some(type_error),
    )
}

/// Implements raw interface dispatch with either the ordinary missing-method fatal or a
/// caller-selected TypeError at the no-match boundary.
fn lower_narrowed_interface_method_call_with_failure(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    interface_name: &str,
    method_name: &str,
    type_error: Option<&str>,
) -> Result<()> {
    let candidates =
        narrowed_interface_candidates(ctx, interface_name, method_name, inst.operands.len())?;
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let no_match_label = ctx.next_label("iface_narrowed_no_match");
    let done_label = ctx.next_label("iface_narrowed_done");
    ctx.load_value_to_result(inst.operands[0])?;
    emit_bare_object_receiver_into_reg(ctx, receiver_reg);
    emit_narrowed_interface_class_dispatch(
        ctx,
        inst,
        receiver_reg,
        &candidates,
        method_name,
        &no_match_label,
        &done_label,
    )?;

    ctx.emitter.label(&no_match_label);
    if let Some(message) = type_error {
        exceptions::emit_type_error(ctx, message);
    } else {
        emit_missing_method_error(ctx, receiver_reg, method_name);
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers the nullable form of an interface call accepted through capability narrowing.
pub(super) fn lower_narrowed_nullable_interface_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    interface_name: &str,
    method_name: &str,
) -> Result<()> {
    let candidates =
        narrowed_interface_candidates(ctx, interface_name, method_name, inst.operands.len())?;
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let null_label = ctx.next_label("iface_narrowed_null");
    let no_match_label = ctx.next_label("iface_narrowed_no_match");
    let done_label = ctx.next_label("iface_narrowed_done");
    objects::emit_nullable_receiver_object_payload(ctx, object, &null_label, receiver_reg)?;
    emit_narrowed_interface_class_dispatch(
        ctx,
        inst,
        receiver_reg,
        &candidates,
        method_name,
        &no_match_label,
        &done_label,
    )?;

    ctx.emitter.label(&no_match_label);
    emit_missing_method_error(ctx, receiver_reg, method_name);
    ctx.emitter.label(&null_label);
    emit_method_call_on_null_fatal(ctx, method_name);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Names the real native receiver when runtime method lookup fails.
fn emit_missing_method_error(ctx: &mut FunctionContext<'_>, receiver_reg: &str, method: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, result_reg, receiver_reg);
    builtins::types::emit_dynamic_object_class_name(ctx, "get_class");
    let (text_ptr, text_len) = abi::string_result_regs(ctx.emitter);
    let (right_ptr, right_len) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x3", "x4"),
        Arch::X86_64 => ("rdi", "rsi"),
    };
    abi::emit_reg_move(ctx.emitter, right_ptr, text_ptr);
    abi::emit_reg_move(ctx.emitter, right_len, text_len);
    let (prefix, prefix_len) = ctx.data.add_string(b"Call to undefined method ");
    abi::emit_symbol_address(ctx.emitter, text_ptr, &prefix);
    abi::emit_load_int_immediate(ctx.emitter, text_len, prefix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
    let (suffix, suffix_len) = ctx.data.add_string(format!("::{method}()").as_bytes());
    abi::emit_symbol_address(ctx.emitter, right_ptr, &suffix);
    abi::emit_load_int_immediate(ctx.emitter, right_len, suffix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    exceptions::emit_error_from_string_result(ctx);
}

/// Collects concrete runtime classes compatible with the receiver type and requested method.
pub(super) fn narrowed_interface_candidates(
    ctx: &FunctionContext<'_>,
    receiver_type: &str,
    method_name: &str,
    operand_count: usize,
) -> Result<Vec<MixedMethodCandidate>> {
    mixed_method_candidates(ctx, method_name, operand_count).map(|candidates| {
        candidates
            .iter()
            .filter(|candidate| {
                receiver_type.is_empty()
                    || class_is_same_or_descendant(ctx, &candidate.class_name, receiver_type)
                    || class_implements_interface(ctx, &candidate.class_name, receiver_type)
            })
            .cloned()
            .collect()
    })
}

/// Returns whether a concrete runtime class is the named class or inherits from it.
fn class_is_same_or_descendant(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    ancestor_name: &str,
) -> bool {
    let ancestor_key = php_symbol_key(ancestor_name.trim_start_matches('\\'));
    let mut current = Some(class_name.trim_start_matches('\\'));
    while let Some(candidate) = current {
        if php_symbol_key(candidate) == ancestor_key {
            return true;
        }
        current = ctx
            .module
            .class_infos
            .get(candidate)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Emits class-id branches and concrete calls for a narrowed interface receiver.
fn emit_narrowed_interface_class_dispatch(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver_reg: &str,
    candidates: &[MixedMethodCandidate],
    method_name: &str,
    no_match_label: &str,
    done_label: &str,
) -> Result<()> {
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "iface_narrowed_{}",
                label_fragment(&candidate.class_name)
            ))
        })
        .collect::<Vec<_>>();
    emit_mixed_method_class_dispatch(ctx, receiver_reg, candidates, &match_labels, no_match_label);
    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        lower_mixed_method_candidate_call(ctx, inst, receiver_reg, candidate, method_name)?;
        abi::emit_jump(ctx.emitter, done_label);
    }
    Ok(())
}

/// Moves a bare object result into the register reserved for runtime class dispatch.
fn emit_bare_object_receiver_into_reg(ctx: &mut FunctionContext<'_>, receiver_reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, x0", receiver_reg));      // stage the bare interface object pointer for dispatch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, rax", receiver_reg));     // stage the bare interface object pointer for dispatch
        }
    }
}

/// Collects concrete class-method candidates for a boxed `Mixed` receiver.
///
/// The answer depends on the module, the calling function's class scope (private methods resolve
/// against it), the method and the operand count, and it is memoized on those: every mixed call
/// site and every mixed property guard asked again, re-resolving each declaring class.
pub(super) fn mixed_method_candidates(
    ctx: &FunctionContext<'_>,
    method_name: &str,
    operand_count: usize,
) -> Result<std::sync::Arc<Vec<MixedMethodCandidate>>> {
    let key = (
        ctx.function.name.rsplit_once("::").map(|(scope, _)| scope.to_string()),
        php_symbol_key(method_name),
        operand_count,
    );
    if let Some(candidates) = ctx.shared.mixed_method_candidates(&key) {
        return Ok(candidates);
    }
    let candidates = std::sync::Arc::new(scan_mixed_method_candidates(ctx, method_name, operand_count)?);
    ctx.shared.cache_mixed_method_candidates(key, std::sync::Arc::clone(&candidates));
    Ok(candidates)
}

/// Resolves every declaring class for [`mixed_method_candidates`].
fn scan_mixed_method_candidates(
    ctx: &FunctionContext<'_>,
    method_name: &str,
    operand_count: usize,
) -> Result<Vec<MixedMethodCandidate>> {
    method_candidates(ctx, method_name, operand_count, None)
}

/// Collects class-method candidates, optionally limited to one inheritance tree.
fn method_candidates(
    ctx: &FunctionContext<'_>,
    method_name: &str,
    operand_count: usize,
    ancestor: Option<&str>,
) -> Result<Vec<MixedMethodCandidate>> {
    let method_key = php_symbol_key(method_name);
    let mut candidates = Vec::new();
    // Only classes declaring the method can be candidates; the module indexes them once instead
    // of every mixed call site scanning the whole class table (a top codegen cost on Symfony).
    let classes_by_method = ctx.module.classes_by_method();
    let declaring = classes_by_method.get(&method_key).map(Vec::as_slice).unwrap_or(&[]);
    for class_name in declaring {
        if let Some(ancestor) = ancestor {
            if !class_is_same_or_subclass(ctx, class_name, ancestor) {
                continue;
            }
        }
        let Some(class_info) = ctx.module.class_infos.get(class_name) else {
            continue;
        };
        // Compiler-runtime classes expose synthetic checker declarations whose
        // methods are lowered by dedicated intrinsics and have no ordinary EIR
        // method entry symbol. They must never enter generic Mixed dispatch.
        if php_symbol_key(class_name.trim_start_matches('\\')) == "fiber" {
            continue;
        }
        let Some(signature) = class_info.methods.get(&method_key) else {
            continue;
        };
        // A body with hidden frame collectors (`func_get_args()`) is called with its
        // SOURCE-visible parameters; that exact count dispatches as is.
        let source_count = crate::codegen_support::source_method_adapters::source_visible_signature(
            signature,
        )
        .ok()
        .map(|source| source.params.len() + 1);
        // Not when the collector opens with the actual count, which only the packed tail below
        // writes: the source adapter appends an EMPTY collector, so `func_num_args()` read no
        // count when a boxed receiver supplied every optional parameter, and warned on key 0.
        let source_visible_match = source_count == Some(operand_count)
            && signature.params.len() + 1 != operand_count
            && !crate::func_args::sig_collects_optional_arg_count(signature);
        // A declared variadic tail collects whatever the call passes past the fixed parameters,
        // an EMPTY array when that is nothing; the candidate call packs it. Only a variadic that
        // occupies the last parameter slot is packed; a signature that lists its variadic
        // separately keeps the old rules.
        let variadic_last = !source_visible_match
            && signature.variadic.as_deref().is_some_and(|name| {
                signature.params.last().is_some_and(|(param, _)| param == name)
            });
        let fixed = if variadic_last {
            signature.params.len().saturating_sub(1)
        } else {
            signature.params.len()
        };
        // php ignores arguments past a non-variadic method's parameters, so a site that passes
        // more -- one lowered against a sibling implementation that declares an extra defaulted
        // parameter, as `Container::get($id, $invalidBehavior)` does for every
        // `ContainerInterface::get($id)` call -- still dispatches to this method; the surplus is
        // simply not materialized.
        let provided = operand_count.saturating_sub(1);
        let surplus = !source_visible_match && !variadic_last && provided > signature.params.len();
        // Constructors are left out: a generated container's `$lazyLoad->__construct(...)` would
        // otherwise dispatch over every class whose constructor takes fewer arguments.
        if surplus && (signature.variadic.is_some() || method_key == "__construct") {
            continue;
        }
        let provided = if source_visible_match { fixed } else { provided.min(fixed) };
        // A default spelled as a constant expression (`int $type = HttpKernelInterface::MAIN_REQUEST`)
        // is folded to the literal it names, so the ladder can stage it. Without this the
        // candidate was dropped and a call with the argument omitted found no implementation:
        // `$kernel->handle($request)` on a boxed receiver died as "Call to undefined method".
        let defaults: Vec<Option<crate::parser::ast::Expr>> = signature
            .defaults
            .iter()
            .map(|default| literalize_default(ctx, class_name, default.as_ref()))
            .collect();
        if (provided..fixed).any(|index| {
            !omitted_default_is_materializable(
                defaults.get(index).and_then(|default| default.as_ref()),
                &signature.params[index].1,
            )
        }) {
            continue;
        }
        let target = match resolve_method_call_target(
            ctx,
            class_name,
            method_name,
            signature.params.len() + 1,
        ) {
            Ok(target) => target,
            // A narrower method that only became a candidate by dropping surplus arguments and
            // has no callable body here (a builtin class's `__construct`, reached from
            // `$lazyLoad->__construct(...)` in a generated container) stays out of the dispatch,
            // as it was before surplus arguments were accepted.
            Err(_) if surplus => continue,
            Err(error) => return Err(error),
        };
        let class_id = class_info.class_id;
        let class_name = class_name.clone();
        candidates.push(MixedMethodCandidate {
            class_id,
            class_name,
            target,
            defaults,
            variadic_last,
            tail_counts_args: variadic_last
                && crate::func_args::sig_collects_optional_arg_count(signature),
        });
    }
    candidates.sort_by_key(|candidate| candidate.class_id);
    Ok(candidates)
}

/// Folds a constant-expression default to the literal it names, when that literal is one the
/// call staging can materialize; any other default is returned as written.
fn literalize_default(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    default: Option<&crate::parser::ast::Expr>,
) -> Option<crate::parser::ast::Expr> {
    use crate::codegen::const_default_values::{
        resolve_const_default, ConstDefaultContext, ConstDefaultValue, CONST_DEFAULT_BOOL,
        CONST_DEFAULT_EMPTY_ARRAY, CONST_DEFAULT_FLOAT, CONST_DEFAULT_INT, CONST_DEFAULT_NULL,
    };
    use crate::parser::ast::{Expr, ExprKind};
    let expr = default?;
    if matches!(
        expr.kind,
        ExprKind::Null
            | ExprKind::BoolLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::ArrayLiteral(_)
    ) {
        return Some(expr.clone());
    }
    let context = ConstDefaultContext::for_class(ctx.module, class_name.trim_start_matches('\\'));
    let kind = match resolve_const_default(expr, &context)? {
        ConstDefaultValue::Scalar { kind, payload } => match kind {
            CONST_DEFAULT_NULL => ExprKind::Null,
            CONST_DEFAULT_BOOL => ExprKind::BoolLiteral(payload != 0),
            CONST_DEFAULT_INT => ExprKind::IntLiteral(payload),
            CONST_DEFAULT_FLOAT => ExprKind::FloatLiteral(f64::from_bits(payload as u64)),
            CONST_DEFAULT_EMPTY_ARRAY => ExprKind::ArrayLiteral(Vec::new()),
            _ => return Some(expr.clone()),
        },
        ConstDefaultValue::String(value) => ExprKind::StringLiteral(value),
        _ => return Some(expr.clone()),
    };
    Some(Expr::new(kind, expr.span))
}

/// Preserves the unboxed object payload or routes non-object `Mixed` receivers to fatal.
pub(super) fn emit_mixed_method_object_payload_or_fatal(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    no_match_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // require an object payload before method dispatch
            ctx.emitter.instruction(&format!("b.ne {}", no_match_label));       // non-object Mixed receivers cannot call instance methods
            ctx.emitter
                .instruction(&format!("mov {}, x1", receiver_reg)); // preserve the unboxed object payload across argument lowering
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // require an object payload before method dispatch
            ctx.emitter.instruction(&format!("jne {}", no_match_label));        // non-object Mixed receivers cannot call instance methods
            ctx.emitter
                .instruction(&format!("mov {}, rdi", receiver_reg)); // preserve the unboxed object payload across argument lowering
        }
    }
}

/// Emits class-id branches for every method candidate discovered for a `Mixed` receiver.
pub(super) fn emit_mixed_method_class_dispatch(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    candidates: &[MixedMethodCandidate],
    match_labels: &[String],
    no_match_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [{}]", receiver_reg)); // load the receiver class id for Mixed method dispatch
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare the receiver class id against this method candidate
                ctx.emitter.instruction(&format!("b.eq {}", label));            // call this candidate when the runtime class id matches
            }
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov r11, QWORD PTR [{}]", receiver_reg)); // load the receiver class id for Mixed method dispatch
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // compare the receiver class id against this method candidate
                ctx.emitter.instruction(&format!("je {}", label));              // call this candidate when the runtime class id matches
            }
        }
    }
    abi::emit_jump(ctx.emitter, no_match_label);
}

/// Re-exports the shared label fragmenter so instruction lowering keeps one implementation.
///
/// `crate::names::label_fragment` is documented as deliberately NON-injective — every
/// non-alphanumeric byte collapses to `_`, so `a_b` and `aéb` collide. A second copy here
/// invited use where uniqueness matters; there is now one definition carrying that warning.
pub(super) use crate::names::label_fragment;
