//! Purpose:
//! Lowers array_push for indexed, associative and boxed PHP array receivers.
//!
//! Called from:
//! - The typed RuntimeFnId::ArrayPush backend dispatch.
//!
//! Key details:
//! - Values are already evaluated before mutation, and every consuming append publishes its result.
//! - A zero-value call reads the count without consuming a receiver owner.

use super::*;

/// Lowers `array_push()` by appending every value and publishing the mutated array.
///
/// The operand list is `[array, value…]` with any number of trailing values, matching PHP's
/// `array_push(array &$array, mixed ...$values)`. `array_push($a)` with no values is legal PHP
/// too and just reads the current length back.
///
/// Values are appended one at a time in source order. Each append can reach `__rt_array_grow`
/// and relocate the array, so the per-value helper republishes the receiver between steps rather
/// than once at the end.
pub(crate) fn lower_array_push(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() {
        return Err(CodegenIrError::invalid_module(
            "array_push expected at least 1 arg, got 0".to_string(),
        ));
    }
    let array = expect_operand(inst, 0)?;
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::AssocArray { .. }
    ) {
        return lower_array_push_into_hash(ctx, inst, array);
    }
    let boxed_receiver = matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    );
    if boxed_receiver {
        ReceiverPlace::resolve(ctx, array)?.reload_local_value(ctx, array)?;
        require_non_null_boxed_receiver(ctx, array)?;
    }
    for index in 1..inst.operands.len() {
        let value = expect_operand(inst, index)?;
        if boxed_receiver {
            require_available_boxed_hash_key(ctx, array)?;
            if index == 1 {
                super::boxed_mutation::prepare_boxed_array_receiver(ctx, array, "array_push")?;
            }
            super::super::super::arrays::lower_mixed_array_append_value(ctx, array, value)?;
        } else {
            super::super::super::arrays::lower_array_push_value(ctx, inst, array, value)?;
        }
    }
    load_array_push_length_to_result(ctx, array)?;
    store_if_result(ctx, inst)
}

/// Checks key exhaustion before transferring a receiver or boxing the next pushed value.
fn require_available_boxed_hash_key(ctx: &mut FunctionContext<'_>, array: ValueId) -> Result<()> {
    let done = ctx.next_label("array_push_boxed_key_checked");
    ctx.load_value_to_result(array)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #5");                              // only associative payloads can exhaust automatic integer keys
            ctx.emitter.instruction(&format!("b.ne {done}"));                   // indexed arrays use their logical length
            ctx.emitter.instruction("mov x0, x1");                              // pass the hash payload to the shared append-key guard
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 5");                              // only associative payloads can exhaust automatic integer keys
            ctx.emitter.instruction(&format!("jne {done}"));                    // indexed arrays use their logical length
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_next_index");
    ctx.emitter.label(&done);
    Ok(())
}

/// Diagnoses a boxed null receiver before mutation or the zero-value count read.
fn require_non_null_boxed_receiver(ctx: &mut FunctionContext<'_>, array: ValueId) -> Result<()> {
    ctx.load_value_to_result(array)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let non_null = ctx.next_label("array_push_boxed_non_null");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #8");                              // compare the runtime cell tag with PHP null
            ctx.emitter.instruction(&format!("b.ne {non_null}"));               // non-null cells continue to array validation
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 8");                              // compare the runtime cell tag with PHP null
            ctx.emitter.instruction(&format!("jne {non_null}"));                // non-null cells continue to array validation
        }
    }
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx, "array_push(): Argument #1 ($array) must be of type array, null given",
    );
    ctx.emitter.label(&non_null);
    Ok(())
}

/// Lowers `array_push()` onto an ASSOCIATIVE receiver, appending at PHP's next automatic
/// integer key.
///
/// PHP draws no distinction between an indexed and an associative array here: `array_push()` is
/// `$hash[] = $value` repeated, and a hash appends at `max(int keys) + 1`, or `0` when it has
/// none. Each insert can split or grow the table, so its consuming owner is prepared and the
/// replacement published before the next value is inserted.
fn lower_array_push_into_hash(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    receiver.require_writable("array_push")?;
    receiver.reload_local_value(ctx, array)?;
    super::boxed_mutation::require_non_null_hash_receiver(ctx, array, "array_push")?;
    let storage_value_ty = match ctx.value_php_type(array)?.codegen_repr() {
        PhpType::AssocArray { value, .. } => value.codegen_repr(),
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_push for PHP type {:?}",
                other
            )))
        }
    };
    for index in 1..inst.operands.len() {
        let value = expect_operand(inst, index)?;
        require_hash_push_value_fits_storage(ctx, value, &storage_value_ty)?;
        crate::codegen::lower_inst::hashes::append_one_value_to_hash(
            ctx, array, value, inst, &receiver, index > 1,
        )?;
        // An insert can split the table for copy-on-write or grow it, so the pointer the
        // receiver must publish is the helper's RESULT, not the one this side started with.
        // Recording it per value is what makes the next insert address the new table. The
        // insert already released or dropped the block it replaced, so the receiver KEEPS the
        // new pointer rather than retiring the old one a second time -- the same publication
        // `$hash[] = $v` uses (#1341: a request superglobal read with `LoadGlobal` lost the
        // table a copy still shared).
        ctx.store_result_value(array)?;
        receiver.store_back_container_writeback(ctx, array)?;
    }
    ctx.writeback_global_array_source(array)?;
    // Read the count back from the table rather than from the last insert's return register:
    // the write-back above can clobber it, and `array_push($hash)` with no values never calls
    // a helper at all. The logical entry count is the table's first payload word.
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(array, result_reg)?;
    abi::emit_load_from_address(ctx.emitter, result_reg, result_reg, 0);
    store_if_result(ctx, inst)
}

/// Refuses a pushed value the receiver's entry storage cannot hold.
///
/// `$hash[] = $value` gets its receiver widened by the checker before it is lowered, so a
/// `string` appended to an `array<string, int>` arrives with `Mixed` entry storage waiting for
/// it. `array_push()` cannot: its receiver is a by-reference builtin argument, which the checker
/// pins as a reference alias root and never retypes, so the declared entry type is still `int`.
/// Without this check, a string in an integer slot would be read back as a pointer-sized integer.
///
/// Refusing is the honest answer until the receiver can be widened. It is not a regression:
/// before this change every associative receiver was a compile error, so the shapes this still
/// rejects are exactly the ones that never compiled.
fn require_hash_push_value_fits_storage(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    storage_value_ty: &PhpType,
) -> Result<()> {
    if matches!(storage_value_ty, PhpType::Mixed | PhpType::Iterable) {
        return Ok(());
    }
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    if value_ty == *storage_value_ty {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_push() of {:?} into an array whose entries are {:?}: the receiver keeps its \
         declared entry type across a by-reference builtin argument, so the value has nowhere \
         to go. Assign through `$array[] = ...` instead, which widens the receiver",
        value_ty, storage_value_ty
    )))
}

/// Materializes the receiver's post-append element count into the int result register.
///
/// PHP's `array_push()` returns the new number of elements. Reading it back from the array
/// rather than from the last append's return register covers the value-less form, which never
/// calls a helper at all, and survives the write-backs each append performs.
///
/// A boxed `Mixed` receiver holds the container behind a cell, so the count comes from the
/// generic length helper; a typed indexed array keeps its logical length in the first payload
/// word and is read directly.
fn load_array_push_length_to_result(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
) -> Result<()> {
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        // -- a boxed receiver keeps its container behind a cell, so ask the generic counter --
        // `__rt_mixed_count` is on the single-argument INT-RESULT ABI (`x0` / `rax` in and
        // out), not the C argument ABI. Handing it `rdi` left `rax` holding whatever the last
        // append had put there, so `array_push()` on a boxed receiver returned 0 instead of
        // the new element count (issue #1191).
        ctx.load_value_to_result(array)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_count");
        return Ok(());
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- a typed indexed array keeps its logical length in the first payload word --
            ctx.load_value_to_reg(array, "x0")?;
            ctx.emitter.instruction("ldr x0, [x0]");                            // read the post-append element count as the int result
        }
        Arch::X86_64 => {
            // -- a typed indexed array keeps its logical length in the first payload word --
            ctx.load_value_to_reg(array, "rax")?;
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // read the post-append element count as the int result
        }
    }
    Ok(())
}
