//! Purpose:
//! Resolves declared property slots and receiver source types.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Case-insensitive class lookup and packed/runtime storage overrides remain authoritative.

use super::*;

/// Resolves the property slot for a concrete object receiver and declared property name.
pub(super) fn resolve_property_slot(
    ctx: &FunctionContext<'_>,
    object: crate::ir::ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let object_ty = ctx.value_php_type(object)?;
    let PhpType::Object(class_name) = object_ty else {
        if let PhpType::Packed(class_name) = object_ty {
            return resolve_packed_field_slot(ctx, &class_name, property, inst);
        }
        return Err(CodegenIrError::unsupported(format!(
            "{} for receiver PHP type {:?}",
            inst.op.name(),
            object_ty
        )));
    };
    resolve_property_slot_for_class(ctx, &class_name, property, inst)
}

/// Returns the dynamic-property hash slot offset for an undeclared allow-dynamic property.
pub(super) fn dynamic_property_hash_offset_for_object(
    ctx: &FunctionContext<'_>,
    object: crate::ir::ValueId,
    property: &str,
) -> Result<Option<usize>> {
    let object_ty = ctx.value_php_type(object)?;
    let PhpType::Object(class_name) = object_ty else {
        return Ok(None);
    };
    dynamic_property_hash_offset_for_class(ctx, &class_name, property)
}

/// Returns the dynamic-property hash slot offset for a known class and property name.
pub(super) fn dynamic_property_hash_offset_for_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
) -> Result<Option<usize>> {
    let normalized = class_name.trim_start_matches('\\');
    if is_builtin_stdclass(normalized) {
        return Ok(Some(dynamic_property_hash_offset(0)));
    }
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", normalized)))?;
    if class_info
        .properties
        .iter()
        .any(|(name, _)| name == property)
    {
        return Ok(None);
    }
    if class_info.allow_dynamic_properties {
        return Ok(Some(dynamic_property_hash_offset(
            class_info.properties.len()
                + crate::internal_extensions::hidden_slot_count_for(
                    &ctx.module.class_infos,
                    normalized,
                ),
        )));
    }
    Ok(None)
}

/// Returns true when a class name is the builtin `stdClass` dynamic-property container.
pub(super) fn is_builtin_stdclass(class_name: &str) -> bool {
    crate::types::checker::builtin_stdclass::is_stdclass(class_name.trim_start_matches('\\'))
}

/// Returns true when the SSA value is known to hold a stdClass object pointer.
pub(super) fn object_is_builtin_stdclass(ctx: &FunctionContext<'_>, object: ValueId) -> Result<bool> {
    Ok(matches!(
        ctx.value_php_type(object)?.codegen_repr(),
        PhpType::Object(class_name) if is_builtin_stdclass(&class_name)
    ))
}

/// Resolves a property slot for a known class name.
pub(super) fn resolve_property_slot_for_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", normalized)))?;
    let Some((index, (_, php_type))) = class_info.visible_property(property) else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for dynamic or missing property {}::${}",
            inst.op.name(),
            normalized,
            property
        )));
    };
    let is_reference = class_info.property_slot_is_reference(index, property);
    let php_type = runtime_property_type_override(ctx, normalized, property)
        .unwrap_or_else(|| php_type.clone());
    ensure_property_type_supported(&php_type, inst)?;
    let offset = 8 + index * 16;
    Ok(PropertySlot {
        class_name: normalized.to_string(),
        property: property.to_string(),
        php_type,
        offset,
        is_declared: class_info.property_slot_is_declared(index, property),
        is_packed: false,
        is_reference,
    })
}

/// Returns precise runtime storage types for inherited SPL callback-filter internals.
pub(super) fn runtime_property_type_override(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
) -> Option<PhpType> {
    if !class_extends_class(ctx, class_name, "CallbackFilterIterator") {
        return None;
    }
    match property {
        "callback" => Some(PhpType::Callable),
        "callbackEnv" => Some(PhpType::Pointer(None)),
        _ => None,
    }
}

/// Returns the source PHP type for an SSA value before codegen representation erasure.
pub(in crate::codegen::lower_inst) fn raw_value_php_type(ctx: &FunctionContext<'_>, value: ValueId) -> Result<PhpType> {
    ctx.function
        .value(value)
        .map(|metadata| metadata.php_type.clone())
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))
}

/// Returns the literal string payload for a value produced by `ConstStr`, when statically known.
pub(super) fn const_string_operand<'a>(ctx: &FunctionContext<'a>, value: ValueId) -> Result<Option<&'a str>> {
    let metadata = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = metadata.def else {
        return Ok(None);
    };
    let instruction = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if instruction.op != Op::ConstStr {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = instruction.immediate else {
        return Err(CodegenIrError::invalid_module(
            "const_str missing data immediate",
        ));
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .map(Some)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Resolves an object or object|null source type for a nullsafe receiver.
pub(in crate::codegen::lower_inst) fn nullable_object_receiver_class(
    ctx: &FunctionContext<'_>,
    object: ValueId,
) -> Result<Option<(String, bool)>> {
    match raw_value_php_type(ctx, object)? {
        PhpType::Object(class_name) => Ok(Some((class_name, false))),
        PhpType::Union(members) => {
            let mut class_name = None;
            let mut nullable = false;
            for member in members {
                match member {
                    PhpType::Void => nullable = true,
                    PhpType::Object(candidate) => {
                        if class_name
                            .as_ref()
                            .is_some_and(|existing: &String| existing != &candidate)
                        {
                            return Ok(None);
                        }
                        class_name = Some(candidate);
                    }
                    _ => return Ok(None),
                }
            }
            Ok(class_name.map(|name| (name, nullable)))
        }
        _ => Ok(None),
    }
}

/// Returns the unique object class carried by a boxed union, ignoring null and scalar arms.
pub(super) fn union_object_member_class(ctx: &FunctionContext<'_>, object: ValueId) -> Result<Option<String>> {
    let PhpType::Union(members) = raw_value_php_type(ctx, object)? else {
        return Ok(None);
    };
    let mut class_name = None;
    for member in members {
        let PhpType::Object(candidate) = member else {
            continue;
        };
        if class_name
            .as_ref()
            .is_some_and(|existing: &String| existing != &candidate)
        {
            return Ok(None);
        }
        class_name = Some(candidate);
    }
    Ok(class_name)
}

/// Unboxes a nullable object receiver and branches when it holds PHP null.
pub(in crate::codegen::lower_inst) fn emit_nullable_receiver_object_payload(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    null_label: &str,
    object_reg: &str,
) -> Result<()> {
    let ty = ctx.load_value_to_result(object)?;
    if ty != PhpType::Mixed {
        return Err(CodegenIrError::unsupported(format!(
            "nullsafe property receiver storage {:?}",
            ty
        )));
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #8");                              // check whether the nullable receiver holds PHP null
            ctx.emitter.instruction(&format!("b.eq {}", null_label));           // short-circuit property access for nullsafe null receivers
            ctx.emitter.instruction(&format!("mov {}, x1", object_reg));        // promote the unboxed object payload into the property base register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 8");                              // check whether the nullable receiver holds PHP null
            ctx.emitter.instruction(&format!("je {}", null_label));             // short-circuit property access for nullsafe null receivers
            ctx.emitter.instruction(&format!("mov {}, rdi", object_reg));       // promote the unboxed object payload into the property base register
        }
    }
    Ok(())
}

/// Boxes a PHP null sentinel as a runtime Mixed cell.
pub(in crate::codegen::lower_inst) fn emit_boxed_null(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        RUNTIME_NULL_SENTINEL,
    );
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
}

/// Resolves a field slot on an embedded packed-class receiver.
pub(super) fn resolve_packed_field_slot(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx
        .module
        .packed_class_infos
        .get(normalized)
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!("unknown packed class {}", normalized))
        })?;
    let Some(field) = class_info
        .fields
        .iter()
        .find(|field| field.name == property)
    else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for missing packed field {}::${}",
            inst.op.name(),
            normalized,
            property
        )));
    };
    ensure_property_type_supported(&field.php_type, inst)?;
    Ok(PropertySlot {
        class_name: normalized.to_string(),
        property: property.to_string(),
        php_type: field.php_type.clone(),
        offset: field.offset,
        is_declared: false,
        is_packed: true,
        is_reference: false,
    })
}
