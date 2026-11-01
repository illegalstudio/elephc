//! Purpose:
//! Lowers `array_unshift()` when the by-reference receiver is a boxed gradual value.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays::unshift::lower_array_unshift()`.
//!
//! Key details:
//! - Runtime tags select indexed or associative storage without relying on a framework-specific
//!   source pattern.
//! - The working container owns an independent copy-on-write reference and is transferred back
//!   through either the original addressable Mixed cell or its local slot.

use super::*;

const WORKING_CONTAINER_OFFSET: usize = 0;
const RUNTIME_TAG_OFFSET: usize = 8;
const OLD_CELL_OFFSET: usize = 16;
const NEW_CELL_OFFSET: usize = 24;
const PREPENDED_VALUE_OFFSET: usize = 32;
const FRAME_SIZE: usize = 48;

/// Lowers a gradual by-reference receiver by dispatching its runtime array storage kind.
pub(super) fn lower_array_unshift_dynamic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    unshift::require_array_unshift_result_type(&inst.result_php_type.codegen_repr())?;
    let write_through_cell = source_is_by_ref_mixed_cell(ctx, array)?;
    let source_local = super::source_load_local_slot(ctx, array)?;
    // A function `static` is writable storage too — its cell lives in a `.comm` symbol rather
    // than a frame slot, which is the only reason `source_load_local_slot` cannot name it. Before
    // this arm existed `array_unshift($q)` on any `static` array was refused outright.
    let source_static = ctx.static_local_source_slot(array)?;
    if !write_through_cell
        && source_local.is_none()
        && source_static.is_none()
        && inst.operands.len() > 1
    {
        return Err(CodegenIrError::unsupported(
            "array_unshift for a gradual by-reference receiver without writable storage"
                .to_string(),
        ));
    }

    ctx.load_value_to_result(array)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, FRAME_SIZE);
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), OLD_CELL_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");

    let indexed = ctx.next_label("array_unshift_dynamic_indexed");
    let hash = ctx.next_label("array_unshift_dynamic_hash");
    let wrong_tag = ctx.next_label("array_unshift_dynamic_wrong_tag");
    let finish = ctx.next_label("array_unshift_dynamic_finish");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // runtime tag 4 selects indexed-array storage
            ctx.emitter.instruction(&format!("b.eq {}", indexed));
            ctx.emitter.instruction("cmp x0, #5");                              // runtime tag 5 selects associative-array storage
            ctx.emitter.instruction(&format!("b.eq {}", hash));
            ctx.emitter.instruction(&format!("b {}", wrong_tag));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // runtime tag 4 selects indexed-array storage
            ctx.emitter.instruction(&format!("je {}", indexed));
            ctx.emitter.instruction("cmp rax, 5");                              // runtime tag 5 selects associative-array storage
            ctx.emitter.instruction(&format!("je {}", hash));
            ctx.emitter.instruction(&format!("jmp {}", wrong_tag));
        }
    }
    union_type_guard::emit_mixed_wrong_tag_type_error_dispatch(
        ctx,
        &wrong_tag,
        &|given| {
            format!(
                "array_unshift(): Argument #1 ($array) must be of type array, {} given",
                given
            )
        },
    );

    ctx.emitter.label(&indexed);
    prepare_dynamic_indexed_working_container(ctx)?;
    for index in (1..inst.operands.len()).rev() {
        prepend_dynamic_indexed_value(ctx, expect_operand(inst, index)?)?;
    }
    abi::emit_jump(ctx.emitter, &finish);

    ctx.emitter.label(&hash);
    prepare_dynamic_hash_working_container(ctx)?;
    for index in (1..inst.operands.len()).rev() {
        prepend_dynamic_hash_value(ctx, expect_operand(inst, index)?)?;
    }
    abi::emit_jump(ctx.emitter, &finish);

    ctx.emitter.label(&finish);
    if write_through_cell {
        replace_addressable_mixed_payload(ctx, array)?;
    } else {
        replace_gradual_local_cell(ctx, array, source_local)?;
    }
    load_dynamic_unshift_length(ctx)?;
    abi::emit_release_temporary_stack(ctx.emitter, FRAME_SIZE);
    store_if_result(ctx, inst)
}

/// Converts an unboxed indexed payload into an independently owned boxed-Mixed-slot array.
fn prepare_dynamic_indexed_working_container(ctx: &mut FunctionContext<'_>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // retain the unboxed indexed-array payload
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.instruction("ldr x1, [x0, #-8]");                       // recover the runtime indexed-slot value type
            ctx.emitter.instruction("lsr x1, x1, #8");                          // shift the value-type byte into the low bits
            ctx.emitter.instruction("and x1, x1, #0x7f");                       // discard heap-kind and persistent metadata
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // retain the unboxed indexed-array payload
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the retained array to the conversion helper
            ctx.emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");            // recover the runtime indexed-slot value type
            ctx.emitter.instruction("shr rsi, 8");                              // shift the value-type byte into the low bits
            ctx.emitter.instruction("and rsi, 0x7f");                           // discard heap-kind and persistent metadata
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        WORKING_CONTAINER_OFFSET,
    );
    store_runtime_tag(ctx, 4)
}

/// Retains an unboxed associative payload as the first owned working hash.
fn prepare_dynamic_hash_working_container(ctx: &mut FunctionContext<'_>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),                 // retain the unboxed associative-array payload
        Arch::X86_64 => ctx.emitter.instruction("mov rax, rdi"),                // retain the unboxed associative-array payload
    }
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        WORKING_CONTAINER_OFFSET,
    );
    store_runtime_tag(ctx, 5)
}

/// Stores the selected runtime array tag in the dynamic mutation frame.
fn store_runtime_tag(ctx: &mut FunctionContext<'_>, tag: i64) -> Result<()> {
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, scratch, tag);
    abi::emit_store_to_sp(ctx.emitter, scratch, RUNTIME_TAG_OFFSET);
    Ok(())
}

/// Boxes one value, grows the indexed working array if needed, and prepends the owned cell.
fn prepend_dynamic_indexed_value(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    unshift::load_array_unshift_value(ctx, value, &PhpType::Mixed)?;
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        PREPENDED_VALUE_OFFSET,
    );
    let capacity_ok = ctx.next_label("array_unshift_dynamic_capacity_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", WORKING_CONTAINER_OFFSET);
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the current logical length
            ctx.emitter.instruction("ldr x10, [x0, #8]");                      // load the current slot capacity
            ctx.emitter.instruction("cmp x9, x10");                            // determine whether the prepend still fits
            ctx.emitter.instruction(&format!("b.lt {}", capacity_ok));
            abi::emit_call_label(ctx.emitter, "__rt_array_grow");
            abi::emit_store_to_sp(ctx.emitter, "x0", WORKING_CONTAINER_OFFSET);
            ctx.emitter.label(&capacity_ok);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", PREPENDED_VALUE_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", WORKING_CONTAINER_OFFSET);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", WORKING_CONTAINER_OFFSET);
            ctx.emitter.instruction("mov r10, QWORD PTR [rdi]");                // load the current logical length
            ctx.emitter.instruction("mov r11, QWORD PTR [rdi + 8]");            // load the current slot capacity
            ctx.emitter.instruction("cmp r10, r11");                           // determine whether the prepend still fits
            ctx.emitter.instruction(&format!("jb {}", capacity_ok));
            abi::emit_call_label(ctx.emitter, "__rt_array_grow");
            abi::emit_store_to_sp(ctx.emitter, "rax", WORKING_CONTAINER_OFFSET);
            ctx.emitter.label(&capacity_ok);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", PREPENDED_VALUE_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", WORKING_CONTAINER_OFFSET);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_unshift");
    Ok(())
}

/// Boxes one value and rebuilds the associative working array in PHP insertion order.
fn prepend_dynamic_hash_value(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    unshift::load_array_unshift_value(ctx, value, &PhpType::Mixed)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the owned boxed value as the prepend payload
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", WORKING_CONTAINER_OFFSET);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the owned boxed value as the prepend payload
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", WORKING_CONTAINER_OFFSET);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_unshift_mixed");
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        WORKING_CONTAINER_OFFSET,
    );
    Ok(())
}

/// Returns whether the operand is an addressable boxed cell fetched for a by-reference call.
fn source_is_by_ref_mixed_cell(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(false);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    Ok(inst_ref.op == Op::LoadRefCell)
}

/// Transfers the working owner into an addressable Mixed cell without replacing its identity.
fn replace_addressable_mixed_payload(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", OLD_CELL_OFFSET);
            ctx.emitter.instruction("ldr x0, [x9, #8]");                        // release the payload previously owned by the cell
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", OLD_CELL_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", RUNTIME_TAG_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x11", WORKING_CONTAINER_OFFSET);
            ctx.emitter.instruction("str x10, [x9]");                           // publish the selected array runtime tag
            ctx.emitter.instruction("str x11, [x9, #8]");                      // transfer the working container owner into the cell
            ctx.emitter.instruction("str xzr, [x9, #16]");                     // arrays do not use the high payload word
            ctx.emitter.instruction("mov x0, x9");
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", OLD_CELL_OFFSET);
            ctx.emitter.instruction("mov rax, QWORD PTR [r10 + 8]");            // release the payload previously owned by the cell
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", OLD_CELL_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", RUNTIME_TAG_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", WORKING_CONTAINER_OFFSET);
            ctx.emitter.instruction("mov QWORD PTR [r10], r11");                // publish the selected array runtime tag
            ctx.emitter.instruction("mov QWORD PTR [r10 + 8], rax");            // transfer the working container owner into the cell
            ctx.emitter.instruction("mov QWORD PTR [r10 + 16], 0");             // arrays do not use the high payload word
            ctx.emitter.instruction("mov rax, r10");
        }
    }
    ctx.store_result_value(array)
}

/// Reboxes the working container and replaces the old Mixed cell stored in a local slot.
fn replace_gradual_local_cell(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    source_local: Option<LocalSlotId>,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", RUNTIME_TAG_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", WORKING_CONTAINER_OFFSET);
            ctx.emitter.instruction("mov x2, xzr");                             // arrays do not use the high payload word
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", RUNTIME_TAG_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", WORKING_CONTAINER_OFFSET);
            ctx.emitter.instruction("xor esi, esi");                            // arrays do not use the high payload word
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), NEW_CELL_OFFSET);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        WORKING_CONTAINER_OFFSET,
    );
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        OLD_CELL_OFFSET,
    );
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        NEW_CELL_OFFSET,
    );
    ctx.store_result_value(array)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, array)?;
    }
    // The static-local twin of that store. `__rt_decref_mixed` above already released the cell
    // the symbol held, so this publishes the replacement without further refcount traffic.
    ctx.writeback_static_local_array_source(array)?;
    Ok(())
}

/// Loads the logical length of the runtime-selected working container.
fn load_dynamic_unshift_length(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let hash = ctx.next_label("array_unshift_dynamic_count_hash");
    let done = ctx.next_label("array_unshift_dynamic_count_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", RUNTIME_TAG_OFFSET);
            ctx.emitter.instruction("cmp x9, #5");                              // associative storage needs the hash count helper
            ctx.emitter.instruction(&format!("b.eq {}", hash));
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", WORKING_CONTAINER_OFFSET);
            ctx.emitter.instruction("ldr x0, [x0]");                            // indexed length is the first header word
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", RUNTIME_TAG_OFFSET);
            ctx.emitter.instruction("cmp r10, 5");                              // associative storage needs the hash count helper
            ctx.emitter.instruction(&format!("je {}", hash));
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", WORKING_CONTAINER_OFFSET);
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // indexed length is the first header word
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&hash);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", WORKING_CONTAINER_OFFSET);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", WORKING_CONTAINER_OFFSET);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_count");
    ctx.emitter.label(&done);
    Ok(())
}
