//! Purpose:
//! Lowers the supported two-array multisort through typed indexed and boxed storage.
//!
//! Called from:
//! - The array builtin facade for RuntimeFnId::ArrayMultisort.
//!
//! Key details:
//! - Separate actual lvalues before mutation, and prepare a shared reference only once.
//! - Boxed cells and their normalized payloads each preserve copy-on-write aliases.
//! - Runtime validation rejects unsupported layouts before interpreting their slots.

use super::*;

/// Lowers the supported two-array form of `array_multisort()`.
///
/// Concrete integer or empty arrays use the direct tandem sorter. Arrays of Mixed slots and
/// declared PHP array parameters use the boxed tandem sorter, which compares runtime scalar values
/// by PHP rules. Every path separates and publishes both receivers before moving any slot owner.
pub(crate) fn lower_array_multisort(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_multisort", 2)?;
    let arr1 = expect_operand(inst, 0)?;
    let arr2 = expect_operand(inst, 1)?;
    let ty1 = ctx.value_php_type(arr1)?;
    let ty2 = ctx.value_php_type(arr2)?;
    let repr1 = ty1.codegen_repr();
    let repr2 = ty2.codegen_repr();

    if repr1 == PhpType::Mixed && repr2 == PhpType::Mixed {
        return lower_boxed_array_multisort(ctx, inst, arr1, arr2);
    }
    if repr1 == PhpType::Mixed || repr2 == PhpType::Mixed {
        return Err(CodegenIrError::unsupported(
            "array_multisort with one boxed and one concrete array receiver",
        ));
    }

    let elem1 = eight_byte_indexed_array_element_type(ty1, "array_multisort")?;
    let elem2 = eight_byte_indexed_array_element_type(ty2, "array_multisort")?;
    let boxed_slots = elem1.codegen_repr() == PhpType::Mixed
        && elem2.codegen_repr() == PhpType::Mixed;
    let direct1 = matches!(elem1.codegen_repr(), PhpType::Int | PhpType::Void);
    let direct2 = matches!(elem2.codegen_repr(), PhpType::Int | PhpType::Void);
    if !boxed_slots && (!direct1 || !direct2) {
        return Err(CodegenIrError::unsupported(format!(
            "array_multisort for concrete indexed-array element types {:?} and {:?}; only integer or empty arrays, or boxed Mixed scalar arrays, are supported",
            elem1, elem2
        )));
    }

    prepare_multisort_receivers(ctx, arr1, arr2, false)?;

    if boxed_slots {
        emit_boxed_multisort_call(ctx, arr1, arr2)?;
    } else {
        emit_array_multisort_call(ctx, arr1, arr2, "__rt_array_multisort")?;
    }
    finish_array_multisort(ctx, inst)
}

/// Validates, separates and normalizes two declared PHP array receivers before boxed sorting.
fn lower_boxed_array_multisort(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    arr1: ValueId,
    arr2: ValueId,
) -> Result<()> {
    require_boxed_indexed_multisort_array(ctx, arr1)?;
    require_boxed_indexed_multisort_array(ctx, arr2)?;
    prepare_multisort_receivers(ctx, arr1, arr2, true)?;
    emit_boxed_cell_multisort_call(ctx, arr1, arr2)?;
    finish_array_multisort(ctx, inst)
}

/// Separates distinct value places independently, but prepares an aliased reference only once.
/// Compare actual storage addresses, not payload pointers: separate value aliases need two COWs.
fn prepare_multisort_receivers(
    ctx: &mut FunctionContext<'_>,
    arr1: ValueId,
    arr2: ValueId,
    boxed: bool,
) -> Result<()> {
    let receiver1 = ReceiverPlace::resolve(ctx, arr1)?;
    let receiver2 = ReceiverPlace::resolve(ctx, arr2)?;
    let slot1 = receiver1.slot().ok_or_else(|| {
        CodegenIrError::unsupported("array_multisort receiver without lowered local storage")
    })?;
    let slot2 = receiver2.slot().ok_or_else(|| {
        CodegenIrError::unsupported("array_multisort receiver without lowered local storage")
    })?;
    let detached_second = detached_multisort_receiver_owner(ctx, slot2, arr2)?;
    detached_multisort_receiver_owner(ctx, slot1, arr1)?;
    let result = abi::int_result_reg(ctx.emitter);
    let first_address = abi::int_arg_reg_name(ctx.emitter.target, 1);
    ctx.materialize_local_storage_address(slot1, result)?;
    abi::emit_push_reg(ctx.emitter, result);
    ctx.materialize_local_storage_address(slot2, result)?;
    abi::emit_pop_reg(ctx.emitter, first_address);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, x1");                              // compare the actual lvalue addresses before COW can relocate storage
            ctx.emitter.instruction("cset x0, eq");                             // one caller reference requires only one separation and publication
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, rsi");                            // compare the actual lvalue addresses before COW can relocate storage
            ctx.emitter.instruction("sete al");                                 // remember whether both arguments name the same caller reference
            ctx.emitter.instruction("movzx eax, al");                           // widen the alias predicate before preserving it across helpers
        }
    }
    abi::emit_push_reg(ctx.emitter, result);
    if boxed {
        prepare_boxed_multisort_receiver(ctx, arr1)?;
    } else {
        ctx.release_mutated_source_local_owner(slot1, arr1)?;
        ensure_unique_sort_source(ctx, arr1)?;
        receiver1.store_back_value(ctx, arr1)?;
    }
    abi::emit_pop_reg(ctx.emitter, result);
    let distinct = ctx.next_label("array_multisort_distinct_receivers");
    let ready = ctx.next_label("array_multisort_receivers_ready");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &distinct);
    // A concrete read from a widened Mixed slot owns a detached payload lease.
    // Retire that second old lease before replacing its cached pointer with the first result.
    if detached_second && arr1 != arr2 {
        ctx.load_value_to_result(arr2)?;
        abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    }
    ctx.load_value_to_result(arr1)?;
    ctx.store_result_value(arr2)?;
    abi::emit_jump(ctx.emitter, &ready);
    ctx.emitter.label(&distinct);
    if boxed {
        prepare_boxed_multisort_receiver(ctx, arr2)?;
    } else {
        ctx.release_mutated_source_local_owner(slot2, arr2)?;
        ensure_unique_sort_source(ctx, arr2)?;
        receiver2.store_back_value(ctx, arr2)?;
    }
    ctx.emitter.label(&ready);
    Ok(())
}

/// Identifies an independently unboxed array read that will transfer into a replacement box.
fn detached_multisort_receiver_owner(
    ctx: &FunctionContext<'_>,
    slot: crate::ir::LocalSlotId,
    value: ValueId,
) -> Result<bool> {
    let boxed_slot = ctx.local_php_type(slot)?.codegen_repr() == PhpType::Mixed;
    let boxed_value = ctx.value_php_type(value)?.codegen_repr() == PhpType::Mixed;
    if boxed_slot == boxed_value { return Ok(false); }
    // A raw Mixed load explicitly retains its detached array payload. Reference-cell
    // reads use the caller's representation directly and cannot reinterpret that storage.
    if !boxed_slot || ctx.local_stores_ref_cell_pointer(slot) {
        return Err(CodegenIrError::unsupported(
            "array_multisort receiver with incompatible reference storage representation",
        ));
    }
    Ok(true)
}

/// Rejects boxed hashes and non-array cells before either by-reference receiver is changed.
fn require_boxed_indexed_multisort_array(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
) -> Result<()> {
    let valid = ctx.next_label("array_multisort_boxed_indexed");
    ctx.load_value_to_result(array)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // only packed PHP array payloads fit the supported key model
            ctx.emitter.instruction(&format!("b.eq {valid}"));                  // continue after validating the runtime layout
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // only packed PHP array payloads fit the supported key model
            ctx.emitter.instruction(&format!("je {valid}"));                    // continue after validating the runtime layout
        }
    }
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx,
        "array_multisort() arguments must be indexed arrays",
    );
    ctx.emitter.label(&valid);
    Ok(())
}

/// Publishes a unique boxed cell whose payload is an owned dense array of Mixed slots.
fn prepare_boxed_multisort_receiver(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
) -> Result<()> {
    super::boxed_mutation::prepare_boxed_array_receiver(ctx, array, "array_multisort")?;
    ctx.load_value_to_result(array)?;
    super::values::emit_loaded_boxed_array_values(
        ctx,
        "array_multisort() arguments must be indexed arrays",
    )?;
    // Conversion can retain an already normalized payload shared by another cell.
    // Consume that owner in payload COW before publishing the sortable slots.
    abi::emit_reg_move(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        abi::int_result_reg(ctx.emitter),
    );
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    super::boxed_mutation::install_boxed_array_payload(ctx, array, 4)?;
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Loads two boxed cell payloads, validates their runtime element tags and calls the tandem sorter.
fn emit_boxed_cell_multisort_call(
    ctx: &mut FunctionContext<'_>,
    arr1: ValueId,
    arr2: ValueId,
) -> Result<()> {
    load_boxed_array_payload(ctx, arr1, 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_sort_require_scalars");
    load_boxed_array_payload(ctx, arr2, 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_sort_require_scalars");
    load_boxed_array_payload(ctx, arr1, 0)?;
    load_boxed_array_payload(ctx, arr2, 1)?;
    abi::emit_call_label(ctx.emitter, "__rt_array_multisort_boxed");
    Ok(())
}

/// Validates two already-unboxed arrays of Mixed slots and calls the boxed tandem sorter.
fn emit_boxed_multisort_call(
    ctx: &mut FunctionContext<'_>,
    arr1: ValueId,
    arr2: ValueId,
) -> Result<()> {
    ctx.load_value_to_reg(arr1, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_sort_require_scalars");
    ctx.load_value_to_reg(arr2, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_sort_require_scalars");
    emit_array_multisort_call(ctx, arr1, arr2, "__rt_array_multisort_boxed")
}

/// Loads one normalized packed-array payload from its boxed cell into an argument register.
fn load_boxed_array_payload(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    argument: usize,
) -> Result<()> {
    let register = abi::int_arg_reg_name(ctx.emitter.target, argument);
    ctx.load_value_to_reg(array, register)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr {register}, [{register}, #8]")); // borrow the normalized packed payload from its unique cell
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {register}, QWORD PTR [{register} + 8]")); // borrow the normalized packed payload from its unique cell
        }
    }
    Ok(())
}

/// Materializes both array operands in ABI order and invokes the selected tandem sorter.
fn emit_array_multisort_call(
    ctx: &mut FunctionContext<'_>,
    arr1: ValueId,
    arr2: ValueId,
    helper: &str,
) -> Result<()> {
    ctx.load_value_to_reg(arr1, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    ctx.load_value_to_reg(arr2, abi::int_arg_reg_name(ctx.emitter.target, 1))?;
    abi::emit_call_label(ctx.emitter, helper);
    Ok(())
}

/// Converts a helper status into PHP's true result or the equal-length ValueError.
fn finish_array_multisort(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let success = ctx.next_label("array_multisort_equal_lengths");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {success}"));            // a nonzero status means every array had the same length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // inspect the tandem sorter's equal-length status
            ctx.emitter.instruction(&format!("jnz {success}"));                 // continue only after a successful sort
        }
    }
    crate::codegen::lower_inst::exceptions::emit_value_error(
        ctx,
        "array_multisort(): Array sizes are inconsistent",
    );
    ctx.emitter.label(&success);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}
