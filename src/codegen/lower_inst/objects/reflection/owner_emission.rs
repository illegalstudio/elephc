//! Purpose:
//! Reflection owner object allocation and core property population.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.
//! - Class-default Errors are retained until getDefaultProperties requests their values.

use super::*;
use crate::codegen::shared_reflection::ReflectionOwnerKind;

/// Releases generic defaults in the builtin Reflection prefix before metadata replaces them.
///
/// A user subclass is allocated through the ordinary object path, which initializes inherited
/// strings, arrays, objects, and boxed values. Reflection metadata writers expect the zeroed
/// builtin allocation shape, so every inherited owner slot is retired by its declared storage
/// type and reset before those writers run. User-declared suffix slots remain untouched.
pub(super) fn clear_reflection_owner_prefix(
    ctx: &mut FunctionContext<'_>,
    owner_class_name: &str,
    object: ValueId,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(owner_class_name)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", owner_class_name)))?;
    let slots = class_info
        .properties
        .iter()
        .enumerate()
        .map(|(index, (name, ty))| {
            let owned_reference = class_info.owned_reference_properties.contains(name);
            (8 + index * 16, ty.codegen_repr(), owned_reference)
        })
        .collect::<Vec<_>>();
    for (offset, ty, owned_reference) in slots {
        ctx.load_value_to_result(object)?;
        let result_reg = abi::int_result_reg(ctx.emitter);
        let object_reg = abi::secondary_scratch_reg(ctx.emitter);
        abi::emit_push_reg(ctx.emitter, result_reg);
        abi::emit_load_from_address(ctx.emitter, result_reg, result_reg, offset);
        if owned_reference {
            abi::emit_call_label(ctx.emitter, "__rt_reference_cell_release");
        } else if ty == PhpType::Str {
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        } else {
            abi::emit_decref_if_refcounted(ctx.emitter, &ty);
        }
        abi::emit_pop_reg(ctx.emitter, object_reg);
        abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset);
        abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset + 8);
        abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    }
    Ok(())
}

/// Allocates and populates one builtin Reflection owner object from metadata, member
/// reflectors included.
pub(super) fn emit_reflection_owner_object(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
) -> Result<()> {
    emit_reflection_owner_object_with_members(ctx, class_name, metadata, None)
}

/// Allocates and populates one builtin Reflection owner object from metadata.
///
/// With `lazy_members`, a class-like owner builds no member reflector: each member slot that has
/// members records the address of its shared filler in `<slot>_filler` (see
/// `REFLECTION_LAZY_MEMBER_SLOTS`), and the getters that read the slot run it on first use.
pub(super) fn emit_reflection_owner_object_with_members(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
    lazy_members: Option<ReflectionOwnerKind>,
) -> Result<()> {
    let (class_id, property_count, uninitialized_marker_offsets) = {
        let class_info = ctx
            .module
            .class_infos
            .get(class_name)
            .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", class_name)))?;
        (
            class_info.class_id,
            class_info.properties.len(),
            super::super::uninitialized_property_marker_offsets(class_info),
        )
    };
    super::super::emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
        &[],
    )?;
    emit_reflection_owner_metadata_with_members(ctx, class_name, metadata, lazy_members)
}

/// Populates Reflection metadata on an object already held in the integer result register.
/// Builtin-owner offsets keep same-named private properties declared by a user subclass intact.
pub(super) fn emit_reflection_owner_metadata(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
) -> Result<()> {
    emit_reflection_owner_metadata_with_members(ctx, class_name, metadata, None)
}

/// The metadata writer behind both entry points; `lazy_members` is `emit_reflection_owner_object_with_members`'s.
fn emit_reflection_owner_metadata_with_members(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
    lazy_members: Option<ReflectionOwnerKind>,
) -> Result<()> {
    let object_class_name = class_name;
    let owner_class_name = reflection_owner_base_class(ctx, object_class_name)
        .unwrap_or_else(|| object_class_name.to_string());
    let class_name = owner_class_name.as_str();
    // `ReflectionEnum` extends `ReflectionClass`: it carries the same member slots, filled the same way.
    let is_reflection_class_owner =
        matches!(class_name, "ReflectionClass" | "ReflectionObject" | "ReflectionEnum");
    emit_reflection_source_properties(ctx, class_name, metadata)?;
    if let Some(reflected_name) = metadata.reflected_name.as_deref() {
        emit_reflection_owner_string_property_by_name(ctx, class_name, "__name", reflected_name)?;
        let public_name_offset = ctx
            .module
            .class_infos
            .get(object_class_name)
            .and_then(|info| info.property_offsets.get("name").copied());
        if let Some(offset) = public_name_offset {
            if owner_class_name != "ReflectionParameter" {
                emit_reflection_literal_string_property(ctx, reflected_name, offset, offset + 8);
            }
        }
        if is_reflection_class_owner {
            if let Some(doc_comment) = metadata.doc_comment.as_deref() {
                emit_reflection_owner_string_property_by_name(
                    ctx,
                    class_name,
                    "__doc_comment",
                    doc_comment,
                )?;
                emit_reflection_owner_bool_property(ctx, class_name, "__has_doc_comment", true)?;
            }
        }
        if is_reflection_class_owner {
            emit_reflection_class_name_parts(ctx, class_name, reflected_name)?;
        }
        if is_reflection_class_owner {
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__interface_names",
                &metadata.interface_names,
            )?;
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__trait_names",
                &metadata.trait_names,
            )?;
            emit_reflection_string_assoc_property_by_name(
                ctx,
                class_name,
                "__trait_aliases",
                &metadata.trait_aliases,
            )?;
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__parent_names",
                &metadata.parent_names,
            )?;
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__method_names",
                &metadata.method_names,
            )?;
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__property_names",
                &metadata.property_names,
            )?;
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__constant_names",
                &metadata.constant_names,
            )?;
            emit_reflection_constant_array_property_by_name(
                ctx,
                class_name,
                "__constants",
                &metadata.constant_members,
            )?;
            emit_reflection_default_property_array_property_by_name(
                ctx,
                class_name,
                "__default_properties",
                &metadata.default_property_members,
            )?;
            if let Some(message) = metadata.default_property_members.iter().find_map(|member| {
                if let ReflectionParameterDefaultValue::DeferredError(message) = &member.value {
                    Some(message.as_str())
                } else {
                    None
                }
            }) {
                emit_reflection_owner_string_property_by_name(ctx, class_name, "__default_error", message)?;
            }
            emit_reflection_static_property_array_property_by_name(
                ctx,
                class_name,
                "__static_properties",
                &metadata.static_property_members,
            )?;
        }
        // `ReflectionFunction`'s `__short_name` is written by `emit_reflection_function_name_parts`
        // below; a second store here overwrote a persisted copy without releasing it.
        if owner_class_name == "ReflectionEnum" {
            let case_names = metadata
                .enum_case_members
                .iter()
                .map(|member| member.name.clone())
                .collect::<Vec<_>>();
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__case_names",
                &case_names,
            )?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_backed",
                metadata.type_metadata.is_some(),
            )?;
            emit_reflection_owner_type_property_by_name(
                ctx,
                class_name,
                "__backing_type",
                metadata.type_metadata.as_ref(),
            )?;
        }
        if is_reflection_class_owner {
            match lazy_members {
                Some(owner) => {
                    emit_reflection_owner_lazy_members(ctx, owner, class_name, metadata, false)?
                }
                None => emit_reflection_owner_members(ctx, class_name, metadata)?,
            }
        }
        if owner_class_name == "ReflectionFunction" {
            emit_reflection_function_name_parts(ctx, reflected_name)?;
        }
        if owner_class_name == "ReflectionMethod" {
            emit_reflection_method_name_parts(ctx, reflected_name)?;
        }
    }
    emit_reflection_attrs_property(ctx, class_name, &metadata.attr_names, &metadata.attr_args)?;
    if is_reflection_class_owner {
        emit_reflection_owner_bool_property(ctx, class_name, "__is_final", metadata.is_final)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_abstract", metadata.is_abstract)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_interface", metadata.is_interface)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_trait", metadata.is_trait)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_enum", metadata.is_enum)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_readonly", metadata.is_readonly)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_anonymous", metadata.is_anonymous)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_instantiable", metadata.is_instantiable)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_cloneable", metadata.is_cloneable)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_iterable", metadata.is_iterable)?;
        let is_internal = metadata
            .reflected_name
            .as_deref()
            .is_some_and(reflection_class_like_is_internal);
        emit_reflection_owner_bool_property(ctx, class_name, "__is_internal", is_internal)?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__is_user_defined",
            metadata.reflected_name.is_some() && !is_internal,
        )?;
        emit_reflection_owner_int_property(ctx, class_name, "__modifiers", metadata.modifiers)?;
    }
    if matches!(
        owner_class_name.as_str(),
        "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    ) {
        emit_reflection_declaring_class_property(
            ctx,
            class_name,
            metadata.parent_class_name.as_deref(),
        )?;
        if matches!(
            owner_class_name.as_str(),
            "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
        ) {
            emit_reflection_owner_string_property_by_name(
                ctx,
                class_name,
                "__class",
                metadata.parent_class_name.as_deref().unwrap_or(""),
            )?;
        }
        if matches!(owner_class_name.as_str(), "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase") {
            emit_reflection_enum_property(ctx, class_name, metadata.parent_class_name.as_deref())?;
        }
    }
    if matches!(owner_class_name.as_str(), "ReflectionFunction" | "ReflectionMethod") {
        // The `__string` slot backs `__toString()`. Both owners declared it and nothing ever
        // filled it, so `(string) $method` answered the empty string (#1080).
        let is_internal =
            reflection_function_or_method_is_internal(ctx, &owner_class_name, metadata);
        let reflected_name = metadata.reflected_name.as_deref().unwrap_or("");
        let rendered = if let Some(rendered) = metadata.rendered_to_string.clone() {
            rendered
        } else if owner_class_name == "ReflectionMethod" {
            reflection_method_to_string(
                reflected_name,
                metadata.parent_class_name.as_deref(),
                metadata.member_flags,
                metadata
                    .prototype_member
                    .as_deref()
                    .and_then(|prototype| prototype.declaring_class_name.as_deref()),
                None,
                None,
                is_internal,
                metadata.is_deprecated,
                metadata.returns_reference,
                &metadata.parameter_members,
                metadata.type_metadata.as_ref(),
            )
        } else {
            reflection_function_to_string(
                reflected_name,
                is_internal,
                metadata.is_deprecated,
                metadata.returns_reference,
                &metadata.parameter_members,
                metadata.type_metadata.as_ref(),
            )
        };
        emit_reflection_owner_dump_property(ctx, class_name, &rendered)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_internal", is_internal)?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__is_user_defined",
            metadata.reflected_name.is_some() && !is_internal,
        )?;
        emit_reflection_lazy_parameters(
            ctx,
            class_name,
            metadata.reflected_name.as_deref().unwrap_or(""),
            &metadata.parameter_members,
        )?;
        emit_reflection_owner_int_property(
            ctx,
            class_name,
            "__required_parameter_count",
            metadata.required_parameter_count,
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__has_return_type",
            metadata.type_metadata.is_some(),
        )?;
        emit_reflection_owner_type_property(ctx, class_name, metadata.type_metadata.as_ref())?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__is_deprecated",
            metadata.is_deprecated,
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__is_generator",
            metadata.is_generator,
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__returns_reference",
            metadata.returns_reference,
        )?;
    }
    if matches!(
        owner_class_name.as_str(),
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        if let Some(value) = &metadata.constant_value {
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            emit_reflection_constant_value_as_mixed(ctx, value);
            emit_reflection_owner_mixed_property_from_result(ctx, class_name, "__value")?;
        }
    }
    if owner_class_name == "ReflectionClassConstant" {
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__has_type",
            metadata.type_metadata.is_some(),
        )?;
        emit_reflection_owner_type_property(ctx, class_name, metadata.type_metadata.as_ref())?;
    }
    if owner_class_name == "ReflectionEnumBackedCase" {
        if let Some(value) = &metadata.backing_value {
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            emit_reflection_constant_value_as_mixed(ctx, value);
            emit_reflection_owner_mixed_property_from_result(ctx, class_name, "__backing_value")?;
        }
    }
    if matches!(
        owner_class_name.as_str(),
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__is_enum_case",
            metadata.is_enum_case,
        )?;
        emit_reflection_owner_int_property(ctx, class_name, "__modifiers", metadata.modifiers)?;
    }
    if owner_class_name == "ReflectionMethod" {
        emit_reflection_owner_string_property_by_name(
            ctx,
            class_name,
            "__class",
            metadata.parent_class_name.as_deref().unwrap_or(""),
        )?;
        emit_reflection_owner_int_property(ctx, class_name, "__modifiers", metadata.modifiers)?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__has_prototype",
            metadata.prototype_member.is_some(),
        )?;
        emit_reflection_method_prototype_property(ctx, metadata.prototype_member.as_deref())?;
    }
    if owner_class_name == "ReflectionProperty" {
        emit_reflection_owner_string_property_by_name(
            ctx,
            class_name,
            "__class",
            metadata.parent_class_name.as_deref().unwrap_or(""),
        )?;
        emit_reflection_owner_int_property(ctx, class_name, "__modifiers", metadata.modifiers)?;
        emit_reflection_owner_type_property(ctx, class_name, metadata.type_metadata.as_ref())?;
        emit_reflection_owner_type_property_by_name(
            ctx,
            class_name,
            "__settable_type",
            metadata.type_metadata.as_ref(),
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__has_default_value",
            metadata.property_default_value.is_some(),
        )?;
        emit_reflection_owner_default_value_property(
            ctx,
            class_name,
            metadata.property_default_value.as_ref(),
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__has_hooks",
            !metadata.property_hook_members.is_empty(),
        )?;
        emit_reflection_property_hook_array_property_by_name(
            ctx,
            class_name,
            "__hooks",
            &metadata.property_hook_members,
        )?;
        let property_string = reflection_property_to_string(
            metadata.reflected_name.as_deref().unwrap_or(""),
            metadata.member_flags,
            metadata.type_metadata.as_ref(),
            metadata.property_default_value.as_ref(),
        );
        emit_reflection_owner_string_property_by_name(
            ctx,
            class_name,
            "__string",
            &property_string,
        )?;
    }
    if owner_class_name == "ReflectionParameter" {
        if let Some(parameter) = metadata.parameter_members.first() {
            emit_reflection_parameter_properties(ctx, parameter)?;
        }
    }
    emit_reflection_member_flag_properties(ctx, class_name, metadata.member_flags)?;
    Ok(())
}

/// Populates the source-file and line slots shared by class-like and callable reflection owners.
fn emit_reflection_source_properties(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
) -> Result<()> {
    if !matches!(
        class_name,
        "ReflectionClass"
            | "ReflectionObject"
            | "ReflectionEnum"
            | "ReflectionFunction"
            | "ReflectionMethod"
    ) {
        return Ok(());
    }
    let Some(source_path) = reflection_source_file(ctx, class_name, metadata) else {
        return Ok(());
    };
    let (start_line, end_line) = reflection_source_lines(ctx, class_name, metadata);
    emit_reflection_owner_string_property_by_name(
        ctx,
        class_name,
        "__file_name",
        &source_path,
    )?;
    emit_reflection_owner_int_property(ctx, class_name, "__start_line", start_line as i64)?;
    emit_reflection_owner_int_property(ctx, class_name, "__end_line", end_line as i64)?;
    Ok(())
}

/// Resolves the physical source path for one reflected declaration.
pub(super) fn reflection_source_file(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
) -> Option<String> {
    if class_name == "ReflectionFunction" {
        return metadata
            .reflected_name
            .as_deref()
            .and_then(|name| {
                ctx.module
                    .declared_function_source_files
                    .get(&php_symbol_key(name))
            })
            .cloned()
            .or_else(|| ctx.module.source_path.clone());
    }
    let reflected_class = if class_name == "ReflectionMethod" {
        metadata.parent_class_name.as_deref()
    } else {
        metadata.reflected_name.as_deref()
    };
    reflected_class
        .and_then(|name| {
            ctx.module
                .declared_class_source_files
                .get(&php_symbol_key(name))
        })
        .cloned()
        .or_else(|| ctx.module.source_path.clone())
}

/// Resolves the best available declaration line range for one reflected owner.
pub(super) fn reflection_source_lines(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
) -> (u32, u32) {
    if class_name == "ReflectionFunction" {
        return metadata
            .reflected_name
            .as_deref()
            .and_then(|name| ctx.module.declared_function_source_lines.get(name).copied())
            .unwrap_or((0, 0));
    }
    let reflected_class = if class_name == "ReflectionMethod" {
        metadata.parent_class_name.as_deref()
    } else {
        metadata.reflected_name.as_deref()
    };
    let Some(reflected_class) = reflected_class else {
        return (0, 0);
    };
    // A method's lines are its OWN declaration's, not its class's: php's
    // `ReflectionMethod::getStartLine()` names the `function` line and `getEndLine()` the
    // closing brace.
    if class_name == "ReflectionMethod" {
        let method_key = metadata.reflected_name.as_deref().map(php_symbol_key);
        let method_span = ctx
            .module
            .class_infos
            .get(reflected_class)
            .zip(method_key)
            .and_then(|(info, method_key)| {
                info.method_decls
                    .iter()
                    .find(|method| php_symbol_key(&method.name) == method_key)
                    .map(|method| method.span)
            });
        if let Some(span) = method_span.filter(|span| span.line > 0) {
            return (span.line, span.end_line.max(span.line));
        }
    }
    if let Some(info) = ctx.module.class_infos.get(reflected_class) {
        return (info.declaration_span.line, info.declaration_span.end_line);
    }
    if let Some(info) = ctx.module.interface_infos.get(reflected_class) {
        return (info.declaration_span.line, info.declaration_span.end_line);
    }
    ctx.module
        .declared_trait_source_lines
        .get(reflected_class)
        .copied()
        .map(|line| (line, line))
        .unwrap_or((0, 0))
}

/// Stores an integer immediate into a Reflection object's property slot.
pub(super) fn emit_reflection_int_property(
    ctx: &mut FunctionContext<'_>,
    value: i64,
    low_offset: usize,
    high_offset: usize,
) {
    let object_reg = abi::int_result_reg(ctx.emitter);
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, scratch, value);
    abi::emit_store_to_address(ctx.emitter, scratch, object_reg, low_offset);
    abi::emit_load_int_immediate(ctx.emitter, scratch, 0);
    abi::emit_store_to_address(ctx.emitter, scratch, object_reg, high_offset);
}

/// Stores namespace-aware name parts for a statically materialized class-like reflector.
pub(super) fn emit_reflection_class_name_parts(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    reflected_name: &str,
) -> Result<()> {
    let (namespace_name, short_name) = reflection_name_parts(reflected_name);
    emit_reflection_owner_string_property_by_name(ctx, class_name, "__short_name", short_name)?;
    emit_reflection_owner_string_property_by_name(
        ctx,
        class_name,
        "__namespace_name",
        namespace_name,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        class_name,
        "__in_namespace",
        !namespace_name.is_empty(),
    )?;
    Ok(())
}

/// Stores namespace-aware name parts for a statically materialized ReflectionFunction.
pub(super) fn emit_reflection_function_name_parts(
    ctx: &mut FunctionContext<'_>,
    reflected_name: &str,
) -> Result<()> {
    let (namespace_name, short_name) = reflection_name_parts(reflected_name);
    emit_reflection_owner_string_property_by_name(
        ctx,
        "ReflectionFunction",
        "__short_name",
        short_name,
    )?;
    emit_reflection_owner_string_property_by_name(
        ctx,
        "ReflectionFunction",
        "__namespace_name",
        namespace_name,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionFunction",
        "__in_namespace",
        !namespace_name.is_empty(),
    )?;
    Ok(())
}

/// Stores PHP's method reflection name parts for a statically materialized method.
pub(super) fn emit_reflection_method_name_parts(
    ctx: &mut FunctionContext<'_>,
    reflected_name: &str,
) -> Result<()> {
    emit_reflection_owner_string_property_by_name(
        ctx,
        "ReflectionMethod",
        "__short_name",
        reflected_name,
    )?;
    emit_reflection_owner_string_property_by_name(
        ctx,
        "ReflectionMethod",
        "__namespace_name",
        "",
    )?;
    emit_reflection_owner_bool_property(ctx, "ReflectionMethod", "__in_namespace", false)?;
    Ok(())
}

/// Splits a canonical PHP class-like name into namespace and short-name parts.
pub(super) fn reflection_name_parts(reflected_name: &str) -> (&str, &str) {
    match reflected_name.rfind('\\') {
        Some(separator) => (
            &reflected_name[..separator],
            &reflected_name[separator + 1..],
        ),
        None => ("", reflected_name),
    }
}

/// The member-reflector slots a compiled class-like reflector fills on first use, each through
/// its own `<slot>_filler` (the checker's `REFLECTION_*_MEMBER_READERS` run them). `__interfaces`
/// and `__traits` are not here: no getter reads them, so a lazy reflector never builds them.
pub(super) const REFLECTION_LAZY_MEMBER_SLOTS: &[&str] = &[
    "__reflection_constants",
    "__methods",
    "__constructor",
    "__parent_class",
    "__properties",
    "__cases",
];

/// Builds a class-like owner's member reflectors into their slots: interfaces, traits, class
/// constants, methods, constructor, parent class, properties and, for an enum, its cases.
///
/// The object is in the integer result register on entry and exit, as for every slot emitter.
pub(super) fn emit_reflection_owner_members(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
) -> Result<()> {
    emit_reflection_class_array_property_by_name(
        ctx,
        class_name,
        "__interfaces",
        &metadata.interface_names,
    )?;
    emit_reflection_class_array_property_by_name(
        ctx,
        class_name,
        "__traits",
        &metadata.trait_names,
    )?;
    for slot in REFLECTION_LAZY_MEMBER_SLOTS {
        emit_reflection_owner_member_slot(ctx, class_name, metadata, slot)?;
    }
    Ok(())
}

/// Leaves each member slot that has members to its filler, and fills the empty ones now: an
/// empty array, a null constructor or a `false` parent costs no reflector object.
fn emit_reflection_owner_lazy_members(
    ctx: &mut FunctionContext<'_>,
    owner: ReflectionOwnerKind,
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
    skip_generic: bool,
) -> Result<()> {
    for slot in REFLECTION_LAZY_MEMBER_SLOTS {
        if !reflection_owner_member_slot_applies(class_name, slot) {
            continue;
        }
        // A table-built reflector builds this slot from the row's name list when it is read.
        if skip_generic && crate::reflection_table::generic_slots_for(ctx.module.closed_world).contains(slot) {
            continue;
        }
        if reflection_owner_member_slot_is_empty(metadata, slot) {
            emit_reflection_owner_member_slot(ctx, class_name, metadata, slot)?;
            continue;
        }
        let label =
            reflection_member_slot_filler_label(ctx, owner.clone(), class_name, metadata, slot)?;
        emit_reflection_owner_code_address_property(
            ctx,
            class_name,
            &format!("{}_filler", slot),
            &label,
        )?;
    }
    Ok(())
}

/// Whether `slot` exists on this owner class: only `ReflectionEnum` has `__cases`.
fn reflection_owner_member_slot_applies(class_name: &str, slot: &str) -> bool {
    slot != "__cases" || class_name == "ReflectionEnum"
}

/// Whether the reflected class-like has nothing for `slot`.
fn reflection_owner_member_slot_is_empty(metadata: &ReflectionOwnerMetadata, slot: &str) -> bool {
    match slot {
        "__reflection_constants" => metadata.constant_reflection_members.is_empty(),
        "__methods" => metadata.method_members.is_empty(),
        "__constructor" => metadata.constructor_member.is_none(),
        "__parent_class" => metadata.parent_class_name.is_none(),
        "__properties" => metadata.property_members.is_empty(),
        "__cases" => metadata.enum_case_members.is_empty(),
        _ => true,
    }
}

/// Builds one member slot of a class-like owner held in the integer result register.
pub(super) fn emit_reflection_owner_member_slot(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
    slot: &str,
) -> Result<()> {
    match slot {
        "__reflection_constants" => emit_reflection_member_array_property_by_name(
            ctx,
            class_name,
            "__reflection_constants",
            "ReflectionClassConstant",
            &metadata.constant_reflection_members,
        ),
        "__methods" => emit_reflection_member_array_property_by_name(
            ctx,
            class_name,
            "__methods",
            "ReflectionMethod",
            &metadata.method_members,
        ),
        "__constructor" => {
            emit_reflection_constructor_property(ctx, class_name, metadata.constructor_member.as_ref())
        }
        "__parent_class" => emit_reflection_parent_class_property(
            ctx,
            class_name,
            metadata.parent_class_name.as_deref(),
        ),
        "__properties" => emit_reflection_member_array_property_by_name(
            ctx,
            class_name,
            "__properties",
            "ReflectionProperty",
            &metadata.property_members,
        ),
        "__cases" if class_name == "ReflectionEnum" => {
            let case_class = if metadata.type_metadata.is_some() {
                "ReflectionEnumBackedCase"
            } else {
                "ReflectionEnumUnitCase"
            };
            emit_reflection_member_array_property_by_name(
                ctx,
                "ReflectionEnum",
                "__cases",
                case_class,
                &metadata.enum_case_members,
            )
        }
        _ => Ok(()),
    }
}

/// Stores the address of the code at `label` into one integer slot of the owner object held in
/// the integer result register, which it leaves there.
pub(super) fn emit_reflection_owner_code_address_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property_name: &str,
    label: &str,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let offset = reflection_property_offset(class_info, property_name)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    let address_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, object_reg, result_reg);
    abi::emit_symbol_address(ctx.emitter, address_reg, label);
    abi::emit_store_to_address(ctx.emitter, address_reg, object_reg, offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset + 8);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Which value-bearing slots the reflection table row already carries as data, so the extras
/// filler need not store them.
#[derive(Clone, Copy, Default)]
pub(super) struct TableValues {
    pub(super) constants: bool,
    pub(super) defaults: bool,
}

/// Fills the slots of a table-built class reflector that are not plain data: attributes, trait
/// aliases, constant values, default and static property values, an enum's backing type, and --
/// when `lazy_members` -- the addresses of the shared member fillers.
///
/// The reflector is in the integer result register on entry and exit, as for every slot emitter.
/// The constructor (`ReflectionClass::__construct`, filled from the module reflection table) has
/// already stored every name, flag, line and list; this is the part that needs code.
pub(super) fn emit_reflection_class_extras(
    ctx: &mut FunctionContext<'_>,
    owner: ReflectionOwnerKind,
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
    lazy_members: bool,
    in_table: TableValues,
) -> Result<()> {
    if !metadata.attr_names.is_empty() {
        emit_reflection_attrs_property(ctx, class_name, &metadata.attr_names, &metadata.attr_args)?;
    }
    if !metadata.trait_aliases.is_empty() {
        emit_reflection_string_assoc_property_by_name(
            ctx,
            class_name,
            "__trait_aliases",
            &metadata.trait_aliases,
        )?;
    }
    if !in_table.constants && !metadata.constant_members.is_empty() {
        emit_reflection_constant_array_property_by_name(
            ctx,
            class_name,
            "__constants",
            &metadata.constant_members,
        )?;
    }
    if !in_table.defaults && !metadata.default_property_members.is_empty() {
        emit_reflection_default_property_array_property_by_name(
            ctx,
            class_name,
            "__default_properties",
            &metadata.default_property_members,
        )?;
    }
    if !metadata.static_property_members.is_empty() {
        emit_reflection_static_property_array_property_by_name(
            ctx,
            class_name,
            "__static_properties",
            &metadata.static_property_members,
        )?;
    }
    if class_name == "ReflectionEnum" && metadata.type_metadata.is_some() {
        emit_reflection_owner_type_property_by_name(
            ctx,
            class_name,
            "__backing_type",
            metadata.type_metadata.as_ref(),
        )?;
    }
    if lazy_members {
        emit_reflection_owner_lazy_members(ctx, owner, class_name, metadata, true)?;
    }
    Ok(())
}

/// Whether a table-built reflector of this class-like needs the extras filler at all.
pub(super) fn reflection_class_extras_are_needed(
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
    lazy_members: bool,
    in_table: TableValues,
) -> bool {
    !metadata.attr_names.is_empty()
        || !metadata.trait_aliases.is_empty()
        || (!in_table.constants && !metadata.constant_members.is_empty())
        || (!in_table.defaults && !metadata.default_property_members.is_empty())
        || !metadata.static_property_members.is_empty()
        || (class_name == "ReflectionEnum" && metadata.type_metadata.is_some())
        || lazy_members
}
