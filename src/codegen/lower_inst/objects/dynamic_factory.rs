//! Purpose:
//! Builds constrained dynamic factory metadata and emits lookup failures.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Class ancestry checks and supported runtime-storage classes stay conservative.

use super::*;

/// Parses the fallback and required-parent class names from the EIR data pool.
pub(super) fn dynamic_object_new_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<(String, String)> {
    let data = expect_data(inst)?;
    let value = ctx
        .module
        .data
        .class_names
        .get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("class data", data.as_raw()))?;
    let Some((fallback_class, required_parent)) = value.split_once('|') else {
        return Err(CodegenIrError::invalid_module(format!(
            "dynamic_object_new metadata '{}' missing fallback|required separator",
            value
        )));
    };
    Ok((
        fallback_class.trim_start_matches('\\').to_string(),
        required_parent.trim_start_matches('\\').to_string(),
    ))
}

/// Collects runtime-instantiable classes for this dynamic factory.
pub(super) fn dynamic_new_candidates(
    ctx: &FunctionContext<'_>,
    required_parent: &str,
    arg_count: usize,
    inst: &Instruction,
) -> Result<Vec<DynamicNewCandidate>> {
    let mut candidates = Vec::new();
    let mut sorted_classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    sorted_classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in sorted_classes {
        if !class_is_same_or_descends_from(ctx, class_name, required_parent) {
            continue;
        }
        if let Some(candidate) =
            dynamic_new_candidate(ctx, class_name, class_info, Some(arg_count), inst)?
        {
            candidates.push(candidate);
        }
    }
    Ok(candidates)
}

/// Returns true when `class_name` is the requested parent or one of its descendants.
pub(super) fn class_is_same_or_descends_from(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    required_parent: &str,
) -> bool {
    let mut current = Some(class_name);
    while let Some(name) = current {
        if php_symbol_key(name.trim_start_matches('\\'))
            == php_symbol_key(required_parent.trim_start_matches('\\'))
        {
            return true;
        }
        current = ctx
            .module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Builds one dynamic factory candidate when its constructor path is emitted.
pub(super) fn dynamic_new_candidate(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    class_info: &ClassInfo,
    arg_count: Option<usize>,
    inst: &Instruction,
) -> Result<Option<DynamicNewCandidate>> {
    if let Some(arg_count) = arg_count {
        if let Some(candidate) =
            spl_runtime_storage_dynamic_new_candidate(class_name, class_info, arg_count)
        {
            return Ok(Some(candidate));
        }
    }
    if class_interfaces_require_missing_method_symbols(ctx, class_name, class_info) {
        return Ok(None);
    }
    let constructor_key = php_symbol_key("__construct");
    let mut padding_thunk = None;
    let constructor_impl = if let Some(constructor) = class_info.methods.get(&constructor_key) {
        // A site that passes FEWER arguments than the constructor declares is still a candidate
        // when every omitted parameter has a default and `ir_lower` emitted the padding thunk for
        // this (class, argc) pair. `new $c(…)` cannot be padded by the checker — it does not know
        // which constructor it is — so without this every builtin with an optional parameter fell
        // to the generic allocation path and came back with no constructor run at all.
        if let Some(arg_count) = arg_count {
            // A VARIADIC constructor needs the thunk at EVERY arity, its declared one included:
            // the site passes N separate arguments and the lowered callee takes the collector as
            // ONE array, so no site arity ever matches the callee's frame. Calling it directly
            // handed a scalar where the array belongs and produced EMPTY OUTPUT rather than a
            // diagnostic — the worst row of the measured matrix.
            if constructor.params.len() != arg_count || constructor.variadic.is_some() {
                let thunk = crate::ir_lower::dynamic_constructor_thunk_name(
                    class_info.class_id,
                    arg_count,
                );
                let emitted = ctx
                    .module
                    .functions
                    .iter()
                    .any(|function| function.name == thunk);
                if !emitted {
                    return Ok(None);
                }
                padding_thunk = Some(thunk);
            }
        }
        let impl_class = class_info
            .method_impl_classes
            .get(&constructor_key)
            .cloned()
            .unwrap_or_else(|| class_name.to_string());
        if !class_method_already_emitted(ctx, &impl_class, &constructor_key, false) {
            return Ok(None);
        }
        // When a thunk pads the call, the SITE's arity is what gets materialized — the thunk
        // itself supplies the rest — so the parameter list is truncated to what is passed.
        let materialized = padding_thunk
            .as_ref()
            .and_then(|_| arg_count)
            .unwrap_or(constructor.params.len());
        // SHARED WITH `ir_lower::lower_dynamic_constructor_thunk`, which declares the thunk's
        // parameters from the same `positional_param_type`. Reading `params[index]` directly here
        // would describe a different frame past the regular parameters, where the declared entry
        // is the COLLECTION and each argument materialized at the site is one ELEMENT.
        let Some(param_types) = (0..materialized)
            .map(|index| {
                crate::types::call_args::positional_param_type(constructor, index)
                    .map(|ty| ty.codegen_repr())
            })
            .collect::<Option<Vec<_>>>()
        else {
            return Ok(None);
        };
        let ref_params = (0..materialized)
            .map(|index| constructor.ref_params.get(index).copied().unwrap_or(false))
            .collect::<Vec<_>>();
        // An overflow argument lands in the COLLECTOR, which is materialized AS the declared
        // element type with no conversion, so a site value of another representation is
        // REINTERPRETED rather than rejected: `new $c("x")` on `int ...$r` came back holding
        // `4370954896` — the string's ADDRESS read as an integer — and `"7"` did the same, so it
        // was never limited to values php refuses.
        //
        // Passing the overflow as `Mixed` is not an alternative: the checker refuses a `mixed`
        // argument to a typed variadic outright, even for a value php accepts. So a site that
        // cannot supply the element type as-is drops the class from the ladder, which is a MISSING
        // constructor call — the behaviour this class had before the thunk existed — rather than a
        // fabricated integer.
        if padding_thunk.is_some()
            && dynamic_new_uncastable_collector(constructor, materialized).is_some()
        {
            // NOT constructed, and NOT silently dropped either: `dynamic_new_mixed_refusals` asks
            // the same judge and emits a ladder arm that REPORTS. Leaving only this half in place
            // is what made `new $c("x")` on `int ...$r` answer with a constructor-less object.
            return Ok(None);
        }
        Some(ConstructorCallTarget {
            impl_class,
            param_types,
            ref_params,
            sig: constructor.clone(),
            padding_thunk: padding_thunk.clone(),
        })
    } else if arg_count.is_none_or(|arg_count| arg_count == 0) {
        None
    } else {
        return Ok(None);
    };
    let property_defaults = collect_property_defaults(class_info, inst)?;
    Ok(Some(DynamicNewCandidate {
        class_name: class_name.to_string(),
        class_id: class_info.class_id,
        property_count: class_info.properties.len(),
        allow_dynamic_properties: class_info.allow_dynamic_properties,
        uninitialized_marker_offsets: uninitialized_property_marker_offsets(class_info),
        owned_reference_property_offsets: owned_reference_property_offsets(class_info),
        property_defaults,
        constructor_impl,
    }))
}

/// Builds a dynamic-new candidate that deliberately omits constructor invocation.
pub(super) fn dynamic_new_without_constructor_candidate(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    class_info: &ClassInfo,
    inst: &Instruction,
) -> Result<Option<DynamicNewCandidate>> {
    if class_info.is_abstract || ctx.module.enum_infos.contains_key(class_name) {
        return Ok(None);
    }
    if class_name != "stdClass" && known_dynamic_new_builtin_class_names().contains(&class_name) {
        return Ok(None);
    }
    if class_name == "SplFixedArray" || is_spl_doubly_linked_list_family(class_name) {
        return Ok(None);
    }
    if class_interfaces_require_missing_method_symbols(ctx, class_name, class_info) {
        return Ok(None);
    }
    let property_defaults = collect_property_defaults(class_info, inst)?;
    Ok(Some(DynamicNewCandidate {
        class_name: class_name.to_string(),
        class_id: class_info.class_id,
        property_count: class_info.properties.len(),
        allow_dynamic_properties: class_info.allow_dynamic_properties,
        uninitialized_marker_offsets: uninitialized_property_marker_offsets(class_info),
        owned_reference_property_offsets: owned_reference_property_offsets(class_info),
        property_defaults,
        constructor_impl: None,
    }))
}

/// Builds dynamic-new metadata for SPL classes whose storage is runtime-managed.
pub(super) fn spl_runtime_storage_dynamic_new_candidate(
    class_name: &str,
    class_info: &ClassInfo,
    arg_count: usize,
) -> Option<DynamicNewCandidate> {
    if class_name == "SplFixedArray" {
        if arg_count > 1 {
            return None;
        }
    } else if is_spl_doubly_linked_list_family(class_name) {
        if arg_count != 0 {
            return None;
        }
    } else {
        return None;
    }
    Some(DynamicNewCandidate {
        class_name: class_name.to_string(),
        class_id: class_info.class_id,
        property_count: class_info.properties.len(),
        allow_dynamic_properties: class_info.allow_dynamic_properties,
        uninitialized_marker_offsets: Vec::new(),
        owned_reference_property_offsets: Vec::new(),
        property_defaults: Vec::new(),
        constructor_impl: None,
    })
}

/// Emits the class-string lookup input and calls the shared target-name resolver.
pub(super) fn emit_dynamic_new_class_lookup(
    ctx: &mut FunctionContext<'_>,
    class_name_value: ValueId,
    required_parent: &str,
) -> Result<()> {
    let class_ty = ctx.value_php_type(class_name_value)?.codegen_repr();
    match class_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(class_name_value, ptr_reg, len_reg)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(class_name_value, abi::int_result_reg(ctx.emitter))?;
            emit_dynamic_new_mixed_class_string(ctx, required_parent);
        }
        _ => {
            emit_dynamic_new_fatal(ctx, required_parent);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_instanceof_lookup");
    Ok(())
}

/// Unboxes a mixed class-string or emits the dynamic-factory fatal.
pub(super) fn emit_dynamic_new_mixed_class_string(ctx: &mut FunctionContext<'_>, required_parent: &str) {
    let string_label = ctx.next_label("dynamic_new_class_string");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // runtime tag 1 means the dynamic factory argument is a string
            ctx.emitter.instruction(&format!("b.eq {}", string_label));         // continue only when the boxed factory argument is a class-string
            emit_dynamic_new_fatal(ctx, required_parent);
            ctx.emitter.label(&string_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // runtime tag 1 means the dynamic factory argument is a string
            ctx.emitter.instruction(&format!("je {}", string_label));           // continue only when the boxed factory argument is a class-string
            emit_dynamic_new_fatal(ctx, required_parent);
            ctx.emitter.label(&string_label);
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed string pointer into the lookup input register
        }
    }
}

/// Branches when the dynamic factory lookup failed or named an interface.
pub(super) fn emit_branch_if_dynamic_new_lookup_invalid(ctx: &mut FunctionContext<'_>, invalid_label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // did the dynamic factory class-string resolve to metadata?
            ctx.emitter.instruction(&format!("b.eq {}", invalid_label));        // abort unresolved factory classes before construction
            ctx.emitter.instruction("cmp x2, #0");                              // target kind 0 means a concrete class, not an interface
            ctx.emitter.instruction(&format!("b.ne {}", invalid_label));        // abort interface targets because factories instantiate objects
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the dynamic factory class-string resolve to metadata?
            ctx.emitter.instruction(&format!("je {}", invalid_label));          // abort unresolved factory classes before construction
            ctx.emitter.instruction("test rdx, rdx");                           // target kind 0 means a concrete class, not an interface
            ctx.emitter.instruction(&format!("jne {}", invalid_label));         // abort interface targets because factories instantiate objects
        }
    }
}

/// Preserves the resolved dynamic factory class id for candidate dispatch.
pub(super) fn emit_push_dynamic_new_class_id(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_push_reg(ctx.emitter, "x1"),
        Arch::X86_64 => abi::emit_push_reg(ctx.emitter, "rdi"),
    }
}

/// Branches to `matched_label` when the saved factory class id matches `class_id`.
pub(super) fn emit_compare_dynamic_new_class_id(
    ctx: &mut FunctionContext<'_>,
    class_id: u64,
    matched_label: &str,
) {
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    abi::emit_load_temporary_stack_slot(ctx.emitter, scratch, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, #{}", scratch, class_id)); // compare the requested factory class with this candidate class id
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // branch when the runtime class-string selected this constructor
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", scratch, class_id)); // compare the requested factory class with this candidate class id
            ctx.emitter.instruction(&format!("je {}", matched_label));          // branch when the runtime class-string selected this constructor
        }
    }
}

/// Allocates and initializes one selected dynamic factory candidate.
pub(super) fn emit_dynamic_new_candidate(
    ctx: &mut FunctionContext<'_>,
    candidate: &DynamicNewCandidate,
    constructor_args: &[ValueId],
    result: ValueId,
) -> Result<()> {
    emit_named_class_object_allocation(
        ctx,
        &candidate.class_name,
        candidate.class_id,
        candidate.property_count,
        candidate.allow_dynamic_properties,
        &candidate.uninitialized_marker_offsets,
        &candidate.owned_reference_property_offsets,
    )?;
    ctx.store_result_value(result)?;
    emit_property_defaults(ctx, result, &candidate.property_defaults)?;
    if let Some(constructor) = &candidate.constructor_impl {
        emit_constructor_call(
            ctx,
            result,
            constructor_args,
            &candidate.class_name,
            &constructor.impl_class,
            &php_symbol_key("__construct"),
            &constructor.param_types,
            &constructor.ref_params,
            constructor.padding_thunk.as_deref(),
        )?;
    }
    Ok(())
}

/// Emits the runtime fatal diagnostic for invalid dynamic factory class names.
pub(super) fn emit_dynamic_new_fatal(ctx: &mut FunctionContext<'_>, required_parent: &str) {
    let message = format!(
        "Fatal error: Dynamic factory class must extend {}\n",
        required_parent
    );
    emit_fatal_message(ctx, message.as_bytes());
}

/// Emits PHP's fatal diagnostic for source-level `new $name` with a missing class.
pub(super) fn emit_dynamic_new_class_not_found_fatal(ctx: &mut FunctionContext<'_>) {
    let (prefix_label, prefix_len) = ctx
        .data
        .add_string(b"Fatal error: Uncaught Error: Class \"");
    let (suffix_label, suffix_len) = ctx.data.add_string(b"\" not found\n");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // select stderr for the class-not-found prefix
            ctx.emitter.adrp("x1", &prefix_label);
            ctx.emitter.add_lo12("x1", "x1", &prefix_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", prefix_len));       // pass the class-not-found prefix byte length
            ctx.emitter.syscall(4);
            ctx.emitter.instruction("mov x0, #2");                              // select stderr for the missing class name
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
            ctx.emitter.syscall(4);
            ctx.emitter.instruction("mov x0, #2");                              // select stderr for the class-not-found suffix
            ctx.emitter.adrp("x1", &suffix_label);
            ctx.emitter.add_lo12("x1", "x1", &suffix_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", suffix_len));       // pass the class-not-found suffix byte length
            ctx.emitter.syscall(4);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rsi", &prefix_label);
            ctx.emitter.instruction(&format!("mov edx, {}", prefix_len));       // pass the class-not-found prefix byte length
            ctx.emitter.instruction("mov edi, 2");                              // select stderr for the class-not-found prefix
            ctx.emitter.instruction("mov eax, 1");                              // select Linux write syscall for the prefix
            ctx.emitter.instruction("syscall");                                 // write the class-not-found prefix
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", 8);
            ctx.emitter.instruction("mov edi, 2");                              // select stderr for the missing class name
            ctx.emitter.instruction("mov eax, 1");                              // select Linux write syscall for the class name
            ctx.emitter.instruction("syscall");                                 // write the missing class name
            abi::emit_symbol_address(ctx.emitter, "rsi", &suffix_label);
            ctx.emitter.instruction(&format!("mov edx, {}", suffix_len));       // pass the class-not-found suffix byte length
            ctx.emitter.instruction("mov edi, 2");                              // select stderr for the class-not-found suffix
            ctx.emitter.instruction("mov eax, 1");                              // select Linux write syscall for the suffix
            ctx.emitter.instruction("syscall");                                 // write the class-not-found suffix
        }
    }
    abi::emit_exit(ctx.emitter, 1);
}

/// Emits PHP's fatal diagnostic for `new $name` when the class expression is not a string.
pub(super) fn emit_dynamic_new_invalid_class_name_fatal(ctx: &mut FunctionContext<'_>) {
    emit_fatal_message(
        ctx,
        b"Fatal error: Uncaught Error: Class name must be a valid object or a string\n",
    );
}

/// Writes a fatal diagnostic to stderr and exits.
pub(super) fn emit_fatal_message(ctx: &mut FunctionContext<'_>, message: &[u8]) {
    let (message_label, message_len) = ctx.data.add_string(message);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // select stderr for the fatal diagnostic
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter
                .instruction(&format!("mov x2, #{}", message_len)); // pass the fatal diagnostic byte length to write()
            ctx.emitter.syscall(4);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter
                .instruction(&format!("mov edx, {}", message_len)); // pass the fatal diagnostic byte length to write()
            ctx.emitter.instruction("mov edi, 2");                              // select stderr for the fatal diagnostic
            ctx.emitter.instruction("mov eax, 1");                              // select Linux write syscall
            ctx.emitter.instruction("syscall");                                 // write the fatal diagnostic bytes
        }
    }
    abi::emit_exit(ctx.emitter, 1);
}
