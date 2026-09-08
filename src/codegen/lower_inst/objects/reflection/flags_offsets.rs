//! Purpose:
//! Reflection predicate properties, modifiers, and synthetic slot offsets.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Stores ReflectionMethod/ReflectionProperty boolean predicate slots when supported.
pub(super) fn emit_reflection_member_flag_properties(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    flags: ReflectionMemberFlags,
) -> Result<()> {
    match class_name {
        "ReflectionMethod" => {
            emit_reflection_owner_bool_property(ctx, class_name, "__is_static", flags.is_static)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_public", flags.is_public)?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_protected",
                flags.is_protected,
            )?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_private", flags.is_private)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_final", flags.is_final)?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_abstract",
                flags.is_abstract,
            )?;
        }
        "ReflectionProperty" => {
            emit_reflection_owner_bool_property(ctx, class_name, "__is_static", flags.is_static)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_public", flags.is_public)?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_protected",
                flags.is_protected,
            )?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_private", flags.is_private)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_final", flags.is_final)?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_abstract",
                flags.is_abstract,
            )?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_readonly",
                flags.is_readonly,
            )?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_promoted",
                flags.is_promoted,
            )?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_virtual", flags.is_virtual)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_dynamic", flags.is_dynamic)?;
        }
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase" => {
            emit_reflection_owner_bool_property(ctx, class_name, "__is_public", flags.is_public)?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_protected",
                flags.is_protected,
            )?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_private", flags.is_private)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_final", flags.is_final)?;
        }
        _ => {}
    }
    Ok(())
}

/// Stores one boolean property on the current Reflection owner object result.
pub(super) fn emit_reflection_owner_bool_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property_name: &str,
    value: bool,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    emit_reflection_int_property(ctx, i64::from(value), low_offset, low_offset + 8);
    Ok(())
}

/// Stores one integer property on the current Reflection owner object result.
pub(super) fn emit_reflection_owner_int_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property_name: &str,
    value: i64,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let value_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, value_reg, value);
    abi::emit_store_to_address(ctx.emitter, value_reg, result_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, result_reg, high_offset);
    Ok(())
}

/// Stores the current boxed Mixed result into one Reflection owner property.
pub(super) fn emit_reflection_owner_mixed_property_from_result(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property_name: &str,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let value_reg = abi::int_result_reg(ctx.emitter);
    let owner_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, owner_reg);
    abi::emit_store_to_address(ctx.emitter, value_reg, owner_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, owner_reg, high_offset);
    abi::emit_reg_move(ctx.emitter, value_reg, owner_reg);
    Ok(())
}

/// Computes PHP's `ReflectionClass::getModifiers()` bitmask for class metadata.
pub(super) fn reflection_class_modifiers(
    is_final: bool,
    is_abstract: bool,
    is_readonly_class: bool,
    is_enum: bool,
) -> i64 {
    let mut modifiers = 0;
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    if is_readonly_class && !is_enum {
        modifiers |= 65_536;
    }
    modifiers
}

/// Computes PHP's `ReflectionClassConstant::getModifiers()` bitmask.
pub(super) fn reflection_class_constant_modifiers(visibility: &Visibility, is_final: bool) -> i64 {
    let mut modifiers = match visibility {
        Visibility::Public => 1,
        Visibility::Protected => 2,
        Visibility::Private => 4,
    };
    if is_final {
        modifiers |= 32;
    }
    modifiers
}

/// Computes PHP's `ReflectionProperty::getModifiers()` bitmask from class metadata.
pub(super) fn reflection_property_modifiers_for_info(
    info: &crate::types::ClassInfo,
    property_name: &str,
) -> Option<i64> {
    if info
        .properties
        .iter()
        .any(|(name, _)| name == property_name)
    {
        let visibility = info
            .property_visibilities
            .get(property_name)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_property_modifiers(
            visibility,
            false,
            info.final_properties.contains(property_name),
            info.abstract_properties.contains(property_name),
            info.readonly_properties.contains(property_name),
            reflection_property_is_virtual(info, property_name),
            info.property_set_visibilities.get(property_name),
        ));
    }
    if info
        .static_properties
        .iter()
        .any(|(name, _)| name == property_name)
    {
        let visibility = info
            .static_property_visibilities
            .get(property_name)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_property_modifiers(
            visibility,
            true,
            info.final_static_properties.contains(property_name),
            false,
            false,
            false,
            None,
        ));
    }
    None
}

/// Returns whether a property is virtual because it has or requires hooks.
pub(super) fn reflection_property_is_virtual(info: &crate::types::ClassInfo, property_name: &str) -> bool {
    let get_method = php_symbol_key(&property_hook_get_method(property_name));
    let set_method = php_symbol_key(&property_hook_set_method(property_name));
    info.abstract_property_hooks.contains_key(property_name)
        || info.methods.contains_key(&get_method)
        || info.methods.contains_key(&set_method)
}

/// Computes PHP's `ReflectionProperty::getModifiers()` bitmask.
pub(super) fn reflection_property_modifiers(
    visibility: &Visibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
    is_readonly: bool,
    is_virtual: bool,
    set_visibility: Option<&Visibility>,
) -> i64 {
    let mut modifiers = match visibility {
        Visibility::Public => 1,
        Visibility::Protected => 2,
        Visibility::Private => 4,
    };
    if is_static {
        modifiers |= 16;
    }
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    if is_readonly {
        modifiers |= 128;
    }
    if is_virtual {
        modifiers |= 512;
    }
    match set_visibility {
        Some(Visibility::Private) => modifiers |= 32 | 4096,
        Some(Visibility::Protected) => modifiers |= 2048,
        Some(Visibility::Public) | None => {
            if is_readonly && visibility == &Visibility::Public {
                modifiers |= 2048;
            }
        }
    }
    modifiers
}

/// Computes PHP's `ReflectionProperty::getModifiers()` bitmask from predicate flags.
pub(super) fn reflection_property_modifiers_from_flags(flags: ReflectionMemberFlags) -> i64 {
    let visibility = reflection_visibility_from_member_flags(flags);
    reflection_property_modifiers(
        &visibility,
        flags.is_static,
        flags.is_final,
        flags.is_abstract,
        flags.is_readonly,
        flags.is_virtual,
        None,
    )
}

/// Converts retained member visibility flags back into a `Visibility` value.
pub(super) fn reflection_visibility_from_member_flags(flags: ReflectionMemberFlags) -> Visibility {
    if flags.is_private {
        Visibility::Private
    } else if flags.is_protected {
        Visibility::Protected
    } else {
        Visibility::Public
    }
}

/// Computes PHP's `ReflectionMethod::getModifiers()` bitmask from method flags.
pub(super) fn reflection_method_modifiers_from_flags(flags: ReflectionMemberFlags) -> i64 {
    let mut modifiers = 0;
    if flags.is_public {
        modifiers |= 1;
    }
    if flags.is_protected {
        modifiers |= 2;
    }
    if flags.is_private {
        modifiers |= 4;
    }
    if flags.is_static {
        modifiers |= 16;
    }
    if flags.is_final {
        modifiers |= 32;
    }
    if flags.is_abstract {
        modifiers |= 64;
    }
    modifiers
}

/// Returns one declared property offset from a synthetic Reflection class layout.
pub(super) fn reflection_property_offset(info: &crate::types::ClassInfo, property: &str) -> Result<usize> {
    info.property_offsets.get(property).copied().ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "Reflection owner missing property offset for ${}",
            property
        ))
    })
}

/// Returns the low/high object offsets for the private `__attrs` slot.
pub(super) fn reflection_attrs_offsets(ctx: &FunctionContext<'_>, class_name: &str) -> Result<(usize, usize)> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let attrs_low_offset = reflection_property_offset(class_info, "__attrs")?;
    Ok((attrs_low_offset, attrs_low_offset + 8))
}
