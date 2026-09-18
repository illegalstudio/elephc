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
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_set");
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
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_set");
    Ok(())
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
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_mixed_cell_runtime_assign_aarch64(ctx, target)?,
        Arch::X86_64 => lower_mixed_cell_runtime_assign_x86_64(ctx, target)?,
    }
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

/// Replaces the payload inside an existing boxed Mixed cell on AArch64.
pub(super) fn lower_mixed_cell_runtime_assign_aarch64(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
) -> Result<()> {
    let drop_new = ctx.next_label("mixed_cell_assign_drop_new");
    let release_string = ctx.next_label("mixed_cell_assign_release_string");
    let copy_new = ctx.next_label("mixed_cell_assign_copy_new");
    let done = ctx.next_label("mixed_cell_assign_done");

    ctx.emitter.instruction("sub sp, sp, #32");                                 // reserve temporary slots for target and replacement Mixed cells
    ctx.emitter.instruction("str x0, [sp, #8]");                                // preserve the boxed replacement while loading the target cell
    ctx.load_value_to_reg(target, "x0")?;
    ctx.emitter.instruction("str x0, [sp, #0]");                                // preserve the target Mixed cell across payload-release helpers
    ctx.emitter.instruction(&format!("cbz x0, {}", drop_new));                  // drop the replacement when the target cell is missing
    ctx.emitter.instruction("ldr x9, [x0]");                                    // inspect the old payload tag before overwriting the cell
    ctx.emitter.instruction("cmp x9, #1");                                      // strings own a persisted heap payload that needs safe free
    ctx.emitter.instruction(&format!("b.eq {}", release_string));               // release string payloads through the string-safe free path
    ctx.emitter.instruction("cmp x9, #4");                                      // tags below array/hash/object/mixed are scalar payloads
    ctx.emitter.instruction(&format!("b.lo {}", copy_new));                     // scalar payloads can be overwritten directly
    ctx.emitter.instruction("cmp x9, #7");                                      // tags above the refcounted payload range are not released here
    ctx.emitter.instruction(&format!("b.hi {}", copy_new));                     // unknown/null payload tags can be overwritten directly
    ctx.emitter.instruction("ldr x0, [x0, #8]");                                // pass the old refcounted child payload to the generic release helper
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    ctx.emitter.instruction(&format!("b {}", copy_new));                        // continue with replacement after releasing the old child
    ctx.emitter.label(&release_string);
    ctx.emitter.instruction("ldr x0, [sp, #0]");                                // reload the target cell before reading its string payload
    ctx.emitter.instruction("ldr x0, [x0, #8]");                                // pass the old string payload pointer to the safe free helper
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    ctx.emitter.instruction(&format!("b {}", copy_new));                        // continue with replacement after freeing the old string
    ctx.emitter.label(&drop_new);
    ctx.emitter.instruction("ldr x0, [sp, #8]");                                // reload the unused replacement Mixed cell
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    ctx.emitter.instruction(&format!("b {}", done));                            // skip payload copy because there is no target cell
    ctx.emitter.label(&copy_new);
    ctx.emitter.instruction("ldr x10, [sp, #0]");                               // reload the destination Mixed cell pointer
    ctx.emitter.instruction("ldr x11, [sp, #8]");                               // reload the replacement Mixed cell pointer
    ctx.emitter.instruction("ldr x12, [x11]");                                  // copy the replacement runtime tag
    ctx.emitter.instruction("str x12, [x10]");                                  // overwrite the target cell tag
    ctx.emitter.instruction("ldr x12, [x11, #8]");                              // copy the replacement low payload word
    ctx.emitter.instruction("str x12, [x10, #8]");                              // overwrite the target cell low payload word
    ctx.emitter.instruction("ldr x12, [x11, #16]");                             // copy the replacement high payload word
    ctx.emitter.instruction("str x12, [x10, #16]");                             // overwrite the target cell high payload word
    ctx.emitter.instruction("mov x0, x11");                                     // pass the now-empty replacement cell storage to heap_free
    abi::emit_call_label(ctx.emitter, "__rt_heap_free");
    ctx.emitter.label(&done);
    ctx.emitter.instruction("add sp, sp, #32");                                 // release replacement temporaries
    Ok(())
}

/// Replaces the payload inside an existing boxed Mixed cell on x86_64.
pub(super) fn lower_mixed_cell_runtime_assign_x86_64(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
) -> Result<()> {
    let drop_new = ctx.next_label("mixed_cell_assign_drop_new");
    let release_string = ctx.next_label("mixed_cell_assign_release_string");
    let copy_new = ctx.next_label("mixed_cell_assign_copy_new");
    let done = ctx.next_label("mixed_cell_assign_done");

    ctx.emitter.instruction("sub rsp, 32");                                     // reserve aligned temporary slots for target and replacement Mixed cells
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                    // preserve the boxed replacement while loading the target cell
    ctx.load_value_to_reg(target, "rax")?;
    ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                        // preserve the target Mixed cell across payload-release helpers
    ctx.emitter.instruction("test rax, rax");                                   // check whether the nested lookup produced a writable cell
    ctx.emitter.instruction(&format!("jz {}", drop_new));                       // drop the replacement when the target cell is missing
    ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                         // inspect the old payload tag before overwriting the cell
    ctx.emitter.instruction("cmp r9, 1");                                       // strings own a persisted heap payload that needs safe free
    ctx.emitter.instruction(&format!("je {}", release_string));                 // release string payloads through the string-safe free path
    ctx.emitter.instruction("cmp r9, 4");                                       // tags below array/hash/object/mixed are scalar payloads
    ctx.emitter.instruction(&format!("jl {}", copy_new));                       // scalar payloads can be overwritten directly
    ctx.emitter.instruction("cmp r9, 7");                                       // tags above the refcounted payload range are not released here
    ctx.emitter.instruction(&format!("jg {}", copy_new));                       // unknown/null payload tags can be overwritten directly
    ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");                    // pass the old refcounted child payload to the generic release helper
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    ctx.emitter.instruction(&format!("jmp {}", copy_new));                      // continue with replacement after releasing the old child
    ctx.emitter.label(&release_string);
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                        // reload the target cell before reading its string payload
    ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");                    // pass the old string payload pointer to the safe free helper
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    ctx.emitter.instruction(&format!("jmp {}", copy_new));                      // continue with replacement after freeing the old string
    ctx.emitter.label(&drop_new);
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                    // reload the unused replacement Mixed cell
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip payload copy because there is no target cell
    ctx.emitter.label(&copy_new);
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                        // reload the destination Mixed cell pointer
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 8]");                    // reload the replacement Mixed cell pointer
    ctx.emitter.instruction("mov r9, QWORD PTR [r11]");                         // copy the replacement runtime tag
    ctx.emitter.instruction("mov QWORD PTR [r10], r9");                         // overwrite the target cell tag
    ctx.emitter.instruction("mov r9, QWORD PTR [r11 + 8]");                     // copy the replacement low payload word
    ctx.emitter.instruction("mov QWORD PTR [r10 + 8], r9");                     // overwrite the target cell low payload word
    ctx.emitter.instruction("mov r9, QWORD PTR [r11 + 16]");                    // copy the replacement high payload word
    ctx.emitter.instruction("mov QWORD PTR [r10 + 16], r9");                    // overwrite the target cell high payload word
    ctx.emitter.instruction("mov rax, r11");                                    // pass the now-empty replacement cell storage to heap_free
    abi::emit_call_label(ctx.emitter, "__rt_heap_free");
    ctx.emitter.label(&done);
    ctx.emitter.instruction("add rsp, 32");                                     // release replacement temporaries
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
        PhpType::Int => "__rt_mixed_cast_int",
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
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the returned boxed Mixed pointer as the SysV first argument
    }
    abi::emit_call_label(ctx.emitter, label);
    Ok(())
}
