//! Purpose:
//! Reflection owner detection and runtime-class dispatch.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Returns true for reflection owner classes that need metadata-aware construction.
pub(in crate::codegen::lower_inst::objects) fn is_reflection_owner_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "ReflectionClass"
            | "ReflectionObject"
            | "ReflectionExtension"
            | "ReflectionFunction"
            | "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionParameter"
            | "ReflectionClassConstant"
            | "ReflectionEnum"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    )
}

/// Lowers builtin Reflection owner allocation by populating compile-time metadata slots.
pub(in crate::codegen::lower_inst::objects) fn lower_reflection_owner_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
) -> Result<()> {
    if class_name == "ReflectionExtension" {
        return lower_reflection_extension_new(ctx, inst);
    }
    if class_name == "ReflectionClass" {
        return lower_reflection_class_new(ctx, inst);
    }
    if class_name == "ReflectionEnum" {
        return lower_reflection_enum_new(ctx, inst);
    }
    if class_name == "ReflectionFunction" {
        return lower_reflection_function_new(ctx, inst);
    }
    if let Some(object_operand) = reflection_object_operand(ctx, class_name, inst)? {
        emit_reflection_owner_from_runtime_object(ctx, class_name, object_operand)?;
    } else {
        let metadata = reflection_owner_metadata(ctx, class_name, inst)?;
        emit_reflection_owner_object(ctx, class_name, &metadata)?;
    }
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("reflection object_new missing result"))?;
    ctx.store_result_value(result)
}

/// Allocates `ReflectionClass` from an object, a static name, or a bounded DOM runtime name.
fn lower_reflection_class_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let Some(value) = inst.operands.first().copied() else {
        let metadata = empty_reflection_metadata();
        emit_reflection_owner_object(ctx, "ReflectionClass", &metadata)?;
        let result = inst.result.ok_or_else(|| {
            CodegenIrError::invalid_module("reflection object_new missing result")
        })?;
        return ctx.store_result_value(result);
    };

    if let Some(object_operand) = reflection_object_operand(ctx, "ReflectionClass", inst)? {
        emit_reflection_owner_from_runtime_object(ctx, "ReflectionClass", object_operand)?;
    } else if let Some(name) = const_optional_string_operand(ctx, value, "ReflectionClass")? {
        let metadata = reflection_class_metadata_for_name(ctx, &name)?;
        if metadata.reflected_name.is_none() {
            super::super::super::exceptions::emit_reflection_class_exception(
                ctx,
                &format!("Class \"{}\" does not exist", name),
            );
            return Ok(());
        }
        if !emit_shared_reflection_owner_factory(ctx, "ReflectionClass", &name, false)? {
            emit_reflection_owner_object(ctx, "ReflectionClass", &metadata)?;
        }
    } else {
        emit_runtime_dom_reflection_class(ctx, value)?;
    }

    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("reflection object_new missing result"))?;
    ctx.store_result_value(result)
}

/// Allocates `ReflectionEnum` from a static name or one of the bounded DOM runtime names.
fn lower_reflection_enum_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let Some(value) = inst.operands.first().copied() else {
        let metadata = empty_reflection_metadata();
        emit_reflection_owner_object(ctx, "ReflectionEnum", &metadata)?;
        let result = inst.result.ok_or_else(|| {
            CodegenIrError::invalid_module("reflection object_new missing result")
        })?;
        return ctx.store_result_value(result);
    };

    if const_optional_string_operand(ctx, value, "ReflectionEnum")?.is_some() {
        let metadata = reflection_enum_metadata(ctx, inst)?;
        if let Some(reflected_name) = metadata.reflected_name.as_deref() {
            if !emit_shared_reflection_owner_factory(ctx, "ReflectionEnum", reflected_name, false)? {
                emit_reflection_owner_object(ctx, "ReflectionEnum", &metadata)?;
            }
        } else {
            emit_reflection_owner_object(ctx, "ReflectionEnum", &metadata)?;
        }
    } else {
        emit_runtime_dom_reflection_enum(ctx, value)?;
    }

    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("reflection object_new missing result"))?;
    ctx.store_result_value(result)
}

/// Selects bounded DOM enum metadata from a case-insensitive runtime enum name.
fn emit_runtime_dom_reflection_enum(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let mut enums = ctx
        .module
        .enum_infos
        .keys()
        .filter(|name| reflection_extension_name_for_class(name).is_some())
        .cloned()
        .collect::<Vec<_>>();
    enums.sort_unstable();
    let done_label = ctx.next_label("reflection_enum_done");
    let case_labels = enums
        .iter()
        .map(|_| ctx.next_label("reflection_enum_case"))
        .collect::<Vec<_>>();

    for (enum_name, label) in enums.iter().zip(case_labels.iter()) {
        emit_reflection_class_name_compare(ctx, value, enum_name, label)?;
        emit_reflection_class_name_compare(ctx, value, &format!("\\{}", enum_name), label)?;
    }
    emit_runtime_reflection_class_exception(ctx, value)?;

    for (enum_name, label) in enums.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        if !emit_shared_reflection_owner_factory(ctx, "ReflectionEnum", enum_name, false)? {
            let metadata = reflection_enum_metadata_for_name(ctx, enum_name)?;
            emit_reflection_owner_object(ctx, "ReflectionEnum", &metadata)?;
        }
        emit_reflection_dispatch_jump(ctx, &done_label);
    }

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Selects static DOM ReflectionClass metadata from a case-insensitive runtime class name.
fn emit_runtime_dom_reflection_class(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let mut classes = ctx
        .module
        .class_infos
        .keys()
        .filter(|name| reflection_extension_name_for_class(name) == Some("dom"))
        .cloned()
        .collect::<Vec<_>>();
    classes.sort_unstable();
    let done_label = ctx.next_label("reflection_class_done");
    let case_labels = classes
        .iter()
        .map(|_| ctx.next_label("reflection_class_case"))
        .collect::<Vec<_>>();

    for (class, label) in classes.iter().zip(case_labels.iter()) {
        emit_reflection_class_name_compare(ctx, value, class, label)?;
        emit_reflection_class_name_compare(ctx, value, &format!("\\{}", class), label)?;
    }
    emit_runtime_reflection_class_exception(ctx, value)?;

    for (class, label) in classes.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        if !emit_shared_reflection_owner_factory(ctx, "ReflectionClass", class, false)? {
            let metadata = reflection_class_metadata_for_name(ctx, class)?;
            emit_reflection_owner_object(ctx, "ReflectionClass", &metadata)?;
        }
        emit_reflection_dispatch_jump(ctx, &done_label);
    }

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Builds PHP's missing-class message from the rejected dynamic ReflectionClass name.
fn emit_runtime_reflection_class_exception(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    const PREFIX: &str = "Class \"";
    const SUFFIX: &str = "\" does not exist";

    let (prefix_label, prefix_len) = ctx.data.add_string(PREFIX.as_bytes());
    let (suffix_label, suffix_len) = ctx.data.add_string(SUFFIX.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", prefix_len as i64);
            ctx.load_string_value_to_regs(value, "x3", "x4")?;
            abi::emit_call_label(ctx.emitter, "__rt_concat");
            abi::emit_symbol_address(ctx.emitter, "x3", &suffix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", suffix_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_concat");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rax", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", prefix_len as i64);
            ctx.load_string_value_to_regs(value, "rdi", "rsi")?;
            abi::emit_call_label(ctx.emitter, "__rt_concat");
            abi::emit_symbol_address(ctx.emitter, "rdi", &suffix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", suffix_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_concat");
        }
    }
    super::super::super::exceptions::emit_reflection_class_exception_from_string_result(ctx);
    Ok(())
}

/// Branches to a DOM ReflectionClass metadata case for a case-insensitive runtime name.
fn emit_reflection_class_name_compare(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    class_name: &str,
    matched_label: &str,
) -> Result<()> {
    let (label, len) = ctx.data.add_string(class_name.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(value, "x1", "x2")?;
            abi::emit_symbol_address(ctx.emitter, "x3", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            abi::emit_branch_if_int_result_zero(ctx.emitter, matched_label);
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(value, "rdi", "rsi")?;
            abi::emit_symbol_address(ctx.emitter, "rdx", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            abi::emit_branch_if_int_result_zero(ctx.emitter, matched_label);
        }
    }
    Ok(())
}

/// Allocates a bounded DOM-family ReflectionExtension from literal or runtime string input.
fn lower_reflection_extension_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let Some(value) = inst.operands.first().copied() else {
        let metadata = empty_reflection_metadata();
        emit_reflection_owner_object(ctx, "ReflectionExtension", &metadata)?;
        let result = inst.result.ok_or_else(|| {
            CodegenIrError::invalid_module("reflection object_new missing result")
        })?;
        return ctx.store_result_value(result);
    };

    if let Some(name) = const_optional_string_operand(ctx, value, "ReflectionExtension")? {
        let metadata = reflection_extension_metadata_for_name(&name)?;
        if metadata.reflected_name.is_none() {
            super::super::super::exceptions::emit_reflection_exception(
                ctx,
                &format!("Extension \"{}\" does not exist", name),
            );
            return Ok(());
        }
        emit_reflection_extension_factory(ctx, &name)?;
    } else {
        emit_runtime_reflection_extension(ctx, value)?;
    }

    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("reflection object_new missing result"))?;
    ctx.store_result_value(result)
}

/// Selects bounded ReflectionExtension metadata by a case-insensitive runtime name.
fn emit_runtime_reflection_extension(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let extensions = ["dom", "libxml", "SimpleXML"];
    let done_label = ctx.next_label("reflection_extension_done");
    let case_labels = extensions
        .iter()
        .map(|_| ctx.next_label("reflection_extension_case"))
        .collect::<Vec<_>>();

    for (extension, label) in extensions.iter().zip(case_labels.iter()) {
        emit_reflection_extension_name_compare(ctx, value, extension, label)?;
    }
    emit_runtime_reflection_extension_exception(ctx, value)?;

    for (extension, label) in extensions.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        emit_reflection_extension_factory(ctx, extension)?;
        emit_reflection_dispatch_jump(ctx, &done_label);
    }

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Builds PHP's unknown-extension message from the rejected runtime name and throws it.
///
/// The dynamic name is reloaded after every case-insensitive comparison because `__rt_strcasecmp`
/// may clobber caller-saved registers; `__rt_concat` returns the composed bytes in the canonical
/// target-specific string-result registers consumed by the dynamic throwable emitter.
fn emit_runtime_reflection_extension_exception(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    const PREFIX: &str = "Extension \"";
    const SUFFIX: &str = "\" does not exist";

    let (prefix_label, prefix_len) = ctx.data.add_string(PREFIX.as_bytes());
    let (suffix_label, suffix_len) = ctx.data.add_string(SUFFIX.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", prefix_len as i64);
            ctx.load_string_value_to_regs(value, "x3", "x4")?;
            abi::emit_call_label(ctx.emitter, "__rt_concat");
            abi::emit_symbol_address(ctx.emitter, "x3", &suffix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", suffix_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_concat");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rax", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", prefix_len as i64);
            ctx.load_string_value_to_regs(value, "rdi", "rsi")?;
            abi::emit_call_label(ctx.emitter, "__rt_concat");
            abi::emit_symbol_address(ctx.emitter, "rdi", &suffix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", suffix_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_concat");
        }
    }
    super::super::super::exceptions::emit_reflection_exception_from_string_result(ctx);
    Ok(())
}

/// Branches to one extension case when the runtime string matches its canonical name.
fn emit_reflection_extension_name_compare(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    extension: &str,
    matched_label: &str,
) -> Result<()> {
    let (label, len) = ctx.data.add_string(extension.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(value, "x1", "x2")?;
            abi::emit_symbol_address(ctx.emitter, "x3", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            abi::emit_branch_if_int_result_zero(ctx.emitter, matched_label);
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(value, "rdi", "rsi")?;
            abi::emit_symbol_address(ctx.emitter, "rdx", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            abi::emit_branch_if_int_result_zero(ctx.emitter, matched_label);
        }
    }
    Ok(())
}

/// Returns the constructor object operand for ReflectionClass/Object object reflection.
pub(super) fn reflection_object_operand(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<Option<ValueId>> {
    if !matches!(class_name, "ReflectionClass" | "ReflectionObject") {
        return Ok(None);
    }
    let Some(object_operand) = inst.operands.first().copied() else {
        return Ok(None);
    };
    if matches!(ctx.value_php_type(object_operand)?, PhpType::Object(_)) {
        Ok(Some(object_operand))
    } else {
        Ok(None)
    }
}

/// Materializes ReflectionClass/Object metadata by dispatching on the object's runtime class id.
pub(super) fn emit_reflection_owner_from_runtime_object(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    object_operand: ValueId,
) -> Result<()> {
    let candidates = reflection_runtime_class_candidates(ctx, object_operand)?;
    if candidates.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "{} constructor for object with no known runtime class candidates",
            class_name
        )));
    }

    let fallback_label = ctx.next_label("reflection_object_fallback");
    let done_label = ctx.next_label("reflection_object_done");
    let case_labels = candidates
        .iter()
        .map(|_| ctx.next_label("reflection_object_case"))
        .collect::<Vec<_>>();

    emit_runtime_object_class_dispatch(ctx, object_operand, &candidates, &case_labels, &fallback_label)?;

    if !emit_shared_reflection_owner_factory(ctx, class_name, &candidates[0].class_name, false)? {
        let fallback_metadata = reflection_class_metadata_for_name(ctx, &candidates[0].class_name)?;
        emit_reflection_owner_object(ctx, class_name, &fallback_metadata)?;
    }
    emit_reflection_dispatch_jump(ctx, &done_label);                            // skip runtime reflection candidates after fallback allocation

    for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        if !emit_shared_reflection_owner_factory(ctx, class_name, &candidate.class_name, false)? {
            let metadata = reflection_class_metadata_for_name(ctx, &candidate.class_name)?;
            emit_reflection_owner_object(ctx, class_name, &metadata)?;
        }
        emit_reflection_dispatch_jump(ctx, &done_label);                        // finish after materializing the matched runtime class
    }

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits an unconditional jump for the reflection runtime-class dispatch.
pub(super) fn emit_reflection_dispatch_jump(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", label));                   // continue after the selected reflection object is ready
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", label));                 // continue after the selected reflection object is ready
        }
    }
}

/// Emits target-specific class-id comparisons for runtime object reflection.
pub(super) fn emit_runtime_object_class_dispatch(
    ctx: &mut FunctionContext<'_>,
    object_operand: ValueId,
    candidates: &[ReflectionRuntimeClassCandidate],
    case_labels: &[String],
    fallback_label: &str,
) -> Result<()> {
    ctx.load_value_to_result(object_operand)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_branch_if_int_result_zero(ctx.emitter, fallback_label);
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the object's concrete runtime class id
            for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", candidate.class_id as i64);
                abi::emit_branch_if_int_regs_equal(ctx.emitter, "x9", "x10", label);
            }
            ctx.emitter.instruction(&format!("b {}", fallback_label));          // fall back when no generated candidate matches
        }
        Arch::X86_64 => {
            abi::emit_branch_if_int_result_zero(ctx.emitter, fallback_label);
            ctx.emitter.instruction("mov r11, QWORD PTR [rax]");                // load the object's concrete runtime class id
            for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", candidate.class_id as i64);
                abi::emit_branch_if_int_regs_equal(ctx.emitter, "r11", "r10", label);
            }
            ctx.emitter.instruction(&format!("jmp {}", fallback_label));        // fall back when no generated candidate matches
        }
    }
    ctx.emitter.label(fallback_label);
    Ok(())
}

/// Returns runtime class candidates compatible with the object's static type metadata.
pub(super) fn reflection_runtime_class_candidates(
    ctx: &FunctionContext<'_>,
    object_operand: ValueId,
) -> Result<Vec<ReflectionRuntimeClassCandidate>> {
    let static_type = reflection_object_static_type_name(ctx, object_operand)?;
    let mut candidates = ctx
        .module
        .class_infos
        .iter()
        .filter(|(class_name, _)| reflection_class_matches_object_type(ctx, class_name, &static_type))
        .map(|(class_name, class_info)| ReflectionRuntimeClassCandidate {
            class_name: class_name.clone(),
            class_id: class_info.class_id,
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| candidate.class_id);
    candidates.dedup_by_key(|candidate| candidate.class_id);
    Ok(candidates)
}

/// Resolves the static object type name used to bound runtime ReflectionObject dispatch.
pub(super) fn reflection_object_static_type_name(
    ctx: &FunctionContext<'_>,
    object_operand: ValueId,
) -> Result<String> {
    match ctx.value_php_type(object_operand)? {
        PhpType::Object(class_name) if class_name.is_empty() => reflection_current_method_class(ctx)
            .map(str::to_string)
            .ok_or_else(|| {
                CodegenIrError::unsupported(
                    "ReflectionObject constructor for object with unknown static class",
                )
            }),
        PhpType::Object(class_name) => Ok(class_name),
        other => Err(CodegenIrError::unsupported(format!(
            "ReflectionObject constructor for PHP type {:?}",
            other
        ))),
    }
}

/// Returns the lexical class name encoded in the current EIR method name, if any.
pub(super) fn reflection_current_method_class<'a>(ctx: &'a FunctionContext<'_>) -> Option<&'a str> {
    ctx.function
        .name
        .rsplit_once("::")
        .map(|(class_name, _)| class_name)
}

/// Returns true when a runtime candidate class can inhabit the operand's static object type.
pub(super) fn reflection_class_matches_object_type(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    static_type: &str,
) -> bool {
    if reflection_same_php_type_name(class_name, static_type) {
        return true;
    }
    if resolve_reflection_interface(ctx, static_type).is_some() {
        return reflection_class_implements_interface(ctx, class_name, static_type);
    }
    reflection_class_extends_class(ctx, class_name, static_type)
}

/// Returns true when two PHP type names compare case-insensitively after namespace trimming.
pub(super) fn reflection_same_php_type_name(left: &str, right: &str) -> bool {
    php_symbol_key(left.trim_start_matches('\\')) == php_symbol_key(right.trim_start_matches('\\'))
}

/// Returns true when a runtime class candidate is or extends `target_class`.
pub(super) fn reflection_class_extends_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    target_class: &str,
) -> bool {
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        if reflection_same_php_type_name(&name, target_class) {
            return true;
        }
        current = resolve_reflection_class(ctx, &name)
            .and_then(|(_, class_info)| class_info.parent.clone());
    }
    false
}

/// Returns true when a runtime class candidate implements the requested interface.
pub(super) fn reflection_class_implements_interface(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    target_interface: &str,
) -> bool {
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        let Some((_, class_info)) = resolve_reflection_class(ctx, &name) else {
            return false;
        };
        if class_info.interfaces.iter().any(|interface_name| {
            reflection_interface_extends_interface(ctx, interface_name, target_interface)
        }) {
            return true;
        }
        current = class_info.parent.clone();
    }
    false
}

/// Returns true when an interface is or extends the requested interface target.
pub(super) fn reflection_interface_extends_interface(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    target_interface: &str,
) -> bool {
    if reflection_same_php_type_name(interface_name, target_interface) {
        return true;
    }
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return false;
    };
    let Some(interface) = ctx.module.interface_infos.get(interface_name) else {
        return false;
    };
    interface
        .parents
        .iter()
        .any(|parent| reflection_interface_extends_interface(ctx, parent, target_interface))
}
