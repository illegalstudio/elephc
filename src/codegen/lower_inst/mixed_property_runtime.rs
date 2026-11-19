//! Purpose:
//! Lowers runtime-typed property-array mutation and Mixed cell replacement.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers `$object->property[$key] = $value` when the property itself is runtime-typed.
pub(super) fn lower_property_array_runtime_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    let data = expect_data(inst)?;
    let property = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    match ctx.value_php_type(object)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => match ctx.emitter.target.arch {
            Arch::AArch64 => lower_mixed_property_array_runtime_set_aarch64(
                ctx,
                object,
                key,
                value,
                &property,
                "__rt_mixed_property_get",
            ),
            Arch::X86_64 => lower_mixed_property_array_runtime_set_x86_64(
                ctx,
                object,
                key,
                value,
                &property,
                "__rt_mixed_property_get",
            ),
        },
        PhpType::Object(class_name)
            if crate::types::checker::builtin_stdclass::is_stdclass(
                class_name.trim_start_matches('\\'),
            ) =>
        {
            match ctx.emitter.target.arch {
                Arch::AArch64 => lower_mixed_property_array_runtime_set_aarch64(
                    ctx,
                    object,
                    key,
                    value,
                    &property,
                    "__rt_stdclass_get",
                ),
                Arch::X86_64 => lower_mixed_property_array_runtime_set_x86_64(
                    ctx,
                    object,
                    key,
                    value,
                    &property,
                    "__rt_stdclass_get",
                ),
            }
        }
        other => Err(CodegenIrError::unsupported(format!(
            "runtime_call property array set with receiver PHP type {:?}",
            other
        ))),
    }
}

/// Lowers a property-array write through stdClass/Mixed property get and Mixed array set on AArch64.
pub(super) fn lower_mixed_property_array_runtime_set_aarch64(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    key: ValueId,
    value: ValueId,
    property: &str,
    getter_label: &str,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    }
    abi::emit_push_reg(ctx.emitter, "x0");
    hashes::materialize_hash_key_aarch64(ctx, key)?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    emit_property_array_target_get_aarch64(ctx, object, property, getter_label)?;
    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
    abi::emit_pop_reg(ctx.emitter, "x3");
    emit_owned_property_cell_set(ctx);
    Ok(())
}

/// Lowers a property-array write through stdClass/Mixed property get and Mixed array set on x86_64.
pub(super) fn lower_mixed_property_array_runtime_set_x86_64(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    key: ValueId,
    value: ValueId,
    property: &str,
    getter_label: &str,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    }
    abi::emit_push_reg(ctx.emitter, "rax");
    hashes::materialize_hash_key_x86_64(ctx, key)?;
    abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
    emit_property_array_target_get_x86_64(ctx, object, property, getter_label)?;
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the property Mixed cell as the array-write target
    abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
    abi::emit_pop_reg(ctx.emitter, "rcx");
    emit_owned_property_cell_set(ctx);
    Ok(())
}

/// Retires the getter's independent cell owner on successful and throwing dimension writes.
fn emit_owned_property_cell_set(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #64");                         // reserve a cleanup record and the three write operands
            ctx.emitter.instruction("str x0, [sp, #24]");                       // transfer the getter's owned Mixed cell to the unwind record
            ctx.emitter.instruction("str x0, [sp, #56]");                       // preserve normal ownership after unguard clears its record slot
            ctx.emitter.instruction("stp x1, x2, [sp, #32]");                   // preserve both normalized key words across guard publication
            ctx.emitter.instruction("str x3, [sp, #48]");                       // preserve the value consumed by the runtime setter
            abi::emit_symbol_address(ctx.emitter, "x9", "__rt_exception_release_owned");
            ctx.emitter.instruction("str x9, [sp, #8]");                        // install the callback that releases an abandoned getter result
            ctx.emitter.instruction("mov x0, sp");                              // pass the stack-local cleanup record
            ctx.emitter.instruction("mov x1, #0");                              // prepend this independent owner to the unwind chain
            abi::emit_call_label(ctx.emitter, "__rt_exception_guard_owned");
            ctx.emitter.instruction("ldr x0, [sp, #24]");                       // borrow the guarded property cell for mutation
            ctx.emitter.instruction("ldp x1, x2, [sp, #32]");                   // restore the normalized key
            ctx.emitter.instruction("ldr x3, [sp, #48]");                       // restore the setter's consumed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_array_set");
            ctx.emitter.instruction("mov x0, sp");                              // remove the exact guard before normal-path destruction
            abi::emit_call_label(ctx.emitter, "__rt_exception_unguard_owned");
            ctx.emitter.instruction("ldr x0, [sp, #56]");                       // recover the independent owner outside the cleared guard record
            ctx.emitter.instruction("add sp, sp, #64");                         // retire the record before a destructor can throw
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 64");                             // reserve an aligned cleanup record and write operands
            ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rdi");           // transfer the getter's owned Mixed cell to the unwind record
            ctx.emitter.instruction("mov QWORD PTR [rsp + 56], rdi");           // preserve normal ownership after unguard clears its record slot
            ctx.emitter.instruction("mov QWORD PTR [rsp + 32], rsi");           // preserve the normalized key payload
            ctx.emitter.instruction("mov QWORD PTR [rsp + 40], rdx");           // preserve the normalized key tag or length
            ctx.emitter.instruction("mov QWORD PTR [rsp + 48], rcx");           // preserve the setter's consumed value
            abi::emit_symbol_address(ctx.emitter, "r9", "__rt_exception_release_owned");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r9");             // install the abandoned-owner cleanup callback
            ctx.emitter.instruction("mov rdi, rsp");                            // pass the stack-local cleanup record
            ctx.emitter.instruction("xor esi, esi");                            // prepend this independent owner to the unwind chain
            abi::emit_call_label(ctx.emitter, "__rt_exception_guard_owned");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 24]");           // borrow the guarded property cell for mutation
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 32]");           // restore the key payload
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 40]");           // restore the key tag or length
            ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 48]");           // restore the setter's consumed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_array_set");
            ctx.emitter.instruction("mov rdi, rsp");                            // remove the exact guard before normal-path destruction
            abi::emit_call_label(ctx.emitter, "__rt_exception_unguard_owned");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 56]");           // recover the independent owner outside the cleared guard record
            ctx.emitter.instruction("add rsp, 64");                             // retire the record before a destructor can throw
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
}

/// Calls the requested property getter and leaves the boxed Mixed property cell in `x0`.
pub(super) fn emit_property_array_target_get_aarch64(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
    getter_label: &str,
) -> Result<()> {
    let (label, len) = ctx.data.add_string(property.as_bytes());
    ctx.load_value_to_reg(object, "x0")?;
    abi::emit_symbol_address(ctx.emitter, "x1", &label);
    abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
    abi::emit_call_label(ctx.emitter, getter_label);
    Ok(())
}

/// Calls the requested property getter and leaves the boxed Mixed property cell in `rax`.
pub(super) fn emit_property_array_target_get_x86_64(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
    getter_label: &str,
) -> Result<()> {
    let (label, len) = ctx.data.add_string(property.as_bytes());
    ctx.load_value_to_reg(object, "rdi")?;
    abi::emit_symbol_address(ctx.emitter, "rsi", &label);
    abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
    abi::emit_call_label(ctx.emitter, getter_label);
    Ok(())
}

/// Lowers a two-operand Mixed-cell replacement emitted for nested runtime assignments.
pub(super) fn lower_mixed_cell_runtime_assign(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let target = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    match ctx.value_php_type(target)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {}
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "runtime_call mixed-cell assignment with target PHP type {:?}",
                other
            )))
        }
    }
    box_value_for_mixed_cell_replacement(ctx, value)?;
    let drop_new = ctx.next_label("mixed_cell_assign_drop_new");
    let release_string = ctx.next_label("mixed_cell_assign_release_string");
    let copy_new = ctx.next_label("mixed_cell_assign_copy_new");
    let done = ctx.next_label("mixed_cell_assign_done");
    let result = abi::int_result_reg(ctx.emitter);

    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    abi::emit_store_to_sp(ctx.emitter, result, 8);
    ctx.load_value_to_reg(target, result)?;
    abi::emit_store_to_sp(ctx.emitter, result, 0);
    elephc_memoir::reference_cells::eval_writeback::emit_replace_staged_if_present(
        ctx.emitter, &release_string, &copy_new, &drop_new, &done,
    );
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    Ok(())
}

/// Boxes the replacement value into a fresh Mixed cell whose payload can be moved.
pub(super) fn box_value_for_mixed_cell_replacement(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        match ctx.emitter.target.arch {
            Arch::AArch64 => emit_box_runtime_payload_as_mixed(ctx.emitter, "x0", "x1", "x2"),
            Arch::X86_64 => emit_box_runtime_payload_as_mixed(ctx.emitter, "rax", "rdi", "rdx"),
        }
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    }
    Ok(())
}

/// Casts the boxed Mixed pointer currently returned by a runtime helper when needed.
pub(super) fn cast_loaded_mixed_pointer_to_result(
    ctx: &mut FunctionContext<'_>,
    target_ty: &PhpType,
) -> Result<()> {
    let label = match target_ty {
        PhpType::Mixed | PhpType::Union(_) => return Ok(()),
        PhpType::Str => "__rt_mixed_cast_string",
        // Pointers and typed buffer handles are raw words whose missing value is 0.
        PhpType::Int | PhpType::Pointer(_) | PhpType::Buffer(_) => "__rt_mixed_cast_int",
        PhpType::Float => "__rt_mixed_cast_float",
        PhpType::Bool => "__rt_mixed_cast_bool",
        PhpType::TaggedScalar => {
            coerce_loaded_value_to_tagged_scalar(ctx, &PhpType::Mixed)?;
            return Ok(());
        }
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Callable
        | PhpType::Iterable
        | PhpType::Object(_) => {
            emit_unbox_mixed_to_owned_refcounted_result(ctx, target_ty);
            return Ok(());
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "runtime mixed result cast to PHP type {:?}",
                other
            )))
        }
    };
    // Runtime-name property dispatch supplies an owned cell. A buffer result needs only its
    // handle, so retire that cell after extracting the payload without losing element metadata.
    let buffer_result = matches!(target_ty, PhpType::Buffer(_));
    let result_reg = abi::int_result_reg(ctx.emitter);
    if buffer_result { abi::emit_push_reg(ctx.emitter, result_reg); }
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the returned boxed Mixed pointer as the SysV first argument
    }
    abi::emit_call_label(ctx.emitter, label);
    if buffer_result {
        abi::emit_push_reg(ctx.emitter, result_reg);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
        abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
        abi::emit_pop_reg(ctx.emitter, result_reg);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
    }
    Ok(())
}
