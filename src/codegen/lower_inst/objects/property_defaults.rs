//! Purpose:
//! Collects property defaults and invokes constructors after allocation.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Literal defaults and constructor argument ownership preserve their established layout.

use super::*;

/// Collects literal defaults that can be copied directly into object property slots.
pub(super) fn collect_property_defaults(
    class_info: &ClassInfo,
    inst: &Instruction,
) -> Result<Vec<PropertyDefault>> {
    let mut defaults = Vec::new();
    for (index, (property, php_type)) in class_info.properties.iter().enumerate() {
        let Some(default_expr) = class_info.defaults.get(index).and_then(Option::as_ref) else {
            continue;
        };
        // A null default whose slot cannot represent null (a scalar slot rebound by
        // constructor-argument propagation) is skipped; the slot is always written
        // before an observable read on those paths.
        if matches!(default_expr.kind, crate::parser::ast::ExprKind::Null)
            && !php_type.null_property_default_required()
        {
            continue;
        }
        let offset = 8 + index * 16;
        defaults.push(PropertyDefault {
            offset,
            value: literal_default_value(
                &format!("property ${}", property),
                php_type,
                &default_expr.kind,
                inst.op.name(),
            )?,
            is_reference: class_info.owned_reference_properties.contains(property),
        });
    }
    Ok(defaults)
}

/// Writes all supported property defaults into the newly allocated object.
pub(super) fn emit_property_defaults(
    ctx: &mut FunctionContext<'_>,
    object: crate::ir::ValueId,
    defaults: &[PropertyDefault],
) -> Result<()> {
    for default in defaults {
        let object_reg = abi::secondary_scratch_reg(ctx.emitter);
        ctx.load_value_to_reg(object, object_reg)?;
        if default.is_reference {
            // Write the default THROUGH the property's ref-cell: load the cell pointer
            // from the slot, then write the value at the cell's value/tag words (offset 0).
            abi::emit_load_from_address(ctx.emitter, object_reg, object_reg, default.offset);
            let cell_default = PropertyDefault {
                offset: 0,
                value: default.value.clone(),
                is_reference: false,
            };
            emit_property_default(ctx, object_reg, &cell_default)?;
        } else {
            emit_property_default(ctx, object_reg, default)?;
        }
    }
    Ok(())
}

/// Writes one literal property default into its object slot.
pub(super) fn emit_property_default(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
    default: &PropertyDefault,
) -> Result<()> {
    match &default.value {
        LiteralDefaultValue::Int(value) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, int_reg, *value);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::Bool(value) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, int_reg, i64::from(*value));
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::Float(value) => {
            let label = ctx.data.add_float(*value);
            let scratch = abi::symbol_scratch_reg(ctx.emitter);
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_symbol_address(ctx.emitter, scratch, &label);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter
                        .instruction(&format!("ldr {}, [{}]", float_reg, scratch));
                    // load the property default float literal through the symbol scratch register
                }
                Arch::X86_64 => {
                    ctx.emitter
                        .instruction(&format!("movsd {}, QWORD PTR [{}]", float_reg, scratch));
                    // load the property default float literal through the symbol scratch register
                }
            }
            abi::emit_store_to_address(ctx.emitter, float_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::Str(value) => {
            emit_string_literal_default_to_result(ctx, value);
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, ptr_reg, object_reg, default.offset);
            abi::emit_store_to_address(ctx.emitter, len_reg, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::Null => {
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::NullSentinel => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, int_reg, RUNTIME_NULL_SENTINEL);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::TaggedNull => {
            emit_tagged_null_literal_to_result(ctx);
            emit_tagged_scalar_property_default_store(ctx, object_reg, default.offset);
        }
        LiteralDefaultValue::TaggedInt(value) => {
            emit_tagged_int_literal_to_result(ctx, *value);
            emit_tagged_scalar_property_default_store(ctx, object_reg, default.offset);
        }
        LiteralDefaultValue::BoxedNull => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_boxed_null_literal_to_result(ctx);
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::BoxedStr(value) => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_boxed_string_literal_default_to_result(ctx, value);
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::BoxedInt(value) => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_boxed_int_literal_to_result(ctx, *value);
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::BoxedBool(value) => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_boxed_bool_literal_to_result(ctx, *value);
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::BoxedFloat(value) => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_boxed_float_literal_to_result(ctx, *value);
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::Array {
            elem_type,
            elements,
        } => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_array_literal_default_to_result(ctx, elem_type, elements)?;
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::AssocArray {
            value_type,
            entries,
        } => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_assoc_array_literal_default_to_result(ctx, value_type, entries)?;
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::BoxedArray {
            elem_type,
            elements,
        } => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_array_literal_default_to_result(ctx, elem_type, elements)?;
            crate::codegen::emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::Array(Box::new(elem_type.clone())),
            );
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::EmptyAssocArray { value_type } => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_empty_assoc_array_literal_to_result(ctx, value_type);
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
    }
    Ok(())
}

/// Calls the resolved `__construct` method with the newly allocated object as `$this`.
pub(super) fn emit_constructor_call(
    ctx: &mut FunctionContext<'_>,
    object: crate::ir::ValueId,
    constructor_args: &[crate::ir::ValueId],
    class_name: &str,
    impl_class: &str,
    constructor_key: &str,
    constructor_param_types: &[PhpType],
    constructor_ref_params: &[bool],
    padding_thunk: Option<&str>,
) -> Result<()> {
    let mut args = Vec::with_capacity(constructor_args.len() + 1);
    args.push(object);
    args.extend(constructor_args.iter().copied());
    let mut param_types = Vec::with_capacity(constructor_param_types.len() + 1);
    param_types.push(PhpType::Object(class_name.to_string()));
    param_types.extend_from_slice(constructor_param_types);
    let mut ref_params = Vec::with_capacity(constructor_ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend_from_slice(constructor_ref_params);
    // `MayOutliveCall`: a constructor may PROMOTE a by-reference parameter into a property
    // (`__construct(public int &$value = 1)`), and that property borrows the argument's cell
    // for the whole life of the object. A caller-stack cell would be gone by the object's
    // first read of it, so this call keeps the heap cell (see `RefArgCellLifetime`).
    let call_args = super::super::materialize_direct_call_args_with_refs_and_options(
        ctx,
        &args,
        &param_types,
        &ref_params,
        true,
        crate::codegen::lower_inst::RefArgCellLifetime::MayOutliveCall,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    // A padding thunk stands in for the constructor when the site omitted defaulted arguments:
    // it takes what was passed and calls the real one with the declared defaults appended.
    // The thunk is an ordinary module function, so it answers to `function_symbol`; naming it
    // raw would emit a call to a symbol nothing defines.
    let symbol = match padding_thunk {
        Some(thunk) => crate::names::function_symbol(thunk),
        None => method_symbol(impl_class, constructor_key),
    };
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    super::super::emit_call_arg_temp_cleanups(ctx, &call_args, None)?;
    super::super::emit_borrowed_stack_mixed_arg_release(ctx, &call_args);
    emit_ref_arg_writebacks(ctx, &call_args)
}
