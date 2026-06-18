//! Purpose:
//! Lowers metadata-aware allocation for builtin Reflection owner objects in the
//! EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::objects::lower_object_new()`.
//!
//! Key details:
//! - `ReflectionClass`, `ReflectionMethod`, `ReflectionProperty`,
//!   `ReflectionClassConstant`, and `ReflectionEnum*`
//!   constructors are compile-time metadata lookups that populate private
//!   `__name`/`__attrs` slots instead of running their public empty bodies.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::names::php_symbol_key;
use crate::types::AttrArgValue;

use super::super::super::context::FunctionContext;

/// Compile-time metadata used to populate one Reflection owner object.
struct ReflectionOwnerMetadata {
    reflected_name: Option<String>,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgValue>>>,
    is_final: bool,
    is_abstract: bool,
    is_interface: bool,
    is_trait: bool,
    is_enum: bool,
}

/// Returns true for reflection owner classes that need metadata-aware construction.
pub(super) fn is_reflection_owner_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "ReflectionClass"
            | "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    )
}

/// Lowers builtin Reflection owner allocation by populating compile-time metadata slots.
pub(super) fn lower_reflection_owner_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
) -> Result<()> {
    let metadata = reflection_owner_metadata(ctx, class_name, inst)?;
    let (class_id, property_count, uninitialized_marker_offsets) = {
        let class_info =
            ctx.module.class_infos.get(class_name).ok_or_else(|| {
                CodegenIrError::unsupported(format!("unknown class {}", class_name))
            })?;
        (
            class_info.class_id,
            class_info.properties.len(),
            super::uninitialized_property_marker_offsets(class_info),
        )
    };
    super::emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
    )?;
    if let Some(reflected_name) = metadata.reflected_name.as_deref() {
        emit_reflection_string_property(ctx, reflected_name, 8, 16);
        if class_name == "ReflectionClass" {
            emit_reflection_class_name_parts(ctx, reflected_name)?;
        }
    }
    emit_reflection_attrs_property(ctx, class_name, &metadata.attr_names, &metadata.attr_args)?;
    if class_name == "ReflectionClass" {
        emit_reflection_bool_property(ctx, "__is_final", metadata.is_final)?;
        emit_reflection_bool_property(ctx, "__is_abstract", metadata.is_abstract)?;
        emit_reflection_bool_property(ctx, "__is_interface", metadata.is_interface)?;
        emit_reflection_bool_property(ctx, "__is_trait", metadata.is_trait)?;
        emit_reflection_bool_property(ctx, "__is_enum", metadata.is_enum)?;
    }
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("reflection object_new missing result"))?;
    ctx.store_result_value(result)
}

/// Stores namespace-aware name parts for a statically materialized ReflectionClass.
fn emit_reflection_class_name_parts(
    ctx: &mut FunctionContext<'_>,
    reflected_name: &str,
) -> Result<()> {
    let (namespace_name, short_name) = reflection_name_parts(reflected_name);
    emit_reflection_string_property_by_name(ctx, "__short_name", short_name)?;
    emit_reflection_string_property_by_name(ctx, "__namespace_name", namespace_name)?;
    emit_reflection_bool_property(ctx, "__in_namespace", !namespace_name.is_empty())?;
    Ok(())
}

/// Splits a canonical PHP class-like name into namespace and short-name parts.
fn reflection_name_parts(reflected_name: &str) -> (&str, &str) {
    match reflected_name.rfind('\\') {
        Some(separator) => (
            &reflected_name[..separator],
            &reflected_name[separator + 1..],
        ),
        None => ("", reflected_name),
    }
}

/// Resolves Reflection constructor operands to captured class/member metadata.
fn reflection_owner_metadata(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    match class_name {
        "ReflectionClass" => reflection_class_metadata(ctx, inst),
        "ReflectionMethod" => reflection_method_metadata(ctx, inst),
        "ReflectionProperty" => reflection_property_metadata(ctx, inst),
        "ReflectionClassConstant" => reflection_class_constant_metadata(ctx, inst),
        "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase" => {
            reflection_enum_case_metadata(ctx, class_name, inst)
        }
        _ => Ok(empty_reflection_metadata()),
    }
}

/// Resolves `ReflectionClass(class)` metadata.
fn reflection_class_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionClass")?;
    if let Some((class_name, info)) = resolve_reflection_class(ctx, &reflected_class) {
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(class_name.to_string()),
            attr_names: info.attribute_names.clone(),
            attr_args: info.attribute_args.clone(),
            is_final: info.is_final,
            is_abstract: info.is_abstract,
            is_interface: false,
            is_trait: false,
            is_enum: is_reflection_enum(ctx, class_name),
        });
    }
    if let Some(interface_name) = resolve_reflection_interface(ctx, &reflected_class) {
        return Ok(class_like_reflection_metadata(
            interface_name,
            true,
            false,
            false,
        ));
    }
    if let Some(trait_name) = resolve_reflection_trait(ctx, &reflected_class) {
        return Ok(class_like_reflection_metadata(
            trait_name, false, true, false,
        ));
    }
    Ok(empty_reflection_metadata())
}

/// Resolves `ReflectionMethod(class, method)` metadata.
fn reflection_method_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(method_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionMethod")?;
    let method_name = const_required_string_operand(ctx, method_operand, "ReflectionMethod")?;
    let method_key = php_symbol_key(&method_name);
    Ok(resolve_reflection_class(ctx, &reflected_class)
        .and_then(|(_, info)| {
            Some(ReflectionOwnerMetadata {
                reflected_name: Some(method_name.clone()),
                attr_names: info.method_attribute_names.get(&method_key)?.clone(),
                attr_args: info.method_attribute_args.get(&method_key)?.clone(),
                is_final: false,
                is_abstract: false,
                is_interface: false,
                is_trait: false,
                is_enum: false,
            })
        })
        .unwrap_or_else(empty_reflection_metadata))
}

/// Resolves `ReflectionProperty(class, property)` metadata.
fn reflection_property_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(property_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionProperty")?;
    let property_name = const_required_string_operand(ctx, property_operand, "ReflectionProperty")?;
    Ok(resolve_reflection_class(ctx, &reflected_class)
        .and_then(|(_, info)| {
            Some(ReflectionOwnerMetadata {
                reflected_name: Some(property_name.clone()),
                attr_names: info.property_attribute_names.get(&property_name)?.clone(),
                attr_args: info.property_attribute_args.get(&property_name)?.clone(),
                is_final: false,
                is_abstract: false,
                is_interface: false,
                is_trait: false,
                is_enum: false,
            })
        })
        .unwrap_or_else(empty_reflection_metadata))
}

/// Resolves `ReflectionClassConstant(class, constant)` metadata.
fn reflection_class_constant_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(constant_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class =
        const_string_or_class_operand(ctx, class_operand, "ReflectionClassConstant")?;
    let constant_name =
        const_required_string_operand(ctx, constant_operand, "ReflectionClassConstant")?;
    if let Some(case) = resolve_reflection_enum_case(ctx, &reflected_class, &constant_name) {
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(constant_name.clone()),
            attr_names: case.attribute_names.clone(),
            attr_args: case.attribute_args.clone(),
            is_final: false,
            is_abstract: false,
            is_interface: false,
            is_trait: false,
            is_enum: false,
        });
    }
    Ok(
        resolve_reflection_class_constant(ctx, &reflected_class, &constant_name)
            .map(|(_, info)| {
                let attr_names = info
                    .constant_attribute_names
                    .get(&constant_name)
                    .cloned()
                    .unwrap_or_default();
                let attr_args = info
                    .constant_attribute_args
                    .get(&constant_name)
                    .cloned()
                    .unwrap_or_default();
                ReflectionOwnerMetadata {
                    reflected_name: Some(constant_name),
                    attr_names,
                    attr_args,
                    is_final: false,
                    is_abstract: false,
                    is_interface: false,
                    is_trait: false,
                    is_enum: false,
                }
            })
            .unwrap_or_else(empty_reflection_metadata),
    )
}

/// Resolves `ReflectionEnumUnitCase/BackedCase(enum, case)` metadata.
fn reflection_enum_case_metadata(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(enum_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(case_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_enum = const_string_or_class_operand(ctx, enum_operand, class_name)?;
    let case_name = const_required_string_operand(ctx, case_operand, class_name)?;
    Ok(
        resolve_reflection_enum_case(ctx, &reflected_enum, &case_name)
            .map(|case| ReflectionOwnerMetadata {
                reflected_name: Some(case_name.clone()),
                attr_names: case.attribute_names.clone(),
                attr_args: case.attribute_args.clone(),
                is_final: false,
                is_abstract: false,
                is_interface: false,
                is_trait: false,
                is_enum: false,
            })
            .unwrap_or_else(empty_reflection_metadata),
    )
}

/// Looks up class metadata by PHP-style case-insensitive name.
fn resolve_reflection_class<'a>(
    ctx: &'a FunctionContext<'_>,
    class_name: &str,
) -> Option<(&'a str, &'a crate::types::ClassInfo)> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.module
        .class_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == class_key)
        .map(|(name, info)| (name.as_str(), info))
}

/// Looks up interface metadata by PHP-style case-insensitive name.
fn resolve_reflection_interface<'a>(
    ctx: &'a FunctionContext<'_>,
    interface_name: &str,
) -> Option<&'a str> {
    let interface_key = php_symbol_key(interface_name.trim_start_matches('\\'));
    ctx.module
        .interface_infos
        .keys()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == interface_key)
        .map(String::as_str)
}

/// Looks up a declared trait by PHP-style case-insensitive name.
fn resolve_reflection_trait<'a>(ctx: &'a FunctionContext<'_>, trait_name: &str) -> Option<&'a str> {
    let trait_key = php_symbol_key(trait_name.trim_start_matches('\\'));
    ctx.module
        .trait_table
        .names
        .iter()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == trait_key)
        .map(String::as_str)
}

/// Looks up enum metadata by PHP-style case-insensitive name.
fn is_reflection_enum(ctx: &FunctionContext<'_>, enum_name: &str) -> bool {
    let enum_key = php_symbol_key(enum_name.trim_start_matches('\\'));
    ctx.module
        .enum_infos
        .keys()
        .any(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == enum_key)
}

/// Builds empty ReflectionClass metadata for class-like symbols without stored attributes.
fn class_like_reflection_metadata(
    class_like_name: &str,
    is_interface: bool,
    is_trait: bool,
    is_enum: bool,
) -> ReflectionOwnerMetadata {
    ReflectionOwnerMetadata {
        reflected_name: Some(class_like_name.to_string()),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        is_final: false,
        is_abstract: false,
        is_interface,
        is_trait,
        is_enum,
    }
}

/// Looks up class-constant metadata by PHP-style class name and case-sensitive constant name.
fn resolve_reflection_class_constant<'a>(
    ctx: &'a FunctionContext<'_>,
    class_name: &str,
    constant_name: &str,
) -> Option<(&'a str, &'a crate::types::ClassInfo)> {
    let (resolved_name, info) = resolve_reflection_class(ctx, class_name)?;
    if info.constants.contains_key(constant_name) {
        return Some((resolved_name, info));
    }
    let parent = info.parent.as_deref()?;
    resolve_reflection_class_constant(ctx, parent, constant_name)
}

/// Looks up enum-case metadata by PHP-style enum name and case-sensitive case name.
fn resolve_reflection_enum_case<'a>(
    ctx: &'a FunctionContext<'_>,
    enum_name: &str,
    case_name: &str,
) -> Option<&'a crate::types::EnumCaseInfo> {
    let enum_key = php_symbol_key(enum_name.trim_start_matches('\\'));
    ctx.module
        .enum_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == enum_key)
        .and_then(|(_, info)| info.cases.iter().find(|case| case.name == case_name))
}

/// Returns empty Reflection metadata for unsupported dynamic constructor operands.
fn empty_reflection_metadata() -> ReflectionOwnerMetadata {
    ReflectionOwnerMetadata {
        reflected_name: None,
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        is_final: false,
        is_abstract: false,
        is_interface: false,
        is_trait: false,
        is_enum: false,
    }
}

/// Extracts a constant string or class-name operand from an EIR value.
fn const_string_or_class_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<String> {
    const_data_operand(ctx, value, owner, true)
}

/// Extracts a constant string operand from an EIR value.
fn const_required_string_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<String> {
    const_data_operand(ctx, value, owner, false)
}

/// Reads a `ConstStr` or optional `ConstClassName` value from the module data pool.
fn const_data_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
    allow_class_name: bool,
) -> Result<String> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(format!(
            "{} constructor with non-literal reflection argument",
            owner
        )));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(format!(
            "{} reflection literal missing data id",
            owner
        )));
    };
    match inst_ref.op {
        Op::ConstStr => ctx
            .module
            .data
            .strings
            .get(data.as_raw() as usize)
            .cloned()
            .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw())),
        Op::ConstClassName if allow_class_name => ctx
            .module
            .data
            .class_names
            .get(data.as_raw() as usize)
            .cloned()
            .ok_or_else(|| CodegenIrError::missing_entry("class data", data.as_raw())),
        _ => Err(CodegenIrError::unsupported(format!(
            "{} constructor with non-literal reflection argument",
            owner
        ))),
    }
}

/// Writes a heap-persisted string into the current Reflection object result slot.
fn emit_reflection_string_property(
    ctx: &mut FunctionContext<'_>,
    value: &str,
    low_offset: usize,
    high_offset: usize,
) {
    let (label, len) = ctx.data.add_string(value.as_bytes());
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "x1", object_reg, low_offset);
            abi::emit_store_to_address(ctx.emitter, "x2", object_reg, high_offset);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rax", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "rax", object_reg, low_offset);
            abi::emit_store_to_address(ctx.emitter, "rdx", object_reg, high_offset);
        }
    }
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
}

/// Writes a heap-persisted string into a named ReflectionClass property slot.
fn emit_reflection_string_property_by_name(
    ctx: &mut FunctionContext<'_>,
    property_name: &str,
    value: &str,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionClass")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    emit_reflection_string_property(ctx, value, low_offset, low_offset + 8);
    Ok(())
}

/// Replaces the Reflection object's default `__attrs` array with populated metadata.
fn emit_reflection_attrs_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    attr_names: &[String],
    attr_args: &[Option<Vec<AttrArgValue>>],
) -> Result<()> {
    let (attrs_low_offset, attrs_high_offset) = reflection_attrs_offsets(class_name);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, attrs_low_offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    super::super::builtins::attributes::emit_reflection_attribute_array(
        ctx, attr_names, attr_args,
    )?;
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, attrs_low_offset);
    abi::emit_load_int_immediate(ctx.emitter, abi::secondary_scratch_reg(ctx.emitter), 4);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        object_reg,
        attrs_high_offset,
    );
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}

/// Stores one boolean property on the current ReflectionClass object result.
fn emit_reflection_bool_property(
    ctx: &mut FunctionContext<'_>,
    property_name: &str,
    value: bool,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionClass")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let value_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, value_reg, i64::from(value));
    abi::emit_store_to_address(ctx.emitter, value_reg, result_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, result_reg, high_offset);
    Ok(())
}

/// Returns one declared property offset from a synthetic Reflection class layout.
fn reflection_property_offset(info: &crate::types::ClassInfo, property: &str) -> Result<usize> {
    info.property_offsets.get(property).copied().ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "Reflection owner missing property offset for ${}",
            property
        ))
    })
}

/// Returns the low/high object offsets for the private `__attrs` slot.
fn reflection_attrs_offsets(class_name: &str) -> (usize, usize) {
    if reflection_owner_has_name(class_name) {
        (24, 32)
    } else {
        (8, 16)
    }
}

/// Returns true when the synthetic Reflection owner stores a private `__name` slot.
fn reflection_owner_has_name(class_name: &str) -> bool {
    matches!(
        class_name,
        "ReflectionClass"
            | "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    )
}
