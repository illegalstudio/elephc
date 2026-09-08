//! Purpose:
//! Builds first-class callable descriptors for functions, methods, and captured receivers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Materializes a first-class callable value as a static descriptor pointer when possible.
pub(super) fn lower_first_class_callable_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let target = callable_target_data(ctx, inst)?.to_string();
    let strict_php = instruction_strict_php_profile(inst);
    if emit_static_late_bound_first_class_callable(ctx, &target)? {
        return store_if_result(ctx, inst);
    }
    if super::callables::emit_runtime_instance_method_first_class_callable(ctx, inst, &target)? {
        return store_if_result(ctx, inst);
    }
    if emit_instance_method_first_class_callable(ctx, inst, &target)? {
        return store_if_result(ctx, inst);
    }
    if let Some(descriptor) = first_class_callable_descriptor(ctx, &target, strict_php)? {
        let display_name = descriptor.display_name.as_deref().unwrap_or(&target);
        let invoker_label = descriptor
            .sig
            .as_ref()
            .map(|sig| super::runtime_wrappers::emit_runtime_callable_invoker_with_ownership(
                ctx, sig, &[], descriptor.owned_object_return,
            ));
        let static_bindings = fake_callable_static_debug_bindings(ctx, display_name);
        let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_debug_meta(
            ctx.data,
            &descriptor.entry_label,
            Some(display_name),
            descriptor.kind,
            descriptor.sig.as_ref(),
            &[],
            &[],
            descriptor.invocation,
            invoker_label.as_deref(),
            Some(callable_descriptor::CallableDebugMetadata {
                flags: callable_descriptor::CALLABLE_DEBUG_FLAG_FAKE_CLOSURE,
                primary_name: display_name,
                source_path: None,
                source_line: 0,
                bindings: &[],
                static_bindings: &static_bindings,
            }),
        );
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
    let done = ctx.next_label("static_late_bound_callable_done");
    emit_eval_late_bound_static_callable_override(ctx, &receiver, method_name, &done);
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
    let descriptor_label = emit_static_late_bound_callable_descriptor(
        ctx,
        &impl_class,
        method_name,
        &method_key,
        &sig,
        dynamic_slot,
    )?;
    let emitted_compiled_selection = if let Some(slot) = dynamic_slot {
        let candidates = late_bound_static_method_metadata_candidates(ctx, &receiver, &method_key);
        if candidates.iter().any(|(_, candidate_impl, candidate_sig)| {
            candidate_impl != &impl_class || candidate_sig != &sig
        }) {
            let mut descriptor_cases = Vec::with_capacity(candidates.len());
            for (class_id, candidate_impl, candidate_sig) in candidates {
                let candidate_descriptor = if candidate_impl == impl_class && candidate_sig == sig {
                    descriptor_label.clone()
                } else {
                    emit_static_late_bound_callable_descriptor(
                        ctx,
                        &candidate_impl,
                        method_name,
                        &method_key,
                        &candidate_sig,
                        Some(slot),
                    )?
                };
                descriptor_cases.push((class_id, candidate_descriptor));
            }
            emit_late_bound_descriptor_selection(
                ctx,
                &called_class_id,
                &descriptor_cases,
                &descriptor_label,
            )?;
            crate::codegen_support::runtime::emit_acquire_object_handle(ctx.emitter);
            true
        } else {
            false
        }
    } else {
        false
    };
    if !emitted_compiled_selection {
        emit_runtime_descriptor_with_called_class_capture(ctx, &descriptor_label, &called_class_id)?;
        crate::codegen_support::runtime::emit_acquire_object_handle(ctx.emitter); // `static::m(...)` is a Closure and consumes an object handle
    }
    ctx.emitter.label(&done);
    Ok(true)
}

/// Prefers a Magician-owned callable when an eval descendant supplies the called scope.
fn emit_eval_late_bound_static_callable_override(
    ctx: &mut FunctionContext<'_>,
    frame_class: &str,
    method_name: &str,
    done: &str,
) {
    if !ctx.module.required_runtime_features.eval_bridge {
        return;
    }
    let fallback = ctx.next_label("static_late_bound_callable_no_eval_override");
    let descriptor_label =
        crate::codegen::eval_callable_helpers::eval_dynamic_callable_descriptor(ctx.data);
    let (class_label, class_len) = ctx.data.add_string(frame_class.as_bytes());
    let (method_label, method_len) = ctx.data.add_string(method_name.as_bytes());
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_native_frame_static_method_callable");
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x0", &class_label);
            abi::emit_load_int_immediate(ctx.emitter, "x1", class_len as i64);
            abi::emit_symbol_address(ctx.emitter, "x2", &method_label);
            abi::emit_load_int_immediate(ctx.emitter, "x3", method_len as i64);
            abi::emit_temporary_stack_address(ctx.emitter, "x4", 0);
            abi::emit_temporary_stack_address(ctx.emitter, "x5", 8);
            abi::emit_call_label(ctx.emitter, &symbol);
            ctx.emitter.instruction("cmn w0, #1");                              // status -1 is the sole benign no-override result
            ctx.emitter.instruction(&format!("b.eq {}", fallback));             // use compiled metadata only for a genuine miss
            ctx.emitter.instruction("ldr x9, [sp, #0]");                        // load callback or escaped Throwable cell
            ctx.emitter.instruction("str x9, [sp, #16]");                       // expose Throwable storage at the shared eval-result offset
            super::builtins::emit_eval_bridge_status_check(ctx);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x0",
                (callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 32) as i64,
            );
            ctx.emitter.instruction("bl __rt_heap_alloc");                      // allocate a callback-adapter descriptor with two runtime captures
            callable_descriptor::emit_copy_static_descriptor_to_runtime(
                ctx.emitter,
                "x0",
                &descriptor_label,
            );
            ctx.emitter.instruction("ldr x10, [sp, #8]");                       // load the retained eval context returned by Magician
            abi::emit_store_to_address(
                ctx.emitter,
                "x10",
                "x0",
                callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET,
            );
            ctx.emitter.instruction("ldr x10, [sp, #0]");                       // reload the owned eval Closure cell
            abi::emit_store_to_address(
                ctx.emitter,
                "x10",
                "x0",
                callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 16,
            );
            ctx.emitter.instruction("str x0, [sp, #16]");                       // preserve the descriptor identity across eval alias registration
            ctx.emitter.instruction("ldr x0, [sp, #8]");                        // pass the retained eval context
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // pass the source eval Closure cell
            ctx.emitter.instruction("ldr x2, [sp, #16]");                       // expose the native descriptor payload as the alias identity
            let alias_symbol = ctx
                .emitter
                .target
                .extern_symbol("__elephc_eval_alias_callable_identity");
            abi::emit_call_label(ctx.emitter, &alias_symbol);
            let alias_ok = ctx.next_label("eval_callable_alias_ok");
            abi::emit_branch_if_int_result_zero(ctx.emitter, &alias_ok);
            ctx.emitter.instruction("str w0, [sp, #24]");                       // preserve the failing eval status across cleanup
            ctx.emitter.instruction("ldr x0, [sp, #8]");                        // release the retained context capture
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // release the owned callback cell
            ctx.emitter.instruction("ldr x2, [sp, #16]");                       // unregister the descriptor identity if it was installed
            let release_symbol = ctx
                .emitter
                .target
                .extern_symbol("__elephc_eval_release_callable_descriptor");
            abi::emit_call_label(ctx.emitter, &release_symbol);
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // free the descriptor whose alias setup failed
            ctx.emitter.instruction("bl __rt_heap_free");                       // return the failed descriptor allocation to the heap
            ctx.emitter.instruction("ldr w0, [sp, #24]");                       // restore and propagate the original failure
            super::builtins::emit_eval_bridge_status_check(ctx);
            ctx.emitter.label(&alias_ok);
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // restore the descriptor after alias registration
            crate::codegen_support::runtime::emit_acquire_object_handle(ctx.emitter);
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            abi::emit_jump(ctx.emitter, done);
        }
        crate::codegen::platform::Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &class_label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", class_len as i64);
            abi::emit_symbol_address(ctx.emitter, "rdx", &method_label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", method_len as i64);
            abi::emit_temporary_stack_address(ctx.emitter, "r8", 0);
            abi::emit_temporary_stack_address(ctx.emitter, "r9", 8);
            abi::emit_call_label(ctx.emitter, &symbol);
            ctx.emitter.instruction("cmp eax, -1");                             // status -1 is the sole benign no-override result
            ctx.emitter.instruction(&format!("je {}", fallback));               // use compiled metadata only for a genuine miss
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 0]");            // load callback or escaped Throwable cell
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], r10");           // expose Throwable storage at the shared eval-result offset
            super::builtins::emit_eval_bridge_status_check(ctx);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "rax",
                (callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 32) as i64,
            );
            ctx.emitter.instruction("call __rt_heap_alloc");                    // allocate a callback-adapter descriptor with two runtime captures
            callable_descriptor::emit_copy_static_descriptor_to_runtime(
                ctx.emitter,
                "rax",
                &descriptor_label,
            );
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 8]");            // load the retained eval context returned by Magician
            abi::emit_store_to_address(
                ctx.emitter,
                "r10",
                "rax",
                callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET,
            );
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 0]");            // reload the owned eval Closure cell
            abi::emit_store_to_address(
                ctx.emitter,
                "r10",
                "rax",
                callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 16,
            );
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the descriptor identity across eval alias registration
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");            // pass the retained eval context
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");            // pass the source eval Closure cell
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");           // expose the native descriptor payload as the alias identity
            let alias_symbol = ctx
                .emitter
                .target
                .extern_symbol("__elephc_eval_alias_callable_identity");
            abi::emit_call_label(ctx.emitter, &alias_symbol);
            let alias_ok = ctx.next_label("eval_callable_alias_ok");
            abi::emit_branch_if_int_result_zero(ctx.emitter, &alias_ok);
            ctx.emitter.instruction("mov DWORD PTR [rsp + 24], eax");           // preserve the failing eval status across cleanup
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");            // release the retained context capture
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");            // release the owned callback cell
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");           // unregister the descriptor identity if it was installed
            let release_symbol = ctx
                .emitter
                .target
                .extern_symbol("__elephc_eval_release_callable_descriptor");
            abi::emit_call_label(ctx.emitter, &release_symbol);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // free the descriptor whose alias setup failed
            ctx.emitter.instruction("call __rt_heap_free");                     // return the failed descriptor allocation to the heap
            ctx.emitter.instruction("mov eax, DWORD PTR [rsp + 24]");           // restore and propagate the original failure
            super::builtins::emit_eval_bridge_status_check(ctx);
            ctx.emitter.label(&alias_ok);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // restore the descriptor after alias registration
            crate::codegen_support::runtime::emit_acquire_object_handle(ctx.emitter);
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            abi::emit_jump(ctx.emitter, done);
        }
    }
    ctx.emitter.label(&fallback);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
}

/// Builds one late-static descriptor whose callable metadata belongs to the effective handler.
fn emit_static_late_bound_callable_descriptor(
    ctx: &mut FunctionContext<'_>,
    impl_class: &str,
    method_name: &str,
    method_key: &str,
    sig: &FunctionSig,
    dynamic_slot: Option<usize>,
) -> Result<String> {
    let wrapper_sig = crate::codegen::callable_dispatch::static_method_runtime_wrapper_sig(sig);
    let captures = vec![("called_class_id".to_string(), PhpType::Int, false)];
    let entry_label = emit_static_late_bound_descriptor_entry_wrapper(
        ctx,
        impl_class,
        method_key,
        &wrapper_sig,
        dynamic_slot,
    )?;
    let invoker_label = emit_runtime_callable_invoker_inline(ctx, &wrapper_sig, &captures);
    let canonical_method_name =
        canonical_first_class_callable_method_name(ctx, impl_class, method_name);
    let debug_primary_name = format!("{impl_class}::{canonical_method_name}");
    let static_bindings = fake_callable_static_debug_bindings(ctx, &debug_primary_name);
    Ok(callable_descriptor::static_descriptor_with_optional_invoker_debug_meta(
        ctx.data,
        &entry_label,
        Some(&debug_primary_name),
        callable_descriptor::CALLABLE_DESC_KIND_STATIC_METHOD,
        Some(&wrapper_sig),
        &captures,
        &[],
        callable_descriptor::CallableDescriptorInvocation::method(
            callable_descriptor::CallableDescriptorShape::StaticMethod,
            Some("static".to_string()),
            method_key.to_string(),
        ),
        Some(&invoker_label),
        Some(callable_descriptor::CallableDebugMetadata {
            flags: callable_descriptor::CALLABLE_DEBUG_FLAG_FAKE_CLOSURE,
            primary_name: &debug_primary_name,
            source_path: None,
            source_line: 0,
            bindings: &[],
            static_bindings: &static_bindings,
        }),
    ))
}

/// Collects every compiled called class and its effective static-method handler metadata.
fn late_bound_static_method_metadata_candidates(
    ctx: &FunctionContext<'_>,
    receiver: &str,
    method_key: &str,
) -> Vec<(u64, String, FunctionSig)> {
    let mut candidates = ctx
        .module
        .class_infos
        .iter()
        .filter(|(class_name, _)| class_is_or_extends(ctx, class_name, receiver))
        .filter_map(|(class_name, class_info)| {
            let impl_class = class_info
                .static_method_impl_classes
                .get(method_key)
                .cloned()
                .unwrap_or_else(|| class_name.clone());
            let sig = ctx
                .module
                .class_infos
                .get(&impl_class)?
                .static_methods
                .get(method_key)?
                .clone();
            Some((class_info.class_id, impl_class, sig))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(class_id, _, _)| *class_id);
    candidates
}

/// Returns whether `class_name` is `expected_parent` or inherits from it.
fn class_is_or_extends(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    expected_parent: &str,
) -> bool {
    let expected_key = php_symbol_key(expected_parent.trim_start_matches('\\'));
    let mut current = Some(class_name.trim_start_matches('\\'));
    while let Some(candidate) = current {
        if php_symbol_key(candidate) == expected_key {
            return true;
        }
        current = ctx
            .module
            .class_infos
            .get(candidate)
            .and_then(|class_info| class_info.parent.as_deref())
            .map(|parent| parent.trim_start_matches('\\'));
    }
    false
}

/// Selects the effective descriptor by called-class id and captures that same id for invocation.
fn emit_late_bound_descriptor_selection(
    ctx: &mut FunctionContext<'_>,
    called_class_id: &CalledClassIdArg,
    cases: &[(u64, String)],
    fallback_descriptor: &str,
) -> Result<()> {
    let done = ctx.next_label("static_late_bound_descriptor_selected");
    for (class_id, descriptor_label) in cases {
        let next = ctx.next_label("static_late_bound_descriptor_next");
        materialize_called_class_id(ctx, called_class_id)?;
        let actual_reg = abi::int_result_reg(ctx.emitter);
        let expected_reg = abi::secondary_scratch_reg(ctx.emitter);
        abi::emit_load_int_immediate(ctx.emitter, expected_reg, *class_id as i64);
        match ctx.emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                ctx.emitter
                    .instruction(&format!("cmp {}, {}", actual_reg, expected_reg)); // compare the runtime called class with this descriptor's metadata owner
                ctx.emitter.instruction(&format!("b.ne {}", next));             // try the next compiled descendant when the class id differs
            }
            crate::codegen::platform::Arch::X86_64 => {
                ctx.emitter
                    .instruction(&format!("cmp {}, {}", actual_reg, expected_reg)); // compare the runtime called class with this descriptor's metadata owner
                ctx.emitter.instruction(&format!("jne {}", next));              // try the next compiled descendant when the class id differs
            }
        }
        emit_runtime_descriptor_with_called_class_capture(ctx, descriptor_label, called_class_id)?;
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&next);
    }
    emit_runtime_descriptor_with_called_class_capture(
        ctx,
        fallback_descriptor,
        called_class_id,
    )?;
    ctx.emitter.label(&done);
    Ok(())
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
    let receiver_ty = ctx.value_php_type(receiver)?;
    let PhpType::Object(class_name) = receiver_ty.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "instance first-class callable '{}' with receiver PHP type {:?}",
            target, receiver_ty
        )));
    };
    let normalized_class = class_name.trim_start_matches('\\').to_string();
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
    let sig = class_info
        .methods
        .get(&method_key)
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "instance first-class callable '{}' with unknown method",
                target
            ))
        })?
        .clone();
    let impl_class = class_info
        .method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| normalized_class.clone());
    if !class_method_body_exists(ctx, &impl_class, &method_key) {
        return Err(CodegenIrError::unsupported(format!(
            "instance first-class callable '{}' without emitted method body",
            target
        )));
    }
    let receiver_ty = PhpType::Object(normalized_class.clone());
    let captures = vec![("receiver".to_string(), receiver_ty.clone(), false)];
    let debug_bindings = vec![("this".to_string(), receiver_ty.clone(), false)];
    let entry_label =
        emit_instance_method_descriptor_entry_wrapper(ctx, &impl_class, &method_key, &sig)?;
    let invoker_label = if is_date_serialize_descriptor(ctx, &normalized_class, &method_key) {
        emit_runtime_date_serialize_invoker_inline(ctx, &sig, &captures)
    } else {
        super::runtime_wrappers::emit_method_callable_invoker_inline(
            ctx, &sig, &captures, &impl_class, &method_key,
        )
    };
    let canonical_method_name =
        canonical_first_class_callable_method_name(ctx, &impl_class, method_name);
    let debug_primary_name = format!("{impl_class}::{canonical_method_name}");
    let static_bindings = fake_callable_static_debug_bindings(ctx, &debug_primary_name);
    let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_debug_meta(
        ctx.data,
        &entry_label,
        Some(&debug_primary_name),
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
        Some(callable_descriptor::CallableDebugMetadata {
            flags: callable_descriptor::CALLABLE_DEBUG_FLAG_FAKE_CLOSURE,
            primary_name: &debug_primary_name,
            source_path: None,
            source_line: 0,
            bindings: &debug_bindings,
            static_bindings: &static_bindings,
        }),
    );
    emit_runtime_descriptor_with_receiver_capture(ctx, &descriptor_label, receiver, &receiver_ty)?;
    // `$o->m(...)` is a Closure in PHP and consumes an object handle. The acquire
    // sits here rather than inside the shared descriptor helper because that helper
    // also builds the internal adapter for calling an `__invoke`-able object, and
    // `$obj()` creates no Closure in PHP.
    crate::codegen_support::runtime::emit_acquire_object_handle(ctx.emitter);
    Ok(true)
}

/// Returns whether an instance first-class descriptor captures a DateTime-family serializer.
///
/// User overrides remain included: the shared runtime finalizer guards property projection by the
/// actual method handler and still must inspect whether the declared `array` result is indexed or
/// associative before boxing it for descriptor consumers.
fn is_date_serialize_descriptor(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
) -> bool {
    if method_key != "__serialize" {
        return false;
    }
    let mut current = Some(class_name.trim_start_matches('\\'));
    while let Some(candidate) = current {
        if matches!(
            candidate,
            "DateTime" | "DateTimeImmutable" | "DateTimeZone" | "DateInterval" | "DatePeriod"
        ) {
            return true;
        }
        current = ctx
            .module
            .class_infos
            .get(candidate)
            .and_then(|class_info| class_info.parent.as_deref())
            .map(|parent| parent.trim_start_matches('\\'));
    }
    false
}

/// Resolves the declaration spelling used by php-src when exposing a method-backed fake Closure.
///
/// Class metadata indexes methods by a case-folded lookup key, whereas the Closure `function`
/// field uses the implementing scope plus the declaration's original spelling. User method
/// bodies retain that spelling in EIR; ext/date methods use the frozen php-src declaration table.
pub(super) fn canonical_first_class_callable_method_name(
    ctx: &FunctionContext<'_>,
    impl_class: &str,
    method_name: &str,
) -> String {
    ctx.module
        .class_methods
        .iter()
        .find_map(|function| {
            let (candidate_class, candidate_method) = function.name.rsplit_once("::")?;
            (php_symbol_key(candidate_class) == php_symbol_key(impl_class)
                && candidate_method.eq_ignore_ascii_case(method_name))
            .then(|| candidate_method.to_string())
        })
        .or_else(|| {
            crate::types::php_src_date_method_canonical_name(impl_class, method_name)
                .map(str::to_string)
        })
        .unwrap_or_else(|| method_name.to_string())
}

/// Collects persistent static-local metadata for a first-class callable's resolved function.
pub(super) fn fake_callable_static_debug_bindings(
    ctx: &FunctionContext<'_>,
    function_name: &str,
) -> Vec<callable_descriptor::CallableDebugStaticBinding> {
    ctx.module
        .functions
        .iter()
        .chain(ctx.module.class_methods.iter())
        .find(|function| function.name.eq_ignore_ascii_case(function_name))
        .map(super::core_closures::static_debug_bindings)
        .unwrap_or_default()
}
