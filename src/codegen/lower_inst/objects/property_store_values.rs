//! Purpose:
//! Materializes property-store values and emits compact packed-field stores.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Coercions and result restoration preserve the declared slot representation.

use super::*;
use crate::codegen::lower_inst::enums::emit_mixed_tag_branch;
use crate::codegen::platform::Arch;

/// Loads an SSA value in the shape required by a typed object property store.
pub(super) fn load_property_store_value_to_result(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    slot: &PropertySlot,
) -> Result<()> {
    let slot_ty = &slot.php_type;
    let storage_ty = &slot.storage_type;
    let value_ty = ctx.value_php_type(value)?;
    if can_box_value_for_mixed_property(&value_ty, slot_ty) {
        let loaded_ty = ctx.load_value_to_result(value)?.codegen_repr();
        // Property stores do not consume the SSA source; explicit release ops still
        // own temporary cleanup after `prop_set`.
        emit_box_current_value_as_mixed(ctx.emitter, &loaded_ty);
        return Ok(());
    }
    if can_store_boxed_value_for_mixed_property(&value_ty, slot_ty) {
        ctx.load_value_to_result(value)?;
        // Transfer an unreleased owning box into the property; retain borrowed values and
        // temporaries whose explicit EIR cleanup still owns the source reference.
        if !ctx.value_can_own_mixed_box_source(value)? {
            abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
        }
        return Ok(());
    }
    if can_convert_indexed_array_to_mixed_property(&value_ty, slot_ty) {
        let loaded_ty = ctx.load_value_to_result(value)?.codegen_repr();
        let PhpType::Array(source_elem) = &loaded_ty else {
            return Err(CodegenIrError::unsupported(format!(
                "property array widening from PHP type {:?}",
                value_ty
            )));
        };
        // Give the conversion helper an owned candidate. Its COW split consumes that retain
        // while leaving the SSA source untouched, and the returned unique array transfers
        // directly into the property slot.
        abi::emit_incref_if_refcounted(ctx.emitter, &loaded_ty);
        emit_loaded_indexed_array_to_mixed(ctx, &source_elem.codegen_repr());
        return Ok(());
    }
    if can_store_assoc_array_as_mixed_property(&value_ty, slot_ty) {
        let loaded_ty = ctx.load_value_to_result(value)?.codegen_repr();
        let PhpType::AssocArray {
            value: source_value,
            ..
        } = &loaded_ty
        else {
            return Err(CodegenIrError::unsupported(format!(
                "property associative-array widening from PHP type {:?}",
                value_ty
            )));
        };
        // Retain before a possible COW conversion so `PropSet` never consumes the SSA source.
        // The retained value itself is the property owner when the hash already stores Mixed
        // entries.
        abi::emit_incref_if_refcounted(ctx.emitter, &loaded_ty);
        if source_value.codegen_repr() != PhpType::Mixed {
            emit_loaded_assoc_array_to_mixed(ctx);
        }
        return Ok(());
    }
    if can_store_value_as_tagged_scalar_property(&value_ty, slot_ty) {
        match value_ty.codegen_repr() {
            PhpType::Void | PhpType::Never => {
                crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
            }
            _ => {
                ctx.load_value_to_result(value)?;
                coerce_loaded_value_to_tagged_scalar(ctx, &value_ty)?;
            }
        }
        return Ok(());
    }
    if can_coerce_scalar_to_int_property(&value_ty, slot_ty) {
        ctx.load_value_to_result(value)?;
        crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
        return Ok(());
    }
    if matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        if matches!(slot_ty.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. }) {
            ctx.load_value_to_result(value)?;
            emit_mixed_typed_property_value(ctx, slot, None);
            return Ok(());
        }
        load_value_to_first_int_arg(ctx, value)?;
        match slot_ty.codegen_repr() {
            PhpType::Str => emit_mixed_string_for_persistent_store(ctx),
            PhpType::Int => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int"),
            PhpType::Bool => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool"),
            PhpType::Float => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float"),
            PhpType::Object(expected_class) => {
                ctx.load_value_to_result(value)?;
                emit_mixed_typed_property_value(ctx, slot, Some(&expected_class));
            }
            _ => {}
        }
        return Ok(());
    }
    let loaded_ty = ctx.load_value_to_result(value)?;
    if storage_ty.codegen_repr() == PhpType::Mixed {
        emit_box_current_value_as_mixed(ctx.emitter, &loaded_ty.codegen_repr());
        return Ok(());
    }
    if matches!(storage_ty.codegen_repr(), PhpType::Str) {
        abi::emit_call_label(ctx.emitter, "__rt_str_persist");
        return Ok(());
    }
    if matches!(storage_ty.codegen_repr(), PhpType::Callable) {
        callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    } else if storage_ty.codegen_repr().is_refcounted() {
        abi::emit_incref_if_refcounted(ctx.emitter, &loaded_ty.codegen_repr());
    }
    Ok(())
}

/// Validates a boxed `Mixed` before retaining a concrete typed-property payload.
///
/// Both packed and associative storage are valid PHP arrays. Object properties additionally
/// accept only the declared class or one of its subclasses. Every rejected runtime shape throws
/// php-src's typed-property `TypeError` before ownership changes or slot writes occur.
fn emit_mixed_typed_property_value(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    expected_object_class: Option<&str>,
) {
    let accepted = ctx.next_label("mixed_typed_property_accepted");
    let hash_to_indexed = ctx.next_label("mixed_typed_property_hash_to_indexed");
    let done = ctx.next_label("mixed_typed_property_done");
    let bool_case = ctx.next_label("mixed_typed_property_bool");
    let true_case = ctx.next_label("mixed_typed_property_true");
    let false_case = ctx.next_label("mixed_typed_property_false");
    let object_case = ctx.next_label("mixed_typed_property_object");
    let incomplete_object_case = ctx.next_label("mixed_typed_property_incomplete_object");
    let fallback_case = ctx.next_label("mixed_typed_property_unknown");
    let generic_object = expected_object_class
        .is_some_and(|class_name| class_name.trim_start_matches('\\').is_empty());
    let closure_object = expected_object_class.is_some_and(|class_name| {
        class_name
            .trim_start_matches('\\')
            .eq_ignore_ascii_case("Closure")
    });
    let expected_type = if generic_object {
        "object".to_string()
    } else {
        expected_object_class
            .unwrap_or("array")
            .trim_start_matches('\\')
            .to_string()
    };
    let mut scalar_types = vec![(0, "int"), (1, "string"), (2, "float"), (8, "null"), (9, "resource"), (10, "Closure")];
    if expected_object_class.is_some() {
        scalar_types.extend([(4, "array"), (5, "array")]);
    }
    let scalar_cases = scalar_types
        .into_iter()
        .map(|(tag, type_name)| (tag, type_name, ctx.next_label("mixed_typed_property_type_error")))
        .collect::<Vec<_>>();
    let mut object_cases = ctx
        .module
        .class_infos
        .iter()
        .map(|(class_name, info)| (info.class_id, class_name.trim_start_matches('\\').to_string()))
        .collect::<Vec<_>>();
    object_cases.sort_by_key(|(class_id, _)| *class_id);
    let accepted_object_class_ids = expected_object_class.map(|_| {
        if generic_object {
            return object_cases
                .iter()
                .map(|(class_id, _)| *class_id)
                .collect::<Vec<_>>();
        }
        object_cases
            .iter()
            .filter_map(|(class_id, class_name)| {
                can_store_object_for_object_property(
                    ctx,
                    &PhpType::Object(class_name.clone()),
                    &slot.php_type,
                )
                .then_some(*class_id)
            })
            .collect::<Vec<_>>()
    })
        .unwrap_or_default();
    let object_error_cases = object_cases
        .iter()
        .map(|(class_id, class_name)| {
            (
                *class_id,
                typed_property_type_error(slot, class_name, &expected_type),
                ctx.next_label("mixed_typed_property_object_error"),
            )
        })
        .collect::<Vec<_>>();

    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let tag_reg = abi::int_result_reg(ctx.emitter);
    if expected_object_class.is_none() {
        emit_mixed_tag_branch(ctx, tag_reg, 4, &accepted);
        emit_mixed_tag_branch(
            ctx,
            tag_reg,
            5,
            if matches!(slot.storage_type.codegen_repr(), PhpType::Array(_)) {
                &hash_to_indexed
            } else {
                &accepted
            },
        );
    }
    emit_mixed_tag_branch(ctx, tag_reg, 3, &bool_case);
    emit_mixed_tag_branch(
        ctx,
        tag_reg,
        6,
        if generic_object { &accepted } else { &object_case },
    );
    if generic_object || closure_object {
        emit_mixed_tag_branch(ctx, tag_reg, 10, &accepted);
    }
    for (tag, _, label) in &scalar_cases {
        emit_mixed_tag_branch(ctx, tag_reg, *tag, label);
    }
    abi::emit_jump(ctx.emitter, &fallback_case);

    ctx.emitter.label(&bool_case);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", false_case));        // name a false boxed value exactly as PHP's property TypeError does
            abi::emit_jump(ctx.emitter, &true_case);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdi, rdi");                           // inspect the unboxed boolean payload before formatting its TypeError
            ctx.emitter.instruction(&format!("jz {}", false_case));             // a zero payload is PHP false
            abi::emit_jump(ctx.emitter, &true_case);
        }
    }

    ctx.emitter.label(&object_case);
    let class_id_reg = abi::secondary_scratch_reg(ctx.emitter);
    let candidate_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let payload_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    };
    abi::emit_load_from_address(ctx.emitter, class_id_reg, payload_reg, 0);
    if !generic_object {
        abi::emit_load_int_immediate(ctx.emitter, candidate_reg, -2);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, candidate_reg)); // identify the reserved __PHP_Incomplete_Class object id
                ctx.emitter.instruction(&format!("b.eq {}", incomplete_object_case)); // preserve php-src's concrete incomplete-object diagnostic
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, candidate_reg)); // identify the reserved __PHP_Incomplete_Class object id
                ctx.emitter.instruction(&format!("je {}", incomplete_object_case)); // preserve php-src's concrete incomplete-object diagnostic
            }
        }
    }
    for class_id in &accepted_object_class_ids {
        abi::emit_load_int_immediate(ctx.emitter, candidate_reg, *class_id as i64);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, candidate_reg)); // compare the concrete payload class with the declared object-property hierarchy
                ctx.emitter.instruction(&format!("b.eq {}", accepted));         // accept an instance of the declared object type or a subclass
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, candidate_reg)); // compare the concrete payload class with the declared object-property hierarchy
                ctx.emitter.instruction(&format!("je {}", accepted));           // accept an instance of the declared object type or a subclass
            }
        }
    }
    for (class_id, _, label) in &object_error_cases {
        abi::emit_load_int_immediate(ctx.emitter, candidate_reg, *class_id as i64);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, candidate_reg)); // compare the payload object's class id with this static PHP name
                ctx.emitter.instruction(&format!("b.eq {}", label));            // raise the matching concrete-object property TypeError
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, candidate_reg)); // compare the payload object's class id with this static PHP name
                ctx.emitter.instruction(&format!("je {}", label));              // raise the matching concrete-object property TypeError
            }
        }
    }
    abi::emit_jump(ctx.emitter, &fallback_case);

    ctx.emitter.label(&true_case);
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx,
        &typed_property_type_error(slot, "true", &expected_type),
    );
    ctx.emitter.label(&false_case);
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx,
        &typed_property_type_error(slot, "false", &expected_type),
    );
    for (_, type_name, label) in &scalar_cases {
        ctx.emitter.label(label);
        crate::codegen::lower_inst::exceptions::emit_type_error(
            ctx,
            &typed_property_type_error(slot, type_name, &expected_type),
        );
    }
    for (_, message, label) in &object_error_cases {
        ctx.emitter.label(label);
        crate::codegen::lower_inst::exceptions::emit_type_error(ctx, message);
    }
    ctx.emitter.label(&incomplete_object_case);
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx,
        &typed_property_type_error(slot, "__PHP_Incomplete_Class", &expected_type),
    );
    ctx.emitter.label(&fallback_case);
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx,
        &typed_property_type_error(slot, "object", &expected_type),
    );

    ctx.emitter.label(&accepted);
    match slot.storage_type.codegen_repr() {
        PhpType::Mixed => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        PhpType::Callable => {
            match ctx.emitter.target.arch {
                Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),         // publish the accepted Closure descriptor as the property-store result
                Arch::X86_64 => ctx.emitter.instruction("mov rax, rdi"),        // publish the accepted Closure descriptor as the property-store result
            }
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
        }
        storage_ty => {
            match ctx.emitter.target.arch {
                Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),         // publish the accepted indexed/hash payload as the property-store result
                Arch::X86_64 => ctx.emitter.instruction("mov rax, rdi"),        // publish the accepted indexed/hash payload as the property-store result
            }
            abi::emit_incref_if_refcounted(ctx.emitter, &storage_ty);
        }
    }
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_to_indexed);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                             // pass the decoded numeric-key hash to the indexed-array converter
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_indexed_array");
        }
        Arch::X86_64 => {
            // `__rt_mixed_unbox` already leaves the low payload in rdi.
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_indexed_array");
        }
    }
    ctx.emitter.label(&done);
}

/// Formats php-src's typed-property assignment error for one rejected runtime value name.
fn typed_property_type_error(
    slot: &PropertySlot,
    actual_type: &str,
    expected_type: &str,
) -> String {
    let property = format!(
        "{}::${}",
        slot.declaring_class_name.trim_start_matches('\\'),
        slot.property,
    );
    if slot.is_reference {
        format!(
            "Cannot assign {} to reference held by property {} of type {}",
            actual_type, property, expected_type,
        )
    } else {
        format!(
            "Cannot assign {} to property {} of type {}",
            actual_type, property, expected_type,
        )
    }
}

/// Emits a compact packed-field store without writing object-property metadata words.
pub(super) fn emit_packed_field_store(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    match &slot.php_type {
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            ctx.load_value_to_reg(value, float_reg)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            abi::emit_store_to_address(ctx.emitter, float_reg, base_reg, slot.offset);
        }
        PhpType::Bool
        | PhpType::False
        | PhpType::Int
        | PhpType::Void
        | PhpType::Never
        | PhpType::Pointer(_)
        | PhpType::Resource(_) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            ctx.load_value_to_reg(value, int_reg)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            abi::emit_store_to_address(ctx.emitter, int_reg, base_reg, slot.offset);
        }
        _ => {
            return Err(CodegenIrError::unsupported(format!(
                "packed field store for PHP type {:?}",
                slot.php_type
            )))
        }
    }
    Ok(())
}

/// Returns true for property values represented as a single pointer-sized word.
pub(super) fn is_pointer_sized_property_type(php_type: &PhpType) -> bool {
    matches!(
        php_type.codegen_repr(),
        PhpType::Iterable
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Buffer(_)
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_)
            | PhpType::Resource(_)
    )
}

/// Lowers `Op::PackedFieldMixedToInt`: narrows a boxed `Mixed` value to the raw `I64`
/// payload a packed `int` field stores. Strict by design — only the int tag passes; every
/// other runtime tag throws a catchable `TypeError` naming the runtime type, because a
/// packed field is a fixed-layout systems extension and the PHP coercions the enum variant
/// performs (float truncation, numeric strings, null-to-0) would silently swallow the very
/// overflow promotion the boxed value exists to carry.
pub(in crate::codegen::lower_inst) fn lower_packed_field_mixed_to_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    use super::super::enums::{emit_mixed_tag_branch, emit_move_reg, emit_throw_int_arg_type_error};
    use crate::codegen::platform::Arch;

    let input = *inst.operands.first().ok_or_else(|| {
        CodegenIrError::unsupported("packed_field_mixed_to_int without operand".to_string())
    })?;
    let Some(crate::ir::Immediate::Data(data_id)) = inst.immediate else {
        return Err(CodegenIrError::unsupported(
            "packed_field_mixed_to_int without a TypeError message prefix".to_string(),
        ));
    };
    let (prefix_label, prefix_len) = ctx.intern_string_data(data_id)?;
    let loaded_ty = ctx.load_value_to_result(input)?.codegen_repr();
    // Constant folding runs AFTER lowering and can retype the operand under the op: a
    // checker-Mixed value becomes a raw scalar. Unboxing it as a pointer is a segfault,
    // so raw ints pass straight through and a raw float throws like its boxed twin.
    if matches!(loaded_ty, crate::types::PhpType::Int) {
        return store_if_result(ctx, inst);
    }
    if matches!(loaded_ty, crate::types::PhpType::Float) {
        emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "float given");
        return store_if_result(ctx, inst);
    }
    // Unbox the Mixed cell. `__rt_mixed_unbox` returns tag in the int-result register and the
    // payload lo/hi in target-specific registers (AArch64: x1/x2; x86_64: rdi/rdx).
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let tag_reg = abi::int_result_reg(ctx.emitter);
    let lo_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    };
    let done = ctx.next_label("packed_mixed_to_int_done");
    let l_int = ctx.next_label("packed_mixed_to_int_ok");
    let l_string = ctx.next_label("packed_mixed_to_int_string");
    let l_float = ctx.next_label("packed_mixed_to_int_float");
    let l_bool = ctx.next_label("packed_mixed_to_int_bool");
    let l_array = ctx.next_label("packed_mixed_to_int_array");
    let l_null = ctx.next_label("packed_mixed_to_int_null");
    let l_resource = ctx.next_label("packed_mixed_to_int_resource");
    let l_callable = ctx.next_label("packed_mixed_to_int_callable");
    // Tag values: 0 int, 1 string, 2 float, 3 bool, 4 indexed array, 5 hash, 6 object,
    // 8 null, 9 resource, 10 callable (7 nested is peeled by `__rt_mixed_unbox`).
    emit_mixed_tag_branch(ctx, tag_reg, 0, &l_int);
    emit_mixed_tag_branch(ctx, tag_reg, 1, &l_string);
    emit_mixed_tag_branch(ctx, tag_reg, 2, &l_float);
    emit_mixed_tag_branch(ctx, tag_reg, 3, &l_bool);
    emit_mixed_tag_branch(ctx, tag_reg, 4, &l_array);
    emit_mixed_tag_branch(ctx, tag_reg, 5, &l_array);
    emit_mixed_tag_branch(ctx, tag_reg, 8, &l_null);
    emit_mixed_tag_branch(ctx, tag_reg, 9, &l_resource);
    emit_mixed_tag_branch(ctx, tag_reg, 10, &l_callable);
    // Any other tag is an object-like value; each arm throws and never falls through.
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "object given");
    ctx.emitter.label(&l_string);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "string given");
    ctx.emitter.label(&l_float);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "float given");
    ctx.emitter.label(&l_bool);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "bool given");
    ctx.emitter.label(&l_array);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "array given");
    ctx.emitter.label(&l_null);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "null given");
    ctx.emitter.label(&l_resource);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "resource given");
    ctx.emitter.label(&l_callable);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "Closure given");
    // int: the payload is already the raw field word.
    ctx.emitter.label(&l_int);
    emit_move_reg(ctx, tag_reg, lo_reg);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}
