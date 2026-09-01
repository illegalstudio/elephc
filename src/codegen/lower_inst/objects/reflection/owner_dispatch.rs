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
    if let Some(object_operand) = reflection_object_operand(ctx, class_name, inst)? {
        emit_reflection_owner_from_runtime_object(ctx, class_name, object_operand)?;
    } else if class_name == "ReflectionClass"
        && function_only_uses_reflection_class_for_constructorless_allocation(ctx)
    {
        let reflected_name = reflection_class_reflected_name(ctx, inst)?;
        emit_reflection_owner_name_only(ctx, class_name, reflected_name.as_deref())?;
    } else {
        let metadata = reflection_owner_metadata(ctx, class_name, inst)?;
        emit_reflection_owner_object(ctx, class_name, &metadata)?;
    }
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("reflection object_new missing result"))?;
    ctx.store_result_value(result)
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

    let fallback_metadata = if class_name == "ReflectionObject"
        && php_symbol_key(&candidates[0].class_name) == "dateinterval"
    {
        reflection_date_interval_object_metadata(ctx, false)?
    } else {
        reflection_class_metadata_for_name(ctx, &candidates[0].class_name)?
    };
    emit_reflection_owner_object(ctx, class_name, &fallback_metadata)?;
    emit_reflection_dispatch_jump(ctx, &done_label);                            // skip runtime reflection candidates after fallback allocation

    for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        if class_name == "ReflectionObject"
            && php_symbol_key(&candidate.class_name) == "dateinterval"
        {
            emit_date_interval_reflection_object(
                ctx,
                object_operand,
                &candidate.class_name,
                &done_label,
            )?;
            continue;
        }
        let metadata = reflection_class_metadata_for_name(ctx, &candidate.class_name)?;
        emit_reflection_owner_object(ctx, class_name, &metadata)?;
        emit_reflection_dispatch_jump(ctx, &done_label);                        // finish after materializing the matched runtime class
    }

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Materializes state-dependent `ReflectionObject(DateInterval)` property metadata.
#[rustfmt::skip]
fn emit_date_interval_reflection_object(
    ctx: &mut FunctionContext<'_>,
    object_operand: ValueId,
    class_name: &str,
    done_label: &str,
) -> Result<()> {
    let from_string_offset = ctx
        .module
        .class_infos
        .get(class_name)
        .and_then(|info| info.property_offsets.get("_from_string"))
        .copied()
        .ok_or_else(|| CodegenIrError::missing_entry("DateInterval::_from_string", 0))?;
    let relative_label = ctx.next_label("reflection_date_interval_relative");
    ctx.load_value_to_result(object_operand)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x9, [x0, #{}]", from_string_offset)); // load DateInterval's relative-string discriminator
            ctx.emitter.instruction(&format!("cbnz x9, {}", relative_label));   // select the two-property relative interval surface
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp QWORD PTR [rax + {}], 0", from_string_offset)); // test DateInterval's relative-string discriminator
            ctx.emitter.instruction(&format!("jne {}", relative_label));        // select the two-property relative interval surface
        }
    }
    let component_metadata = reflection_date_interval_object_metadata(ctx, false)?;
    emit_reflection_owner_object(ctx, "ReflectionObject", &component_metadata)?;
    emit_reflection_dispatch_jump(ctx, done_label);                              // finish after materializing component interval metadata
    ctx.emitter.label(&relative_label);
    let relative_metadata = reflection_date_interval_object_metadata(ctx, true)?;
    emit_reflection_owner_object(ctx, "ReflectionObject", &relative_metadata)?;
    emit_reflection_dispatch_jump(ctx, done_label);                              // finish after materializing relative interval metadata
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
            ctx.emitter.instruction(&format!("cbz x0, {}", fallback_label));    // use fallback metadata for null object pointers
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the object's concrete runtime class id
            for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare the object class id with this reflection candidate
                ctx.emitter.instruction(&format!("b.eq {}", label));            // select metadata for the matched runtime class
            }
            ctx.emitter.instruction(&format!("b {}", fallback_label));          // fall back when no generated candidate matches
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // use fallback metadata for null object pointers
            ctx.emitter.instruction(&format!("je {}", fallback_label));         // skip class-id loading when the object pointer is null
            ctx.emitter.instruction("mov r11, QWORD PTR [rax]");                // load the object's concrete runtime class id
            for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // compare the object class id with this reflection candidate
                ctx.emitter.instruction(&format!("je {}", label));              // select metadata for the matched runtime class
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
