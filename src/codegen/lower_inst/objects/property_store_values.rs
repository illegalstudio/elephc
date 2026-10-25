//! Purpose:
//! Materializes property-store values and emits compact packed-field stores.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Coercions and result restoration preserve the declared slot representation.

use super::*;

/// Loads an SSA value in the shape required by a typed object property store.
///
/// A value that is only known as a boxed `Mixed`/`Union` runs the weak-mode typed-property
/// guard first. The guard reads the box and never touches the destination, so a value PHP
/// rejects throws `TypeError` while the slot still owns its previous contents.
pub(super) fn load_property_store_value_to_result(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    slot: &PropertySlot,
) -> Result<()> {
    let slot_ty = &slot.php_type;
    let value_ty = ctx.value_php_type(value)?;
    // A `false`-declared slot accepts ONLY the exact false bool: route a boxed mixed/union
    // source through the false-slot guard instead of the bool truthiness cast, which would
    // coerce `0`, `""` and `[]` into `false` where php throws. The nullable `?false` shape is
    // a recorded limitation: its slot stores a box through the mixed machinery, and its
    // default/read paths do not round-trip a stored `false` today.
    if slot_ty == &PhpType::False
        && matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
    {
        return emit_false_slot_mixed_guard(ctx, value, slot);
    }
    let coerced_done_label = if matches!(value_ty.codegen_repr(), PhpType::Mixed) {
        emit_mixed_property_type_guard(ctx, value, slot)?
    } else {
        None
    };
    load_accepted_property_store_value_to_result(ctx, value, slot_ty, &value_ty)?;
    if let Some(done_label) = coerced_done_label {
        ctx.emitter.label(&done_label);
    }
    Ok(())
}

/// Materializes a property-store value the weak-mode guard has already accepted.
fn load_accepted_property_store_value_to_result(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    slot_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    let value_ty = value_ty.clone();
    if can_box_value_for_mixed_property(&value_ty, slot_ty) {
        let loaded_ty = ctx.load_value_to_result(value)?.codegen_repr();
        let box_ty = ctx.mixed_box_type(value)?;
        let staged = if loaded_ty == PhpType::Str {
            let (pointer_reg, length_reg) = abi::string_result_regs(ctx.emitter);
            super::super::ownership::stage_string_byte_view(
                ctx,
                value,
                pointer_reg,
                length_reg,
            )
        } else {
            false
        };
        // Property stores do not consume the SSA source; explicit release ops still
        // own temporary cleanup after `prop_set`.
        emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
        super::super::ownership::release_string_byte_stage(ctx, staged);
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
        // Inline tagged storage copies the payload OUT of the source box and keeps no pointer
        // to it, so a source EIR expects the consumer to adopt has no owner left afterwards.
        // Without this release a `?int` slot leaked one boxed cell per runtime-shaped write.
        release_adopted_mixed_source(ctx, value, &PhpType::TaggedScalar)?;
        return Ok(());
    }
    if can_coerce_tagged_scalar_to_int_property(&value_ty, slot_ty) {
        ctx.load_value_to_result(value)?;
        crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
        return Ok(());
    }
    if matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        load_value_to_first_int_arg(ctx, value)?;
        match slot_ty.codegen_repr() {
            PhpType::Str => {
                emit_mixed_string_for_persistent_store(ctx);
                // The slot retains the string payload, not the cell that carried it. Keep this
                // cleanup shared by nullable and mixed receiver dispatch, like scalar narrowing.
                release_adopted_mixed_source(ctx, value, &PhpType::Str)?;
            }
            // A scalar slot stores the payload word the cast extracted, so the box's own
            // reference has no owner left; retire it once the cast has copied the value out.
            // A `?ptr<T>` slot stores the address word, cast like an int: PHP null becomes 0.
            PhpType::Int | PhpType::Pointer(_) => {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
                release_adopted_mixed_source(ctx, value, &PhpType::Int)?;
            }
            PhpType::Bool => {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
                release_adopted_mixed_source(ctx, value, &PhpType::Bool)?;
            }
            PhpType::Float => {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
                release_adopted_mixed_source(ctx, value, &PhpType::Float)?;
            }
            // An object slot stores the unboxed object POINTER, retained on its own, so the
            // adopted cell keeps no owner here either. Without this release a declared object
            // property leaked one boxed cell and its payload owner per runtime-shaped write.
            PhpType::Object(_) => {
                property_values::emit_mixed_object_for_property_store(ctx);
                release_adopted_mixed_source(ctx, value, &PhpType::Object(String::new()))?;
            }
            // An `iterable` slot stores the raw container/object pointer, not the cell wrapping
            // it, so the payload is promoted out of the box and retained on its own. The box's
            // own reference then has no owner left here, which is what the release ends.
            PhpType::Iterable => {
                emit_mixed_iterable_for_property_store(ctx);
                release_adopted_mixed_source(ctx, value, &PhpType::Iterable)?;
            }
            _ => {}
        }
        return Ok(());
    }
    let loaded_ty = ctx.load_value_to_result(value)?;
    if matches!(slot_ty.codegen_repr(), PhpType::Str) {
        super::super::ownership::persist_loaded_string(ctx, value)?;
        return Ok(());
    }
    if matches!(slot_ty.codegen_repr(), PhpType::Callable) {
        callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    } else if slot_ty.codegen_repr().is_refcounted() {
        abi::emit_incref_if_refcounted(ctx.emitter, &loaded_ty.codegen_repr());
    }
    Ok(())
}

/// The `__rt_mixed_unbox` tags an `iterable` slot can hold: both array shapes and an object.
const ITERABLE_MIXED_TAGS: [u8; 3] = [4, 5, 6];

/// Promotes the array or `Traversable` payload of a boxed `Mixed` into `iterable` storage.
///
/// An `iterable` slot holds the same raw heap pointer a statically typed `array`/`Traversable`
/// write stores: `__rt_decref_any` and the `iterable` foreach dispatch both read the heap kind off
/// that pointer, so storing the Mixed cell itself would publish a cell where a container is
/// expected. The retained owner is therefore the PAYLOAD, taken out of the cell.
///
/// The weak-mode guard already refused every tag this slot cannot hold, so the fallback only
/// normalizes to a null pointer rather than republishing an unrelated payload as a heap pointer,
/// which both `__rt_incref` and the slot's later `__rt_decref_any` already ignore.
fn emit_mixed_iterable_for_property_store(ctx: &mut FunctionContext<'_>) {
    let payload_label = ctx.next_label("prop_store_mixed_iterable_payload");
    let done = ctx.next_label("prop_store_mixed_iterable_done");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let result_reg = abi::int_result_reg(ctx.emitter);
    for tag in ITERABLE_MIXED_TAGS {
        super::super::enums::emit_mixed_tag_branch(ctx, result_reg, i64::from(tag), &payload_label);
    }
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0); // normalize a payload `iterable` cannot hold to a null pointer
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&payload_label);
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    abi::emit_reg_move(ctx.emitter, result_reg, payload_reg); // promote the unboxed iterable pointer into the result register
    ctx.emitter.label(&done);
    abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Iterable); // retain the payload independently for property storage
}

/// Releases a boxed source the slot was expected to adopt but did not keep a pointer to.
///
/// `value_can_own_mixed_box_source()` is EIR's promise that this consumer takes the box over, so
/// EIR emits no cleanup of its own for it. A slot that copies the payload instead of storing the
/// box has to end that ownership here.
pub(in crate::codegen::lower_inst) fn release_adopted_mixed_source(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    result_ty: &PhpType,
) -> Result<()> {
    if !matches!(ctx.value_php_type(value)?.codegen_repr(), PhpType::Mixed)
        || !ctx.value_can_own_mixed_box_source(value)?
    {
        return Ok(());
    }
    let result_ty = result_ty.codegen_repr();
    abi::emit_push_result_value(ctx.emitter, &result_ty);
    ctx.load_value_to_result(value)?;
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    restore_property_store_result(ctx, &result_ty);
    Ok(())
}

/// Emits a compact packed-field store without writing object-property metadata words.
///
/// A width-declared field stores only its width: an integer keeps its low bytes like a C
/// conversion and an `f32` is rounded to single precision.
pub(super) fn emit_packed_field_store(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    if let Some(width) = slot.width {
        return emit_packed_width_field_store(ctx, value, slot, width, base_reg);
    }
    match &slot.php_type {
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            if matches!(ctx.value_php_type(value)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
                load_property_store_value_to_result(ctx, value, slot)?;
            } else {
                ctx.load_value_to_reg(value, float_reg)?;
            }
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
            ensure_packed_word_value_is_unboxed(ctx, value, slot)?;
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            ctx.load_value_to_reg(value, int_reg)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            if slot.php_type == PhpType::False {
                emit_false_slot_payload_guard(ctx, slot);
            }
            abi::emit_store_to_address(ctx.emitter, int_reg, base_reg, slot.offset);
        }
        PhpType::Packed(record) => {
            emit_packed_record_copy(ctx, value, record, slot.offset, base_reg)?;
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

/// Refuses a boxed value at a packed field stored from the integer result register.
///
/// Lowering narrows every boxed value bound for a fixed-layout `int`, `bool` or integer-width
/// field to its raw payload (`narrow_mixed_value_for_fixed_layout_field` in
/// `ir_lower::stmt::instance_property_writes`), so a box reaching this store is a lowering bug.
/// Loading it would write the cell's ADDRESS into the field, which is silent data corruption;
/// failing the compile reports the bug instead.
fn ensure_packed_word_value_is_unboxed(
    ctx: &FunctionContext<'_>,
    value: crate::ir::ValueId,
    slot: &PropertySlot,
) -> Result<()> {
    if matches!(ctx.value_php_type(value)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "packed field {}::${} store of a boxed value that lowering did not narrow",
            slot.class_name, slot.property
        )));
    }
    Ok(())
}

/// Stores a value into a packed field declared with a C width name.
///
/// The integer payload is loaded into the integer result register (a boxed `Mixed` was already
/// narrowed to a raw int by lowering), an `f32` payload into the float result register with the
/// same `Mixed`-to-float conversion a `float` field uses; the receiver address survives the load
/// on the temporary stack, and the shared width emitter truncates or rounds the payload.
fn emit_packed_width_field_store(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    slot: &PropertySlot,
    width: ScalarWidth,
    base_reg: &str,
) -> Result<()> {
    if width != ScalarWidth::F32 {
        ensure_packed_word_value_is_unboxed(ctx, value, slot)?;
    }
    abi::emit_push_reg(ctx.emitter, base_reg);
    let payload_reg = match width {
        ScalarWidth::F32 => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            if matches!(ctx.value_php_type(value)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
                load_property_store_value_to_result(ctx, value, slot)?;
            } else {
                ctx.load_value_to_reg(value, float_reg)?;
            }
            float_reg
        }
        _ => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            ctx.load_value_to_reg(value, int_reg)?;
            int_reg
        }
    };
    abi::emit_pop_reg(ctx.emitter, base_reg);
    super::super::scalar_widths::emit_width_store(ctx, width.abi(), base_reg, slot.offset, payload_reg)
}

/// Copies a whole embedded record into a nested packed field.
///
/// `value` holds the address of a record of the same packed class (a buffer element, another
/// nested field, or a `ptr<T>`); its `total_size` bytes are copied into the field at
/// `base_reg + field_offset`, one 8-byte word at a time and then the tail a record of
/// width-declared fields can leave (4, 2 and 1 bytes, in that order). Like C struct assignment,
/// the copy assumes the source and the destination are the same record or do not overlap. A
/// `ptr<T>` source is null-checked first. The copy advances `base_reg`, the source register and
/// two scratch registers; the store is the last thing the property write emits, so none of them
/// is read afterwards.
fn emit_packed_record_copy(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    record: &str,
    field_offset: usize,
    base_reg: &str,
) -> Result<()> {
    let size = ctx
        .module
        .packed_class_infos
        .get(record.trim_start_matches('\\'))
        .map(|info| info.total_size)
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!("unknown packed record type {}", record))
        })?;
    // -- load the source record address and null-check a ptr<T> source --
    let source_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, base_reg);
    ctx.load_value_to_reg(value, source_reg)?;
    abi::emit_pop_reg(ctx.emitter, base_reg);
    if matches!(ctx.value_php_type(value)?.codegen_repr(), PhpType::Pointer(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_ptr_check_nonnull");
    }
    if size == 0 {
        return Ok(());
    }
    let data_reg = abi::secondary_scratch_reg(ctx.emitter);
    let count_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let loop_label = ctx.next_label("packed_record_copy");
    if field_offset != 0 {
        // -- point the destination at the nested record field --
        abi::emit_load_int_immediate(ctx.emitter, count_reg, field_offset as i64);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("add {}, {}, {}", base_reg, base_reg, count_reg)); // point the destination at the nested record field
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("add {}, {}", base_reg, count_reg)); // point the destination at the nested record field
            }
        }
    }
    let words = size / 8;
    if words > 0 {
        // -- copy the record one 8-byte word at a time --
        abi::emit_load_int_immediate(ctx.emitter, count_reg, words as i64);
        ctx.emitter.label(&loop_label);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("ldr {}, [{}], #8", data_reg, source_reg)); // load the next source record word and advance the source
                ctx.emitter.instruction(&format!("str {}, [{}], #8", data_reg, base_reg)); // store it into the nested field and advance the destination
                ctx.emitter.instruction(&format!("subs {}, {}, #1", count_reg, count_reg)); // count down the record words still to copy
                ctx.emitter.instruction(&format!("b.ne {}", loop_label));       // keep copying until the whole record is written
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{}]", data_reg, source_reg)); // load the next source record word
                ctx.emitter.instruction(&format!("mov QWORD PTR [{}], {}", base_reg, data_reg)); // store it into the nested field
                ctx.emitter.instruction(&format!("add {}, 8", source_reg));     // advance the source to the next record word
                ctx.emitter.instruction(&format!("add {}, 8", base_reg));       // advance the destination to the next field word
                ctx.emitter.instruction(&format!("sub {}, 1", count_reg));      // count down the record words still to copy
                ctx.emitter.instruction(&format!("jne {}", loop_label));        // keep copying until the whole record is written
            }
        }
    }
    // -- copy the tail left by width-declared fields, widest piece first --
    let arch = ctx.emitter.target.arch;
    let tail = size % 8;
    for piece in [4usize, 2, 1] {
        if tail & piece == 0 {
            continue;
        }
        let piece_reg = super::super::scalar_widths::sub_register(arch, data_reg, piece);
        match arch {
            Arch::AArch64 => {
                let (load, store) = match piece {
                    4 => ("ldr", "str"),
                    2 => ("ldrh", "strh"),
                    _ => ("ldrb", "strb"),
                };
                ctx.emitter.instruction(&format!("{} {}, [{}], #{}", load, piece_reg, source_reg, piece)); // load the next tail piece and advance the source
                ctx.emitter.instruction(&format!("{} {}, [{}], #{}", store, piece_reg, base_reg, piece)); // store it into the nested field and advance the destination
            }
            Arch::X86_64 => {
                let ptr = match piece {
                    4 => "DWORD PTR",
                    2 => "WORD PTR",
                    _ => "BYTE PTR",
                };
                ctx.emitter.instruction(&format!("mov {}, {} [{}]", piece_reg, ptr, source_reg)); // load the next tail piece of the source record
                ctx.emitter.instruction(&format!("mov {} [{}], {}", ptr, base_reg, piece_reg)); // store it into the nested field
                ctx.emitter.instruction(&format!("add {}, {}", source_reg, piece)); // advance the source past the tail piece
                ctx.emitter.instruction(&format!("add {}, {}", base_reg, piece)); // advance the destination past the tail piece
            }
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
