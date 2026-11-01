//! Purpose:
//! Lowers `array_pop()` and `array_shift()` for boxed gradual array receivers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays::misc_dispatch`.
//!
//! Key details:
//! - Dispatches indexed/hash runtime kinds, preserves copy-on-write aliases, and writes the
//!   rebound Mixed cell back to caller-visible storage on every supported target.

use super::*;

/// Lowers `shuffle($array)` when flow narrowing proved a boxed gradual value is an array.
///
/// The runtime value may use indexed or associative storage. Both cases are copied into a fresh
/// indexed array before shuffling, which both enforces copy-on-write and implements PHP's key
/// reindexing rule. The fresh container is then transferred into a replacement Mixed cell and
/// published back to the caller-visible local variable.
pub(super) fn lower_shuffle_dynamic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "shuffle", 1)?;
    let source_local = source_load_local_slot(ctx, array)?.ok_or_else(|| {
        CodegenIrError::unsupported(
            "shuffle for a gradual by-reference receiver that is not a local variable slot"
                .to_string(),
        )
    })?;

    ctx.load_value_to_result(array)?;                                          // load the borrowed boxed Mixed cell
    let old_cell_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);                        // [sp+0]=old cell, [sp+8]=fresh array, [sp+16]=new cell
    abi::emit_store_to_sp(ctx.emitter, old_cell_reg, 0);                       // preserve the local's old cell until replacement
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");                    // tag in result register, payload low in the first argument register

    let indexed_label = ctx.next_label("shuffle_dyn_indexed");
    let hash_label = ctx.next_label("shuffle_dyn_hash");
    let wrong_tag_label = ctx.next_label("shuffle_dyn_wrong_tag");
    let shuffle_label = ctx.next_label("shuffle_dyn_apply");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // tag 4 = indexed array
            ctx.emitter.instruction(&format!("b.eq {}", indexed_label));
            ctx.emitter.instruction("cmp x0, #5");                              // tag 5 = associative array
            ctx.emitter.instruction(&format!("b.eq {}", hash_label));
            ctx.emitter.instruction(&format!("b {}", wrong_tag_label));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // tag 4 = indexed array
            ctx.emitter.instruction(&format!("je {}", indexed_label));
            ctx.emitter.instruction("cmp rax, 5");                              // tag 5 = associative array
            ctx.emitter.instruction(&format!("je {}", hash_label));
            ctx.emitter.instruction(&format!("jmp {}", wrong_tag_label));
        }
    }
    super::union_type_guard::emit_mixed_wrong_tag_type_error_dispatch(
        ctx,
        &wrong_tag_label,
        &|given| {
            format!(
                "shuffle(): Argument #1 ($array) must be of type array, {} given",
                given
            )
        },
    );

    ctx.emitter.label(&indexed_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),                 // pass the unboxed indexed array to the clone helper
        Arch::X86_64 => {}                                                       // the unboxed payload is already in rdi
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_clone_shallow");            // clone before the mutating shuffle
    abi::emit_jump(ctx.emitter, &shuffle_label);

    ctx.emitter.label(&hash_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),                 // load the unboxed hash for value extraction
        Arch::X86_64 => ctx.emitter.instruction("mov rax, rdi"),                // value extraction consumes the hash from the result register
    }
    super::values::emit_loaded_assoc_array_values(ctx, &PhpType::Mixed)?;       // copy values and reindex keys

    ctx.emitter.label(&shuffle_label);
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), 8);    // preserve the fresh indexed array across shuffle and boxing
    // `__rt_shuffle` draws through the late-bound Mt19937 slot, which only a call site publishes;
    // without this the helper tail-called a zero slot (SIGSEGV at address 0).
    crate::codegen::random::publish_elephc_random_function_pointers(ctx.emitter);
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, array_arg_reg, 8);
    abi::emit_call_label(ctx.emitter, "__rt_shuffle");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 8);         // Mixed payload low = fresh indexed array
            ctx.emitter.instruction("mov x2, xzr");                             // indexed arrays do not use the payload high word
            ctx.emitter.instruction("mov x0, #4");                              // Mixed tag 4 = indexed array
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 8);        // Mixed payload low = fresh indexed array
            ctx.emitter.instruction("xor rsi, rsi");                            // indexed arrays do not use the payload high word
            ctx.emitter.instruction("mov rax, 4");                              // Mixed tag 4 = indexed array
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");                // retain the fresh container into a replacement cell
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);   // preserve the replacement cell while balancing owners
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 8);
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");                     // transfer the fresh container's original owner to the Mixed cell
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");                   // release the old raw-local Mixed-cell owner

    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    ctx.store_result_value(array)?;
    ctx.store_value_to_local(source_local, array)?;
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}

/// Lowers `array_pop($array)` when `$array` is a checker-accepted Mixed / array-union receiver.
///
/// The by-ref slot holds a boxed Mixed cell. This unboxes it, dispatches on the runtime array kind
/// (tag 4 = indexed, tag 5 = associative hash), removes the LAST element IN PLACE via the
/// kind-specific container helper (`__rt_indexed_pop` / `__rt_hash_pop`), reboxes the mutated
/// container into a FRESH Mixed cell written back into the by-ref slot, and returns the removed
/// element (boxed Mixed). Any non-array runtime tag throws a catchable `\TypeError` matching PHP.
///
/// Ownership mirrors `array_unshift`'s dynamic path: an unconditional `__rt_incref` on the unboxed
/// container forces `__rt_*_ensure_unique` to copy-on-write split (so cell-shared aliases like
/// `$b = $a;` stay intact); the synthetic reference is released with `__rt_decref_any` once the
/// mutated container is safely rewrapped, and the original cell reference the by-ref slot held is
/// released with `__rt_decref_mixed`.
pub(super) fn lower_array_pop_dynamic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    require_array_pop_result_type(&inst.result_php_type.codegen_repr())?;
    let source_local = source_load_local_slot(ctx, array)?;
    let ref_cell_slot = by_ref_receiver_slot(ctx, array)?;

    ctx.load_value_to_result(array)?;                                          // load $array's current boxed Mixed cell pointer
    let old_cell_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_reserve_temporary_stack(ctx.emitter, 48);                        // [sp+0]=container, [sp+16]=old cell, [sp+32]=new cell, [sp+40]=removed value
    abi::emit_store_to_sp(ctx.emitter, old_cell_reg, 16);                      // preserve the old Mixed cell pointer across everything below
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");                     // tag in result reg, payload lo/hi in the unbox pair

    let indexed_label = ctx.next_label("array_pop_dyn_indexed");
    let hash_label = ctx.next_label("array_pop_dyn_hash");
    let wrong_tag_label = ctx.next_label("array_pop_dyn_wrong_tag");
    let finish_label = ctx.next_label("array_pop_dyn_finish");
    // `__rt_mixed_unbox` output: AArch64 tag=x0, payload_lo=x1; x86_64 tag=rax, payload_lo=rdi.
    // The payload-lo register stays live until the kind branch consumes it (cmp/jump only touch flags).
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // tag 4 = indexed array?
            ctx.emitter.instruction(&format!("b.eq {}", indexed_label));
            ctx.emitter.instruction("cmp x0, #5");                              // tag 5 = associative hash?
            ctx.emitter.instruction(&format!("b.eq {}", hash_label));
            ctx.emitter.instruction(&format!("b {}", wrong_tag_label));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // tag 4 = indexed array?
            ctx.emitter.instruction(&format!("je {}", indexed_label));
            ctx.emitter.instruction("cmp rax, 5");                              // tag 5 = associative hash?
            ctx.emitter.instruction(&format!("je {}", hash_label));
            ctx.emitter.instruction(&format!("jmp {}", wrong_tag_label));
        }
    }
    super::union_type_guard::emit_mixed_wrong_tag_type_error_dispatch(
        ctx,
        &wrong_tag_label,
        &|given| {
            format!(
                "array_pop(): Argument #1 ($array) must be of type array, {} given",
                given
            )
        },
    );

    ctx.emitter.label(&indexed_label);
    emit_array_pop_dyn_kind(
        ctx,
        "__rt_indexed_pop",
        4,
        &finish_label,
        source_local.is_some() || ref_cell_slot.is_some(),
    );
    ctx.emitter.label(&hash_label);
    emit_array_pop_dyn_kind(
        ctx,
        "__rt_hash_pop",
        5,
        &finish_label,
        source_local.is_some() || ref_cell_slot.is_some(),
    );

    ctx.emitter.label(&finish_label);
    // Publish the bound Mixed cell (the reused borrowed cell for the in-place case, or the fresh
    // diverged cell for the shared-cell case) into the SSA value's home and the by-ref local slot.
    // A replaced raw local releases its own old-cell share inside the kind branch. Addressable
    // by-ref cells remain borrowed and are never released here.
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 32);
    ctx.store_result_value(array)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, array)?;
    }
    if let Some(slot) = ref_cell_slot {
        publish_cell_through_by_ref_slot(ctx, slot, array)?;
    }
    // Load the removed element (boxed Mixed) as the PHP-visible result.
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 40);
    abi::emit_release_temporary_stack(ctx.emitter, 48);
    store_if_result(ctx, inst)
}

/// Returns the by-reference parameter slot a receiver was read from (`load_ref_cell`), if any.
///
/// The fresh diverged cell the SHARED-cell branch builds has to be published into the caller's
/// storage, and a bare SSA store reaches nothing: `array_shift($p)` on an `array &$p` parameter
/// whose box was shared with a sibling (a `$params` copy, an argument held by a caller) answered
/// the right element and left the caller's array untouched.
fn by_ref_receiver_slot(ctx: &FunctionContext<'_>, array: ValueId) -> Result<Option<LocalSlotId>> {
    Ok(match ReceiverPlace::resolve(ctx, array)? {
        ReceiverPlace::RefCell(slot) => Some(slot),
        _ => None,
    })
}

/// Stores the Mixed cell now in `array`'s SSA home through the by-reference parameter's slot.
///
/// The branch that diverged into a fresh cell already released the old cell's share; the in-place
/// branch republishes the very same pointer, which is harmless.
fn publish_cell_through_by_ref_slot(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    array: ValueId,
) -> Result<()> {
    crate::codegen::lower_inst::store_value_through_ref_cell_slot(
        ctx,
        slot,
        array,
        &PhpType::Mixed,
        crate::codegen::lower_inst::RefCellStorePrevious::Keep,
    )
}

/// Emits one runtime-kind branch of `array_pop`'s dynamic path.
///
/// Two sub-paths, chosen at runtime by the argument CELL's refcount:
/// - Cell refcount == 1 (the common case: the boxed argument cell is not shared with a sibling
///   PHP variable): call `container_helper` WITHOUT forcing a clone (it copy-on-write splits the
///   container only if the underlying array is genuinely shared with a persistent holder), then
///   rebind the SAME borrowed cell's payload to the mutated container in place. This is leak-free:
///   the caller still owns and frees the (now-mutated) cell.
/// - Cell refcount > 1 (the cell is aliased, e.g. `$b = $a;`): force a copy-on-write split and
///   publish a FRESH diverged cell so the sibling keeps the original array. The fresh cell is not
///   reclaimed until scope end (matching the pre-existing copy-on-write leak of the concrete
///   `array_pop` path), but aliasing stays correct.
///
/// `container_helper` returns the unique container plus the removed boxed value; `rebox_tag`
/// is 4 (indexed) or 5 (hash).
fn emit_array_pop_dyn_kind(
    ctx: &mut FunctionContext<'_>,
    container_helper: &str,
    rebox_tag: i64,
    finish_label: &str,
    release_replaced_local_owner: bool,
) {
    let inplace_label = ctx.next_label("array_pop_dyn_inplace");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 16);      // x9 = old (borrowed) Mixed cell
            ctx.emitter.instruction("ldr w10, [x9, #-12]");                     // w10 = cell refcount from the uniform heap header
            ctx.emitter.instruction("cmp w10, #1");                             // is the argument cell unshared?
            ctx.emitter.instruction(&format!("b.eq {}", inplace_label));        // reuse the borrowed cell in place when unshared
            // -- shared cell: diverge into a fresh cell so the sibling variable keeps the original --
            ctx.emitter.instruction("mov x0, x1");                              // move the unboxed container pointer into the incref argument register
            abi::emit_call_label(ctx.emitter, "__rt_incref");               // synthetic extra owner forces the copy-on-write split below
            abi::emit_call_label(ctx.emitter, container_helper);            // x0 = unique container, x1 = removed boxed value
            abi::emit_store_to_sp(ctx.emitter, "x0", 0);                     // save the mutated container pointer
            abi::emit_store_to_sp(ctx.emitter, "x1", 40);                    // save the removed boxed value
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);       // x1 = container pointer (mixed_from_value payload lo)
            ctx.emitter.instruction("mov x2, xzr");                             // container payloads use only the low word
            ctx.emitter.instruction(&format!("mov x0, #{}", rebox_tag));        // runtime array tag for the reboxed cell
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");      // retain the container into a brand-new boxed cell
            abi::emit_store_to_sp(ctx.emitter, "x0", 32);                    // publish the fresh diverged cell
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 0);       // reload the container for the synthetic release
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");           // drop the synthetic owner; the fresh cell now owns the container
            if release_replaced_local_owner {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 16);  // raw local's old boxed-cell owner
                abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");      // release the share replaced by the fresh cell
            }
            ctx.emitter.instruction(&format!("b {}", finish_label));
            // -- unshared cell: mutate the container and rebind the borrowed cell's payload in place --
            ctx.emitter.label(&inplace_label);
            ctx.emitter.instruction("mov x0, x1");                              // move the unboxed container pointer into the helper argument register
            abi::emit_call_label(ctx.emitter, container_helper);            // x0 = unique container, x1 = removed boxed value (no forced clone)
            abi::emit_store_to_sp(ctx.emitter, "x1", 40);                    // save the removed boxed value
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 16);      // x9 = the borrowed cell to rebind
            ctx.emitter.instruction("str x0, [x9, #8]");                        // cell payload low word = mutated container
            ctx.emitter.instruction("str xzr, [x9, #16]");                      // container payloads leave the high word clear
            ctx.emitter.instruction(&format!("mov x11, #{}", rebox_tag));       // normalize the cell tag to the container kind
            ctx.emitter.instruction("str x11, [x9]");                           // stamp the runtime tag in the reused cell
            abi::emit_store_to_sp(ctx.emitter, "x9", 32);                    // publish the reused (in-place mutated) cell
            ctx.emitter.instruction(&format!("b {}", finish_label));
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r9", 16);      // r9 = old (borrowed) Mixed cell
            ctx.emitter.instruction("mov ecx, DWORD PTR [r9 - 12]");            // ecx = cell refcount from the uniform heap header
            ctx.emitter.instruction("cmp ecx, 1");                              // is the argument cell unshared?
            ctx.emitter.instruction(&format!("je {}", inplace_label));          // reuse the borrowed cell in place when unshared
            // -- shared cell: diverge into a fresh cell so the sibling variable keeps the original --
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed container pointer into the incref argument register
            abi::emit_call_label(ctx.emitter, "__rt_incref");               // synthetic extra owner forces the copy-on-write split below
            ctx.emitter.instruction("mov rdi, rax");                            // pass the container pointer to the kind-specific helper
            abi::emit_call_label(ctx.emitter, container_helper);            // rax = unique container, rdx = removed boxed value
            abi::emit_store_to_sp(ctx.emitter, "rax", 0);                    // save the mutated container pointer
            abi::emit_store_to_sp(ctx.emitter, "rdx", 40);                   // save the removed boxed value
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);      // rdi = container pointer (mixed_from_value payload lo)
            ctx.emitter.instruction("xor rsi, rsi");                            // container payloads use only the low word
            ctx.emitter.instruction(&format!("mov rax, {}", rebox_tag));        // runtime array tag for the reboxed cell
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");      // retain the container into a brand-new boxed cell
            abi::emit_store_to_sp(ctx.emitter, "rax", 32);                   // publish the fresh diverged cell
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 0);      // reload the container for the synthetic release
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");           // drop the synthetic owner; the fresh cell now owns the container
            if release_replaced_local_owner {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 16); // raw local's old boxed-cell owner
                abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");      // release the share replaced by the fresh cell
            }
            ctx.emitter.instruction(&format!("jmp {}", finish_label));
            // -- unshared cell: mutate the container and rebind the borrowed cell's payload in place --
            ctx.emitter.label(&inplace_label);
            ctx.emitter.instruction("mov rdi, rdi");                            // the unboxed container pointer is already the helper argument
            abi::emit_call_label(ctx.emitter, container_helper);            // rax = unique container, rdx = removed boxed value (no forced clone)
            abi::emit_store_to_sp(ctx.emitter, "rdx", 40);                   // save the removed boxed value
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r9", 16);      // r9 = the borrowed cell to rebind
            ctx.emitter.instruction("mov QWORD PTR [r9 + 8], rax");             // cell payload low word = mutated container
            ctx.emitter.instruction("mov QWORD PTR [r9 + 16], 0");              // container payloads leave the high word clear
            ctx.emitter.instruction(&format!("mov QWORD PTR [r9], {}", rebox_tag)); // stamp the runtime tag in the reused cell
            abi::emit_store_to_sp(ctx.emitter, "r9", 32);                    // publish the reused (in-place mutated) cell
            ctx.emitter.instruction(&format!("jmp {}", finish_label));
        }
    }
}

/// Lowers `array_shift($array)` when `$array` is a checker-accepted Mixed / array-union receiver.
///
/// Byte-for-byte the same runtime-tag-dispatched, borrow/copy-on-write/writeback recipe as
/// `lower_array_pop_dynamic` (it reuses `emit_array_pop_dyn_kind` and the shared wrong-tag
/// dispatch), but routes each runtime array kind to the FIRST-element removers
/// (`__rt_indexed_shift` / `__rt_hash_shift`) instead of the last-element `array_pop` helpers.
/// Those helpers additionally reindex integer keys (indexed arrays compact toward the front;
/// hashes are rebuilt so surviving integer keys renumber `0,1,2,…` while string keys persist),
/// matching PHP `array_shift()`. The Mixed cell is a BORROW (caller owns/frees it), so the old
/// cell is never released here — identical ownership to the pop dynamic path.
pub(super) fn lower_array_shift_dynamic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    require_array_pop_result_type(&inst.result_php_type.codegen_repr())?;
    let source_local = source_load_local_slot(ctx, array)?;
    let ref_cell_slot = by_ref_receiver_slot(ctx, array)?;

    ctx.load_value_to_result(array)?;                                          // load $array's current boxed Mixed cell pointer
    let old_cell_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_reserve_temporary_stack(ctx.emitter, 48);                        // [sp+0]=container, [sp+16]=old cell, [sp+32]=new cell, [sp+40]=removed value
    abi::emit_store_to_sp(ctx.emitter, old_cell_reg, 16);                      // preserve the old Mixed cell pointer across everything below
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");                     // tag in result reg, payload lo/hi in the unbox pair

    let indexed_label = ctx.next_label("array_shift_dyn_indexed");
    let hash_label = ctx.next_label("array_shift_dyn_hash");
    let wrong_tag_label = ctx.next_label("array_shift_dyn_wrong_tag");
    let finish_label = ctx.next_label("array_shift_dyn_finish");
    // `__rt_mixed_unbox` output: AArch64 tag=x0, payload_lo=x1; x86_64 tag=rax, payload_lo=rdi.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // tag 4 = indexed array?
            ctx.emitter.instruction(&format!("b.eq {}", indexed_label));
            ctx.emitter.instruction("cmp x0, #5");                              // tag 5 = associative hash?
            ctx.emitter.instruction(&format!("b.eq {}", hash_label));
            ctx.emitter.instruction(&format!("b {}", wrong_tag_label));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // tag 4 = indexed array?
            ctx.emitter.instruction(&format!("je {}", indexed_label));
            ctx.emitter.instruction("cmp rax, 5");                              // tag 5 = associative hash?
            ctx.emitter.instruction(&format!("je {}", hash_label));
            ctx.emitter.instruction(&format!("jmp {}", wrong_tag_label));
        }
    }
    super::union_type_guard::emit_mixed_wrong_tag_type_error_dispatch(
        ctx,
        &wrong_tag_label,
        &|given| {
            format!(
                "array_shift(): Argument #1 ($array) must be of type array, {} given",
                given
            )
        },
    );

    ctx.emitter.label(&indexed_label);
    emit_array_pop_dyn_kind(
        ctx,
        "__rt_indexed_shift",
        4,
        &finish_label,
        source_local.is_some() || ref_cell_slot.is_some(),
    );
    ctx.emitter.label(&hash_label);
    emit_array_pop_dyn_kind(
        ctx,
        "__rt_hash_shift",
        5,
        &finish_label,
        source_local.is_some() || ref_cell_slot.is_some(),
    );

    ctx.emitter.label(&finish_label);
    // Publish the bound Mixed cell (reused borrowed cell in place, or fresh diverged cell) into
    // the SSA value's home and the by-ref local slot. The OLD Mixed cell is a BORROW — the caller
    // owns and frees it — except for the raw local's own share, released when that local is
    // replaced by a fresh diverged cell.
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 32);
    ctx.store_result_value(array)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, array)?;
    }
    if let Some(slot) = ref_cell_slot {
        publish_cell_through_by_ref_slot(ctx, slot, array)?;
    }
    // Load the removed element (boxed Mixed) as the PHP-visible result.
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 40);
    abi::emit_release_temporary_stack(ctx.emitter, 48);
    store_if_result(ctx, inst)
}

/// Lowers `array_shift()` directly on associative hash storage and writes the rebuilt hash back.
pub(super) fn lower_assoc_array_shift(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    lower_assoc_array_remove(ctx, inst, array, "__rt_hash_shift")
}

/// Applies one associative end-removal helper and publishes both its container and value results.
fn lower_assoc_array_remove(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    runtime_helper: &str,
) -> Result<()> {
    require_array_pop_result_type(&inst.result_php_type.codegen_repr())?;
    let source_local = source_load_local_slot(ctx, array)?;
    if let Some(slot) = source_local {
        ctx.release_mutated_source_local_owner(slot, array)?;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.load_value_to_reg(array, "x0")?,
        Arch::X86_64 => ctx.load_value_to_reg(array, "rdi")?,
    };
    abi::emit_call_label(ctx.emitter, runtime_helper);
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_push_reg(ctx.emitter, "x1"),
        Arch::X86_64 => abi::emit_push_reg(ctx.emitter, "rdx"),
    }
    ctx.store_result_value(array)?;
    if let Some(slot) = source_local {
        ctx.store_mutated_container_to_local(slot, array)?;
    }
    ctx.writeback_global_array_source(array)?;
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    store_if_result(ctx, inst)
}
