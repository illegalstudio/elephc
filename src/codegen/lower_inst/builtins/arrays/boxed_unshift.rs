//! Purpose:
//! Prepends values to boxed PHP arrays while preserving numeric and string key semantics.
//!
//! Called from:
//! - `super::unshift::lower_array_unshift()` for declared PHP array storage.
//!
//! Key details:
//! - The prefix retains its operands before separating the receiver, including self-insertion.
//! - A fresh merged hash retains all old values before the previous payload is released.

use super::*;

/// Builds a retained prefix, merges both layouts, and publishes the new payload into a unique cell.
pub(super) fn lower_boxed_array_unshift(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    receiver.require_writable("array_unshift")?;
    validate_receiver(ctx, array)?;
    emit_prefix(ctx, &inst.operands[1..])?;
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result);
    // An argument expression may have mutated the receiver after the EIR receiver load, so
    // re-materialize the local before the cell is separated.
    receiver.reload_local_value(ctx, array)?;
    // Prefix values can themselves contain the original cell. Separate only
    // after retaining them so prepending the receiver snapshots its old value.
    super::boxed_mutation::prepare_boxed_array_receiver(ctx, array, "array_unshift")?;
    ctx.load_value_to_result(array)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x2, x1");                              // borrow the current receiver payload as the second source
            ctx.emitter.instruction("mov x3, x0");                              // preserve its packed or hash discriminator
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdx, rdi");                            // borrow the receiver payload as the second source
            ctx.emitter.instruction("mov rcx, rax");                            // preserve its actual array layout
        }
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 4);
    abi::emit_call_label(ctx.emitter, "__rt_array_merge_boxed");
    abi::emit_push_reg(ctx.emitter, result);
    super::boxed_mutation::install_boxed_array_payload(ctx, array, 5)?;
    // The merged hash now owns every copied value, so neither release below
    // can run a destructor for a value still present in the receiver.
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 16);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("ldr x0, [x0]"),               // return the new logical entry count
        Arch::X86_64 => ctx.emitter.instruction("mov rax, QWORD PTR [rax]"),    // return the new logical entry count
    }
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    store_if_result(ctx, inst)
}

/// Prepends values to a concrete associative array, renumbering integer keys from zero.
///
/// The boxed path mutates a Mixed cell in place; a concrete hash has no cell, so the merged table
/// is published as the receiver and the replaced table and the prefix are released, mirroring the
/// hash-reindexing sorts. The prefix is built by the shared [`emit_prefix`].
pub(super) fn lower_assoc_array_unshift(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    receiver.require_writable("array_unshift")?;
    emit_prefix(ctx, &inst.operands[1..])?;
    let result = abi::int_result_reg(ctx.emitter);
    let arg0 = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_push_reg(ctx.emitter, result);                                    // park the owned prefix at stack offset 0
    // An argument expression may have mutated the receiver after the EIR receiver load, so
    // re-materialize the local before the split and merge read it.
    receiver.reload_local_value(ctx, array)?;
    receiver.prepare_consuming_storeback(ctx, array)?;
    super::sort_dispatch::ensure_unique_hash_sort_source(ctx, array)?;
    ctx.load_value_to_reg(array, result)?;                                      // borrow the concrete hash being replaced
    abi::emit_push_reg(ctx.emitter, result);                                    // park the old table: old at 0, prefix at 16
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 0);          // pass the old table as the merge source payload
            abi::emit_load_int_immediate(ctx.emitter, "x3", 5);                 // its layout tag is associative hash
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", 0);         // pass the old table as the merge source payload
            abi::emit_load_int_immediate(ctx.emitter, "rcx", 5);                // its layout tag is associative hash
        }
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg0, 16);                 // the packed prefix is the merge destination
    abi::emit_load_int_immediate(ctx.emitter, arg1, 4);                         // the prefix's own layout tag is packed array
    abi::emit_call_label(ctx.emitter, "__rt_array_merge_boxed");
    abi::emit_push_reg(ctx.emitter, result);                                    // park the merged table: 0 merged, 16 old, 32 prefix
    ctx.store_result_value(array)?;
    receiver.store_back_after_consuming_split(ctx, array)?;
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 16);               // release the replaced table
    abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 32);               // release the prefix
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 0);                // return the merged table's logical entry count
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("ldr x0, [x0]"),               // read the new count from the published table
        Arch::X86_64 => ctx.emitter.instruction("mov rax, QWORD PTR [rax]"),    // read the new count from the published table
    }
    abi::emit_release_temporary_stack(ctx.emitter, 48);
    store_if_result(ctx, inst)
}

/// Rejects invalid runtime values before allocating prefix owners or touching receiver storage.
fn validate_receiver(ctx: &mut FunctionContext<'_>, array: ValueId) -> Result<()> {
    ctx.load_value_to_result(array)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let valid = ctx.next_label("array_unshift_boxed_valid");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub x9, x0, #4");                          // accept only packed and associative array tags
            ctx.emitter.instruction("cmp x9, #1");                              // scalar and null cells fail before allocation
            ctx.emitter.instruction(&format!("b.ls {valid}"));                  // both array layouts use the checked merge path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("lea r10, [rax - 4]");                      // accept only packed and associative array tags
            ctx.emitter.instruction("cmp r10, 1");                              // scalar and null cells fail before allocation
            ctx.emitter.instruction(&format!("jbe {valid}"));                   // both array layouts use the checked merge path
        }
    }
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx, "array_unshift(): Argument #1 ($array) must be of type array",
    );
    ctx.emitter.label(&valid);
    Ok(())
}

/// Creates an owned packed prefix from borrowed operands without stealing their EIR ownership.
fn emit_prefix(ctx: &mut FunctionContext<'_>, values: &[ValueId]) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter);
    let arg0 = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_int_immediate(ctx.emitter, arg0, values.len() as i64);
    abi::emit_load_int_immediate(ctx.emitter, arg1, 8);
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    abi::emit_push_reg(ctx.emitter, result);
    for &value in values {
        let ty = ctx.value_php_type(value)?.codegen_repr();
        ctx.load_value_to_result(value)?;
        if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
            abi::emit_incref_if_refcounted(ctx.emitter, &ty);
        } else {
            emit_box_current_value_as_mixed(ctx.emitter, &ty);
        }
        abi::emit_push_reg(ctx.emitter, result);
        abi::emit_reg_move(ctx.emitter, arg1, result);
        abi::emit_load_temporary_stack_slot(ctx.emitter, arg0, 16);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
        abi::emit_store_to_sp(ctx.emitter, result, 16);
        abi::emit_pop_reg(ctx.emitter, result);
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    }
    abi::emit_pop_reg(ctx.emitter, result);
    Ok(())
}
