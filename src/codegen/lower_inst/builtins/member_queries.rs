//! Purpose:
//! Lowers class-like existence, callable, method, and property metadata queries.
//!
//! Called from:
//! - `super` runtime-function and language-construct dispatch.
//!
//! Key details:
//! - Preserves PHP case-insensitive lookup, visibility, inheritance, and eval-bridge fallbacks.

use super::*;

/// Lowers AOT class/interface/enum existence checks for literal or dynamic string names.
pub(crate) fn lower_class_like_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    ensure_arg_count_between(inst, name, 1, 2)?;
    let value = expect_operand(inst, 0)?;
    if let Some(symbol_name) = maybe_const_string_operand(ctx, value)? {
        let exists = match name {
            "class_exists" => contains_folded(
                ctx.module
                    .class_infos
                    .keys()
                    .filter(|class_name| !is_internal_synthetic_class_name(class_name)),
                &symbol_name,
            ),
            "interface_exists" => contains_folded(ctx.module.interface_infos.keys(), &symbol_name),
            "trait_exists" => contains_folded(ctx.module.trait_table.names.iter(), &symbol_name),
            "enum_exists" => contains_folded(ctx.module.enum_infos.keys(), &symbol_name),
            _ => false,
        };
        emit_static_bool(ctx, exists);
    } else {
        lower_dynamic_class_like_exists(ctx, name, value)?;
    }
    store_if_result(ctx, inst)
}

/// Lowers a dynamic string `class_exists()`-family lookup against known AOT metadata.
pub(in crate::codegen::lower_inst) fn lower_dynamic_class_like_exists(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    value: ValueId,
) -> Result<()> {
    if ctx.value_php_type(value)?.codegen_repr() != PhpType::Str {
        return Err(CodegenIrError::unsupported(format!(
            "{} with non-string dynamic name",
            name
        )));
    }
    let candidates = dynamic_class_like_exists_candidates(ctx, name);
    if candidates.is_empty() {
        emit_static_bool(ctx, false);
        return Ok(());
    }

    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    let matched_label = ctx.next_label(&format!("{}_dynamic_match", name));
    let done_label = ctx.next_label(&format!("{}_dynamic_done", name));
    for candidate in candidates {
        emit_branch_if_dynamic_class_like_exists_candidate(ctx, &candidate, &matched_label);
    }
    emit_static_bool(ctx, false);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&matched_label);
    emit_static_bool(ctx, true);

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Collects deterministic class-like name candidates for a dynamic existence lookup.
pub(in crate::codegen::lower_inst) fn dynamic_class_like_exists_candidates(ctx: &FunctionContext<'_>, name: &str) -> Vec<String> {
    let mut candidates = BTreeSet::new();
    match name {
        "class_exists" => {
            candidates.extend(
                ctx.module
                    .class_infos
                    .keys()
                    .filter(|class_name| !is_internal_synthetic_class_name(class_name))
                    .cloned(),
            );
        }
        "interface_exists" => candidates.extend(ctx.module.interface_infos.keys().cloned()),
        "trait_exists" => candidates.extend(ctx.module.trait_table.names.iter().cloned()),
        "enum_exists" => candidates.extend(ctx.module.enum_infos.keys().cloned()),
        _ => {}
    }
    candidates.into_iter().collect()
}

/// Branches when the saved dynamic class-like string matches a metadata candidate.
pub(in crate::codegen::lower_inst) fn emit_branch_if_dynamic_class_like_exists_candidate(
    ctx: &mut FunctionContext<'_>,
    candidate: &str,
    matched_label: &str,
) {
    let bare_candidate = candidate.trim_start_matches('\\');
    emit_branch_if_saved_string_matches_ci(ctx, bare_candidate.as_bytes(), matched_label);
    let qualified_candidate = format!("\\{}", bare_candidate);
    emit_branch_if_saved_string_matches_ci(ctx, qualified_candidate.as_bytes(), matched_label);
}

/// Emits one case-insensitive comparison of the string saved in the current 16-byte temporary
/// stack slot (`[sp] = pointer`, `[sp + 8] = length`) against a baked candidate, branching to
/// `matched_label` on equality.
///
/// Shared by the dynamic `class_exists()`-family lookup and dynamic `extension_loaded()`. The
/// caller owns the temporary slot: it must push the runtime string with `emit_push_reg_pair`
/// before the first comparison and release 16 bytes once all comparisons are done. `__rt_strcasecmp`
/// is a leaf helper (no nested call, no SP adjustment) that reads its operands from
/// `x1/x2/x3/x4` on AArch64 and `rdi/rsi/rdx/rcx` on x86_64 and clobbers only caller-saved
/// scratch, so the slot and SP are intact across every emitted comparison.
pub(in crate::codegen::lower_inst) fn emit_branch_if_saved_string_matches_ci(
    ctx: &mut FunctionContext<'_>,
    candidate: &[u8],
    matched_label: &str,
) {
    let (candidate_label, candidate_len) = ctx.data.add_string(candidate);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
            abi::emit_symbol_address(ctx.emitter, "x3", &candidate_label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", candidate_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("cmp x0, #0");                              // did the dynamic class-like name match this metadata entry?
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // report existence when the runtime name matches case-insensitively
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 8);
            abi::emit_symbol_address(ctx.emitter, "rdx", &candidate_label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", candidate_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("test rax, rax");                           // did the dynamic class-like name match this metadata entry?
            ctx.emitter.instruction(&format!("je {}", matched_label));          // report existence when the runtime name matches case-insensitively
        }
    }
}

/// Lowers `is_callable(value)` through static lookup or runtime callable-shape helpers.
pub(crate) fn lower_is_callable(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_callable", 1)?;
    let strict_php = super::instruction_strict_php_profile(inst);
    set_is_callable_strict_profile(ctx, strict_php);
    let value = expect_operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    if has_eval_context(ctx) && value_ty != PhpType::Callable {
        return lower_eval_is_callable(ctx, inst, value);
    }
    match value_ty {
        PhpType::Callable => emit_static_bool(ctx, true),
        PhpType::Str => {
            if let Ok(function_name) = const_string_operand(ctx, value) {
                if let Some((class_name, method_name)) = function_name.rsplit_once("::") {
                    emit_static_bool(ctx, static_method_string_is_callable(ctx, class_name, method_name));
                } else {
                    emit_static_bool(
                        ctx,
                        callable_name_exists(ctx, &function_name, strict_php),
                    );
                }
            } else {
                ctx.load_value_to_result(value)?;
                emit_is_callable_dynamic_string_lookup(ctx);
            }
        }
        PhpType::Array(_) => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_array");
        }
        PhpType::AssocArray { .. } => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_assoc");
        }
        PhpType::Object(_) => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_object");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_mixed");
        }
        PhpType::Iterable => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_heap");
        }
        PhpType::Int
        | PhpType::Bool
        | PhpType::False
        | PhpType::Float
        | PhpType::Void
        | PhpType::Never
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Resource(_)
        | PhpType::TaggedScalar => {
            emit_static_bool(ctx, false);
        }
    }
    store_if_result(ctx, inst)
}

/// Calls the runtime `is_callable` helper for pointer-shaped values already in result regs.
pub(in crate::codegen::lower_inst) fn emit_is_callable_pointer_lookup(ctx: &mut FunctionContext<'_>, label: &str) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // move pointer-shaped value into helper argument 0
    }
    abi::emit_call_label(ctx.emitter, label);
}

/// Stores the current `is_callable()` call site's builtin visibility for nested runtime helpers.
pub(in crate::codegen::lower_inst) fn set_is_callable_strict_profile(ctx: &mut FunctionContext<'_>, strict_php: bool) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_callable_strict_profile");
            abi::emit_load_int_immediate(ctx.emitter, "x10", i64::from(strict_php));
            ctx.emitter.instruction("str x10, [x9]");                           // publish the call-site builtin visibility for nested string probes
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r10", "_callable_strict_profile");
            abi::emit_load_int_immediate(ctx.emitter, "r11", i64::from(strict_php));
            ctx.emitter.instruction("mov QWORD PTR [r10], r11");                // publish the call-site builtin visibility for nested string probes
        }
    }
}

/// Calls the runtime `is_callable` string-name helper for a loaded dynamic string value.
pub(in crate::codegen::lower_inst) fn emit_is_callable_dynamic_string_lookup(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // move string pointer into helper argument 0
            ctx.emitter.instruction("mov x1, x2");                              // move string length into helper argument 1
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // move string pointer into helper argument 0
            ctx.emitter.instruction("mov rsi, rdx");                            // move string length into helper argument 1
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_is_callable_string");
}

/// Lowers `method_exists()` and `property_exists()` through eval or static metadata.
pub(in crate::codegen::lower_inst) fn lower_member_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    ensure_arg_count(inst, name, 2)?;
    let target = expect_operand(inst, 0)?;
    let member = expect_operand(inst, 1)?;
    if has_eval_context(ctx) {
        return lower_eval_member_exists(ctx, inst, target, member, name);
    }
    let member_name = const_string_operand(ctx, member)?;
    let exists = match ctx.value_php_type(target)?.codegen_repr() {
        PhpType::Object(class_name) => {
            static_member_exists_on_class(ctx, &class_name, &member_name, name, true)
        }
        PhpType::Str => {
            let class_name = const_string_operand(ctx, target)?;
            static_member_exists_on_class(ctx, &class_name, &member_name, name, false)
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} target PHP type {:?}",
                name, other
            )))
        }
    };
    emit_static_bool(ctx, exists);
    store_if_result(ctx, inst)
}

/// Checks one static class-like target for `method_exists()` or `property_exists()`.
pub(in crate::codegen::lower_inst) fn static_member_exists_on_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    member_name: &str,
    name: &str,
    target_is_object: bool,
) -> bool {
    let Some((resolved_class, class_info)) = lookup_class_info(ctx, class_name) else {
        return false;
    };
    match name {
        "method_exists" => static_method_exists_on_class_info(
            ctx,
            &resolved_class,
            class_info,
            member_name,
            target_is_object,
        ),
        "property_exists" => static_property_exists_on_class_info(
            &resolved_class,
            class_info,
            member_name,
            target_is_object,
        ),
        _ => false,
    }
}

/// Looks up class metadata using PHP's case-insensitive class-name semantics.
pub(in crate::codegen::lower_inst) fn lookup_class_info<'a>(
    ctx: &'a FunctionContext<'_>,
    class_name: &str,
) -> Option<(String, &'a ClassInfo)> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.module
        .class_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == class_key)
        .map(|(candidate, class_info)| (candidate.clone(), class_info))
}

/// Checks static method metadata while hiding inherited private methods on class-string targets.
pub(in crate::codegen::lower_inst) fn static_method_exists_on_class_info(
    ctx: &FunctionContext<'_>,
    resolved_class: &str,
    class_info: &ClassInfo,
    method_name: &str,
    target_is_object: bool,
) -> bool {
    if !crate::codegen_support::callable_dispatch::runtime_method_callable_visible(method_name) {
        return false;
    }
    let method_key = php_symbol_key(method_name);
    if class_info.methods.contains_key(&method_key) {
        return target_is_object
            || method_visible_from_class_string(
                resolved_class,
                &method_key,
                &class_info.method_visibilities,
                &class_info.method_declaring_classes,
            );
    }
    if class_info.static_methods.contains_key(&method_key) {
        return target_is_object
            || method_visible_from_class_string(
                resolved_class,
                &method_key,
                &class_info.static_method_visibilities,
                &class_info.static_method_declaring_classes,
            );
    }
    if target_is_object {
        return static_parent_chain_method_exists(ctx, class_info, &method_key);
    }
    false
}

/// Checks parent class metadata for private methods visible to object-target `method_exists()`.
pub(in crate::codegen::lower_inst) fn static_parent_chain_method_exists(
    ctx: &FunctionContext<'_>,
    class_info: &ClassInfo,
    method_key: &str,
) -> bool {
    let mut visited = BTreeSet::new();
    let mut parent_name = class_info.parent.as_deref();
    while let Some(candidate) = parent_name {
        let parent_key = php_symbol_key(candidate.trim_start_matches('\\'));
        if !visited.insert(parent_key) {
            return false;
        }
        let Some((_resolved_class, parent_info)) = lookup_class_info(ctx, candidate) else {
            return false;
        };
        if parent_info.methods.contains_key(method_key)
            || parent_info.static_methods.contains_key(method_key)
        {
            return true;
        }
        parent_name = parent_info.parent.as_deref();
    }
    false
}

/// Returns whether a method should be visible for a class-string member probe.
pub(in crate::codegen::lower_inst) fn method_visible_from_class_string(
    resolved_class: &str,
    method_key: &str,
    visibilities: &std::collections::HashMap<String, Visibility>,
    declaring_classes: &std::collections::HashMap<String, String>,
) -> bool {
    visibilities.get(method_key) != Some(&Visibility::Private)
        || declaring_classes
            .get(method_key)
            .is_none_or(|declaring_class| {
                php_symbol_key(declaring_class.trim_start_matches('\\'))
                    == php_symbol_key(resolved_class.trim_start_matches('\\'))
            })
}

/// Checks static property metadata while hiding inherited private properties.
pub(in crate::codegen::lower_inst) fn static_property_exists_on_class_info(
    resolved_class: &str,
    class_info: &ClassInfo,
    property_name: &str,
    target_is_object: bool,
) -> bool {
    let class_key = php_symbol_key(resolved_class.trim_start_matches('\\'));
    if class_key == "dateinterval" {
        return target_is_object
            && matches!(
                property_name,
                "y" | "m" | "d" | "h" | "i" | "s" | "f" | "invert" | "days"
            );
    }
    if let Some(property_names) = crate::types::php_src_date_property_names(resolved_class) {
        return property_names.contains(&property_name);
    }
    property_visible_from_class_string(
        resolved_class,
        property_name,
        &class_info.property_visibilities,
        &class_info.property_declaring_classes,
    ) || property_visible_from_class_string(
        resolved_class,
        property_name,
        &class_info.static_property_visibilities,
        &class_info.static_property_declaring_classes,
    )
}

/// Returns whether a property exists for a class-string or ordinary object probe.
pub(in crate::codegen::lower_inst) fn property_visible_from_class_string(
    resolved_class: &str,
    property_name: &str,
    visibilities: &std::collections::HashMap<String, Visibility>,
    declaring_classes: &std::collections::HashMap<String, String>,
) -> bool {
    let Some(visibility) = visibilities.get(property_name) else {
        return false;
    };
    visibility != &Visibility::Private
        || declaring_classes
            .get(property_name)
            .is_none_or(|declaring_class| {
                php_symbol_key(declaring_class.trim_start_matches('\\'))
                    == php_symbol_key(resolved_class.trim_start_matches('\\'))
            })
}

/// Returns true when a static `Class::method` string names a public static method.
pub(in crate::codegen::lower_inst) fn static_method_string_is_callable(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
) -> bool {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    let Some((_, class_info)) = ctx.module.class_infos.iter().find(|(candidate, _)| {
        php_symbol_key(candidate.trim_start_matches('\\')) == class_key
    }) else {
        return false;
    };
    let method_key = php_symbol_key(method_name);
    if !class_info.static_methods.contains_key(&method_key) {
        return false;
    }
    class_info.static_method_visibilities.get(&method_key) == Some(&Visibility::Public)
}
