//! Purpose:
//! Collects property defaults and invokes constructors after allocation.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Literal defaults and constructor argument ownership preserve their established layout.

use super::*;

/// Selects direct literal copies or the class's EIR initializer for object property slots.
pub(super) fn collect_property_initialization(
    module: &crate::ir::Module,
    class_name: &str,
    class_info: &ClassInfo,
    inst: &Instruction,
) -> PropertyInitialization {
    if class_info.deferred_property_default_error.is_some() {
        return PropertyInitialization::Thunk {
            class_id: class_info.class_id,
        };
    }
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
        if let Some(value) = crate::codegen::literal_defaults::enum_case_literal_default(
            module,
            class_name,
            php_type,
            &default_expr.kind,
        ) {
            defaults.push(PropertyDefault {
                offset,
                value,
                is_reference: class_info.property_slot_is_owned_reference(index),
            });
            continue;
        }
        let resolved = crate::codegen::eval_class_constant_helpers::resolve_class_like_constants_in_literal(
            module,
            class_name,
            &default_expr.kind,
        );
        let resolved = crate::codegen::literal_defaults::resolve_literal_default_global_constants(
            &resolved,
            &module.global_constants,
        );
        let Ok(value) = literal_default_value(
            &format!("property {}::${}", class_name, property),
            php_type,
            &resolved,
            inst.op.name(),
        ) else {
            // No literal form (an array of enum cases, a constant this path cannot fold): the
            // class initializer is ordinary EIR and preserves lazy branches in constant
            // expressions that the direct literal copier deliberately does not interpret.
            return PropertyInitialization::Thunk {
                class_id: class_info.class_id,
            };
        };
        defaults.push(PropertyDefault {
            offset,
            value,
            is_reference: class_info.property_slot_is_owned_reference(index),
        });
    }
    PropertyInitialization::Inline(defaults)
}

/// Initializes an object held in an EIR value through literals or its EIR thunk.
pub(super) fn emit_property_initialization(
    ctx: &mut FunctionContext<'_>,
    object: crate::ir::ValueId,
    initialization: &PropertyInitialization,
) -> Result<()> {
    match initialization {
        PropertyInitialization::Inline(defaults) => emit_property_defaults(ctx, object, defaults),
        PropertyInitialization::Thunk { class_id } => {
            let argument = abi::int_arg_reg_name(ctx.emitter.target, 0);
            ctx.load_value_to_reg(object, argument)?;
            emit_property_init_thunk_call(ctx, *class_id)
        }
    }
}

/// Initializes an object parked in a temporary stack slot through literals or its EIR thunk.
pub(super) fn emit_stacked_property_initialization(
    ctx: &mut FunctionContext<'_>,
    object_stack_offset: usize,
    initialization: &PropertyInitialization,
) -> Result<()> {
    match initialization {
        PropertyInitialization::Inline(defaults) => {
            let object_reg = abi::secondary_scratch_reg(ctx.emitter);
            for default in defaults {
                abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, object_stack_offset);
                emit_property_default_into_slot_or_cell(ctx, object_reg, default)?;
            }
            Ok(())
        }
        PropertyInitialization::Thunk { class_id } => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 0),
                object_stack_offset,
            );
            emit_property_init_thunk_call(ctx, *class_id)
        }
    }
}

/// Calls one emitted class-property initializer with the object in ABI argument zero.
fn emit_property_init_thunk_call(ctx: &mut FunctionContext<'_>, class_id: u64) -> Result<()> {
    let thunk = format!("_class_propinit_{class_id}");
    if !ctx.module.has_function_named(&thunk) {
        return Err(CodegenIrError::unsupported(format!(
            "object property initializer thunk {thunk} was not emitted"
        )));
    }
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, 0);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &thunk);
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    Ok(())
}

/// Calls `_class_propinit_<class_id>(object)` through `_class_propinit_ptrs`, the table
/// `__rt_new_by_name` already uses, when the class has a thunk (a zero entry means none).
///
/// `object_reg` holds the object on entry and again on exit; the thunk is an ordinary compiled
/// function, so the object is kept on the stack across the call.
fn emit_property_init_thunk_call_via_table(ctx: &mut FunctionContext<'_>, object_reg: &str, class_id: u64) {
    let skip = ctx.next_label("propinit_thunk_done");
    abi::emit_push_reg(ctx.emitter, object_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // The object moves to the argument register FIRST: `object_reg` may be one of the
            // scratch registers the table lookup below uses.
            ctx.emitter.instruction(&format!("mov x0, {}", object_reg)); // this = the new object (first argument)
            abi::emit_symbol_address(ctx.emitter, "x10", "_class_propinit_ptrs");
            abi::emit_load_int_immediate(ctx.emitter, "x11", class_id as i64);
            ctx.emitter.instruction("ldr x10, [x10, x11, lsl #3]");             // _class_propinit_ptrs[class_id] (0 = no thunk)
            ctx.emitter.instruction(&format!("cbz x10, {}", skip)); // no thunk: the literal defaults are all there is
            ctx.emitter.instruction("blr x10");                                 // _class_propinit_<id>(this) assigns every default
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rdi, {}", object_reg)); // this = the new object (first SysV argument)
            abi::emit_symbol_address(ctx.emitter, "r10", "_class_propinit_ptrs");
            ctx.emitter.instruction(&format!("mov r10, QWORD PTR [r10 + {}]", class_id * 8)); // _class_propinit_ptrs[class_id] (0 = no thunk)
            ctx.emitter.instruction("test r10, r10");                           // does this class have a property-init thunk?
            ctx.emitter.instruction(&format!("jz {}", skip)); // no thunk: the literal defaults are all there is
            ctx.emitter.instruction("call r10");                                // _class_propinit_<id>(this) assigns every default
        }
    }
    ctx.emitter.label(&skip);
    abi::emit_pop_reg(ctx.emitter, object_reg);
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
        emit_property_default_into_slot_or_cell(ctx, object_reg, default)?;
    }
    Ok(())
}

/// Writes one property default into a freshly allocated object whose address is in `object_reg`.
///
/// An object-owned reference property's slot holds its cell pointer, so the default goes
/// THROUGH the cell: load the pointer, then write at the cell's value/tag words (offset 0).
/// Writing it into the slot replaces the pointer with the value, and the first read of the
/// property dereferences that. Clobbers `object_reg` for a reference default.
pub(super) fn emit_property_default_into_slot_or_cell(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
    default: &PropertyDefault,
) -> Result<()> {
    if !default.is_reference {
        return emit_property_default(ctx, object_reg, default);
    }
    abi::emit_load_from_address(ctx.emitter, object_reg, object_reg, default.offset);
    let cell_default = PropertyDefault {
        offset: 0,
        value: default.value.clone(),
        is_reference: false,
    };
    emit_property_default(ctx, object_reg, &cell_default)
}

/// Writes one literal property default into its object slot.
pub(super) fn emit_property_default(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
    default: &PropertyDefault,
) -> Result<()> {
    match &default.value {
        LiteralDefaultValue::InitThunk { class_id } => {
            emit_property_init_thunk_call_via_table(ctx, object_reg, *class_id);
        }
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
        LiteralDefaultValue::EnumCase {
            enum_name,
            case_name,
            boxed,
        } => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            crate::codegen::literal_defaults::emit_enum_case_default_to_result(
                ctx, enum_name, case_name, *boxed,
            );
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
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
        LiteralDefaultValue::BoxedAssocArray { value_type, entries } => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            crate::codegen::literal_defaults::emit_boxed_assoc_array_literal_to_result(
                ctx, value_type, entries,
            )?;
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
    let lifetime = super::super::constructor_ref_cell_lifetime(ctx, class_name);
    let call_args = super::super::materialize_direct_call_args_with_refs_and_options(
        ctx,
        &args,
        &param_types,
        &ref_params,
        true,
        lifetime,
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
