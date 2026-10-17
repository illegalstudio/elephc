//! Purpose:
//! Slice, splice, chunk, pad, and result normalization.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Calls the appropriate legacy runtime helper after materializing slice arguments.
pub(super) fn lower_array_slice_call(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
    source_elem_ty: &PhpType,
) -> Result<()> {
    lower_slice_like_args(ctx, array, offset, length, "array_slice")?;
    abi::emit_call_label(ctx.emitter, array_slice_runtime_helper(source_elem_ty));
    Ok(())
}

/// Calls the appropriate legacy runtime helper after materializing splice arguments.
pub(super) fn lower_array_splice_call(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
    elem_ty: &PhpType,
) -> Result<()> {
    lower_slice_like_args(ctx, array, offset, length, "array_splice")?;
    abi::emit_call_label(ctx.emitter, array_splice_runtime_helper(elem_ty));
    Ok(())
}

/// Materializes the shared `(array, offset, length, length_present)` argument tuple for
/// `array_slice` and `array_splice` into the runtime argument registers.
///
/// The offset, the length and the length-present flag are resolved to plain integers first —
/// unboxing a `Mixed` cell read from a heterogeneous array via `__rt_mixed_cast_int` — and spilled to
/// the stack, because those unbox calls clobber caller-saved registers. The array pointer (a plain
/// stack load that clobbers nothing) is then placed, and the staged integers are restored into the
/// offset/length/flag argument registers, so the runtime helper sees the array pointer plus three
/// genuine integers rather than a boxed pointer.
pub(super) fn lower_slice_like_args(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
    name: &str,
) -> Result<()> {
    resolve_int_operand_to_result(ctx, offset, &format!("{} offset", name))?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    resolve_slice_length_present_to_result(ctx, length)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    resolve_slice_length_to_result(ctx, length, name)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_pop_reg(ctx.emitter, "x2"); // restore the resolved length into the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "x3"); // restore the length-present flag into the fourth runtime argument
            abi::emit_pop_reg(ctx.emitter, "x1"); // restore the resolved offset into the second runtime argument
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            abi::emit_pop_reg(ctx.emitter, "rdx"); // restore the resolved length into the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "rcx"); // restore the length-present flag into the fourth runtime argument
            abi::emit_pop_reg(ctx.emitter, "rsi"); // restore the resolved offset into the second runtime argument
        }
    }
    Ok(())
}

/// Resolves an optional `array_slice`/`array_splice` length into the integer result register.
///
/// An absent or `Void` length materializes a zero placeholder that the helper ignores because the
/// companion length-present flag is zero; otherwise the length is resolved through the shared integer
/// resolver, unboxing a `Mixed` value to a plain integer.
pub(super) fn resolve_slice_length_to_result(
    ctx: &mut FunctionContext<'_>,
    length: Option<ValueId>,
    name: &str,
) -> Result<()> {
    if slice_length_is_statically_absent(ctx, length)? {
        let reg = abi::int_result_reg(ctx.emitter);
        abi::emit_load_int_immediate(ctx.emitter, reg, 0);
        return Ok(());
    }
    resolve_int_operand_to_result(
        ctx,
        length.expect("length present"),
        &format!("{} length", name),
    )
}

/// Resolves the offset/length arguments for a boxed-Mixed `array_slice`/`array_splice` into the
/// refcounted runtime helper's argument registers, restoring a previously-staged array pointer.
///
/// On entry the converted (now-owned) indexed-array pointer must be the topmost value on the
/// temporary stack. The offset, the length and the length-present flag are resolved to plain integers
/// first — `__rt_mixed_cast_int` unboxes a `Mixed` cell read from a heterogeneous array, and an
/// absent/`Void`/boxed-null length clears the length-present flag — and spilled to the stack, because
/// each unbox call clobbers caller-saved registers. The four staged values are then popped into the
/// array/offset/length/flag argument registers so the helper sees a pointer plus three genuine
/// integers rather than a boxed pointer.
pub(super) fn materialize_mixed_slice_args(
    ctx: &mut FunctionContext<'_>,
    offset: ValueId,
    length: Option<ValueId>,
    name: &str,
) -> Result<()> {
    resolve_int_operand_to_result(ctx, offset, &format!("{} offset", name))?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    resolve_slice_length_present_to_result(ctx, length)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    resolve_slice_length_to_result(ctx, length, name)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x2"); // restore the resolved length into the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "x3"); // restore the length-present flag into the fourth runtime argument
            abi::emit_pop_reg(ctx.emitter, "x1"); // restore the resolved offset into the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "x0"); // restore the converted array pointer into the first runtime argument
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rdx"); // restore the resolved length into the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "rcx"); // restore the length-present flag into the fourth runtime argument
            abi::emit_pop_reg(ctx.emitter, "rsi"); // restore the resolved offset into the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "rdi"); // restore the converted array pointer into the first runtime argument
        }
    }
    Ok(())
}

/// Slices a private normalized payload without rewriting the borrowed ARM64 source box.
///
/// The caller has pushed the resolved `$preserve_keys` word, which stays the topmost temporary
/// stack slot across this whole sequence. An indexed payload is copied into Mixed slots and sliced
/// into a renumbered indexed array, or, when the flag is set, sliced straight into a hash that
/// keeps the source integer keys. A hash payload goes through `__rt_hash_slice`, which keeps
/// string keys and applies the flag to integer ones. Any other payload yields an empty array. The
/// caller boxes whichever storage comes back when the result type is boxed. `keys_may_be_kept` is
/// false only for an omitted or literal `false` flag, which needs no runtime test on the list path.
pub(super) fn lower_mixed_array_slice_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
    result_elem_ty: &PhpType,
    keys_may_be_kept: bool,
) -> Result<()> {
    let hash_label = ctx.next_label("mixed_array_slice_hash");
    let keep_keys_label = ctx.next_label("mixed_array_slice_keep_keys");
    let empty_label = ctx.next_label("mixed_array_slice_empty");
    let done_label = ctx.next_label("mixed_array_slice_done");
    ctx.load_value_to_reg(array, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp x0, #5");                                      // detect a hash payload, whose string keys survive the slice
    ctx.emitter.instruction(&format!("b.eq {}", hash_label));                   // slice hash payloads by insertion position
    ctx.emitter.instruction("cmp x0, #4");                                      // require an indexed-array payload before slicing the Mixed cell
    ctx.emitter.instruction(&format!("b.ne {}", empty_label));                  // return an empty slice for non-array Mixed payloads
    ctx.emitter.instruction(&format!("cbz x1, {}", empty_label));               // return an empty slice for null array payloads
    if keys_may_be_kept {
        abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 0);
        ctx.emitter.instruction(&format!("cbnz x9, {}", keep_keys_label));      // a key-preserving list slice builds a hash instead
    }
    ctx.emitter.instruction("mov x0, x1");                                      // pass the unboxed indexed-array payload to the Mixed conversion helper
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.instruction("ldr x1, [x0, #-8]");                               // load indexed-array metadata before Mixed-slot conversion
    ctx.emitter.instruction("lsr x1, x1, #8");                                  // move the runtime value_type tag into the low bits
    ctx.emitter.instruction("and x1, x1, #0x7f");                               // isolate the indexed-array value_type tag
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    slice_owned_mixed_payload(ctx, offset, length)?;
    normalize_indexed_array_result(ctx, "array_slice", &PhpType::Mixed, result_elem_ty)?;
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the other payload shapes after slicing the indexed payload
    if keys_may_be_kept {
        ctx.emitter.label(&keep_keys_label);
        abi::emit_push_reg(ctx.emitter, "x1");
        slice_borrowed_mixed_list_payload_keeping_keys(ctx, offset, length)?;
        ctx.emitter.instruction(&format!("b {}", done_label));                  // skip the other payload shapes after the key-preserving list slice
    }
    ctx.emitter.label(&hash_label);
    ctx.emitter.instruction(&format!("cbz x1, {}", empty_label));               // return an empty slice for null hash payloads
    abi::emit_push_reg(ctx.emitter, "x1");
    slice_borrowed_mixed_hash_payload(ctx, offset, length)?;
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the empty-array fallback after slicing the hash payload
    ctx.emitter.label(&empty_label);
    allocate_empty_mixed_array_result(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Slices a private normalized payload without rewriting the borrowed x86_64 source box.
///
/// Mirrors `lower_mixed_array_slice_aarch64`, including the staged `$preserve_keys` word on top
/// of the temporary stack: indexed payloads are renumbered or keep their keys in a hash, hash
/// payloads go through `__rt_hash_slice` with the flag, anything else yields an empty array.
pub(super) fn lower_mixed_array_slice_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
    result_elem_ty: &PhpType,
    keys_may_be_kept: bool,
) -> Result<()> {
    let hash_label = ctx.next_label("mixed_array_slice_hash");
    let keep_keys_label = ctx.next_label("mixed_array_slice_keep_keys");
    let empty_label = ctx.next_label("mixed_array_slice_empty");
    let done_label = ctx.next_label("mixed_array_slice_done");
    ctx.load_value_to_reg(array, "rax")?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp rax, 5");                                      // detect a hash payload, whose string keys survive the slice
    ctx.emitter.instruction(&format!("je {}", hash_label));                     // slice hash payloads by insertion position
    ctx.emitter.instruction("cmp rax, 4");                                      // require an indexed-array payload before slicing the Mixed cell
    ctx.emitter.instruction(&format!("jne {}", empty_label));                   // return an empty slice for non-array Mixed payloads
    ctx.emitter.instruction("test rdi, rdi");                                   // verify the unboxed indexed-array payload is present
    ctx.emitter.instruction(&format!("je {}", empty_label));                    // return an empty slice for null array payloads
    if keys_may_be_kept {
        abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 0);
        ctx.emitter.instruction("test r10, r10");                               // is the staged preserve_keys word set?
        ctx.emitter.instruction(&format!("jne {}", keep_keys_label));           // a key-preserving list slice builds a hash instead
    }
    ctx.emitter.instruction("mov rax, rdi");                                    // retain an independent source owner before consuming conversion
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the retained array to the consuming conversion helper
    ctx.emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");                    // load indexed-array metadata before Mixed-slot conversion
    ctx.emitter.instruction("shr rsi, 8");                                      // move the runtime value_type tag into the low bits
    ctx.emitter.instruction("and rsi, 0x7f");                                   // isolate the indexed-array value_type tag
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    slice_owned_mixed_payload(ctx, offset, length)?;
    normalize_indexed_array_result(ctx, "array_slice", &PhpType::Mixed, result_elem_ty)?;
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the other payload shapes after slicing the indexed payload
    if keys_may_be_kept {
        ctx.emitter.label(&keep_keys_label);
        abi::emit_push_reg(ctx.emitter, "rdi");
        slice_borrowed_mixed_list_payload_keeping_keys(ctx, offset, length)?;
        ctx.emitter.instruction(&format!("jmp {}", done_label));                // skip the other payload shapes after the key-preserving list slice
    }
    ctx.emitter.label(&hash_label);
    ctx.emitter.instruction("test rdi, rdi");                                   // verify the unboxed hash payload is present
    ctx.emitter.instruction(&format!("je {}", empty_label));                    // return an empty slice for null hash payloads
    abi::emit_push_reg(ctx.emitter, "rdi");
    slice_borrowed_mixed_hash_payload(ctx, offset, length)?;
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the empty-array fallback after slicing the hash payload
    ctx.emitter.label(&empty_label);
    allocate_empty_mixed_array_result(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Slices the borrowed list payload of a Mixed cell into a hash that keeps the source keys.
///
/// The list pointer must be the topmost value on the temporary stack, above the staged
/// `$preserve_keys` word; the shared materializer pops it into the first argument register after
/// resolving the window operands. `__rt_array_slice_to_hash` only reads the source and persists
/// or retains what it copies, so the list stays owned by its Mixed cell.
fn slice_borrowed_mixed_list_payload_keeping_keys(
    ctx: &mut FunctionContext<'_>,
    offset: ValueId,
    length: Option<ValueId>,
) -> Result<()> {
    materialize_mixed_slice_args(ctx, offset, length, "array_slice")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_slice_to_hash");
    Ok(())
}

/// Slices the borrowed hash payload of a Mixed cell with `__rt_hash_slice`.
///
/// The hash pointer must be the topmost value on the temporary stack, above the staged
/// `$preserve_keys` word; the shared materializer pops it into the first argument register after
/// resolving the window operands, which leaves the flag on top for the fifth argument register.
/// The hash stays owned by its Mixed cell, because the helper only reads the source and retains
/// what it copies.
fn slice_borrowed_mixed_hash_payload(
    ctx: &mut FunctionContext<'_>,
    offset: ValueId,
    length: Option<ValueId>,
) -> Result<()> {
    materialize_mixed_slice_args(ctx, offset, length, "array_slice")?;
    let preserve_keys_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x4",
        Arch::X86_64 => "r8",
    };
    abi::emit_load_temporary_stack_slot(ctx.emitter, preserve_keys_reg, 0);
    abi::emit_call_label(ctx.emitter, "__rt_hash_slice");
    Ok(())
}

/// Retires the private source after the result has retained every selected child owner.
fn slice_owned_mixed_payload(
    ctx: &mut FunctionContext<'_>,
    offset: ValueId,
    length: Option<ValueId>,
) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_push_reg(ctx.emitter, result);
    materialize_mixed_slice_args(ctx, offset, length, "array_slice")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_slice_refcounted");
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 16);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    abi::emit_pop_reg(ctx.emitter, result);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Materializes and mutates a boxed-Mixed indexed array for `array_splice()` on AArch64.
pub(super) fn lower_mixed_array_splice_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
    replacement: &SpliceReplacement,
) -> Result<()> {
    let drop_label = ctx.next_label("mixed_array_splice_empty");
    let done_label = ctx.next_label("mixed_array_splice_done");
    ctx.load_value_to_reg(array, "x0")?;
    abi::emit_push_reg(ctx.emitter, "x0");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp x0, #4");                                      // require an indexed-array payload before splicing the Mixed cell
    ctx.emitter.instruction(&format!("b.ne {}", drop_label));                   // return an empty removed-elements array for non-array Mixed payloads
    ctx.emitter.instruction(&format!("cbz x1, {}", drop_label));                // return an empty removed-elements array for null array payloads
    ctx.emitter.instruction("mov x0, x1");                                      // pass the unboxed indexed-array payload to the Mixed conversion helper
    ctx.emitter.instruction("ldr x1, [x0, #-8]");                               // load indexed-array metadata before Mixed-slot conversion
    ctx.emitter.instruction("lsr x1, x1, #8");                                  // move the runtime value_type tag into the low bits
    ctx.emitter.instruction("and x1, x1, #0x7f");                               // isolate the indexed-array value_type tag
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    abi::emit_pop_reg(ctx.emitter, "x10");
    ctx.emitter.instruction("str x0, [x10, #8]");                               // publish the converted unique array back into the Mixed cell
    abi::emit_push_reg(ctx.emitter, "x0");
    materialize_mixed_slice_args(ctx, offset, length, "array_splice")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_splice_refcounted");
    emit_mixed_splice_replacement_insert(ctx, array, replacement)?;
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the empty-array fallback after splicing the boxed payload
    ctx.emitter.label(&drop_label);
    abi::emit_pop_reg(ctx.emitter, "x9");
    allocate_empty_mixed_array_result(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Materializes and mutates a boxed-Mixed indexed array for `array_splice()` on x86_64.
pub(super) fn lower_mixed_array_splice_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
    replacement: &SpliceReplacement,
) -> Result<()> {
    let drop_label = ctx.next_label("mixed_array_splice_empty");
    let done_label = ctx.next_label("mixed_array_splice_done");
    ctx.load_value_to_reg(array, "rax")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp rax, 4");                                      // require an indexed-array payload before splicing the Mixed cell
    ctx.emitter.instruction(&format!("jne {}", drop_label));                    // return an empty removed-elements array for non-array Mixed payloads
    ctx.emitter.instruction("test rdi, rdi");                                   // verify the unboxed indexed-array payload is present
    ctx.emitter.instruction(&format!("je {}", drop_label));                     // return an empty removed-elements array for null array payloads
    ctx.emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");                    // load indexed-array metadata before Mixed-slot conversion
    ctx.emitter.instruction("shr rsi, 8");                                      // move the runtime value_type tag into the low bits
    ctx.emitter.instruction("and rsi, 0x7f");                                   // isolate the indexed-array value_type tag
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    abi::emit_pop_reg(ctx.emitter, "r10");
    ctx.emitter.instruction("mov QWORD PTR [r10 + 8], rax");                    // publish the converted unique array back into the Mixed cell
    abi::emit_push_reg(ctx.emitter, "rax");
    materialize_mixed_slice_args(ctx, offset, length, "array_splice")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_splice_refcounted");
    emit_mixed_splice_replacement_insert(ctx, array, replacement)?;
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the empty-array fallback after splicing the boxed payload
    ctx.emitter.label(&drop_label);
    abi::emit_pop_reg(ctx.emitter, "r11");
    allocate_empty_mixed_array_result(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Adapts the removed-elements array returned by `array_splice` to the EIR result type.
pub(super) fn normalize_array_splice_result(
    ctx: &mut FunctionContext<'_>,
    elem_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    let removed_ty = PhpType::Array(Box::new(elem_ty.codegen_repr()));
    match result_ty {
        PhpType::Mixed => {
            emit_box_current_owned_value_as_mixed(ctx.emitter, &removed_ty);
            Ok(())
        }
        PhpType::Array(result_elem) if result_elem.codegen_repr() == elem_ty.codegen_repr() => {
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_splice result PHP type {:?}",
            other
        ))),
    }
}

/// Allocates an empty boxed-Mixed indexed array for dynamic splice fallback paths.
pub(super) fn allocate_empty_mixed_array_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", 0);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 8);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", 0);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Mixed,
    );
}

/// Calls the appropriate legacy runtime helper after materializing chunk arguments.
pub(super) fn lower_array_chunk_call(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    length: ValueId,
    runtime_label: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(length, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(length, "rsi")?;
        }
    }
    emit_array_chunk_length_guard(ctx);
    abi::emit_call_label(ctx.emitter, runtime_label);
    Ok(())
}

/// Calls `__rt_hash_chunk` for an associative `array_chunk()` receiver.
///
/// Same receiver/length prologue as the indexed helpers, plus the literal `preserve_keys` flag in
/// the third argument register. That register is loaded AFTER the length guard, which can branch
/// into the ValueError path and does not promise to preserve it.
pub(super) fn lower_hash_chunk_call(
    ctx: &mut FunctionContext<'_>,
    hash: ValueId,
    length: ValueId,
    preserve_keys: bool,
) -> Result<()> {
    let flag_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(hash, "x0")?;
            ctx.load_value_to_reg(length, "x1")?;
            "x2"
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(hash, "rdi")?;
            ctx.load_value_to_reg(length, "rsi")?;
            "rdx"
        }
    };
    emit_array_chunk_length_guard(ctx);
    abi::emit_load_int_immediate(ctx.emitter, flag_reg, i64::from(preserve_keys));
    abi::emit_call_label(ctx.emitter, "__rt_hash_chunk");
    Ok(())
}

/// Calls the appropriate legacy runtime helper after materializing pad arguments.
pub(super) fn lower_array_pad_call(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    target_size: ValueId,
    pad_value: ValueId,
    source_elem_ty: &PhpType,
) -> Result<()> {
    // A string pad value travels as its {pointer, length} pair in the third and fourth
    // argument registers (`__rt_array_pad_str`); every other layout fits the third alone.
    let pad_is_str = source_elem_ty == &PhpType::Str;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(target_size, "x1")?;
            if pad_is_str {
                ctx.load_string_value_to_regs(pad_value, "x2", "x3")?;
            } else {
                ctx.load_value_to_reg(pad_value, "x2")?;
            }
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(target_size, "rsi")?;
            if pad_is_str {
                ctx.load_string_value_to_regs(pad_value, "rdx", "rcx")?;
            } else {
                ctx.load_value_to_reg(pad_value, "rdx")?;
            }
        }
    }
    emit_array_pad_length_guard(ctx);
    abi::emit_call_label(ctx.emitter, array_pad_runtime_helper(source_elem_ty));
    Ok(())
}


/// The largest `array_pad()` `$length` magnitude reference PHP will build an array for.
///
/// php-src rejects anything past `HT_MAX_SIZE / 2` before it looks at the input array, so
/// the bound is a plain constant: `array_pad($a, 1073741824, …)` is accepted (and then
/// fails on memory), `array_pad($a, 1073741825, …)` is a `ValueError` for every `$a`.
const ARRAY_PAD_MAX_LENGTH: i64 = 1_073_741_824;

/// php-src's verbatim `ValueError` wording for an oversized `array_pad()` `$length`.
const ARRAY_PAD_LENGTH_TOO_LARGE_MESSAGE: &str =
    "array_pad(): Argument #2 ($length) must not exceed the maximum allowed array size";

/// Rejects the `array_pad()` `$length` magnitudes reference PHP refuses to build an array for.
///
/// The pad helpers derive the destination capacity and the destination header length from
/// `abs($length)`, and that absolute value was never bounded: a huge magnitude asked the
/// allocator for a payload the process cannot own, and `PHP_INT_MIN` has no representable
/// magnitude at all, so the negation wrapped straight back to a negative "length". Bounding
/// the signed argument here — before it reaches either helper — keeps both out of reach and
/// raises PHP's catchable `ValueError` in their place. `$length` sits in the second ABI
/// argument register for every pad helper on every supported target.
pub(super) fn emit_array_pad_length_guard(ctx: &mut FunctionContext<'_>) {
    let length_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rsi",
    };
    crate::codegen::lower_inst::exceptions::emit_value_error_unless(
        ctx,
        crate::codegen::lower_inst::exceptions::ValueGuard::SignedMagnitudeAtMost(
            length_reg,
            ARRAY_PAD_MAX_LENGTH,
        ),
        ARRAY_PAD_LENGTH_TOO_LARGE_MESSAGE,
    );
}

/// Returns the helper that matches the chunk source element ownership representation.
pub(super) fn array_chunk_runtime_helper(source_elem_ty: &PhpType) -> &'static str {
    if source_elem_ty == &PhpType::Str {
        "__rt_array_chunk_str"
    } else if source_elem_ty.is_refcounted() {
        "__rt_array_chunk_refcounted"
    } else {
        "__rt_array_chunk"
    }
}

/// Returns the helper that matches the pad source element ownership representation.
pub(super) fn array_pad_runtime_helper(source_elem_ty: &PhpType) -> &'static str {
    if source_elem_ty == &PhpType::Str {
        "__rt_array_pad_str"
    } else if source_elem_ty.is_refcounted() {
        "__rt_array_pad_refcounted"
    } else {
        "__rt_array_pad"
    }
}

/// Returns the helper that matches the source element ownership representation.
pub(super) fn array_slice_runtime_helper(source_elem_ty: &PhpType) -> &'static str {
    if source_elem_ty.codegen_repr() == PhpType::Str {
        // An indexed string array stores 16-byte `{pointer, length}` slots. The shared helpers
        // copy 8 bytes per element, so neither can carry a string pair — `array_slice()` on one
        // was refused at compile time rather than run (issue #675). Same split, and the same
        // ownership rule, as `array_splice_runtime_helper` below.
        "__rt_array_slice_str"
    } else if source_elem_ty.is_refcounted() {
        "__rt_array_slice_refcounted"
    } else {
        "__rt_array_slice"
    }
}

/// Returns the helper that matches the spliced element ownership representation.
pub(super) fn array_splice_runtime_helper(elem_ty: &PhpType) -> &'static str {
    if elem_ty.codegen_repr() == PhpType::Str {
        // Indexed string arrays store 16-byte `{pointer, length}` slots; the shared helpers copy
        // and compact 8 bytes at a time, which returned raw pointers as PHP integers and left
        // the receiver half-shifted.
        "__rt_array_splice_str"
    } else if elem_ty.is_refcounted() {
        "__rt_array_splice_refcounted"
    } else {
        "__rt_array_splice"
    }
}

/// Stamps the result array and widens typed slots when the EIR result expects Mixed.
pub(super) fn normalize_indexed_array_result(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    source_elem_ty: &PhpType,
    result_elem_ty: &PhpType,
) -> Result<()> {
    if result_elem_ty == &PhpType::Mixed && source_elem_ty != &PhpType::Mixed {
        let source_tag = runtime_value_tag(name, source_elem_ty)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("mov x1, #{}", source_tag));   // pass the source slot value_type tag to widen the indexed-array result to Mixed
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the produced indexed-array pointer to the Mixed-widening helper
                ctx.emitter.instruction(&format!("mov rsi, {}", source_tag));   // pass the source slot value_type tag to widen the indexed-array result to Mixed
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
        return Ok(());
    }
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        result_elem_ty,
    );
    Ok(())
}
