//! Purpose:
//! Builds first-class callable descriptors for functions, methods, and captured receivers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;
use crate::parser::ast::Visibility;

/// Concrete descriptor candidate for first-class method syntax on a gradual receiver.
struct MixedFirstClassMethodCandidate {
    class_id: u64,
    class_name: String,
    method_key: String,
    impl_class: String,
    sig: FunctionSig,
}

/// Materializes a first-class callable value as a static descriptor pointer when possible.
pub(super) fn lower_first_class_callable_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let target = callable_target_data(ctx, inst)?.to_string();
    let strict_php = instruction_strict_php_profile(inst);
    if emit_static_late_bound_first_class_callable(ctx, &target)? {
        return store_if_result(ctx, inst);
    }
    if emit_instance_method_first_class_callable(ctx, inst, &target)? {
        return store_if_result(ctx, inst);
    }
    if let Some(descriptor) = first_class_callable_descriptor(ctx, &target, strict_php)? {
        let owns_string_return = descriptor.owns_string_return;
        let default_scope = if descriptor.kind == callable_descriptor::CALLABLE_DESC_KIND_STATIC_METHOD {
            descriptor.invocation.receiver_name.as_ref().and_then(|receiver| {
                let info = ctx.module.class_infos.get(receiver)?;
                let method = descriptor.invocation.method_name.as_ref()?;
                Some(info.static_method_impl_classes.get(method).unwrap_or(receiver).clone())
            })
        } else { None };
        let invoker_label = descriptor
            .sig
            .as_ref()
            .map(|sig| emit_runtime_callable_invoker_in_class(
                ctx, sig, &[], owns_string_return,
                descriptor.kind != callable_descriptor::CALLABLE_DESC_KIND_BUILTIN
                    && descriptor.kind != callable_descriptor::CALLABLE_DESC_KIND_EXTERN,
                default_scope.as_deref(),
            ));
        let descriptor_label = match descriptor.entry_label.as_deref() {
            Some(entry_label) => {
                callable_descriptor::static_descriptor_with_optional_invoker_meta(
                    ctx.data,
                    entry_label,
                    Some(&target),
                    descriptor.kind,
                    descriptor.sig.as_ref(),
                    &[],
                    &[],
                    descriptor.invocation,
                    invoker_label.as_deref(),
                )
            }
            None => callable_descriptor::static_only_descriptor(
                ctx.data,
                &target,
                descriptor.kind,
                descriptor.invocation,
            ),
        };
        // `f(...)` produces a Closure in PHP and therefore consumes an object
        // handle, exactly like `function () {}` does. Give it the same runtime
        // descriptor storage so the handle can be bound at creation and returned
        // when the descriptor is released — see `lower_closure_new`.
        emit_runtime_closure_descriptor_with_captures(ctx, &descriptor_label, &[], &[])?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    }
    store_if_result(ctx, inst)
}

/// Emits a runtime descriptor for `static::method(...)` first-class callables.
pub(super) fn emit_static_late_bound_first_class_callable(
    ctx: &mut FunctionContext<'_>,
    target: &str,
) -> Result<bool> {
    let Some((receiver_label, method_name)) = target.rsplit_once("::") else {
        return Ok(false);
    };
    if receiver_label.trim_start_matches('\\') != "static" {
        return Ok(false);
    }

    let receiver = resolve_static_method_receiver(ctx, receiver_label)?;
    let called_class_id = resolve_static_called_class_arg(ctx, receiver_label, &receiver)?;
    let receiver_info = ctx
        .module
        .class_infos
        .get(receiver.as_str())
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "late-bound first-class callable '{}' on unknown class '{}'",
                target, receiver
            ))
        })?;
    let method_key = php_symbol_key(method_name);
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| receiver.clone());
    let dynamic_slot = receiver_info.static_vtable_slots.get(&method_key).copied();
    let sig = ctx
        .module
        .class_infos
        .get(impl_class.as_str())
        .and_then(|class_info| class_info.static_methods.get(&method_key))
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "late-bound first-class callable '{}' with unknown implementation",
                target
            ))
        })?
        .clone();
    let wrapper_sig = crate::codegen::callable_dispatch::static_method_runtime_wrapper_sig(&sig);
    let captures = vec![("called_class_id".to_string(), PhpType::Int, false)];
    let entry_label = emit_static_late_bound_descriptor_entry_wrapper(
        ctx,
        impl_class.as_str(),
        &method_key,
        &wrapper_sig,
        dynamic_slot,
    )?;
    // The late-bound entry wrapper normalizes each selected implementation's
    // string ownership before returning, including borrowed-result overrides.
    let owns_string_return = wrapper_sig.return_type.codegen_repr() == PhpType::Str
        && !wrapper_sig.by_ref_return;
    let invoker_label = emit_runtime_callable_invoker_in_class(
        ctx, &wrapper_sig, &captures, owns_string_return, true, Some(&impl_class),
    );
    let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
        ctx.data,
        &entry_label,
        Some(target),
        callable_descriptor::CALLABLE_DESC_KIND_STATIC_METHOD,
        Some(&wrapper_sig),
        &captures,
        &[],
        callable_descriptor::CallableDescriptorInvocation::method(
            callable_descriptor::CallableDescriptorShape::StaticMethod,
            Some("static".to_string()),
            method_key.clone(),
        ),
        Some(&invoker_label),
    );
    emit_runtime_descriptor_with_called_class_capture(ctx, &descriptor_label, &called_class_id)?;
    crate::codegen_support::runtime::emit_acquire_object_handle(ctx.emitter); // `static::m(...)` is a Closure and consumes an object handle
    Ok(true)
}

/// Emits a runtime descriptor for receiver-bound `object::method` first-class callables.
pub(super) fn emit_instance_method_first_class_callable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: &str,
) -> Result<bool> {
    let Some((receiver_label, method_name)) = target.rsplit_once("::") else {
        return Ok(false);
    };
    if receiver_label.trim_start_matches('\\') != "object" {
        return Ok(false);
    }
    let receiver = inst.operands.first().copied().ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "instance first-class callable '{}' has no receiver operand",
            target
        ))
    })?;
    let receiver_ty = ctx.value_php_type(receiver)?.codegen_repr();
    let PhpType::Object(class_name) = receiver_ty else {
        if matches!(receiver_ty, PhpType::Mixed | PhpType::Union(_)) {
            emit_mixed_receiver_first_class_callable(ctx, receiver, method_name)?;
            return Ok(true);
        }
        return Err(CodegenIrError::unsupported(format!(
            "instance first-class callable '{}' with receiver PHP type {:?}",
            target, receiver_ty
        )));
    };
    let normalized_class = class_name.trim_start_matches('\\').to_string();
    // A class the program does not declare (an uninstalled package's, or an extension's the build
    // does not link) names no method table: the receiver is resolved when it runs, as for a
    // generic `object` -- the checker already typed the callable dynamically.
    if class_name.is_empty()
        || (!ctx.module.class_infos.contains_key(normalized_class.as_str())
            && !module_has_interface(ctx, &normalized_class))
    {
        emit_object_receiver_first_class_callable(ctx, receiver, method_name, None)?;
        return Ok(true);
    }
    if module_has_interface(ctx, &normalized_class) {
        emit_object_receiver_first_class_callable(
            ctx,
            receiver,
            method_name,
            Some(&normalized_class),
        )?;
        return Ok(true);
    }
    let method_key = php_symbol_key(method_name);
    let class_info = ctx
        .module
        .class_infos
        .get(normalized_class.as_str())
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "instance first-class callable '{}' with unknown receiver class '{}'",
                target, normalized_class
            ))
        })?;
    let Some(sig) = class_info.methods.get(&method_key).cloned() else {
        if class_info.static_methods.contains_key(&method_key) {
            let static_target = format!("{}::{}", normalized_class, method_name);
            let descriptor = first_class_callable_descriptor(ctx, &static_target, false)?
                .ok_or_else(|| {
                    CodegenIrError::unsupported(format!(
                        "instance first-class callable '{}' with unknown static method",
                        target
                    ))
                })?;
            // Bounded: an interpreted caller wraps the call in a PHP `try`, and a native throw
            // must come back as a status instead of unwinding past it.
            let owns_string_return = crate::codegen::runtime_callable_invoker::method_returns_owned_string(
                ctx.module, &normalized_class, &method_key, true,
            );
            let invoker_label = descriptor.sig.as_ref().map(|sig| {
                emit_runtime_callable_invoker_with_string_owner(ctx, sig, &[], owns_string_return, true)
            });
            let descriptor_label = match descriptor.entry_label.as_deref() {
                Some(entry_label) => callable_descriptor::static_descriptor_with_optional_invoker_meta(
                    ctx.data,
                    entry_label,
                    Some(&static_target),
                    descriptor.kind,
                    descriptor.sig.as_ref(),
                    &[],
                    &[],
                    descriptor.invocation,
                    invoker_label.as_deref(),
                ),
                None => callable_descriptor::static_only_descriptor(
                    ctx.data,
                    &static_target,
                    descriptor.kind,
                    descriptor.invocation,
                ),
            };
            emit_runtime_closure_descriptor_with_captures(ctx, &descriptor_label, &[], &[])?;
            return Ok(true);
        }
        // `__invoke`, and a method only a SUBCLASS declares, which the checker admits behind a
        // `method_exists($this, 'validator')` guard (Laravel's `FormRequest`): the run-time
        // receiver's class resolves it, as for a generic `object`.
        emit_object_receiver_first_class_callable(ctx, receiver, method_name, None)?;
        return Ok(true);
    };
    let impl_class = class_info
        .method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| normalized_class.clone());
    // `$this->m(...)` is late-bound like `$this->m()`: the Closure calls the method the RECEIVER's
    // class resolves, not the one the creating class declares. Binding the static class's body
    // skipped every override -- Symfony's `Container::getEnv()` stores `$this->getEnv(...)`, and
    // on a ContainerBuilder the stored callable ran `Container::getEnv`, never the override that
    // resolves nested env placeholders, so `lint:container` read a placeholder string as a value.
    // A private method is the lexical class's own and never overridden, so it keeps the static
    // binding, as does a method no compiled descendant replaces.
    let is_private = class_info.method_visibilities.get(&method_key) == Some(&Visibility::Private);
    if !is_private && method_overridden_below(ctx, &normalized_class, &method_key, &impl_class) {
        emit_object_receiver_first_class_callable(
            ctx,
            receiver,
            method_name,
            Some(&normalized_class),
        )?;
        return Ok(true);
    }
    if !class_method_body_exists(ctx, &impl_class, &method_key) {
        return Err(CodegenIrError::unsupported(format!(
            "instance first-class callable '{}' without emitted method body",
            target
        )));
    }
    let receiver_ty = PhpType::Object(normalized_class.clone());
    let captures = vec![("receiver".to_string(), receiver_ty.clone(), false)];
    let entry_label =
        emit_instance_method_descriptor_entry_wrapper(ctx, &impl_class, &method_key, &sig)?;
    let owns_string_return = crate::codegen::runtime_callable_invoker::method_returns_owned_string(
        ctx.module, &impl_class, &method_key, false,
    );
    let invoker_label = emit_runtime_callable_invoker_in_class(
        ctx, &sig, &captures, owns_string_return, true, Some(&impl_class),
    );
    let declared_name = format!("{}::{}", impl_class, method_name);
    let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
        ctx.data,
        &entry_label,
        Some(&declared_name),
        callable_descriptor::CALLABLE_DESC_KIND_FIRST_CLASS,
        Some(&sig),
        &captures,
        &[],
        callable_descriptor::CallableDescriptorInvocation::method(
            callable_descriptor::CallableDescriptorShape::InstanceMethod,
            Some(normalized_class),
            method_name,
        ),
        Some(&invoker_label),
    );
    emit_runtime_descriptor_with_receiver_capture(ctx, &descriptor_label, receiver, &receiver_ty)?;
    // `$o->m(...)` is a Closure in PHP and consumes an object handle. The acquire
    // sits here rather than inside the shared descriptor helper because that helper
    // also builds the internal adapter for calling an `__invoke`-able object, and
    // `$obj()` creates no Closure in PHP.
    crate::codegen_support::runtime::emit_acquire_object_handle(ctx.emitter);
    Ok(true)
}

/// Materializes a receiver-bound callable when the receiver is boxed as `Mixed`.
fn emit_mixed_receiver_first_class_callable(
    ctx: &mut FunctionContext<'_>,
    receiver: ValueId,
    method_name: &str,
) -> Result<()> {
    let candidates = mixed_first_class_method_candidates(ctx, method_name, None);
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let fatal_label = ctx.next_label("mixed_first_class_callable_invalid_receiver");
    let done_label = ctx.next_label("mixed_first_class_callable_done");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_first_class_callable_{}",
                callable_label_fragment(&candidate.class_name)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_result(receiver)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_mixed_method_object_payload_or_fatal(ctx, receiver_reg, &fatal_label);
    emit_mixed_first_class_method_dispatch(
        ctx,
        receiver_reg,
        &candidates,
        &match_labels,
        &fatal_label,
    );

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let template = callables::runtime_instance_method_descriptor_template(
            ctx,
            &candidate.class_name,
            method_name,
            &candidate.method_key,
            &candidate.impl_class,
            &candidate.sig,
        )?;
        let receiver_ty = PhpType::Object(candidate.class_name.clone());
        emit_runtime_descriptor_with_unboxed_receiver_capture(
            ctx,
            &template.descriptor_label,
            receiver_reg,
            &receiver_ty,
        );
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&fatal_label);
    // An object the interpreter declared matches no compiled candidate; the eval bridge binds it.
    if builtins::has_eval_context(ctx) || ctx.module.required_runtime_features.eval_bridge {
        let null_label = ctx.next_label("mixed_first_class_callable_null");
        callables::emit_eval_owned_bound_method_descriptor(ctx, receiver, method_name, &null_label)?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&null_label);
    }
    emit_method_call_on_null_fatal(ctx, method_name);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Materializes a receiver-bound callable when an `object` hint erased the concrete class.
fn emit_object_receiver_first_class_callable(
    ctx: &mut FunctionContext<'_>,
    receiver: ValueId,
    method_name: &str,
    interface_filter: Option<&str>,
) -> Result<()> {
    let candidates = mixed_first_class_method_candidates(ctx, method_name, interface_filter);
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let fatal_label = ctx.next_label("object_first_class_callable_invalid_receiver");
    let done_label = ctx.next_label("object_first_class_callable_done");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "object_first_class_callable_{}",
                callable_label_fragment(&candidate.class_name)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(receiver, receiver_reg)?;
    emit_mixed_first_class_method_dispatch(
        ctx,
        receiver_reg,
        &candidates,
        &match_labels,
        &fatal_label,
    );

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let template = callables::runtime_instance_method_descriptor_template(
            ctx,
            &candidate.class_name,
            method_name,
            &candidate.method_key,
            &candidate.impl_class,
            &candidate.sig,
        )?;
        let receiver_ty = PhpType::Object(candidate.class_name.clone());
        emit_runtime_descriptor_with_unboxed_receiver_capture(
            ctx,
            &template.descriptor_label,
            receiver_reg,
            &receiver_ty,
        );
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&fatal_label);
    emit_method_call_on_null_fatal(ctx, method_name);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Collects accessible concrete method implementations for a gradual callable receiver.
fn mixed_first_class_method_candidates(
    ctx: &FunctionContext<'_>,
    method_name: &str,
    interface_filter: Option<&str>,
) -> Vec<MixedFirstClassMethodCandidate> {
    let method_key = php_symbol_key(method_name);
    let mut candidates = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        if !class_info.methods.contains_key(&method_key) {
            continue;
        }
        if interface_filter.is_some_and(|ancestor| {
            !class_implements_interface(ctx, class_name, ancestor)
                && !class_is_or_extends(ctx, class_name, ancestor)
        }) {
            continue;
        }
        let sig = &class_info.methods[&method_key];
        let declaring_class = class_info
            .method_declaring_classes
            .get(&method_key)
            .map(String::as_str)
            .unwrap_or(class_name.as_str());
        let visibility = class_info
            .method_visibilities
            .get(&method_key)
            .unwrap_or(&Visibility::Public);
        if !codegen_can_access_member(ctx, declaring_class, visibility) {
            continue;
        }
        let impl_class = class_info
            .method_impl_classes
            .get(&method_key)
            .cloned()
            .unwrap_or_else(|| class_name.clone());
        if !class_method_body_exists(ctx, &impl_class, &method_key) {
            continue;
        }
        candidates.push(MixedFirstClassMethodCandidate {
            class_id: class_info.class_id,
            class_name: class_name.clone(),
            method_key: method_key.clone(),
            impl_class,
            sig: sig.clone(),
        });
    }
    candidates.sort_by_key(|candidate| candidate.class_id);
    candidates
}

/// Returns whether `class_name` is `ancestor` or extends it, under PHP's case-insensitive rules.
fn class_is_or_extends(ctx: &FunctionContext<'_>, class_name: &str, ancestor: &str) -> bool {
    let wanted = php_symbol_key(ancestor.trim_start_matches('\\'));
    let mut current = Some(class_name.trim_start_matches('\\'));
    while let Some(candidate) = current {
        if php_symbol_key(candidate) == wanted {
            return true;
        }
        current = ctx
            .module
            .class_infos
            .get(candidate)
            .and_then(|info| info.parent.as_deref());
    }
    false
}

/// Returns whether a compiled descendant of `class_name` runs a different body for `method_key`.
fn method_overridden_below(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
    impl_class: &str,
) -> bool {
    ctx.module.class_infos.iter().any(|(candidate, info)| {
        candidate.as_str() != class_name
            && class_is_or_extends(ctx, candidate, class_name)
            && info
                .method_impl_classes
                .get(method_key)
                .is_some_and(|candidate_impl| candidate_impl != impl_class)
    })
}

/// Returns whether the EIR module contains an interface under PHP's case-insensitive rules.
fn module_has_interface(ctx: &FunctionContext<'_>, interface_name: &str) -> bool {
    let wanted = php_symbol_key(interface_name.trim_start_matches('\\'));
    ctx.module
        .interface_infos
        .keys()
        .any(|name| php_symbol_key(name.trim_start_matches('\\')) == wanted)
}

/// Returns whether the current EIR method scope can access a declared member.
pub(super) fn codegen_can_access_member(
    ctx: &FunctionContext<'_>,
    declaring_class: &str,
    visibility: &Visibility,
) -> bool {
    let current_class = ctx
        .function
        .name
        .rsplit_once("::")
        .map(|(class_name, _)| class_name);
    match visibility {
        Visibility::Public => true,
        Visibility::Protected => current_class.is_some_and(|current| {
            current == declaring_class
                || codegen_is_subclass_of(ctx, current, declaring_class)
                || codegen_is_subclass_of(ctx, declaring_class, current)
        }),
        Visibility::Private => current_class == Some(declaring_class),
    }
}

/// Returns whether `class_name` inherits from `ancestor_name`.
fn codegen_is_subclass_of(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    ancestor_name: &str,
) -> bool {
    let mut current = ctx
        .module
        .class_infos
        .get(class_name)
        .and_then(|class_info| class_info.parent.as_deref());
    while let Some(parent) = current {
        if parent == ancestor_name {
            return true;
        }
        current = ctx
            .module
            .class_infos
            .get(parent)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Emits class-id branches for first-class method descriptors selected from a gradual receiver.
fn emit_mixed_first_class_method_dispatch(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    candidates: &[MixedFirstClassMethodCandidate],
    match_labels: &[String],
    fatal_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [{}]", receiver_reg)); // load the runtime receiver class id for callable selection
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare this concrete callable receiver class id
                ctx.emitter.instruction(&format!("b.eq {}", label));            // capture the matching concrete method descriptor
            }
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov r11, QWORD PTR [{}]", receiver_reg)); // load the runtime receiver class id for callable selection
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // compare this concrete callable receiver class id
                ctx.emitter.instruction(&format!("je {}", label));              // capture the matching concrete method descriptor
            }
        }
    }
    abi::emit_jump(ctx.emitter, fatal_label);
}

/// Allocates a runtime callable descriptor and captures an already-unboxed object receiver.
fn emit_runtime_descriptor_with_unboxed_receiver_capture(
    ctx: &mut FunctionContext<'_>,
    descriptor_label: &str,
    receiver_reg: &str,
    receiver_ty: &PhpType,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    let total_bytes = callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 16;
    move_reg_to_int_result(ctx, receiver_reg);
    abi::emit_incref_if_refcounted(ctx.emitter, receiver_ty);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, total_bytes as i64);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter
        .instruction(&format!("mov {}, {}", descriptor_reg, result_reg)); // keep the runtime callable descriptor while copying its static header
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        ctx.emitter,
        descriptor_reg,
        descriptor_label,
    );
    abi::emit_pop_reg(ctx.emitter, result_reg);
    callable_descriptor::emit_store_current_result_to_runtime_capture(
        ctx.emitter,
        descriptor_reg,
        0,
        receiver_ty,
    );
    if descriptor_reg != result_reg {
        ctx.emitter
            .instruction(&format!("mov {}, {}", result_reg, descriptor_reg)); // return the receiver-bound callable descriptor
    }
    crate::codegen_support::runtime::emit_acquire_object_handle(ctx.emitter);
}

/// Converts a PHP symbol into an assembler-safe label fragment.
fn callable_label_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}
