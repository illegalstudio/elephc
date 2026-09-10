//! Purpose:
//! Empty metadata and constant Reflection operand extraction.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Returns empty Reflection metadata for unsupported dynamic constructor operands.
pub(super) fn empty_reflection_metadata() -> ReflectionOwnerMetadata {
    ReflectionOwnerMetadata {
        reflected_name: None,
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        interface_names: Vec::new(),
        trait_names: Vec::new(),
        trait_aliases: Vec::new(),
        parent_names: Vec::new(),
        method_names: Vec::new(),
        property_names: Vec::new(),
        constant_names: Vec::new(),
        constant_members: Vec::new(),
        default_property_members: Vec::new(),
        static_property_members: Vec::new(),
        constant_reflection_members: Vec::new(),
        enum_case_members: Vec::new(),
        method_members: Vec::new(),
        property_members: Vec::new(),
        property_hook_members: Vec::new(),
        constructor_member: None,
        parent_class_name: None,
        constant_value: None,
        backing_value: None,
        is_enum_case: false,
        parameter_members: Vec::new(),
        type_metadata: None,
        property_default_value: None,
        required_parameter_count: 0,
        is_deprecated: false,
        is_generator: false,
        prototype_member: None,
        is_final: false,
        is_abstract: false,
        is_interface: false,
        is_trait: false,
        is_enum: false,
        is_readonly: false,
        is_anonymous: false,
        is_instantiable: false,
        is_cloneable: false,
        is_iterable: false,
        modifiers: 0,
        member_flags: ReflectionMemberFlags::default(),
    }
}

/// Extracts a constant string or class-name operand from an EIR value.
pub(super) fn const_string_or_class_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<String> {
    const_data_operand(ctx, value, owner, true)
}

/// Extracts a constant string operand from an EIR value.
pub(super) fn const_required_string_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<String> {
    const_data_operand(ctx, value, owner, false)
}

/// Returns a string literal operand when its EIR value retains a data-pool identifier.
pub(super) fn const_optional_string_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<Option<String>> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(format!(
            "{} reflection literal missing data id",
            owner
        )));
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .map(Some)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Extracts a constant ReflectionParameter name or offset selector from EIR.
pub(super) fn const_parameter_selector_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<ReflectionParameterSelector> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(
            "ReflectionParameter constructor with non-literal parameter selector",
        ));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    match inst_ref.op {
        Op::ConstI64 => match inst_ref.immediate {
            Some(Immediate::I64(value)) => Ok(ReflectionParameterSelector::Position(value)),
            _ => Err(CodegenIrError::invalid_module(
                "ReflectionParameter position selector missing i64 immediate",
            )),
        },
        Op::ConstStr => {
            let Some(Immediate::Data(data)) = inst_ref.immediate else {
                return Err(CodegenIrError::invalid_module(
                    "ReflectionParameter name selector missing data id",
                ));
            };
            ctx.module
                .data
                .strings
                .get(data.as_raw() as usize)
                .cloned()
                .map(ReflectionParameterSelector::Name)
                .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
        }
        _ => Err(CodegenIrError::unsupported(
            "ReflectionParameter constructor with non-literal parameter selector",
        )),
    }
}

/// Reads a `ConstStr` or optional `ConstClassName` value from the module data pool.
pub(super) fn const_data_operand(
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
