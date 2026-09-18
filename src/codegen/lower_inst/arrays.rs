//! Purpose:
//! Lowers basic indexed-array allocation, length reads, and append operations
//! for the Phase 04 EIR backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Runtime append helpers may grow arrays and return a new heap pointer, so
//!   the backend writes that pointer back to the source SSA slot and local slot.

use crate::codegen::{
    abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
    emit_box_runtime_payload_as_mixed, emit_release_pushed_refcounted_temp_after_array_push,
    runtime_value_tag,
};
use crate::codegen::callable_invoker_args::INVOKER_ARG_REF_CELL_TAG;
use crate::codegen::platform::Arch;
use crate::codegen::sentinels::TAGGED_SCALAR_ARRAY_VALUE_TYPE;
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::receiver_place::ReceiverPlace;
use super::{expect_operand, store_if_result};
use crate::codegen::{CodegenIrError, Result};

#[cfg(test)]
mod element_address_tests;
#[cfg(test)]
mod promotion_owner_tests;

/// Lowers indexed-array allocation through the shared runtime constructor.
pub(super) fn lower_array_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let capacity = expect_capacity(inst)?.max(4);
    let result_ty = inst.result_php_type.codegen_repr();
    let elem_ty = indexed_array_element_type(&result_ty, inst)?;
    let elem_size = array_element_size(&result_ty)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", elem_size);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", elem_size);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    let result_reg = abi::int_result_reg(ctx.emitter);
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        result_reg,
        &elem_ty,
    );
    if matches!(elem_ty, PhpType::TaggedScalar) {
        emit_tagged_scalar_array_value_type_stamp(ctx, result_reg);
    }
    store_if_result(ctx, inst)
}

/// Lowers an indexed-array length read by loading the first header word.
pub(super) fn lower_array_len(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    require_indexed_array(ctx.load_value_to_result(array)?, inst)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let null_label = ctx.next_label("array_len_null");
    let done_label = ctx.next_label("array_len_done");
    let scratch_reg = abi::secondary_scratch_reg(ctx.emitter);
    crate::codegen::sentinels::emit_branch_if_null_container(
        ctx.emitter,
        result_reg,
        scratch_reg,
        &null_label,
    );
    abi::emit_load_from_address(ctx.emitter, result_reg, result_reg, 0);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    super::exceptions::emit_error(
        ctx,
        "Only arrays and Traversables can be unpacked, null given",
    );
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers typed indexed-array widening to boxed Mixed slots.
///
/// Null and in-band null-container-sentinel inputs (missed array reads that a
/// branch merge forwards, issue #549) pass through unconverted: the runtime
/// slot tag is recovered from the header at this call site, so the sentinel
/// must be filtered before the header dereference — a helper-side guard per
/// the issue #533 convention would fire too late.
pub(super) fn lower_array_to_mixed(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expects exactly one operand",
            inst.op.name()
        )));
    }
    let array = expect_operand(inst, 0)?;
    indexed_array_element_type(&ctx.value_php_type(array)?, inst)?;
    require_array_to_mixed_result(&inst.result_php_type.codegen_repr(), inst)?;
    let done = ctx.next_label("array_to_mixed_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.emitter.instruction(&format!("cbz x0, {}", done));              // null containers have no header or slots to box
            abi::emit_load_int_immediate(ctx.emitter, "x9", crate::codegen::NULL_SENTINEL);
            ctx.emitter.instruction("cmp x0, x9");                              // does the array carry the in-band null-container sentinel?
            ctx.emitter.instruction(&format!("b.eq {}", done));                 // missed-read sentinels pass through unconverted
            ctx.emitter.instruction("ldr x1, [x0, #-8]");                       // load the indexed-array packed header to recover the runtime slot tag
            ctx.emitter.instruction("lsr x1, x1, #8");                          // move the runtime value_type byte into the low bits
            ctx.emitter.instruction("and x1, x1, #0x7f");                       // isolate the source element value_type for Mixed boxing
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.emitter.instruction("mov rax, rdi");                            // default to passing null/sentinel containers through unconverted
            ctx.emitter.instruction("test rdi, rdi");                           // null containers have no header or slots to box
            ctx.emitter.instruction(&format!("je {}", done));                   // keep the null container as the passthrough result
            abi::emit_load_int_immediate(ctx.emitter, "r10", crate::codegen::NULL_SENTINEL);
            ctx.emitter.instruction("cmp rdi, r10");                            // does the array carry the in-band null-container sentinel?
            ctx.emitter.instruction(&format!("je {}", done));                   // missed-read sentinels pass through unconverted
            ctx.emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");            // load the indexed-array packed header to recover the runtime slot tag
            ctx.emitter.instruction("shr rsi, 8");                              // move the runtime value_type byte into the low bits
            ctx.emitter.instruction("and rsi, 0x7f");                           // isolate the source element value_type for Mixed boxing
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
        }
    }
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers indexed-array promotion to associative hash storage.
pub(super) fn lower_array_to_hash(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expects exactly one operand",
            inst.op.name()
        )));
    }
    let array = expect_operand(inst, 0)?;
    require_indexed_array(ctx.value_php_type(array)?.codegen_repr(), inst)?;
    let result_value_ty = require_array_to_hash_result(&inst.result_php_type.codegen_repr(), inst)?;
    if let Some(slot) = source_load_local_slot(ctx, array)? {
        // A late whole-frame widening can make this concrete LoadLocal unbox an owned child from
        // Mixed storage even though lowering emitted no ReleaseLocalSlot before the conversion.
        // Retire that outer box now, while the retained child keeps the source alive. Raw array
        // slots take the no-op branch, so ArrayToHash remains the sole consumer of their owner.
        ctx.release_mutated_source_local_owner(slot, array)?;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let already_hash = ctx.next_label("array_to_hash_already_hash");
            let convert = ctx.next_label("array_to_hash_convert");
            let done = ctx.next_label("array_to_hash_done");
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp x0, #3");                              // check whether the source is already associative hash storage
            ctx.emitter.instruction(&format!("b.eq {}", already_hash));         // reuse already-promoted hashes without reinterpreting them as indexed arrays
            ctx.emitter.instruction("cmp x0, #2");                              // check whether the source is still indexed-array storage
            ctx.emitter.instruction(&format!("b.eq {}", convert));              // convert indexed arrays to hash storage
            ctx.emitter.label(&already_hash);
            abi::emit_pop_reg(ctx.emitter, "x0");
            if result_value_ty == PhpType::Mixed {
                abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            }
            ctx.emitter.instruction(&format!("b {}", done));                    // finish after reusing an existing hash payload
            ctx.emitter.label(&convert);
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_load_int_immediate(ctx.emitter, "x0", 16);
            abi::emit_load_int_immediate(ctx.emitter, "x1", runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.instruction("mov x1, x0");                              // pass the empty temporary hash as the right union operand
            abi::emit_pop_reg(ctx.emitter, "x0");
            // Keep both the source indexed array and the empty temporary hash on
            // the stack across the union so the conversion can release them after
            // the copy: array_hash_union borrows both operands and returns a fresh
            // result hash, so the source array (an owning temporary or a moved-out
            // local reference) and the temporary hash both leak unless freed here.
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_push_reg(ctx.emitter, "x1");
            abi::emit_call_label(ctx.emitter, "__rt_array_hash_union");
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the empty temporary hash from the stack
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            ctx.emitter.instruction("ldr x0, [sp, #32]");                       // reload the temporary source indexed array from the stack
            abi::emit_call_label(ctx.emitter, "__rt_decref_array");
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x1");
            if result_value_ty == PhpType::Mixed {
                abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            }
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            let already_hash = ctx.next_label("array_to_hash_already_hash");
            let convert = ctx.next_label("array_to_hash_convert");
            let done = ctx.next_label("array_to_hash_done");
            ctx.load_value_to_reg(array, "rax")?;
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp rax, 3");                              // check whether the source is already associative hash storage
            ctx.emitter.instruction(&format!("je {}", already_hash));           // reuse already-promoted hashes without reinterpreting them as indexed arrays
            ctx.emitter.instruction("cmp rax, 2");                              // check whether the source is still indexed-array storage
            ctx.emitter.instruction(&format!("je {}", convert));                // convert indexed arrays to hash storage
            ctx.emitter.label(&already_hash);
            abi::emit_pop_reg(ctx.emitter, "rax");
            if result_value_ty == PhpType::Mixed {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the existing hash to the Mixed-entry conversion helper
                abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            }
            ctx.emitter.instruction(&format!("jmp {}", done));                  // finish after reusing an existing hash payload
            ctx.emitter.label(&convert);
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_push_reg(ctx.emitter, "rdi");
            abi::emit_load_int_immediate(ctx.emitter, "rdi", 16);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.instruction("mov rsi, rax");                            // pass the empty temporary hash as the right union operand
            abi::emit_pop_reg(ctx.emitter, "rdi");
            // Keep both the source indexed array and the empty temporary hash on
            // the stack across the union so the conversion can release them after
            // the copy: array_hash_union borrows both operands and returns a fresh
            // result hash, so the source array (an owning temporary or a moved-out
            // local reference) and the temporary hash both leak unless freed here.
            abi::emit_push_reg(ctx.emitter, "rdi");
            abi::emit_push_reg(ctx.emitter, "rsi");
            abi::emit_call_label(ctx.emitter, "__rt_array_hash_union");
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload the empty temporary hash from the stack
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 32]");           // reload the temporary source indexed array from the stack
            abi::emit_call_label(ctx.emitter, "__rt_decref_array");
            abi::emit_pop_reg(ctx.emitter, "rax");
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rsi");
            if result_value_ty == PhpType::Mixed {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the promoted hash to the Mixed-entry conversion helper
                abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            }
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Selects what an indexed-array element read does with the payload it loads.
#[derive(Clone, Copy)]
enum ArrayGetMode {
    /// Plain rvalue read: the result carries a caller reference, so refcounted payloads are
    /// increfed on the way out.
    Retaining,
    /// Copy-on-write fetch for a by-reference `foreach` source: the element is separated from
    /// any co-owner through `helper`, the separated container is published back into the parent
    /// slot, and the result is handed back BORROWED — the parent slot owns it, the reader does
    /// not.
    ForWrite {
        /// Runtime COW helper matching the element's container kind.
        helper: &'static str,
    },
    /// Nested-write fetch for a boxed Mixed slot: retain the owning cell so the later write can
    /// publish a replacement back into the parent container.
    MixedForWrite,
}

/// Lowers an indexed-array element read with PHP null-sentinel fallback on misses.
pub(super) fn lower_array_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    warn_on_missing: bool,
) -> Result<()> {
    lower_array_get_in_mode(ctx, inst, warn_on_missing, ArrayGetMode::Retaining)
}

/// Lowers `ArrayGetForWrite`: the same element read as `array_get`, missing-key warning and
/// null-container sentinel fallback included, but the element is copy-on-write separated and
/// returned without a caller reference.
///
/// A by-reference `foreach` mutates the container it iterates in place, and `iter_start` gets
/// there through `__rt_array_ensure_unique`, which copies whenever the source is shared. The
/// plain `array_get` read hands the loop the parent's container PLUS a reference of its own, so
/// the element sat at refcount 2, the loop copied it, wrote into the copy and dropped it: every
/// write was lost (issue #580).
///
/// Simply skipping the retain is not enough, and is in fact worse: `__rt_array_ensure_unique`
/// CONSUMES one reference from the source when it splits, so on a genuinely shared element that
/// decrement would come out of the parent's own reference and leave the parent slot dangling.
/// This op therefore does the splits itself — the receiver first, then the element — publishing
/// each back into the slot it came from, exactly as PHP separates `$a` and then `$a[0]` before
/// iterating it by reference. What reaches the loop is unique, so `iter_start`'s own
/// `ensure_unique` is a no-op and the writes land in the container the parent holds.
///
/// Boxed `Mixed` elements use the nested-write ownership contract from issue #555 instead: the
/// stored cell is retained and returned so later write-back can publish any replacement into the
/// parent. Statically typed array/hash elements use the copy-on-write path from issue #580.
pub(super) fn lower_array_get_for_write(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let elem_ty = indexed_array_element_type(&ctx.value_php_type(array)?, inst)?;
    require_array_get_result(&elem_ty, inst)?;
    if matches!(inst.result_php_type.codegen_repr(), PhpType::Mixed) {
        return lower_array_get_in_mode(ctx, inst, true, ArrayGetMode::MixedForWrite);
    }
    let helper = array_get_for_write_cow_helper(&elem_ty).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "array_get_for_write element PHP type {:?}",
            elem_ty
        ))
    })?;
    separate_get_for_write_receiver(ctx, array, "__rt_array_ensure_unique")?;
    lower_array_get_in_mode(ctx, inst, true, ArrayGetMode::ForWrite { helper })
}

/// Separates the receiver itself before its element slot is rewritten.
///
/// PHP separates `$a` on the way to separating `$a[0]`, and so does elephc's element WRITE path:
/// `$a[0] = ...` splits the receiver inside `__rt_array_set_*` and stores the unique pointer back
/// to the source local. Fetch-for-write publishes a new element pointer into the receiver's
/// payload, so it owes the same guarantee — otherwise `$b = $a; foreach ($a[0] as &$v)` would
/// mutate storage `$b` still observes.
///
/// Only receivers that came from a local (or a global mirrored through one) are separated here:
/// the split returns a NEW container, and without a slot to publish it to the caller would keep
/// reading the old one. A chained receiver needs no split at this point anyway — the lowering
/// walks `$a[0][0]` down to its base local and fetches every level for write on the way back up,
/// so an inner level always hands the next one a container that is already unique.
///
/// `helper` selects the copy-on-write split matching the receiver's own container kind:
/// `__rt_array_ensure_unique` for an indexed receiver, `__rt_hash_ensure_unique` for a hash one.
pub(super) fn separate_get_for_write_receiver(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    helper: &str,
) -> Result<()> {
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    let Some(slot) = receiver.slot() else {
        return Ok(());
    };
    receiver.reload_local_value(ctx, array)?;
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(array, arg_reg)?;
    abi::emit_call_label(ctx.emitter, helper);
    ctx.store_result_value(array)?;
    ctx.store_container_writeback_to_local(slot, array)?;
    ctx.writeback_global_array_source(array)
}

/// Returns the copy-on-write helper that separates an element of this type, if there is one.
///
/// Only the two container shapes need — and survive — the split: an indexed array and a hash,
/// each with its own clone helper. Everything else is rejected. `Mixed` in particular is not a
/// single container to separate: its slot can hold an invoker ref-cell marker whose read
/// materializes a freshly boxed value instead of the slot's own storage.
///
/// Shared with the hash receiver path: what selects the helper is the ELEMENT's container kind,
/// which is independent of whether the receiver holding it is indexed or associative.
pub(super) fn array_get_for_write_cow_helper(elem_ty: &PhpType) -> Option<&'static str> {
    match elem_ty.codegen_repr() {
        PhpType::Array(_) => Some("__rt_array_ensure_unique"),
        PhpType::AssocArray { .. } => Some("__rt_hash_ensure_unique"),
        _ => None,
    }
}

/// Lowers an indexed-array element read under the requested ownership mode.
fn lower_array_get_in_mode(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    warn_on_missing: bool,
    mode: ArrayGetMode,
) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let index = expect_operand(inst, 1)?;
    let elem_ty = indexed_array_element_type(&ctx.value_php_type(array)?, inst)?;
    require_array_get_result(&elem_ty, inst)?;
    let result_ty = inst.result_php_type.codegen_repr();
    if matches!(result_ty, PhpType::Mixed) {
        return lower_array_get_runtime_mixed(
            ctx,
            inst,
            array,
            index,
            warn_on_missing,
            matches!(mode, ArrayGetMode::MixedForWrite),
        );
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_get_aarch64(
            ctx,
            inst,
            array,
            index,
            &elem_ty,
            &result_ty,
            warn_on_missing,
            mode,
        ),
        Arch::X86_64 => lower_array_get_x86_64(
            ctx,
            inst,
            array,
            index,
            &elem_ty,
            &result_ty,
            warn_on_missing,
            mode,
        ),
    }
}

/// Lowers `LoadArrayElemRefCell` to an addressable element cell.
///
/// Concrete receivers select inline indexed storage or a runtime-promoted hash. A boxed Mixed
/// receiver is normalized to a Mixed-entry hash first, updating the box in place, so recursive
/// nested-source preparation always returns a managed tag-11 cell rather than retaining an
/// interior indexed slot as though it were heap-owned.
pub(super) fn lower_load_array_elem_ref_cell(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let create_missing = inst.op == Op::LoadArrayElemRefCell;
    let array = expect_operand(inst, 0)?;
    let index = expect_operand(inst, 1)?;
    let array_ty = ctx.value_php_type(array)?;
    if create_missing
        && matches!(array_ty.codegen_repr(), PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed)
    {
        let receiver = ReceiverPlace::resolve(ctx, array)?;
        receiver.require_writable("runtime-shaped array element reference")?;
        receiver.reload_local_value(ctx, array)?;
        receiver.prepare_consuming_storeback(ctx, array)?;
        ctx.load_value_to_result(array)?;
        super::iterators::convert_loaded_indexed_source_to_hash(ctx);
        ctx.store_result_value(array)?;
        receiver.store_back_container_writeback(ctx, array)?;
        return lower_hash_elem_ref_cell(ctx, inst, array, index, true);
    }
    if matches!(array_ty.codegen_repr(), PhpType::AssocArray { .. }) {
        return lower_hash_elem_ref_cell(ctx, inst, array, index, create_missing);
    }
    if matches!(array_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        ReceiverPlace::resolve(ctx, array)?.reload_local_value(ctx, array)?;
        return match ctx.emitter.target.arch {
            Arch::AArch64 => lower_load_mixed_array_elem_ref_cell_aarch64(
                ctx, inst, array, index, create_missing,
            ),
            Arch::X86_64 => lower_load_mixed_array_elem_ref_cell_x86_64(
                ctx, inst, array, index, create_missing,
            ),
        };
    }
    let elem_ty = indexed_array_element_type(&array_ty, inst)?;
    let elem_size = ref_cell_element_size(&elem_ty.codegen_repr());
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_load_array_elem_ref_cell_aarch64(ctx, inst, array, index, elem_size),
        Arch::X86_64 => lower_load_array_elem_ref_cell_x86_64(ctx, inst, array, index, elem_size),
    }
}

/// Resolves a writable hash entry, inserting boxed null when the key is absent.
///
/// The receiver is separated and republished before exposing its entry. Insertion may grow the
/// table, so that replacement is published a second time before the final lookup promotes the
/// entry to a persistent tag-11 reference cell.
fn lower_hash_elem_ref_cell(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    hash: ValueId,
    key: ValueId,
    create_missing: bool,
) -> Result<()> {
    let receiver = ReceiverPlace::resolve(ctx, hash)?;
    receiver.require_writable("hash element reference")?;
    receiver.reload_local_value(ctx, hash)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            lower_hash_elem_ref_cell_aarch64(ctx, hash, key, &receiver, create_missing)?
        }
        Arch::X86_64 => {
            lower_hash_elem_ref_cell_x86_64(ctx, hash, key, &receiver, create_missing)?
        }
    }
    store_ref_cell_pointer_result(ctx, inst)
}

/// Resolves a writable hash entry reference on AArch64.
fn lower_hash_elem_ref_cell_aarch64(
    ctx: &mut FunctionContext<'_>,
    hash: ValueId,
    key: ValueId,
    receiver: &ReceiverPlace,
    create_missing: bool,
) -> Result<()> {
    let found = ctx.next_label("hash_elem_ref_found");
    let done = ctx.next_label("hash_elem_ref_done");
    receiver.prepare_consuming_storeback(ctx, hash)?;
    ctx.load_value_to_reg(hash, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
    ctx.store_result_value(hash)?;
    receiver.store_back_container_writeback(ctx, hash)?;
    super::hashes::materialize_hash_key_aarch64(ctx, key)?;
    ctx.load_value_to_reg(hash, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    ctx.emitter.instruction(&format!("cbnz x4, {found}"));                      // skip insertion when the requested key already exists

    if create_missing {
        receiver.reprepare_consuming_storeback(ctx, hash)?;
        crate::codegen::literal_defaults::emit_boxed_null_literal_to_result(ctx);
        abi::emit_push_reg(ctx.emitter, "x0");
        super::hashes::materialize_hash_key_aarch64(ctx, key)?;
        abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
        ctx.load_value_to_reg(hash, "x0")?;
        abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        abi::emit_pop_reg(ctx.emitter, "x3");
        ctx.emitter.instruction("mov x4, xzr");                                 // boxed null occupies only the low hash payload word
        abi::emit_load_int_immediate(ctx.emitter, "x5", runtime_value_tag(&PhpType::Mixed) as i64);
        abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        ctx.store_result_value(hash)?;
        receiver.store_back_container_writeback(ctx, hash)?;
        super::hashes::materialize_hash_key_aarch64(ctx, key)?;
        ctx.load_value_to_reg(hash, "x0")?;
        abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    } else {
        super::hashes::emit_undefined_hash_key_warning_aarch64(ctx, key)?;
        abi::emit_load_int_immediate(ctx.emitter, "x0", 0);
        ctx.emitter.instruction(&format!("b {done}"));                          // a foreach source miss does not create an entry
    }

    ctx.emitter.label(&found);
    ctx.emitter.instruction("add x0, x4, #24");                                 // address the entry's low value word for reference promotion
    abi::emit_call_label(ctx.emitter, "__rt_hash_entry_make_reference");
    ctx.emitter.label(&done);
    Ok(())
}

/// Resolves a writable hash entry reference on x86_64.
fn lower_hash_elem_ref_cell_x86_64(
    ctx: &mut FunctionContext<'_>,
    hash: ValueId,
    key: ValueId,
    receiver: &ReceiverPlace,
    create_missing: bool,
) -> Result<()> {
    let found = ctx.next_label("hash_elem_ref_found");
    let done = ctx.next_label("hash_elem_ref_done");
    receiver.prepare_consuming_storeback(ctx, hash)?;
    ctx.load_value_to_reg(hash, "rdi")?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
    ctx.store_result_value(hash)?;
    receiver.store_back_container_writeback(ctx, hash)?;
    super::hashes::materialize_hash_key_x86_64(ctx, key)?;
    ctx.load_value_to_reg(hash, "rdi")?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    ctx.emitter.instruction("test r8, r8");                                     // check whether the requested key already exists
    ctx.emitter.instruction(&format!("jnz {found}"));                           // preserve an existing entry without overwriting its value

    if create_missing {
        receiver.reprepare_consuming_storeback(ctx, hash)?;
        crate::codegen::literal_defaults::emit_boxed_null_literal_to_result(ctx);
        abi::emit_push_reg(ctx.emitter, "rax");
        super::hashes::materialize_hash_key_x86_64(ctx, key)?;
        abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
        ctx.load_value_to_reg(hash, "rdi")?;
        abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
        abi::emit_pop_reg(ctx.emitter, "rcx");
        ctx.emitter.instruction("xor r8d, r8d");                                // boxed null occupies only the low hash payload word
        abi::emit_load_int_immediate(ctx.emitter, "r9", runtime_value_tag(&PhpType::Mixed) as i64);
        abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        ctx.store_result_value(hash)?;
        receiver.store_back_container_writeback(ctx, hash)?;
        super::hashes::materialize_hash_key_x86_64(ctx, key)?;
        ctx.load_value_to_reg(hash, "rdi")?;
        abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    } else {
        super::hashes::emit_undefined_hash_key_warning_x86_64(ctx, key)?;
        abi::emit_load_int_immediate(ctx.emitter, "rax", 0);
        ctx.emitter.instruction(&format!("jmp {done}"));                        // a foreach source miss does not create an entry
    }

    ctx.emitter.label(&found);
    ctx.emitter.instruction("lea rdi, [r8 + 24]");                              // address the entry's low value word for reference promotion
    abi::emit_call_label(ctx.emitter, "__rt_hash_entry_make_reference");
    ctx.emitter.label(&done);
    Ok(())
}

/// Resolves an element reference through a boxed Mixed container on AArch64.
fn lower_load_mixed_array_elem_ref_cell_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    index: ValueId,
    create_missing: bool,
) -> Result<()> {
    let indexed_label = ctx.next_label("mixed_array_elem_ref_indexed");
    let hash_label = ctx.next_label("mixed_array_elem_ref_hash");
    let fetch_label = ctx.next_label("mixed_array_elem_ref_fetch");
    let missing_label = ctx.next_label("mixed_array_elem_ref_missing");
    let null_label = ctx.next_label("mixed_array_elem_ref_null");
    let done_label = ctx.next_label("mixed_array_elem_ref_done");
    ctx.load_value_to_reg(array, "x0")?;
    abi::emit_push_reg(ctx.emitter, "x0");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp x0, #4");                                      // runtime tag 4 identifies an indexed child container
    ctx.emitter.instruction(&format!("b.eq {indexed_label}"));                  // promote indexed children before exposing an element reference
    ctx.emitter.instruction("cmp x0, #5");                                      // runtime tag 5 identifies an associative child container
    ctx.emitter.instruction(&format!("b.eq {hash_label}"));                     // normalize existing hashes to Mixed entry storage
    ctx.emitter.instruction(&format!("b {null_label}"));                        // non-array Mixed payloads have no addressable element

    ctx.emitter.label(&indexed_label);
    ctx.emitter.instruction("mov x0, x1");                                      // pass the unboxed indexed child to shared hash promotion
    super::iterators::convert_loaded_indexed_source_to_hash(ctx);
    ctx.emitter.instruction(&format!("b {fetch_label}"));                       // publish the promoted child and fetch its requested entry

    ctx.emitter.label(&hash_label);
    ctx.emitter.instruction("mov x0, x1");                                      // pass the unboxed hash child to Mixed-entry normalization
    abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");

    ctx.emitter.label(&fetch_label);
    ctx.emitter.instruction("ldr x9, [sp]");                                    // reload the owning Mixed box after container conversion
    ctx.emitter.instruction("mov x10, #5");                                     // runtime tag 5 publishes associative hash storage
    ctx.emitter.instruction("str x10, [x9]");                                   // update the box tag before exposing the replacement hash
    ctx.emitter.instruction("str x0, [x9, #8]");                                // write the normalized hash back into the parent entry box
    ctx.emitter.instruction("str xzr, [x9, #16]");                              // clear the unused high payload word
    abi::emit_push_reg(ctx.emitter, "x0");
    super::hashes::materialize_hash_key_aarch64(ctx, index)?;
    abi::emit_pop_reg(ctx.emitter, "x0");
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    ctx.emitter.instruction(&format!("cbz x4, {missing_label}"));               // handle a missing key according to address-of versus foreach semantics
    ctx.emitter.instruction("add x0, x4, #24");                                 // pass the matched hash entry's value-low address
    abi::emit_call_label(ctx.emitter, "__rt_hash_entry_make_reference");
    abi::emit_pop_reg(ctx.emitter, "x9");
    ctx.emitter.instruction(&format!("b {done_label}"));                        // return the managed tag-11 reference cell

    ctx.emitter.label(&missing_label);
    if create_missing {
        ctx.emitter.instruction("ldr x9, [sp]");                                // reload the owning Mixed box before inserting the missing key
        ctx.emitter.instruction("ldr x0, [x9, #8]");                            // recover the normalized hash pointer from its box
        abi::emit_push_reg(ctx.emitter, "x0");
        crate::codegen::literal_defaults::emit_boxed_null_literal_to_result(ctx);
        abi::emit_push_reg(ctx.emitter, "x0");
        super::hashes::materialize_hash_key_aarch64(ctx, index)?;
        abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
        abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        abi::emit_pop_reg(ctx.emitter, "x3");
        abi::emit_pop_reg(ctx.emitter, "x0");
        ctx.emitter.instruction("mov x4, xzr");                                 // boxed null occupies only the low hash payload word
        abi::emit_load_int_immediate(ctx.emitter, "x5", runtime_value_tag(&PhpType::Mixed) as i64);
        abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        ctx.emitter.instruction("ldr x9, [sp]");                                // republish a hash pointer changed by insertion growth
        ctx.emitter.instruction("str x0, [x9, #8]");                            // keep the owning Mixed cell synchronized
        abi::emit_push_reg(ctx.emitter, "x0");
        super::hashes::materialize_hash_key_aarch64(ctx, index)?;
        abi::emit_pop_reg(ctx.emitter, "x0");
        abi::emit_call_label(ctx.emitter, "__rt_hash_get");
        ctx.emitter.instruction("add x0, x4, #24");                             // address the newly inserted entry payload
        abi::emit_call_label(ctx.emitter, "__rt_hash_entry_make_reference");
        abi::emit_pop_reg(ctx.emitter, "x9");
        ctx.emitter.instruction(&format!("b {done_label}"));                    // return the new managed entry cell
    } else {
        super::hashes::emit_undefined_hash_key_warning_aarch64(ctx, index)?;
        abi::emit_pop_reg(ctx.emitter, "x9");
        abi::emit_load_int_immediate(ctx.emitter, "x0", 0);
        ctx.emitter.instruction(&format!("b {done_label}"));                    // foreach source lookup leaves the hash unchanged
    }

    ctx.emitter.label(&null_label);
    abi::emit_pop_reg(ctx.emitter, "x9");
    abi::emit_load_int_immediate(ctx.emitter, "x0", 0);
    ctx.emitter.label(&done_label);
    store_ref_cell_pointer_result(ctx, inst)
}

/// Resolves an element reference through a boxed Mixed container on x86_64.
fn lower_load_mixed_array_elem_ref_cell_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    index: ValueId,
    create_missing: bool,
) -> Result<()> {
    let indexed_label = ctx.next_label("mixed_array_elem_ref_indexed");
    let hash_label = ctx.next_label("mixed_array_elem_ref_hash");
    let fetch_label = ctx.next_label("mixed_array_elem_ref_fetch");
    let missing_label = ctx.next_label("mixed_array_elem_ref_missing");
    let null_label = ctx.next_label("mixed_array_elem_ref_null");
    let done_label = ctx.next_label("mixed_array_elem_ref_done");
    ctx.load_value_to_reg(array, "rax")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp rax, 4");                                      // runtime tag 4 identifies an indexed child container
    ctx.emitter.instruction(&format!("je {indexed_label}"));                    // promote indexed children before exposing an element reference
    ctx.emitter.instruction("cmp rax, 5");                                      // runtime tag 5 identifies an associative child container
    ctx.emitter.instruction(&format!("je {hash_label}"));                       // normalize existing hashes to Mixed entry storage
    ctx.emitter.instruction(&format!("jmp {null_label}"));                      // non-array Mixed payloads have no addressable element

    ctx.emitter.label(&indexed_label);
    ctx.emitter.instruction("mov rax, rdi");                                    // pass the unboxed indexed child to shared hash promotion
    super::iterators::convert_loaded_indexed_source_to_hash(ctx);
    ctx.emitter.instruction(&format!("jmp {fetch_label}"));                     // publish the promoted child and fetch its requested entry

    ctx.emitter.label(&hash_label);
    abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");

    ctx.emitter.label(&fetch_label);
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                        // reload the owning Mixed box after container conversion
    ctx.emitter.instruction("mov QWORD PTR [r10], 5");                          // publish associative hash storage in the Mixed tag
    ctx.emitter.instruction("mov QWORD PTR [r10 + 8], rax");                    // write the normalized hash back into the parent entry box
    ctx.emitter.instruction("mov QWORD PTR [r10 + 16], 0");                     // clear the unused high payload word
    abi::emit_push_reg(ctx.emitter, "rax");
    super::hashes::materialize_hash_key_x86_64(ctx, index)?;
    abi::emit_pop_reg(ctx.emitter, "rdi");
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    ctx.emitter.instruction("test r8, r8");                                     // a missing key cannot supply a managed entry cell
    ctx.emitter.instruction(&format!("jz {missing_label}"));                    // handle a missing key according to address-of versus foreach semantics
    ctx.emitter.instruction("lea rdi, [r8 + 24]");                              // pass the matched hash entry's value-low address
    abi::emit_call_label(ctx.emitter, "__rt_hash_entry_make_reference");
    abi::emit_pop_reg(ctx.emitter, "r10");
    ctx.emitter.instruction(&format!("jmp {done_label}"));                      // return the managed tag-11 reference cell

    ctx.emitter.label(&missing_label);
    if create_missing {
        ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                    // reload the owning Mixed box before inserting the missing key
        ctx.emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                // recover the normalized hash pointer from its box
        abi::emit_push_reg(ctx.emitter, "rax");
        crate::codegen::literal_defaults::emit_boxed_null_literal_to_result(ctx);
        abi::emit_push_reg(ctx.emitter, "rax");
        super::hashes::materialize_hash_key_x86_64(ctx, index)?;
        abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
        abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
        abi::emit_pop_reg(ctx.emitter, "rcx");
        abi::emit_pop_reg(ctx.emitter, "rdi");
        ctx.emitter.instruction("xor r8d, r8d");                                // boxed null occupies only the low hash payload word
        abi::emit_load_int_immediate(ctx.emitter, "r9", runtime_value_tag(&PhpType::Mixed) as i64);
        abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                    // republish a hash pointer changed by insertion growth
        ctx.emitter.instruction("mov QWORD PTR [r10 + 8], rax");                // keep the owning Mixed cell synchronized
        abi::emit_push_reg(ctx.emitter, "rax");
        super::hashes::materialize_hash_key_x86_64(ctx, index)?;
        abi::emit_pop_reg(ctx.emitter, "rdi");
        abi::emit_call_label(ctx.emitter, "__rt_hash_get");
        ctx.emitter.instruction("lea rdi, [r8 + 24]");                          // address the newly inserted entry payload
        abi::emit_call_label(ctx.emitter, "__rt_hash_entry_make_reference");
        abi::emit_pop_reg(ctx.emitter, "r10");
        ctx.emitter.instruction(&format!("jmp {done_label}"));                  // return the new managed entry cell
    } else {
        super::hashes::emit_undefined_hash_key_warning_x86_64(ctx, index)?;
        abi::emit_pop_reg(ctx.emitter, "r10");
        abi::emit_load_int_immediate(ctx.emitter, "rax", 0);
        ctx.emitter.instruction(&format!("jmp {done_label}"));                  // foreach source lookup leaves the hash unchanged
    }

    ctx.emitter.label(&null_label);
    abi::emit_pop_reg(ctx.emitter, "r10");
    abi::emit_load_int_immediate(ctx.emitter, "rax", 0);
    ctx.emitter.label(&done_label);
    store_ref_cell_pointer_result(ctx, inst)
}

/// Returns the inline storage width for an indexed-array element from its value type.
///
/// `Str` and `TaggedScalar` elements occupy a 16-byte `{ptr,len}` / `{payload,tag}` slot; all
/// other scalar and refcounted-pointer elements occupy a single 8-byte word.
fn ref_cell_element_size(elem_ty: &PhpType) -> i64 {
    if matches!(elem_ty, PhpType::Str | PhpType::TaggedScalar) {
        16
    } else {
        8
    }
}

/// Lowers `LoadArrayElemRefCell` for AArch64: returns the element address in the int result reg.
fn lower_load_array_elem_ref_cell_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    index: ValueId,
    elem_size: i64,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let idx_reg = abi::int_result_reg(ctx.emitter);
    let len_reg = abi::secondary_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(array, array_reg)?;
    ctx.emitter.instruction(&format!("mov x0, {array_reg}"));                   // classify runtime-promoted array storage before address arithmetic
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    let indexed_label = ctx.next_label("array_elem_ref_indexed");
    let hash_label = ctx.next_label("array_elem_ref_hash");
    let null_label = ctx.next_label("array_elem_ref_null");
    let done_label = ctx.next_label("array_elem_ref_done");
    ctx.emitter.instruction("cmp x0, #3");                                      // heap kind 3 identifies a promoted hash receiver
    ctx.emitter.instruction(&format!("b.eq {hash_label}"));                     // hash entries require managed tag-11 reference promotion
    ctx.emitter.instruction(&format!("b {indexed_label}"));                     // ordinary indexed storage still exposes its inline slot

    ctx.emitter.label(&hash_label);
    super::hashes::materialize_hash_key_aarch64(ctx, index)?;
    ctx.load_value_to_reg(array, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    ctx.emitter.instruction(&format!("cbz x4, {null_label}"));                  // a missing key has no entry that can own a reference cell
    ctx.emitter.instruction("add x0, x4, #24");                                 // pass the matching entry's value-low address
    abi::emit_call_label(ctx.emitter, "__rt_hash_entry_make_reference");
    ctx.emitter.instruction(&format!("b {done_label}"));                        // return the managed reference-cell pointer

    ctx.emitter.label(&indexed_label);
    ctx.load_value_to_reg(array, array_reg)?;
    ctx.load_value_to_reg(index, idx_reg)?;
    ctx.emitter.instruction(&format!("cmp {}, #0", idx_reg));                   // check whether the indexed-array offset is negative
    ctx.emitter.instruction(&format!("b.lt {}", null_label));                   // negative offsets yield a null cell pointer
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);            // load the indexed-array logical length
    ctx.emitter.instruction(&format!("cmp {}, {}", idx_reg, len_reg));          // compare the requested offset against the array length
    ctx.emitter.instruction(&format!("b.ge {}", null_label));                   // out-of-bounds offsets yield a null cell pointer
    ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header to reach element payloads
    if elem_size == 16 {
        ctx.emitter.instruction(&format!("lsl {}, {}, #4", idx_reg, idx_reg));  // scale the offset by the 16-byte element slot width
    } else {
        ctx.emitter.instruction(&format!("lsl {}, {}, #3", idx_reg, idx_reg));  // scale the offset by the 8-byte element slot width
    }
    ctx.emitter
        .instruction(&format!("add {}, {}, {}", idx_reg, array_reg, idx_reg));  // compute the element address within the array payload
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the null fallback after computing the element address
    ctx.emitter.label(&null_label);
    abi::emit_load_int_immediate(ctx.emitter, idx_reg, 0);                      // materialize a null cell pointer for invalid indices
    ctx.emitter.label(&done_label);
    store_ref_cell_pointer_result(ctx, inst)
}

/// Lowers `LoadArrayElemRefCell` for x86_64: returns the element address in the int result reg.
fn lower_load_array_elem_ref_cell_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    index: ValueId,
    elem_size: i64,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let idx_reg = abi::int_result_reg(ctx.emitter);
    let len_reg = abi::secondary_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(array, array_reg)?;
    ctx.emitter.instruction(&format!("mov rax, {array_reg}"));                  // classify runtime-promoted array storage before address arithmetic
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    let indexed_label = ctx.next_label("array_elem_ref_indexed");
    let hash_label = ctx.next_label("array_elem_ref_hash");
    let null_label = ctx.next_label("array_elem_ref_null");
    let done_label = ctx.next_label("array_elem_ref_done");
    ctx.emitter.instruction("cmp rax, 3");                                      // heap kind 3 identifies a promoted hash receiver
    ctx.emitter.instruction(&format!("je {hash_label}"));                       // hash entries require managed tag-11 reference promotion
    ctx.emitter.instruction(&format!("jmp {indexed_label}"));                   // ordinary indexed storage still exposes its inline slot

    ctx.emitter.label(&hash_label);
    super::hashes::materialize_hash_key_x86_64(ctx, index)?;
    ctx.load_value_to_reg(array, "rdi")?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    ctx.emitter.instruction("test r8, r8");                                     // a missing key has no entry that can own a reference cell
    ctx.emitter.instruction(&format!("jz {null_label}"));                       // return null rather than dereferencing a missing entry
    ctx.emitter.instruction("lea rdi, [r8 + 24]");                              // pass the matching entry's value-low address
    abi::emit_call_label(ctx.emitter, "__rt_hash_entry_make_reference");
    ctx.emitter.instruction(&format!("jmp {done_label}"));                      // return the managed reference-cell pointer

    ctx.emitter.label(&indexed_label);
    ctx.load_value_to_reg(array, array_reg)?;
    ctx.load_value_to_reg(index, idx_reg)?;
    ctx.emitter.instruction(&format!("cmp {}, 0", idx_reg));                    // check whether the indexed-array offset is negative
    ctx.emitter.instruction(&format!("jl {}", null_label));                     // negative offsets yield a null cell pointer
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);            // load the indexed-array logical length
    ctx.emitter.instruction(&format!("cmp {}, {}", idx_reg, len_reg));          // compare the requested offset against the array length
    ctx.emitter.instruction(&format!("jge {}", null_label));                    // out-of-bounds offsets yield a null cell pointer
    ctx.emitter
        .instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg));      // skip the indexed-array header to reach element payloads
    if elem_size == 16 {
        ctx.emitter.instruction(&format!("shl {}, 4", idx_reg));                // scale the offset by the 16-byte element slot width
    } else {
        ctx.emitter.instruction(&format!("shl {}, 3", idx_reg));                // scale the offset by the 8-byte element slot width
    }
    ctx.emitter.instruction(&format!("add {}, {}", idx_reg, array_reg));        // compute the element address within the array payload
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the null fallback after computing the element address
    ctx.emitter.label(&null_label);
    abi::emit_load_int_immediate(ctx.emitter, idx_reg, 0);                      // materialize a null cell pointer for invalid indices
    ctx.emitter.label(&done_label);
    store_ref_cell_pointer_result(ctx, inst)
}

/// Stores the materialized reference-cell pointer (in the integer result register) into the
/// instruction's result value as a single machine word, mirroring `LoadPropRefCell` codegen.
fn store_ref_cell_pointer_result(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if let Some(result) = inst.result {
        ctx.store_int_result_value(result)?;
    }
    Ok(())
}

/// Lowers an indexed-array element address for by-reference call arguments.
pub(super) fn lower_array_elem_addr(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let index = expect_operand(inst, 1)?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("array_elem_addr missing result value"))?;
    let array_ty = ctx.value_php_type(array)?;
    require_indexed_array(array_ty.clone(), inst)?;
    require_integer_like_index(ctx.value_php_type(index)?, inst)?;
    let elem_size = array_element_size(&array_ty)?;
    let source_local = source_load_local_slot(ctx, array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_elem_addr_prepare_aarch64(ctx, array, index, elem_size)?,
        Arch::X86_64 => lower_array_elem_addr_prepare_x86_64(ctx, array, index, elem_size)?,
    }
    ctx.store_result_value(array)?;
    if let Some(slot) = source_local {
        ctx.store_container_writeback_to_local(slot, array)?;
    }
    ctx.writeback_global_array_source(array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_array_elem_addr_result_aarch64(ctx, array, index, elem_size)?,
        Arch::X86_64 => emit_array_elem_addr_result_x86_64(ctx, array, index, elem_size)?,
    }
    ctx.store_int_result_value(result)
}

/// Lowers an indexed-array element write through target-aware runtime helpers.
pub(super) fn lower_array_set(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let index = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    let elem_ty = indexed_array_element_type(&ctx.value_php_type(array)?, inst)?;
    let raw_value_ty = ctx.value_php_type(value)?.codegen_repr();
    let value_ty = effective_array_set_value_type(&elem_ty, &raw_value_ty, inst)?;
    require_integer_like_index(ctx.value_php_type(index)?, inst)?;
    let source_local = source_load_local_slot(ctx, array)?;
    if matches!(elem_ty.codegen_repr(), PhpType::Mixed) {
        lower_runtime_polymorphic_array_set(ctx, array, index, value, &raw_value_ty)?;
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                lower_array_set_aarch64(ctx, array, index, value, &raw_value_ty, &value_ty)?
            }
            Arch::X86_64 => {
                lower_array_set_x86_64(ctx, array, index, value, &raw_value_ty, &value_ty)?
            }
        }
    }
    stamp_scalar_array_write_result(ctx, &value_ty);
    ctx.store_result_value(array)?;
    if let Some(slot) = source_local {
        ctx.store_container_writeback_to_local(slot, array)?;
    }
    ctx.writeback_global_array_source(array)?;
    Ok(())
}

/// Writes an integer key into `Array(Mixed)` after dispatching on its runtime storage kind.
///
/// A runtime-shaped key write can promote the value before static flow typing observes a string
/// key. The indexed setter and hash setter have different headers and ownership protocols, so the
/// runtime kind must select the helper before either reads storage.
fn lower_runtime_polymorphic_array_set(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    let hash = ctx.next_label("array_set_mixed_hash");
    let done = ctx.next_label("array_set_mixed_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp x0, #3");                              // detect runtime-promoted associative storage
            ctx.emitter.instruction(&format!("b.eq {hash}"));                   // hash entries need tag-11-aware write-through
            lower_mixed_array_set_aarch64(ctx, array, index, value, value_ty)?;
            ctx.emitter.instruction(&format!("b {done}"));                      // join with the replacement container in x0

            ctx.emitter.label(&hash);
            super::hashes::lower_hash_set_aarch64(
                ctx,
                array,
                index,
                value,
                value_ty,
                &PhpType::Mixed,
            )?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rax")?;
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp rax, 3");                              // detect runtime-promoted associative storage
            ctx.emitter.instruction(&format!("je {hash}"));                     // hash entries need tag-11-aware write-through
            lower_mixed_array_set_x86_64(ctx, array, index, value, value_ty)?;
            ctx.emitter.instruction(&format!("jmp {done}"));                    // join with the replacement container in rax

            ctx.emitter.label(&hash);
            super::hashes::lower_hash_set_x86_64(
                ctx,
                array,
                index,
                value,
                value_ty,
                &PhpType::Mixed,
            )?;
        }
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Lowers a boxed-Mixed-key write into a statically `Array(Mixed)` indexed local.
///
/// The key tag is only known at runtime (PHP `foreach` keys are always `Mixed`
/// in EIR), so the write goes through `__rt_array_set_mixed_key`, which keeps
/// integer keys on indexed storage and promotes string keys to a hash. The value
/// is consumed as a boxed `Mixed` cell exactly like `__rt_array_set_mixed`; the
/// key is read (not consumed) by the helper.
pub(super) fn lower_array_set_mixed_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    require_indexed_array(ctx.value_php_type(array)?.codegen_repr(), inst)?;
    let key_ty = ctx.value_php_type(key)?.codegen_repr();
    if !matches!(key_ty, PhpType::Mixed | PhpType::Union(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "array_set_mixed_key key PHP type {:?}",
            key_ty
        )));
    }
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            lower_array_set_mixed_key_aarch64(ctx, array, key, value, &value_ty)?
        }
        Arch::X86_64 => lower_array_set_mixed_key_x86_64(ctx, array, key, value, &value_ty)?,
    }
    // The storeback to the destination local is driven by the EIR-level
    // `store_local` of this op's result value (emitted by `store_mutated_local`
    // in `ir_lower`), so here we only materialize the call result into its SSA
    // slot. Performing the storeback via `store_result_value`/`store_value_to_local`
    // instead would leave the result SSA value unmaterialized, and the later
    // EIR `store_local <result>` would read an uninitialized slot back into the
    // destination local (clobbering it with garbage on every write).
    store_if_result(ctx, inst)
}

/// Reads a mixed-key (string or int) element from an indexed array local via the
/// `__rt_array_get_mixed_key` runtime helper. Returns a boxed `Mixed` cell;
/// missing keys yield `Mixed(null)`.
pub(super) fn lower_array_get_mixed_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    warn_on_missing: bool,
) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    require_indexed_array(ctx.value_php_type(array)?.codegen_repr(), inst)?;
    let key_ty = ctx.value_php_type(key)?.codegen_repr();
    if !matches!(
        key_ty,
        PhpType::Mixed | PhpType::Union(_) | PhpType::Str | PhpType::Void | PhpType::Never
    ) {
        return Err(CodegenIrError::unsupported(format!(
            "array_get_mixed_key key PHP type {:?}",
            key_ty
        )));
    }
    lower_array_get_runtime_mixed(ctx, inst, array, key, warn_on_missing, false)
}

/// Reads an indexed-or-promoted array through its runtime storage metadata and returns a fresh
/// boxed Mixed cell, preserving typed slots when control-flow has widened only the static type.
fn lower_array_get_runtime_mixed(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    key: ValueId,
    warn_on_missing: bool,
    for_write: bool,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            super::hashes::materialize_hash_key_aarch64(ctx, key)?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            let flags = i64::from(warn_on_missing) | (i64::from(for_write) << 1);
            abi::emit_load_int_immediate(ctx.emitter, "x3", flags);
        }
        Arch::X86_64 => {
            super::hashes::materialize_hash_key_x86_64(ctx, key)?;
            abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
            ctx.load_value_to_reg(array, "rdi")?;
            abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
            let flags = i64::from(warn_on_missing) | (i64::from(for_write) << 1);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", flags);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_get_mixed_key");
    store_if_result(ctx, inst)
}

/// Boxes or retains a value, then stores it into a `Mixed`-keyed indexed array on AArch64.
fn lower_array_set_mixed_key_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    key: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        ctx.load_value_to_result(value)?;
        abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
    } else {
        box_value_for_mixed_container(ctx, value, value_ty)?;
    }
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.load_value_to_reg(array, "x0")?;
    // A raw StoreLocal transfers its existing slot owner into the helper. Stores through a ref
    // cell, static, or global retire their previous owner after publication, so those paths need a
    // separate helper owner. Acquiring for the raw-local path would leak an abandoned array.
    if mixed_key_storeback_retires_source(ctx, array)? {
        abi::emit_incref_if_refcounted(ctx.emitter, &ctx.value_php_type(array)?);
    }
    ctx.load_value_to_reg(key, "x1")?;
    abi::emit_pop_reg(ctx.emitter, "x2");
    abi::emit_call_label(ctx.emitter, "__rt_array_set_mixed_key");
    Ok(())
}

/// Boxes or retains a value, then stores it into a `Mixed`-keyed indexed array on x86_64.
fn lower_array_set_mixed_key_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    key: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        ctx.load_value_to_result(value)?;
        abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
    } else {
        box_value_for_mixed_container(ctx, value, value_ty)?;
    }
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.load_value_to_reg(array, "rax")?;
    // See the AArch64 twin: only a storeback that retires its previous owner needs a distinct
    // owner for the helper. The retain helper consumes and returns the integer result register.
    if mixed_key_storeback_retires_source(ctx, array)? {
        abi::emit_incref_if_refcounted(ctx.emitter, &ctx.value_php_type(array)?);
    }
    ctx.emitter.instruction("mov rdi, rax");                                    // publish the transferred or retained helper owner
    ctx.load_value_to_reg(key, "rsi")?;
    abi::emit_pop_reg(ctx.emitter, "rdx");
    abi::emit_call_label(ctx.emitter, "__rt_array_set_mixed_key");
    Ok(())
}

/// Returns whether the destination storeback retires the owner currently in storage.
fn mixed_key_storeback_retires_source(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<bool> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::invalid_module(
            "array_set_mixed_key destination must be loaded from writable storage",
        ));
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    match inst_ref.op {
        Op::LoadLocal => Ok(false),
        Op::LoadRefCell | Op::LoadStaticLocal | Op::LoadGlobal => Ok(true),
        other => Err(CodegenIrError::invalid_module(format!(
            "array_set_mixed_key destination was produced by {} instead of a writable load",
            other.name()
        ))),
    }
}

/// Lowers an indexed-array append through the runtime helper for the value type.
pub(super) fn lower_array_push(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let array_ty = ctx.value_php_type(array)?;
    require_indexed_array(array_ty.clone(), inst)?;
    let elem_ty = indexed_array_element_type(&array_ty, inst)?;
    let source_local = source_load_local_slot(ctx, array)?;
    let stored_type = if matches!(elem_ty.codegen_repr(), PhpType::Void | PhpType::Never) {
        ctx.value_php_type(value)?.codegen_repr()
    } else {
        elem_ty.codegen_repr()
    };
    lower_runtime_polymorphic_array_push(ctx, array, value, &elem_ty, &stored_type)?;
    ctx.store_result_value(array)?;
    if let Some(slot) = source_local {
        ctx.store_container_writeback_to_local(slot, array)?;
    }
    ctx.writeback_global_array_source(array)?;
    Ok(())
}

/// Appends to an indexed array whose payload may have been promoted to associative hash storage.
///
/// A runtime-shaped key write or a by-reference `foreach` can preserve the static array type while
/// changing the runtime heap kind. Dispatching every append here prevents a typed append after
/// promotion from
/// passing a hash header to an indexed-array helper. Each branch materializes its own owned Mixed
/// payload because indexed append retains then releases the temporary, while hash append consumes
/// the owned payload directly. Scalar storage stamps apply only to the indexed branch because a
/// promoted hash has its own header layout and always stores boxed Mixed values.
fn lower_runtime_polymorphic_array_push(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
    stored_type: &PhpType,
) -> Result<()> {
    let hash = ctx.next_label("array_push_hash");
    let done = ctx.next_label("array_push_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_push_reg(ctx.emitter, "x0");                              // preserve the receiver while heap-kind classification clobbers x0
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp x0, #3");                              // select associative storage after a runtime promotion
            ctx.emitter.instruction(&format!("b.eq {hash}"));                   // hash append has a distinct header and growth helper
            abi::emit_pop_reg(ctx.emitter, "x0");                               // restore the indexed receiver before ordinary append lowering
            ctx.store_result_value(array)?;
            lower_array_push_aarch64(ctx, array, value, elem_ty)?;
            stamp_scalar_array_write_result(ctx, stored_type);
            ctx.emitter.instruction(&format!("b {done}"));                      // join with the updated container pointer in x0

            ctx.emitter.label(&hash);
            prepare_boxed_mixed_value_for_container(ctx, value)?;
            abi::emit_push_reg(ctx.emitter, "x0");                              // stack the owned Mixed cell above the preserved receiver
            abi::emit_pop_reg(ctx.emitter, "x1");                               // transfer the owned Mixed cell into the hash entry
            abi::emit_pop_reg(ctx.emitter, "x0");                               // recover the receiver without reloading a call-clobbered SSA register
            ctx.emitter.instruction("mov x2, xzr");                             // boxed Mixed values use only the low payload word
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x3",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_append");
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rax")?;
            abi::emit_push_reg(ctx.emitter, "rax");                             // preserve the receiver while heap-kind classification clobbers rax
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp rax, 3");                              // select associative storage after a runtime promotion
            ctx.emitter.instruction(&format!("je {hash}"));                     // hash append has a distinct header and growth helper
            abi::emit_pop_reg(ctx.emitter, "rax");                              // restore the indexed receiver before ordinary append lowering
            ctx.store_result_value(array)?;
            lower_array_push_x86_64(ctx, array, value, elem_ty)?;
            stamp_scalar_array_write_result(ctx, stored_type);
            ctx.emitter.instruction(&format!("jmp {done}"));                    // join with the updated container pointer in rax

            ctx.emitter.label(&hash);
            prepare_boxed_mixed_value_for_container(ctx, value)?;
            abi::emit_push_reg(ctx.emitter, "rax");                             // stack the owned Mixed cell above the preserved receiver
            abi::emit_pop_reg(ctx.emitter, "rsi");                              // transfer the owned Mixed cell into the hash entry
            abi::emit_pop_reg(ctx.emitter, "rdi");                              // recover the receiver without reloading a call-clobbered SSA register
            ctx.emitter.instruction("xor edx, edx");                            // boxed Mixed values use only the low payload word
            abi::emit_load_int_immediate(
                ctx.emitter,
                "rcx",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_append");
        }
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Restores semantic tags after shared word-write helpers specialize an empty array's storage.
fn stamp_scalar_array_write_result(ctx: &mut FunctionContext<'_>, stored_type: &PhpType) {
    if matches!(stored_type, PhpType::Float | PhpType::Bool | PhpType::Callable) {
        // The helper has already performed COW and possible growth. Stamp its returned
        // owner, not the original pointer, so aliases retain their original metadata.
        let result = abi::int_result_reg(ctx.emitter);
        crate::codegen::emit_array_value_type_stamp(ctx.emitter, result, stored_type);
    }
}

/// Lowers appends through a boxed Mixed array cell.
pub(super) fn lower_mixed_array_append(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let receiver = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    match ctx.value_php_type(receiver)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {}
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "mixed_array_append receiver PHP type {:?}",
                other
            )))
        }
    }
    prepare_boxed_mixed_value_for_container(ctx, value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.load_value_to_reg(receiver, "x0")?;
            abi::emit_pop_reg(ctx.emitter, "x1");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.load_value_to_reg(receiver, "rdi")?;
            abi::emit_pop_reg(ctx.emitter, "rsi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_append");
    Ok(())
}

/// Lowers PHP indexed-array union through the shared runtime helper.
pub(super) fn lower_array_union(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let left = expect_operand(inst, 0)?;
    let right = expect_operand(inst, 1)?;
    require_indexed_array(ctx.value_php_type(left)?, inst)?;
    require_indexed_array(ctx.value_php_type(right)?, inst)?;
    require_indexed_array(inst.result_php_type.codegen_repr(), inst)?;
    lower_runtime_polymorphic_array_union(ctx, left, right)?;
    store_if_result(ctx, inst)
}

/// Unions two statically indexed arrays after selecting their runtime storage pair.
///
/// Runtime-shaped key writes may promote either operand to hash storage before a containing flow
/// fact changes. Every pair therefore uses the matching union helper, preserving PHP keys without
/// interpreting a hash header as packed.
fn lower_runtime_polymorphic_array_union(
    ctx: &mut FunctionContext<'_>,
    left: ValueId,
    right: ValueId,
) -> Result<()> {
    let left_hash = ctx.next_label("array_union_left_hash");
    let array_hash = ctx.next_label("array_union_array_hash");
    let hash_array = ctx.next_label("array_union_hash_array");
    let hash_hash = ctx.next_label("array_union_hash_hash");
    let done = ctx.next_label("array_union_runtime_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve operand pointers and runtime-kind flags
            ctx.load_value_to_reg(left, "x0")?;
            ctx.emitter.instruction("str x0, [sp]");                            // preserve the left container across kind classification
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("str x0, [sp, #16]");                       // save the left runtime storage kind
            ctx.load_value_to_reg(right, "x0")?;
            ctx.emitter.instruction("str x0, [sp, #8]");                        // preserve the right container across kind classification
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("ldr x9, [sp, #16]");                       // reload the left runtime storage kind
            ctx.emitter.instruction("cmp x9, #3");                              // kind 3 identifies a hash-backed left operand
            ctx.emitter.instruction(&format!("b.eq {left_hash}"));              // choose a hash-left helper when required
            ctx.emitter.instruction("cmp x0, #3");                              // classify the right operand for an indexed left
            ctx.emitter.instruction(&format!("b.eq {array_hash}"));             // indexed plus hash uses the cross-layout helper
            load_array_union_operands_aarch64(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_array_union");
            ctx.emitter.instruction(&format!("b {done}"));                      // join with the fresh union result in x0

            ctx.emitter.label(&left_hash);
            ctx.emitter.instruction("cmp x0, #3");                              // classify the right operand for a hash left
            ctx.emitter.instruction(&format!("b.eq {hash_hash}"));              // two hashes use the associative union helper
            ctx.emitter.instruction(&format!("b {hash_array}"));                // hash plus indexed uses the reverse cross-layout helper
            ctx.emitter.label(&array_hash);
            load_array_union_operands_aarch64(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_array_hash_union");
            ctx.emitter.instruction(&format!("b {done}"));                      // join with the fresh hash result in x0
            ctx.emitter.label(&hash_array);
            load_array_union_operands_aarch64(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_hash_array_union");
            ctx.emitter.instruction(&format!("b {done}"));                      // join with the fresh hash result in x0
            ctx.emitter.label(&hash_hash);
            load_array_union_operands_aarch64(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_hash_union");
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add sp, sp, #32");                         // release runtime-kind dispatch temporaries
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                             // reserve operand pointers and runtime-kind flags
            ctx.load_value_to_reg(left, "rax")?;
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // preserve the left container across kind classification
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // save the left runtime storage kind
            ctx.load_value_to_reg(right, "rax")?;
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // preserve the right container across kind classification
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 16], 3");             // kind 3 identifies a hash-backed left operand
            ctx.emitter.instruction(&format!("je {left_hash}"));                // choose a hash-left helper when required
            ctx.emitter.instruction("cmp rax, 3");                              // classify the right operand for an indexed left
            ctx.emitter.instruction(&format!("je {array_hash}"));               // indexed plus hash uses the cross-layout helper
            load_array_union_operands_x86_64(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_array_union");
            ctx.emitter.instruction(&format!("jmp {done}"));                    // join with the fresh union result in rax

            ctx.emitter.label(&left_hash);
            ctx.emitter.instruction("cmp rax, 3");                              // classify the right operand for a hash left
            ctx.emitter.instruction(&format!("je {hash_hash}"));                // two hashes use the associative union helper
            ctx.emitter.instruction(&format!("jmp {hash_array}"));              // hash plus indexed uses the reverse cross-layout helper
            ctx.emitter.label(&array_hash);
            load_array_union_operands_x86_64(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_array_hash_union");
            ctx.emitter.instruction(&format!("jmp {done}"));                    // join with the fresh hash result in rax
            ctx.emitter.label(&hash_array);
            load_array_union_operands_x86_64(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_hash_array_union");
            ctx.emitter.instruction(&format!("jmp {done}"));                    // join with the fresh hash result in rax
            ctx.emitter.label(&hash_hash);
            load_array_union_operands_x86_64(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_hash_union");
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add rsp, 32");                             // release runtime-kind dispatch temporaries
        }
    }
    Ok(())
}

/// Reloads staged union operands into the AArch64 argument registers.
fn load_array_union_operands_aarch64(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.instruction("ldr x0, [sp]");                                    // restore the left union operand
    ctx.emitter.instruction("ldr x1, [sp, #8]");                                // restore the right union operand
}

/// Reloads staged union operands into the x86_64 argument registers.
fn load_array_union_operands_x86_64(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                        // restore the left union operand
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                    // restore the right union operand
}

/// Lowers indexed+associative array union through the shared hash runtime helper.
pub(super) fn lower_array_hash_union(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let left = expect_operand(inst, 0)?;
    let right = expect_operand(inst, 1)?;
    require_indexed_array(ctx.value_php_type(left)?, inst)?;
    require_assoc_union_hash_operand(ctx.value_php_type(right)?, inst)?;
    let result_value_ty = require_array_to_hash_result(&inst.result_php_type.codegen_repr(), inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let hash_left = ctx.next_label("array_hash_union_hash_left");
            let done = ctx.next_label("array_hash_union_runtime_done");
            ctx.load_value_to_reg(left, "x0")?;
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp x0, #3");                              // detect a runtime-promoted left array
            ctx.emitter.instruction(&format!("b.eq {hash_left}"));              // two hash operands use the associative helper
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.load_value_to_reg(right, "x1")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_hash_union");
            ctx.emitter.instruction(&format!("b {done}"));                      // join with the fresh hash result in x0
            ctx.emitter.label(&hash_left);
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.load_value_to_reg(right, "x1")?;
            abi::emit_call_label(ctx.emitter, "__rt_hash_union");
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            let hash_left = ctx.next_label("array_hash_union_hash_left");
            let done = ctx.next_label("array_hash_union_runtime_done");
            ctx.load_value_to_reg(left, "rax")?;
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp rax, 3");                              // detect a runtime-promoted left array
            ctx.emitter.instruction(&format!("je {hash_left}"));                // two hash operands use the associative helper
            abi::emit_pop_reg(ctx.emitter, "rdi");
            ctx.load_value_to_reg(right, "rsi")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_hash_union");
            ctx.emitter.instruction(&format!("jmp {done}"));                    // join with the fresh hash result in rax
            ctx.emitter.label(&hash_left);
            abi::emit_pop_reg(ctx.emitter, "rdi");
            ctx.load_value_to_reg(right, "rsi")?;
            abi::emit_call_label(ctx.emitter, "__rt_hash_union");
            ctx.emitter.label(&done);
        }
    }
    convert_hash_union_result_to_mixed_if_needed(ctx, &result_value_ty);
    store_if_result(ctx, inst)
}

/// Lowers an indexed-array element read for AArch64 targets.
fn lower_array_get_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    index: ValueId,
    elem_ty: &PhpType,
    result_ty: &PhpType,
    warn_on_missing: bool,
    mode: ArrayGetMode,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let len_reg = abi::secondary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let null_label = ctx.next_label("array_get_null");
    let null_receiver_label = ctx.next_label("array_get_null_recv");
    let fallback_label = ctx.next_label("array_get_fallback");
    let done_label = ctx.next_label("array_get_done");
    let promoted_label = ctx.next_label("array_get_promoted");

    // An `Array(_)`-typed local can be backed by HASH storage at runtime: a mixed-key write
    // promotes the storage kind while the checker only promotes the STATIC type to `AssocArray` at
    // a provably string-keyed write. The packed payload walk below is only valid on kind-2 storage
    // — on a hash it bounds-checks the index against the header's live-entry count and then reads
    // the header's own fields as if they were elements, which SEGFAULTED. Dispatch on the runtime
    // kind, exactly as the `isset` probe and `__rt_array_get_mixed_key` already do.
    // An array with no element type has no elements to read, and the hash-value materializer has no
    // representation to produce for `Void`/`Never`, so such a receiver keeps the packed-only path:
    // every read of it is a miss either way.
    let elem_is_empty = matches!(elem_ty.codegen_repr(), PhpType::Void | PhpType::Never);
    // The promoted read is emitted speculatively, so it must be *representable* for every element
    // type it is emitted for — an unsupported one fails the compile instead of sitting unreached.
    // `?int` (`TaggedScalar`) has no hash representation on either side of the lookup, so an array
    // of them can never be hash-backed and the packed-only path stays correct.
    let can_read_promoted = !elem_is_empty && super::hashes::hash_get_supports_value_type(elem_ty);
    // Gate on the IR type, NOT the PHP type: `Op::IChecked*` — what `$i++` lowers to — reports a
    // PHP type of `Mixed` while its runtime value is a RAW INTEGER. Unboxing that as a cell pointer
    // reads garbage, and every read through an incremented loop counter silently returned nothing.
    let index_is_mixed_key = matches!(
        ctx.value_ir_type(index)?,
        crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed)
    );
    ctx.load_value_to_reg(array, array_reg)?;
    // -- guard the receiver before reading its storage-kind metadata --
    crate::codegen::sentinels::emit_branch_if_null_container(
        ctx.emitter,
        array_reg,
        len_reg,
        &null_receiver_label,
    );
    if can_read_promoted {
        ctx.emitter
            .instruction(
            &format!("ldr {}, [{}, #-8]", len_reg, array_reg)
        );                                                                      // load the storage-kind metadata word from the array header
        ctx.emitter
            .instruction(
            &format!("and {}, {}, #0xff", len_reg, len_reg)
        );                                                                      // isolate the low byte holding the storage kind
        ctx.emitter.instruction(&format!("cmp {}, #3", len_reg));               // kind 3 = storage was promoted to a hash at runtime
        ctx.emitter.instruction(&format!("b.eq {}", promoted_label));           // a promoted array has no packed payload to index into
    }

    // A `Mixed` key arrives as a boxed cell, not an integer. Materialize it into the normalized
    // (key_lo, key_hi) pair — which also applies PHP's numeric-string rule — and reject a genuine
    // string key outright: packed storage never holds one.
    if index_is_mixed_key {
        super::hashes::materialize_hash_key_aarch64(ctx, index)?;
        ctx.emitter.instruction("cmn x2, #1");                                  // key_hi == -1 marks an integer key
        ctx.emitter.instruction(&format!("b.ne {}", null_label));               // a string key never exists in packed storage
        ctx.emitter.instruction(&format!("mov {}, x1", result_reg));            // adopt the normalized integer key as the offset
    } else {
        ctx.load_value_to_reg(index, result_reg)?;
    }
    ctx.load_value_to_reg(array, array_reg)?;
    ctx.emitter.instruction(&format!("cmp {}, #0", result_reg));                // check whether the indexed-array offset is negative
    ctx.emitter.instruction(&format!("b.lt {}", null_label));                   // negative indexed-array offsets read as null
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);
    ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg));       // compare the requested offset against the indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", null_label));                   // out-of-range indexed-array offsets read as null
    emit_array_get_in_bounds_aarch64(ctx, array_reg, result_reg, elem_ty, result_ty, mode)?;
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the null fallback after a successful indexed-array read

    // -- promoted to hash storage: read through the hash, materializing the SAME representation
    //    the packed path produces, so the op's result type is unchanged --
    if can_read_promoted {
        ctx.emitter.label(&promoted_label);
        super::hashes::materialize_hash_key_aarch64(ctx, index)?;
        ctx.load_value_to_reg(array, "x0")?;
        abi::emit_call_label(ctx.emitter, "__rt_hash_get");
        ctx.emitter.instruction(&format!("cbz x0, {}", null_label));            // a missing key falls through to the shared null/warning path
        super::hashes::emit_hash_get_success_aarch64(ctx, elem_ty, result_ty, false)?;
        ctx.emitter.instruction(&format!("b {}", done_label));                  // skip the null fallback after a promoted-hash read
    }

    ctx.emitter.label(&null_label);
    if warn_on_missing {
        emit_undefined_array_key_warning(ctx);
    }
    abi::emit_jump(ctx.emitter, &fallback_label);
    ctx.emitter.label(&null_receiver_label);
    if warn_on_missing {
        emit_array_offset_on_null_warning(ctx);
    }
    ctx.emitter.label(&fallback_label);
    emit_array_get_null_fallback(ctx, result_ty, !warn_on_missing);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers an indexed-array element write for AArch64 targets.
fn lower_array_set_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    value: ValueId,
    raw_value_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty, PhpType::Mixed) {
        return lower_mixed_array_set_aarch64(ctx, array, index, value, raw_value_ty);
    }
    ctx.load_value_to_reg(array, "x0")?;
    ctx.load_value_to_reg(index, "x1")?;
    match value_ty {
        PhpType::Int | PhpType::Bool | PhpType::Float => {
            ctx.load_value_to_reg(value, "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_int");
        }
        PhpType::Callable => {
            ctx.load_value_to_reg(value, "x0")?;
            abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
            ctx.emitter.instruction("mov x2, x0");                              // pass an array-owned callable descriptor to the indexed-array setter
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(index, "x1")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_int");
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "x2", "x3")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_str");
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_reg(value, "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_set value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Lowers an indexed-array element read for x86_64 targets.
fn lower_array_get_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    index: ValueId,
    elem_ty: &PhpType,
    result_ty: &PhpType,
    warn_on_missing: bool,
    mode: ArrayGetMode,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let len_reg = abi::secondary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let null_label = ctx.next_label("array_get_null");
    let null_receiver_label = ctx.next_label("array_get_null_recv");
    let fallback_label = ctx.next_label("array_get_fallback");
    let done_label = ctx.next_label("array_get_done");
    let promoted_label = ctx.next_label("array_get_promoted");

    // Storage-kind guarded exactly like the AArch64 twin: an `Array(_)`-typed local can be
    // hash-backed at runtime, and walking its packed payload then reads the hash header's own
    // fields as elements.
    // See the AArch64 twin: an array with no element type keeps the packed-only path.
    let elem_is_empty = matches!(elem_ty.codegen_repr(), PhpType::Void | PhpType::Never);
    // See the AArch64 twin: the promoted read is emitted speculatively, so an element type the hash
    // path cannot materialize (`?int` — `TaggedScalar`) fails the compile rather than sitting
    // unreached. Such an array can never be hash-backed, so the packed-only path stays correct.
    let can_read_promoted = !elem_is_empty && super::hashes::hash_get_supports_value_type(elem_ty);
    // Gate on the IR type, NOT the PHP type: `Op::IChecked*` — what `$i++` lowers to — reports a
    // PHP type of `Mixed` while its runtime value is a RAW INTEGER. Unboxing that as a cell pointer
    // reads garbage, and every read through an incremented loop counter silently returned nothing.
    let index_is_mixed_key = matches!(
        ctx.value_ir_type(index)?,
        crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed)
    );
    ctx.load_value_to_reg(array, array_reg)?;
    // -- guard the receiver before reading its storage-kind metadata --
    crate::codegen::sentinels::emit_branch_if_null_container(
        ctx.emitter,
        array_reg,
        len_reg,
        &null_receiver_label,
    );
    if can_read_promoted {
        ctx.emitter
            .instruction(
            &format!("mov {}, QWORD PTR [{} - 8]", len_reg, array_reg)
        );                                                                      // load the storage-kind metadata word from the array header
        ctx.emitter.instruction(&format!("and {}, 0xff", len_reg));             // isolate the low byte holding the storage kind
        ctx.emitter.instruction(&format!("cmp {}, 3", len_reg));                // kind 3 = storage was promoted to a hash at runtime
        ctx.emitter.instruction(&format!("je {}", promoted_label));             // a promoted array has no packed payload to index into
    }

    // See the AArch64 twin: a `Mixed` key is materialized, not loaded as an integer.
    if index_is_mixed_key {
        super::hashes::materialize_hash_key_x86_64(ctx, index)?;
        ctx.emitter.instruction("cmp rdx, -1");                                 // key_hi == -1 marks an integer key
        ctx.emitter.instruction(&format!("jne {}", null_label));                // a string key never exists in packed storage
        ctx.emitter.instruction(&format!("mov {}, rsi", result_reg));           // adopt the normalized integer key as the offset
    } else {
        ctx.load_value_to_reg(index, result_reg)?;
    }
    ctx.load_value_to_reg(array, array_reg)?;
    ctx.emitter.instruction(&format!("cmp {}, 0", result_reg));                 // check whether the indexed-array offset is negative
    ctx.emitter.instruction(&format!("jl {}", null_label));                     // negative indexed-array offsets read as null
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);
    ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg));       // compare the requested offset against the indexed-array length
    ctx.emitter.instruction(&format!("jge {}", null_label));                    // out-of-range indexed-array offsets read as null
    emit_array_get_in_bounds_x86_64(ctx, array_reg, result_reg, elem_ty, result_ty, mode)?;
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the null fallback after a successful indexed-array read

    // -- promoted to hash storage: read through the hash, materializing the SAME representation
    //    the packed path produces, so the op's result type is unchanged --
    if can_read_promoted {
        ctx.emitter.label(&promoted_label);
        super::hashes::materialize_hash_key_x86_64(ctx, index)?;
        ctx.load_value_to_reg(array, "rdi")?;
        abi::emit_call_label(ctx.emitter, "__rt_hash_get");
        ctx.emitter.instruction("test rax, rax");                               // did the promoted hash storage hold the key?
        ctx.emitter.instruction(&format!("jz {}", null_label));                 // a missing key falls through to the shared null/warning path
        super::hashes::emit_hash_get_success_x86_64(ctx, elem_ty, result_ty, false)?;
        ctx.emitter.instruction(&format!("jmp {}", done_label));                // skip the null fallback after a promoted-hash read
    }

    ctx.emitter.label(&null_label);
    if warn_on_missing {
        emit_undefined_array_key_warning(ctx);
    }
    abi::emit_jump(ctx.emitter, &fallback_label);
    ctx.emitter.label(&null_receiver_label);
    if warn_on_missing {
        emit_array_offset_on_null_warning(ctx);
    }
    ctx.emitter.label(&fallback_label);
    emit_array_get_null_fallback(ctx, result_ty, !warn_on_missing);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers an indexed-array element write for x86_64 targets.
fn lower_array_set_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    value: ValueId,
    raw_value_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty, PhpType::Mixed) {
        return lower_mixed_array_set_x86_64(ctx, array, index, value, raw_value_ty);
    }
    ctx.load_value_to_reg(array, "rdi")?;
    ctx.load_value_to_reg(index, "rsi")?;
    match value_ty {
        PhpType::Int | PhpType::Bool | PhpType::Float => {
            ctx.load_value_to_reg(value, "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_int");
        }
        PhpType::Callable => {
            ctx.load_value_to_reg(value, "rax")?;
            abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
            ctx.emitter.instruction("mov rdx, rax");                            // pass an array-owned callable descriptor to the indexed-array setter
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(index, "rsi")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_int");
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "rdx", "rcx")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_str");
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_reg(value, "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_set value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Emits the in-bounds indexed-array payload load for AArch64.
fn emit_array_get_in_bounds_aarch64(
    ctx: &mut FunctionContext<'_>,
    array_reg: &str,
    index_reg: &str,
    elem_ty: &PhpType,
    result_ty: &PhpType,
    mode: ArrayGetMode,
) -> Result<()> {
    if let ArrayGetMode::ForWrite { helper } = mode {
        emit_array_get_for_write_in_bounds_aarch64(ctx, array_reg, index_reg, helper);
        return Ok(());
    }
    let widened_done = if !matches!(elem_ty, PhpType::Void | PhpType::Never | PhpType::Mixed) {
        let typed = ctx.next_label("array_get_typed_payload");
        let done = ctx.next_label("array_get_widened_done");
        ctx.emitter.instruction(&format!("ldr x11, [{}, #-8]", array_reg));     // load the runtime indexed-array value type
        ctx.emitter.instruction("ubfx x11, x11, #8, #7");                       // isolate the value type without the COW flag
        ctx.emitter.instruction("cmp x11, #7");                                 // tag 7 means the slots were widened to boxed Mixed
        ctx.emitter.instruction(&format!("b.ne {}", typed));                    // preserve the ordinary typed-slot fast path
        ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // move to the pointer-sized Mixed payload base
        let load_widened = format!(
            "ldr x0, [{}, {}, lsl #3]",
            array_reg, index_reg
        );
        ctx.emitter.instruction(&load_widened);                                 // load the boxed element using its widened slot layout
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        materialize_widened_array_payload_aarch64(ctx, elem_ty, result_ty);
        ctx.emitter.instruction(&format!("b {}", done));                        // skip the original static-layout load
        ctx.emitter.label(&typed);
        Some(done)
    } else {
        None
    };
    match elem_ty {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, index_reg, 0x7fff_ffff_ffff_fffe);
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter
                .instruction(
                &format!("add {}, {}, #24", array_reg, array_reg)
            );                                                                  // skip the indexed-array header to reach element payloads
            ctx.emitter
                .instruction(
                &format!("ldr {}, [{}, {}, lsl #3]", index_reg, array_reg, index_reg)
            );                                                                  // load the selected pointer-sized indexed-array element
            if matches!(elem_ty, PhpType::Callable) {
                abi::emit_incref_if_refcounted(ctx.emitter, elem_ty);
            }
            if matches!(result_ty, PhpType::TaggedScalar) {
                crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
            }
        }
        PhpType::Float => {
            ctx.emitter
                .instruction(
                &format!("add {}, {}, #24", array_reg, array_reg)
            );                                                                  // skip the indexed-array header to reach float payloads
            ctx.emitter
                .instruction(
                &format!("ldr d0, [{}, {}, lsl #3]", array_reg, index_reg)
            );                                                                  // load the selected indexed-array float element
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.emitter
                .instruction(
                &format!("lsl {}, {}, #4", index_reg, index_reg)
            );                                                                  // scale the string-array offset by the pointer-plus-length slot size
            ctx.emitter
                .instruction(
                &format!("add {}, {}, {}", array_reg, array_reg, index_reg)
            );                                                                  // move to the selected string slot within the indexed array
            ctx.emitter
                .instruction(
                &format!("add {}, {}, #24", array_reg, array_reg)
            );                                                                  // skip the indexed-array header before loading the string slot
            abi::emit_load_from_address(ctx.emitter, ptr_reg, array_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 8);
        }
        PhpType::TaggedScalar => {
            let tag_reg = crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter);
            ctx.emitter
                .instruction(
                &format!("lsl {}, {}, #4", index_reg, index_reg)
            );                                                                  // scale the tagged-scalar offset by the payload-plus-tag slot size
            ctx.emitter
                .instruction(
                &format!("add {}, {}, {}", array_reg, array_reg, index_reg)
            );                                                                  // move to the selected tagged-scalar slot within the indexed array
            ctx.emitter
                .instruction(
                &format!("add {}, {}, #24", array_reg, array_reg)
            );                                                                  // skip the indexed-array header before loading the tagged-scalar slot
            abi::emit_load_from_address(ctx.emitter, index_reg, array_reg, 0);
            abi::emit_load_from_address(ctx.emitter, tag_reg, array_reg, 8);
        }
        PhpType::Mixed => {
            ctx.emitter
                .instruction(
                &format!("add {}, {}, #24", array_reg, array_reg)
            );                                                                  // skip the indexed-array header to reach Mixed cell payloads
            ctx.emitter
                .instruction(
                &format!("ldr {}, [{}, {}, lsl #3]", index_reg, array_reg, index_reg)
            );                                                                  // load the selected boxed Mixed cell
            emit_mixed_array_get_deref_invoker_ref_cell(ctx, index_reg);
        }
        other if other.is_refcounted() => {
            ctx.emitter
                .instruction(
                &format!("add {}, {}, #24", array_reg, array_reg)
            );                                                                  // skip the indexed-array header to reach pointer payloads
            ctx.emitter
                .instruction(
                &format!("ldr {}, [{}, {}, lsl #3]", index_reg, array_reg, index_reg)
            );                                                                  // load the selected refcounted indexed-array element
            abi::emit_incref_if_refcounted(ctx.emitter, other);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_get element PHP type {:?}",
                other
            )));
        }
    }
    if let Some(done) = widened_done {
        ctx.emitter.label(&done);
    }
    Ok(())
}

/// Emits the in-bounds indexed-array payload load for x86_64.
fn emit_array_get_in_bounds_x86_64(
    ctx: &mut FunctionContext<'_>,
    array_reg: &str,
    index_reg: &str,
    elem_ty: &PhpType,
    result_ty: &PhpType,
    mode: ArrayGetMode,
) -> Result<()> {
    if let ArrayGetMode::ForWrite { helper } = mode {
        emit_array_get_for_write_in_bounds_x86_64(ctx, array_reg, index_reg, helper);
        return Ok(());
    }
    let widened_done = if !matches!(elem_ty, PhpType::Void | PhpType::Never | PhpType::Mixed) {
        let typed = ctx.next_label("array_get_typed_payload");
        let done = ctx.next_label("array_get_widened_done");
        let load_value_type = format!(
            "mov rcx, QWORD PTR [{} - 8]",
            array_reg
        );
        ctx.emitter.instruction(&load_value_type);                              // load the runtime indexed-array value type
        ctx.emitter.instruction("shr rcx, 8");                                  // move the value type byte into the low bits
        ctx.emitter.instruction("and rcx, 0x7f");                               // isolate the value type without the COW flag
        ctx.emitter.instruction("cmp rcx, 7");                                  // tag 7 means the slots were widened to boxed Mixed
        ctx.emitter.instruction(&format!("jne {}", typed));                     // preserve the ordinary typed-slot fast path
        ctx.emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // move to the pointer-sized Mixed payload base
        let load_widened = format!(
            "mov rax, QWORD PTR [{} + {} * 8]",
            array_reg, index_reg
        );
        ctx.emitter.instruction(&load_widened);                                 // load the boxed element using its widened slot layout
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        materialize_widened_array_payload_x86_64(ctx, elem_ty, result_ty);
        ctx.emitter.instruction(&format!("jmp {}", done));                      // skip the original static-layout load
        ctx.emitter.label(&typed);
        Some(done)
    } else {
        None
    };
    match elem_ty {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, index_reg, 0x7fff_ffff_ffff_fffe);
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter
                .instruction(
                &format!("lea {}, [{} + 24]", array_reg, array_reg)
            );                                                                  // skip the indexed-array header to reach element payloads
            ctx.emitter
                .instruction(
                &format!("mov {}, QWORD PTR [{} + {} * 8]", index_reg, array_reg, index_reg)
            );                                                                  // load the selected pointer-sized indexed-array element
            if matches!(elem_ty, PhpType::Callable) {
                abi::emit_incref_if_refcounted(ctx.emitter, elem_ty);
            }
            if matches!(result_ty, PhpType::TaggedScalar) {
                crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
            }
        }
        PhpType::Float => {
            ctx.emitter
                .instruction(
                &format!("lea {}, [{} + 24]", array_reg, array_reg)
            );                                                                  // skip the indexed-array header to reach float payloads
            ctx.emitter
                .instruction(
                &format!("movsd xmm0, QWORD PTR [{} + {} * 8]", array_reg, index_reg)
            );                                                                  // load the selected indexed-array float element
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.emitter.instruction(&format!("shl {}, 4", index_reg));          // scale the string-array offset by the pointer-plus-length slot size
            ctx.emitter
                .instruction(
                &format!("add {}, {}", array_reg, index_reg)
            );                                                                  // move to the selected string slot within the indexed array
            ctx.emitter.instruction(&format!("add {}, 24", array_reg));         // skip the indexed-array header before loading the string slot
            abi::emit_load_from_address(ctx.emitter, ptr_reg, array_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 8);
        }
        PhpType::TaggedScalar => {
            let tag_reg = crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter);
            ctx.emitter.instruction(&format!("shl {}, 4", index_reg));          // scale the tagged-scalar offset by the payload-plus-tag slot size
            ctx.emitter
                .instruction(
                &format!("add {}, {}", array_reg, index_reg)
            );                                                                  // move to the selected tagged-scalar slot within the indexed array
            ctx.emitter.instruction(&format!("add {}, 24", array_reg));         // skip the indexed-array header before loading the tagged-scalar slot
            abi::emit_load_from_address(ctx.emitter, index_reg, array_reg, 0);
            abi::emit_load_from_address(ctx.emitter, tag_reg, array_reg, 8);
        }
        PhpType::Mixed => {
            ctx.emitter
                .instruction(
                &format!("lea {}, [{} + 24]", array_reg, array_reg)
            );                                                                  // skip the indexed-array header to reach Mixed cell payloads
            ctx.emitter
                .instruction(
                &format!("mov {}, QWORD PTR [{} + {} * 8]", index_reg, array_reg, index_reg)
            );                                                                  // load the selected boxed Mixed cell
            emit_mixed_array_get_deref_invoker_ref_cell(ctx, index_reg);
        }
        other if other.is_refcounted() => {
            ctx.emitter
                .instruction(
                &format!("lea {}, [{} + 24]", array_reg, array_reg)
            );                                                                  // skip the indexed-array header to reach pointer payloads
            ctx.emitter
                .instruction(
                &format!("mov {}, QWORD PTR [{} + {} * 8]", index_reg, array_reg, index_reg)
            );                                                                  // load the selected refcounted indexed-array element
            abi::emit_incref_if_refcounted(ctx.emitter, other);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_get element PHP type {:?}",
                other
            )));
        }
    }
    if let Some(done) = widened_done {
        ctx.emitter.label(&done);
    }
    Ok(())
}

/// Adapts an unboxed Mixed slot to a statically concrete AArch64 array result.
fn materialize_widened_array_payload_aarch64(
    ctx: &mut FunctionContext<'_>,
    elem_ty: &PhpType,
    result_ty: &PhpType,
) {
    match elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter.instruction("mov x0, x1");                              // move the unboxed low word into the integer result register
            if matches!(elem_ty, PhpType::Callable) {
                abi::emit_incref_if_refcounted(ctx.emitter, elem_ty);
            }
            if matches!(result_ty, PhpType::TaggedScalar) {
                crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
            }
        }
        PhpType::Float => ctx.emitter.instruction("fmov d0, x1"),               // move the unboxed float bits into the float result register
        PhpType::Str => {}
        PhpType::TaggedScalar => {
            ctx.emitter.instruction("mov x9, x0");                              // preserve the runtime scalar tag
            ctx.emitter.instruction("mov x0, x1");                              // move the unboxed payload into the scalar result register
            ctx.emitter.instruction("mov x1, x9");                              // pair the payload with its runtime tag
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("mov x0, x1");                              // return the unboxed pointer-backed payload
            abi::emit_incref_if_refcounted(ctx.emitter, other);
        }
        _ => {}
    }
}

/// Adapts an unboxed Mixed slot to a statically concrete x86_64 array result.
fn materialize_widened_array_payload_x86_64(
    ctx: &mut FunctionContext<'_>,
    elem_ty: &PhpType,
    result_ty: &PhpType,
) {
    match elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed low word into the integer result register
            if matches!(elem_ty, PhpType::Callable) {
                abi::emit_incref_if_refcounted(ctx.emitter, elem_ty);
            }
            if matches!(result_ty, PhpType::TaggedScalar) {
                crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
            }
        }
        PhpType::Float => ctx.emitter.instruction("movq xmm0, rdi"),            // move the unboxed float bits into the float result register
        PhpType::Str => {
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed string pointer into the string result register
        }
        PhpType::TaggedScalar => {
            ctx.emitter.instruction("mov r10, rax");                            // preserve the runtime scalar tag
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed payload into the scalar result register
            ctx.emitter.instruction("mov rdx, r10");                            // pair the payload with its runtime tag
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("mov rax, rdi");                            // return the unboxed pointer-backed payload
            abi::emit_incref_if_refcounted(ctx.emitter, other);
        }
        _ => {}
    }
}

/// Emits the in-bounds copy-on-write element fetch for AArch64.
///
/// Computes the element slot address, separates the container it holds through `helper`, and
/// stores the (possibly new) pointer straight back into that slot. The store is unconditional
/// because the helper returns the original pointer untouched whenever no split was needed, and
/// it is what keeps the refcounts balanced on the split path: the `ensure_unique` helpers drop
/// one reference from the shared original, and the slot it came from is exactly the owner giving
/// that reference up, taking the fresh clone in exchange.
///
/// The slot address is spilled across the call because both scratch registers used here are
/// caller-saved. Result: the unique element pointer in the integer result register, BORROWED —
/// the parent slot owns it.
fn emit_array_get_for_write_in_bounds_aarch64(
    ctx: &mut FunctionContext<'_>,
    array_reg: &str,
    index_reg: &str,
    helper: &str,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header to reach element payloads
    ctx.emitter
        .instruction(
        &format!("add {}, {}, {}, lsl #3", array_reg, array_reg, index_reg)
    );                                                                          // address the selected element slot within the payload
    abi::emit_push_reg(ctx.emitter, array_reg);                                // preserve the element slot address across the copy-on-write helper call
    abi::emit_load_from_address(ctx.emitter, result_reg, array_reg, 0);
    abi::emit_call_label(ctx.emitter, helper);
    abi::emit_pop_reg(ctx.emitter, array_reg);                                 // restore the element slot address after the copy-on-write helper call
    abi::emit_store_to_address(ctx.emitter, result_reg, array_reg, 0);
}

/// Emits the in-bounds copy-on-write element fetch for x86_64. Mirrors the AArch64 shape.
fn emit_array_get_for_write_in_bounds_x86_64(
    ctx: &mut FunctionContext<'_>,
    array_reg: &str,
    index_reg: &str,
    helper: &str,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.emitter
        .instruction(
        &format!("lea {}, [{} + 24]", array_reg, array_reg)
    );                                                                          // skip the indexed-array header to reach element payloads
    ctx.emitter
        .instruction(
        &format!("lea {}, [{} + {} * 8]", array_reg, array_reg, index_reg)
    );                                                                          // address the selected element slot within the payload
    abi::emit_push_reg(ctx.emitter, array_reg);                                // preserve the element slot address across the copy-on-write helper call
    abi::emit_load_from_address(ctx.emitter, "rdi", array_reg, 0);
    abi::emit_call_label(ctx.emitter, helper);
    abi::emit_pop_reg(ctx.emitter, array_reg);                                 // restore the element slot address after the copy-on-write helper call
    abi::emit_store_to_address(ctx.emitter, result_reg, array_reg, 0);
}

/// Copies a loaded Mixed slot into a fresh zval cell, dereferencing ref-cell markers first.
fn emit_mixed_array_get_deref_invoker_ref_cell(
    ctx: &mut FunctionContext<'_>,
    mixed_reg: &str,
) {
    let ref_label = ctx.next_label("array_get_mixed_ref_cell");
    let done_label = ctx.next_label("array_get_mixed_done");
    let tag_reg = abi::secondary_scratch_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz {}, {}", mixed_reg, done_label)); // null gap cells read as PHP null and carry no tag word to inspect
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", mixed_reg, mixed_reg)); // null gap cells read as PHP null and carry no tag word to inspect
            ctx.emitter.instruction(&format!("jz {}", done_label));             // skip marker detection for null gap cells
        }
    }
    abi::emit_load_from_address(ctx.emitter, tag_reg, mixed_reg, 0);
    emit_branch_if_invoker_ref_cell_tag(ctx, tag_reg, &ref_label);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rsi, rdx");                                // adapt the unboxed high payload word to the boxing helper ABI
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&ref_label);
    emit_box_loaded_invoker_ref_cell_value_as_mixed(ctx, mixed_reg);
    ctx.emitter.label(&done_label);
}

/// Boxes the current value referenced by a loaded invoker ref-cell marker.
fn emit_box_loaded_invoker_ref_cell_value_as_mixed(
    ctx: &mut FunctionContext<'_>,
    mixed_reg: &str,
) {
    let ref_cell_reg = abi::symbol_scratch_reg(ctx.emitter);
    let tag_reg = abi::secondary_scratch_reg(ctx.emitter);
    let lo_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let hi_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x12",
        Arch::X86_64 => "rdx",
    };
    let string_hi_label = ctx.next_label("array_get_mixed_ref_string_hi");
    let mixed_cell_label = ctx.next_label("array_get_mixed_ref_cell");
    let box_label = ctx.next_label("array_get_mixed_ref_box");
    let done_label = ctx.next_label("array_get_mixed_ref_done");

    abi::emit_load_from_address(ctx.emitter, ref_cell_reg, mixed_reg, 8);
    abi::emit_load_from_address(ctx.emitter, tag_reg, mixed_reg, 16);
    abi::emit_load_from_address(ctx.emitter, lo_reg, ref_cell_reg, 0);
    abi::emit_load_int_immediate(ctx.emitter, hi_reg, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(
                &format!("cmp {}, #{}", tag_reg, runtime_value_tag(&PhpType::Mixed))
            );                                                                  // check whether the ref-cell stores a boxed Mixed handle
            ctx.emitter.instruction(&format!("b.eq {}", mixed_cell_label));     // retain and forward boxed Mixed values without reboxing their pointer
            ctx.emitter.instruction(&format!("cmp {}, #1", tag_reg));           // check whether the referenced value is a string slot
            ctx.emitter.instruction(&format!("b.eq {}", string_hi_label));      // load string length only for string ref-cells
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(
                &format!("cmp {}, {}", tag_reg, runtime_value_tag(&PhpType::Mixed))
            );                                                                  // check whether the ref-cell stores a boxed Mixed handle
            ctx.emitter.instruction(&format!("je {}", mixed_cell_label));       // retain and forward boxed Mixed values without reboxing their pointer
            ctx.emitter.instruction(&format!("cmp {}, 1", tag_reg));            // check whether the referenced value is a string slot
            ctx.emitter.instruction(&format!("je {}", string_hi_label));        // load string length only for string ref-cells
        }
    }
    abi::emit_jump(ctx.emitter, &box_label);

    ctx.emitter.label(&string_hi_label);
    abi::emit_load_from_address(ctx.emitter, hi_reg, ref_cell_reg, 8);

    ctx.emitter.label(&box_label);
    emit_box_runtime_payload_as_mixed(ctx.emitter, tag_reg, lo_reg, hi_reg);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&mixed_cell_label);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, result_reg, lo_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rsi, rdx");                                // adapt the unboxed high payload word to the boxing helper ABI
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");

    ctx.emitter.label(&done_label);
}

/// Branches when a loaded Mixed tag is an invoker ref-cell marker.
fn emit_branch_if_invoker_ref_cell_tag(
    ctx: &mut FunctionContext<'_>,
    tag_reg: &str,
    label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(
                &format!("cmp {}, #{}", tag_reg, INVOKER_ARG_REF_CELL_TAG)
            );                                                                  // check for a by-reference variadic marker
            ctx.emitter.instruction(&format!("b.eq {}", label));                // dereference marker slots instead of returning the marker
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(
                &format!("cmp {}, {}", tag_reg, INVOKER_ARG_REF_CELL_TAG)
            );                                                                  // check for a by-reference variadic marker
            ctx.emitter.instruction(&format!("je {}", label));                  // dereference marker slots instead of returning the marker
        }
    }
}

/// Emits PHP's undefined integer array-key warning for the key in the result register.
fn emit_undefined_array_key_warning(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_warn_undefined_array_key_int");
}

/// Emits PHP's warning for a direct array-offset read whose receiver is null.
pub(super) fn emit_array_offset_on_null_warning(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_warn_array_offset_on_null");
}

/// Emits the null/miss fallback in the result shape expected by the array element type.
///
/// `miss_reads_as_null` is true for the *silent* read variants — the ones `??`, `isset()` and
/// `empty()` lower to — where the caller goes on to ask whether the read produced PHP null.
/// Only those get the float null marker; a warned read keeps materializing `0.0` so a plain
/// `$a[$missing]` in value position renders as it always has. See `emit_float_null_sentinel`.
pub(super) fn emit_array_get_null_fallback(
    ctx: &mut FunctionContext<'_>,
    elem_ty: &PhpType,
    miss_reads_as_null: bool,
) {
    match elem_ty {
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
        }
        PhpType::Float if miss_reads_as_null => {
            crate::codegen::sentinels::emit_float_null_sentinel(ctx.emitter);
        }
        PhpType::Float => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("fmov d0, xzr");                        // materialize a stable zero float for an out-of-bounds array read
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xorpd xmm0, xmm0");                    // materialize a stable zero float for an out-of-bounds array read
            }
        },
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_int_immediate(
                ctx.emitter,
                ptr_reg,
                crate::codegen::NULL_SENTINEL,
            );
            abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        }
        PhpType::Mixed => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_load_int_immediate(ctx.emitter, "x0", 8);
                abi::emit_load_int_immediate(ctx.emitter, "x1", 0);
                abi::emit_load_int_immediate(ctx.emitter, "x2", 0);
                abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            }
            Arch::X86_64 => {
                abi::emit_load_int_immediate(ctx.emitter, "rax", 8);
                abi::emit_load_int_immediate(ctx.emitter, "rdi", 0);
                abi::emit_load_int_immediate(ctx.emitter, "rsi", 0);
                abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            }
        },
        _ => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
        }
    }
}

/// Lowers an indexed-array append for AArch64 targets.
fn lower_array_push_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?;
    if array_push_value_needs_mixed_unbox(elem_ty, &value_ty) {
        return lower_array_push_unboxed_mixed_aarch64(ctx, array, value, elem_ty);
    }
    if array_push_value_needs_mixed_box(elem_ty, &value_ty) {
        return lower_mixed_array_push_aarch64(ctx, array, value, &value_ty);
    }
    match value_ty {
        PhpType::TaggedScalar if elem_ty.codegen_repr() == PhpType::TaggedScalar => {
            lower_array_push_tagged_scalar_aarch64(ctx, array, value)?;
        }
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_reg(value, "x1")?;
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::TaggedScalar if elem_ty.codegen_repr() == PhpType::Int => {
            ctx.load_value_to_result(value)?;
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            ctx.emitter.instruction("mov x1, x0");                              // pass the nullable integer payload after PHP null-to-zero coercion
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Callable => {
            ctx.load_value_to_reg(value, "x0")?;
            abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
            ctx.emitter.instruction("mov x1, x0");                              // pass an array-owned callable descriptor to the append helper
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Float => {
            ctx.load_value_to_reg(value, "x1")?;
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "x1", "x2")?;
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the string append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_reg(value, "x1")?;
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the refcounted append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_push for PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Lowers an indexed-array append for x86_64 targets.
fn lower_array_push_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?;
    if array_push_value_needs_mixed_unbox(elem_ty, &value_ty) {
        return lower_array_push_unboxed_mixed_x86_64(ctx, array, value, elem_ty);
    }
    if array_push_value_needs_mixed_box(elem_ty, &value_ty) {
        return lower_mixed_array_push_x86_64(ctx, array, value, &value_ty);
    }
    match value_ty {
        PhpType::TaggedScalar if elem_ty.codegen_repr() == PhpType::TaggedScalar => {
            lower_array_push_tagged_scalar_x86_64(ctx, array, value)?;
        }
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_reg(array, "r11")?;
            ctx.load_value_to_reg(value, "rsi")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::TaggedScalar if elem_ty.codegen_repr() == PhpType::Int => {
            ctx.load_value_to_result(value)?;
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            ctx.emitter.instruction("mov rsi, rax");                            // pass the nullable integer payload after PHP null-to-zero coercion
            ctx.load_value_to_reg(array, "r11")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Callable => {
            ctx.load_value_to_reg(value, "rax")?;
            abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
            ctx.emitter.instruction("mov rsi, rax");                            // pass an array-owned callable descriptor to the append helper
            ctx.load_value_to_reg(array, "r11")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Float => {
            ctx.load_value_to_reg(array, "r11")?;
            ctx.load_value_to_reg(value, "rsi")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Str => {
            ctx.load_value_to_reg(array, "r11")?;
            ctx.load_string_value_to_regs(value, "rsi", "rdx")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the string append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_reg(array, "r11")?;
            ctx.load_value_to_reg(value, "rsi")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the refcounted append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_push for PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Returns true when an append into a Mixed array must box a concrete value first.
fn array_push_value_needs_mixed_box(elem_ty: &PhpType, value_ty: &PhpType) -> bool {
    matches!(elem_ty.codegen_repr(), PhpType::Mixed)
        && !matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
}

/// Returns true when a boxed Mixed value should be unboxed before a typed append.
fn array_push_value_needs_mixed_unbox(elem_ty: &PhpType, value_ty: &PhpType) -> bool {
    !matches!(elem_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        && matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
}

/// Appends an inline tagged scalar into a 16-byte tagged-scalar indexed array on AArch64.
fn lower_array_push_tagged_scalar_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
) -> Result<()> {
    let check_label = ctx.next_label("array_push_tagged_check");
    let grow_label = ctx.next_label("array_push_tagged_grow");
    let done_label = ctx.next_label("array_push_tagged_done");
    ctx.load_value_to_result(value)?;
    ctx.emitter.instruction("sub sp, sp, #32");                                 // reserve spill slots for the tagged payload and mutable array pointer
    ctx.emitter.instruction("str x0, [sp, #0]");                                // save the tagged-scalar payload across uniqueness and growth calls
    ctx.emitter.instruction("str x1, [sp, #8]");                                // save the tagged-scalar runtime tag across uniqueness and growth calls
    ctx.load_value_to_reg(array, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    ctx.emitter.instruction("str x0, [sp, #16]");                               // preserve the unique indexed-array pointer across the capacity check
    ctx.emitter.instruction("ldr x9, [x0]");                                    // load length before first-write tagged-scalar shape specialization
    ctx.emitter.instruction(&format!("cbnz x9, {}", check_label));              // existing arrays already have their tagged-scalar shape fixed
    ctx.emitter.instruction("mov x10, #16");                                    // tagged-scalar slots store payload and runtime tag words
    ctx.emitter.instruction("str x10, [x0, #16]");                              // elem_size = 16 before growth can copy tagged-scalar slots
    emit_tagged_scalar_array_value_type_stamp(ctx, "x0");
    ctx.emitter.label(&check_label);
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // reload the current indexed-array pointer before checking capacity
    ctx.emitter.instruction("ldr x9, [x0]");                                    // load the current logical length
    ctx.emitter.instruction("ldr x10, [x0, #8]");                               // load the current capacity
    ctx.emitter.instruction("cmp x9, x10");                                     // is the tagged-scalar array already full?
    ctx.emitter.instruction(&format!("b.ge {}", grow_label));                   // grow before writing when the append would exceed capacity
    ctx.emitter.instruction("lsl x10, x9, #4");                                 // convert length to a byte offset for 16-byte tagged-scalar slots
    ctx.emitter.instruction("add x10, x0, x10");                                // move to the selected append slot base
    ctx.emitter.instruction("add x10, x10, #24");                               // skip the indexed-array header before storing the slot
    ctx.emitter.instruction("ldr x11, [sp, #0]");                               // reload the tagged-scalar payload for the appended slot
    ctx.emitter.instruction("ldr x12, [sp, #8]");                               // reload the tagged-scalar runtime tag for the appended slot
    ctx.emitter.instruction("str x11, [x10]");                                  // store the tagged-scalar payload word in the append slot
    ctx.emitter.instruction("str x12, [x10, #8]");                              // store the tagged-scalar runtime tag word in the append slot
    ctx.emitter.instruction("add x9, x9, #1");                                  // advance the indexed-array logical length
    ctx.emitter.instruction("str x9, [x0]");                                    // publish the updated logical length
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the growth path after storing the tagged-scalar slot
    ctx.emitter.label(&grow_label);
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // reload the unique indexed-array pointer for growth
    abi::emit_call_label(ctx.emitter, "__rt_array_grow");
    ctx.emitter.instruction("str x0, [sp, #16]");                               // preserve the grown indexed-array pointer before retrying the append
    ctx.emitter.instruction(&format!("b {}", check_label));                     // retry the capacity check against the grown storage
    ctx.emitter.label(&done_label);
    ctx.emitter.instruction("add sp, sp, #32");                                 // release tagged-scalar append spill slots
    Ok(())
}

/// Appends an inline tagged scalar into a 16-byte tagged-scalar indexed array on x86_64.
fn lower_array_push_tagged_scalar_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
) -> Result<()> {
    let check_label = ctx.next_label("array_push_tagged_check");
    let grow_label = ctx.next_label("array_push_tagged_grow");
    let done_label = ctx.next_label("array_push_tagged_done");
    ctx.load_value_to_result(value)?;
    ctx.emitter.instruction("sub rsp, 32");                                     // reserve spill slots for the tagged payload and mutable array pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                        // save the tagged-scalar payload across uniqueness and growth calls
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                    // save the tagged-scalar runtime tag across uniqueness and growth calls
    ctx.load_value_to_reg(array, "rdi")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");                   // preserve the unique indexed-array pointer across the capacity check
    ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                        // load length before first-write tagged-scalar shape specialization
    ctx.emitter.instruction("test r10, r10");                                   // is this the first append into a tagged-scalar array?
    ctx.emitter.instruction(&format!("jnz {}", check_label));                   // existing arrays already have their tagged-scalar shape fixed
    ctx.emitter.instruction("mov QWORD PTR [rax + 16], 16");                    // elem_size = 16 before growth can copy tagged-scalar slots
    emit_tagged_scalar_array_value_type_stamp(ctx, "rax");
    ctx.emitter.label(&check_label);
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                   // reload the current indexed-array pointer before checking capacity
    ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                        // load the current logical length
    ctx.emitter.instruction("mov r11, QWORD PTR [rax + 8]");                    // load the current capacity
    ctx.emitter.instruction("cmp r10, r11");                                    // is the tagged-scalar array already full?
    ctx.emitter.instruction(&format!("jae {}", grow_label));                    // grow before writing when the append would exceed capacity
    ctx.emitter.instruction("mov rcx, r10");                                    // copy the logical length before scaling it into a byte offset
    ctx.emitter.instruction("shl rcx, 4");                                      // convert length to a byte offset for 16-byte tagged-scalar slots
    ctx.emitter.instruction("lea rcx, [rax + rcx + 24]");                       // compute the address of the next tagged-scalar append slot
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp]");                         // reload the tagged-scalar payload for the appended slot
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");                     // reload the tagged-scalar runtime tag for the appended slot
    ctx.emitter.instruction("mov QWORD PTR [rcx], r8");                         // store the tagged-scalar payload word in the append slot
    ctx.emitter.instruction("mov QWORD PTR [rcx + 8], r9");                     // store the tagged-scalar runtime tag word in the append slot
    ctx.emitter.instruction("add r10, 1");                                      // advance the indexed-array logical length
    ctx.emitter.instruction("mov QWORD PTR [rax], r10");                        // publish the updated logical length
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the growth path after storing the tagged-scalar slot
    ctx.emitter.label(&grow_label);
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the unique indexed-array pointer to the growth helper
    abi::emit_call_label(ctx.emitter, "__rt_array_grow");
    ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");                   // preserve the grown indexed-array pointer before retrying the append
    ctx.emitter.instruction(&format!("jmp {}", check_label));                   // retry the capacity check against the grown storage
    ctx.emitter.label(&done_label);
    ctx.emitter.instruction("add rsp, 32");                                     // release tagged-scalar append spill slots
    Ok(())
}

/// Appends an unboxed Mixed payload into a typed indexed array on AArch64.
fn lower_array_push_unboxed_mixed_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    ctx.load_value_to_reg(value, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match elem_ty.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float => {
            ctx.emitter.instruction("mov x11, x1");                             // keep the unboxed scalar payload while loading the array receiver
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x1, x11");                             // pass the unboxed scalar payload to the append helper
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Str => {
            ctx.emitter.instruction("mov x11, x1");                             // keep the unboxed string pointer while loading the array receiver
            ctx.emitter.instruction("mov x12, x2");                             // keep the unboxed string length while loading the array receiver
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x1, x11");                             // pass the unboxed string pointer to the string append helper
            ctx.emitter.instruction("mov x2, x12");                             // pass the unboxed string length to the string append helper
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the string append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("mov x11, x1");                             // keep the unboxed heap payload while loading the array receiver
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x1, x11");                             // pass the unboxed heap payload to the append helper
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the refcounted append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_push unboxed Mixed into PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Appends an unboxed Mixed payload into a typed indexed array on x86_64 targets.
fn lower_array_push_unboxed_mixed_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    ctx.load_value_to_reg(value, "rax")?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match elem_ty.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float => {
            ctx.emitter.instruction("mov r10, rdi");                            // keep the unboxed scalar payload while loading the array receiver
            ctx.load_value_to_reg(array, "r11")?;
            ctx.emitter.instruction("mov rsi, r10");                            // pass the unboxed scalar payload to the append helper
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Str => {
            ctx.emitter.instruction("mov r10, rdi");                            // keep the unboxed string pointer while loading the array receiver
            ctx.emitter.instruction("mov r9, rdx");                             // keep the unboxed string length while loading the array receiver
            ctx.load_value_to_reg(array, "r11")?;
            ctx.emitter.instruction("mov rsi, r10");                            // pass the unboxed string pointer to the string append helper
            ctx.emitter.instruction("mov rdx, r9");                             // pass the unboxed string length to the string append helper
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the string append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("mov r10, rdi");                            // keep the unboxed heap payload while loading the array receiver
            ctx.load_value_to_reg(array, "r11")?;
            ctx.emitter.instruction("mov rsi, r10");                            // pass the unboxed heap payload to the append helper
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the refcounted append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_push unboxed Mixed into PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Materializes the appended value as an owned boxed Mixed cell.
fn prepare_boxed_mixed_value_for_container(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        ctx.load_value_to_result(value)?;
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
    } else {
        box_value_for_mixed_container(ctx, value, &value_ty)?;
    }
    Ok(())
}

/// Boxes a concrete AArch64 value and appends the owned Mixed cell to a Mixed array.
fn lower_mixed_array_push_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    box_value_for_mixed_container(ctx, value, value_ty)?;
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.load_value_to_reg(array, "x9")?;
    ctx.emitter.instruction("mov x1, x0");                                      // pass the boxed Mixed payload to the refcounted append helper
    ctx.emitter.instruction("mov x0, x9");                                      // pass the indexed-array receiver to the refcounted append helper
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
    emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, &PhpType::Mixed);
    Ok(())
}

/// Boxes a concrete x86_64 value and appends the owned Mixed cell to a Mixed array.
fn lower_mixed_array_push_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    box_value_for_mixed_container(ctx, value, value_ty)?;
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.load_value_to_reg(array, "r11")?;
    ctx.emitter.instruction("mov rsi, rax");                                    // pass the boxed Mixed payload to the refcounted append helper
    ctx.emitter.instruction("mov rdi, r11");                                    // pass the indexed-array receiver to the refcounted append helper
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
    emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, &PhpType::Mixed);
    Ok(())
}

/// Boxes or retains a value, then stores it into a Mixed indexed array on AArch64.
fn lower_mixed_array_set_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    let value_ty = value_ty.codegen_repr();
    let fresh_boxed_value = !matches!(value_ty, PhpType::Mixed | PhpType::Union(_));
    if fresh_boxed_value {
        box_value_for_mixed_container(ctx, value, &value_ty)?;
    } else {
        ctx.load_value_to_result(value)?;
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
    }
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.load_value_to_reg(array, "x0")?;
    ctx.load_value_to_reg(index, "x1")?;
    abi::emit_pop_reg(ctx.emitter, "x2");
    if fresh_boxed_value {
        emit_mixed_array_set_ref_marker_writeback_aarch64(ctx);
        return Ok(());
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_set_mixed");
    Ok(())
}

/// Boxes or retains a value, then stores it into a Mixed indexed array on x86_64.
fn lower_mixed_array_set_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    let value_ty = value_ty.codegen_repr();
    let fresh_boxed_value = !matches!(value_ty, PhpType::Mixed | PhpType::Union(_));
    if fresh_boxed_value {
        box_value_for_mixed_container(ctx, value, &value_ty)?;
    } else {
        ctx.load_value_to_result(value)?;
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
    }
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.load_value_to_reg(array, "rdi")?;
    ctx.load_value_to_reg(index, "rsi")?;
    abi::emit_pop_reg(ctx.emitter, "rdx");
    if fresh_boxed_value {
        emit_mixed_array_set_ref_marker_writeback_x86_64(ctx);
        return Ok(());
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_set_mixed");
    Ok(())
}

/// Stores a fresh boxed-Mixed value through an invoker ref-cell marker on AArch64.
fn emit_mixed_array_set_ref_marker_writeback_aarch64(ctx: &mut FunctionContext<'_>) {
    let runtime_label = ctx.next_label("mixed_array_set_runtime");
    let mixed_cell_label = ctx.next_label("mixed_array_set_ref_mixed_cell");
    let done_label = ctx.next_label("mixed_array_set_done");

    ctx.emitter.instruction("cmp x1, #0");                                      // reject negative indexes before checking for by-reference markers
    ctx.emitter.instruction(&format!("b.lt {}", runtime_label));                // let the runtime setter drop ignored negative-index writes
    ctx.emitter.instruction("ldr x9, [x0]");                                    // load the current logical length of the indexed array
    ctx.emitter.instruction("cmp x1, x9");                                      // only existing slots can hold by-reference marker cells
    ctx.emitter.instruction(&format!("b.hs {}", runtime_label));                // delegate appends and gap writes to the runtime setter
    ctx.emitter.instruction("add x10, x0, #24");                                // compute the boxed-Mixed payload base for indexed slots
    ctx.emitter.instruction("ldr x11, [x10, x1, lsl #3]");                      // load the existing boxed Mixed slot
    ctx.emitter.instruction(&format!("cbz x11, {}", runtime_label));            // null gap slots are ordinary array writes
    ctx.emitter.instruction("ldr x12, [x11]");                                  // load the existing Mixed tag for marker detection
    ctx.emitter.instruction(&format!("cmp x12, #{}", INVOKER_ARG_REF_CELL_TAG));// check whether the slot aliases caller storage
    ctx.emitter.instruction(&format!("b.ne {}", runtime_label));                // ordinary boxed Mixed slots are replaced by the runtime setter
    ctx.emitter.instruction("ldr x12, [x11, #16]");                             // load the source runtime tag carried by the by-reference marker
    ctx.emitter.instruction("ldr x10, [x11, #8]");                              // load the caller ref-cell address from the marker payload
    ctx.emitter
        .instruction(
        &format!("cmp x12, #{}", runtime_value_tag(&PhpType::Mixed))
    );                                                                          // check whether the caller ref-cell stores a boxed Mixed handle
    ctx.emitter.instruction(&format!("b.eq {}", mixed_cell_label));             // transfer boxed Mixed replacements as handles rather than payload words
    ctx.emitter.instruction("ldr x12, [x2, #8]");                               // load the replacement Mixed low payload word
    ctx.emitter.instruction("str x12, [x10]");                                  // write the replacement low word through the caller ref-cell
    ctx.emitter.instruction("ldr x12, [x2, #16]");                              // load the replacement Mixed high payload word
    ctx.emitter.instruction("str x12, [x10, #8]");                              // write the replacement high word through the caller ref-cell
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // preserve the array result while freeing only the Mixed wrapper
    ctx.emitter.instruction("mov x0, x2");                                      // pass the consumed fresh Mixed wrapper to heap_free
    abi::emit_call_label(ctx.emitter, "__rt_heap_free");
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the array pointer as the ArraySet result
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the runtime setter after marker write-through

    ctx.emitter.label(&mixed_cell_label);
    ctx.emitter.instruction("str x2, [x10]");                                   // transfer the fresh boxed Mixed handle into the caller ref-cell
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the runtime setter after handle transfer

    ctx.emitter.label(&runtime_label);
    abi::emit_call_label(ctx.emitter, "__rt_array_set_mixed");
    ctx.emitter.label(&done_label);
}

/// Stores a fresh boxed-Mixed value through an invoker ref-cell marker on x86_64.
fn emit_mixed_array_set_ref_marker_writeback_x86_64(ctx: &mut FunctionContext<'_>) {
    let runtime_label = ctx.next_label("mixed_array_set_runtime");
    let mixed_cell_label = ctx.next_label("mixed_array_set_ref_mixed_cell");
    let done_label = ctx.next_label("mixed_array_set_done");

    ctx.emitter.instruction("cmp rsi, 0");                                      // reject negative indexes before checking for by-reference markers
    ctx.emitter.instruction(&format!("jl {}", runtime_label));                  // let the runtime setter drop ignored negative-index writes
    ctx.emitter.instruction("mov r9, QWORD PTR [rdi]");                         // load the current logical length of the indexed array
    ctx.emitter.instruction("cmp rsi, r9");                                     // only existing slots can hold by-reference marker cells
    ctx.emitter.instruction(&format!("jae {}", runtime_label));                 // delegate appends and gap writes to the runtime setter
    ctx.emitter.instruction("mov r10, QWORD PTR [rdi + 24 + rsi * 8]");         // load the existing boxed Mixed slot
    ctx.emitter.instruction("test r10, r10");                                   // check whether the existing slot is a null gap
    ctx.emitter.instruction(&format!("jz {}", runtime_label));                  // null gap slots are ordinary array writes
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load the existing Mixed tag for marker detection
    ctx.emitter.instruction(&format!("cmp r11, {}", INVOKER_ARG_REF_CELL_TAG)); // check whether the slot aliases caller storage
    ctx.emitter.instruction(&format!("jne {}", runtime_label));                 // ordinary boxed Mixed slots are replaced by the runtime setter
    ctx.emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                   // load the source runtime tag carried by the by-reference marker
    ctx.emitter.instruction("mov r10, QWORD PTR [r10 + 8]");                    // load the caller ref-cell address from the marker payload
    ctx.emitter
        .instruction(
        &format!("cmp r11, {}", runtime_value_tag(&PhpType::Mixed))
    );                                                                          // check whether the caller ref-cell stores a boxed Mixed handle
    ctx.emitter.instruction(&format!("je {}", mixed_cell_label));               // transfer boxed Mixed replacements as handles rather than payload words
    ctx.emitter.instruction("mov r11, QWORD PTR [rdx + 8]");                    // load the replacement Mixed low payload word
    ctx.emitter.instruction("mov QWORD PTR [r10], r11");                        // write the replacement low word through the caller ref-cell
    ctx.emitter.instruction("mov r11, QWORD PTR [rdx + 16]");                   // load the replacement Mixed high payload word
    ctx.emitter.instruction("mov QWORD PTR [r10 + 8], r11");                    // write the replacement high word through the caller ref-cell
    abi::emit_push_reg(ctx.emitter, "rdi");
    ctx.emitter.instruction("mov rax, rdx");                                    // pass the consumed fresh Mixed wrapper to heap_free
    abi::emit_call_label(ctx.emitter, "__rt_heap_free");
    abi::emit_pop_reg(ctx.emitter, "rax");
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the runtime setter after marker write-through

    ctx.emitter.label(&mixed_cell_label);
    ctx.emitter.instruction("mov QWORD PTR [r10], rdx");                        // transfer the fresh boxed Mixed handle into the caller ref-cell
    ctx.emitter.instruction("mov rax, rdi");                                    // return the unchanged indexed array after marker handle transfer
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the runtime setter after handle transfer

    ctx.emitter.label(&runtime_label);
    abi::emit_call_label(ctx.emitter, "__rt_array_set_mixed");
    ctx.emitter.label(&done_label);
}

/// Boxes a value for a Mixed array, consuming owned producers when possible.
fn box_value_for_mixed_container(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    ctx.load_value_to_result(value)?;
    if ctx.value_can_own_mixed_box_source(value)? {
        emit_box_current_owned_value_as_mixed(ctx.emitter, &value_ty.codegen_repr());
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty.codegen_repr());
    }
    Ok(())
}

/// Returns the PHP element type for an indexed-array operand.
fn indexed_array_element_type(array_ty: &PhpType, inst: &Instruction) -> Result<PhpType> {
    match array_ty {
        PhpType::Array(elem_ty) => Ok(elem_ty.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Resolves the runtime value type that an indexed array write can store.
fn effective_array_set_value_type(
    elem_ty: &PhpType,
    value_ty: &PhpType,
    inst: &Instruction,
) -> Result<PhpType> {
    let elem_ty = elem_ty.codegen_repr();
    let value_ty = value_ty.codegen_repr();
    if matches!(elem_ty, PhpType::Mixed) {
        return Ok(PhpType::Mixed);
    }
    if matches!(elem_ty, PhpType::Never | PhpType::Void) {
        return require_supported_array_set_value(value_ty, inst);
    }
    if elem_ty == value_ty {
        return require_supported_array_set_value(value_ty, inst);
    }
    Err(CodegenIrError::unsupported(format!(
        "array_set element PHP type {:?} with value PHP type {:?}",
        elem_ty, value_ty
    )))
}

/// Rejects indexed-array write payload types that do not have Phase 04 storage lowering yet.
fn require_supported_array_set_value(value_ty: PhpType, inst: &Instruction) -> Result<PhpType> {
    if matches!(
        value_ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Callable
            | PhpType::Float
            | PhpType::Str
            | PhpType::TaggedScalar
    ) {
        return Ok(value_ty);
    }
    if value_ty.is_refcounted() {
        return Ok(value_ty);
    }
    Err(CodegenIrError::unsupported(format!(
        "{} value PHP type {:?}",
        inst.op.name(),
        value_ty
    )))
}

/// Verifies that an indexed-array write uses an integer-like offset value.
fn require_integer_like_index(index_ty: PhpType, inst: &Instruction) -> Result<()> {
    if matches!(index_ty.codegen_repr(), PhpType::Int | PhpType::Bool | PhpType::Callable) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} index PHP type {:?}",
        inst.op.name(),
        index_ty
    )))
}

/// Rejects array-get result shapes that do not match the lowered array element type.
fn require_array_get_result(elem_ty: &PhpType, inst: &Instruction) -> Result<()> {
    let result_ty = inst.result_php_type.codegen_repr();
    if crate::codegen::sentinels::null_repr_is_tagged()
        && matches!(elem_ty, PhpType::Int)
        && result_ty == PhpType::TaggedScalar
    {
        return Ok(());
    }
    if matches!(elem_ty, PhpType::TaggedScalar) && result_ty == PhpType::TaggedScalar {
        return Ok(());
    }
    if matches!(elem_ty, PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float | PhpType::Str)
        && result_ty == *elem_ty
    {
        return Ok(());
    }
    if elem_ty.is_refcounted() && result_ty == *elem_ty {
        return Ok(());
    }
    if matches!(elem_ty, PhpType::Void | PhpType::Never)
        && matches!(result_ty, PhpType::Void | PhpType::Never)
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_get element PHP type {:?} with result PHP type {:?}",
        elem_ty, inst.result_php_type
    )))
}

/// Verifies that `array_to_mixed` produces an indexed array with boxed Mixed slots.
fn require_array_to_mixed_result(result_ty: &PhpType, inst: &Instruction) -> Result<()> {
    match result_ty {
        PhpType::Array(elem_ty) if elem_ty.codegen_repr() == PhpType::Mixed => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} result PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Verifies that `array_to_hash` produces hash-capable storage.
fn require_array_to_hash_result(result_ty: &PhpType, inst: &Instruction) -> Result<PhpType> {
    match result_ty {
        PhpType::AssocArray { value, .. } => Ok(value.codegen_repr()),
        PhpType::Array(value) if value.codegen_repr() == PhpType::Mixed => Ok(PhpType::Mixed),
        other => Err(CodegenIrError::unsupported(format!(
            "{} result PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Verifies that a cross-array union operand uses associative hash storage.
fn require_assoc_union_hash_operand(ty: PhpType, inst: &Instruction) -> Result<()> {
    if matches!(ty.codegen_repr(), PhpType::AssocArray { .. }) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} hash operand PHP type {:?}",
        inst.op.name(),
        ty
    )))
}

/// Converts a just-returned hash union result to boxed Mixed entries when required.
fn convert_hash_union_result_to_mixed_if_needed(
    ctx: &mut FunctionContext<'_>,
    result_value_ty: &PhpType,
) {
    if result_value_ty != &PhpType::Mixed {
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the hash result to the Mixed-entry conversion helper
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
        }
    }
}

/// Ensures indexed-array storage is unique and addressable on AArch64.
fn lower_array_elem_addr_prepare_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    elem_size: i64,
) -> Result<()> {
    let grow_check = ctx.next_label("array_elem_addr_grow_check");
    let ready = ctx.next_label("array_elem_addr_ready");
    let fill_loop = ctx.next_label("array_elem_addr_fill_loop");
    let store_len = ctx.next_label("array_elem_addr_store_len");
    let done = ctx.next_label("array_elem_addr_done");
    ctx.load_value_to_reg(index, "x1")?;
    ctx.emitter.instruction("cmp x1, #0");                                      // reject negative by-reference offsets by clamping to slot zero
    ctx.emitter.instruction("csel x1, xzr, x1, lt");                            // keep generated code memory-safe for unsupported negative offsets
    abi::emit_push_reg(ctx.emitter, "x1");
    ctx.load_value_to_reg(array, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    abi::emit_pop_reg(ctx.emitter, "x1");
    ctx.emitter.label(&grow_check);
    ctx.emitter.instruction("ldr x10, [x0, #8]");                               // load indexed-array capacity before exposing an element address
    ctx.emitter.instruction("cmp x1, x10");                                     // does the referenced slot fit in the current allocation?
    ctx.emitter.instruction(&format!("b.lo {}", ready));                        // skip growth once the slot is addressable
    abi::emit_push_reg(ctx.emitter, "x1");
    abi::emit_call_label(ctx.emitter, "__rt_array_grow");
    abi::emit_pop_reg(ctx.emitter, "x1");
    ctx.emitter.instruction(&format!("b {}", grow_check));                      // keep growing until the by-reference slot fits
    ctx.emitter.label(&ready);
    ctx.emitter.instruction("ldr x9, [x0]");                                    // load current logical length before filling missing by-reference slots
    ctx.emitter.instruction("cmp x1, x9");                                      // is the referenced slot already inside the logical array length?
    ctx.emitter.instruction(&format!("b.lo {}", done));                         // existing slots can be referenced without extending length
    ctx.emitter.instruction("mov x11, x9");                                     // start zero-filling at the previous logical end
    ctx.emitter.label(&fill_loop);
    ctx.emitter.instruction("cmp x11, x1");                                     // have all gap slots before the referenced slot been initialized?
    ctx.emitter.instruction(&format!("b.ge {}", store_len));                    // stop filling before the referenced slot
    emit_zero_array_slot_aarch64(ctx, elem_size, "x0", "x11")?;
    ctx.emitter.instruction("add x11, x11, #1");                                // advance to the next gap slot
    ctx.emitter.instruction(&format!("b {}", fill_loop));                       // continue zero-filling until the referenced slot
    ctx.emitter.label(&store_len);
    ctx.emitter.instruction("add x11, x1, #1");                                 // compute new logical length after materializing the reference slot
    ctx.emitter.instruction("str x11, [x0]");                                   // publish the extended indexed-array length
    ctx.emitter.label(&done);
    Ok(())
}

/// Ensures indexed-array storage is unique and addressable on x86_64.
fn lower_array_elem_addr_prepare_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    elem_size: i64,
) -> Result<()> {
    let grow_check = ctx.next_label("array_elem_addr_grow_check");
    let ready = ctx.next_label("array_elem_addr_ready");
    let fill_loop = ctx.next_label("array_elem_addr_fill_loop");
    let store_len = ctx.next_label("array_elem_addr_store_len");
    let done = ctx.next_label("array_elem_addr_done");
    ctx.load_value_to_reg(index, "rsi")?;
    ctx.emitter.instruction("xor r10, r10");                                    // prepare the safe fallback offset for unsupported negative indexes
    ctx.emitter.instruction("cmp rsi, 0");                                      // check whether the by-reference offset is negative
    ctx.emitter.instruction("cmovl rsi, r10");                                  // clamp negative offsets to slot zero to avoid invalid addresses
    abi::emit_push_reg(ctx.emitter, "rsi");
    ctx.load_value_to_reg(array, "rdi")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    abi::emit_pop_reg(ctx.emitter, "rsi");
    ctx.emitter.label(&grow_check);
    ctx.emitter.instruction("mov r10, QWORD PTR [rax + 8]");                    // load indexed-array capacity before exposing an element address
    ctx.emitter.instruction("cmp rsi, r10");                                    // does the referenced slot fit in the current allocation?
    ctx.emitter.instruction(&format!("jb {}", ready));                          // skip growth once the slot is addressable
    abi::emit_push_reg(ctx.emitter, "rsi");
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the current indexed-array pointer to the growth helper
    abi::emit_call_label(ctx.emitter, "__rt_array_grow");
    abi::emit_pop_reg(ctx.emitter, "rsi");
    ctx.emitter.instruction(&format!("jmp {}", grow_check));                    // keep growing until the by-reference slot fits
    ctx.emitter.label(&ready);
    ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                         // load current logical length before filling missing by-reference slots
    ctx.emitter.instruction("cmp rsi, r9");                                     // is the referenced slot already inside the logical array length?
    ctx.emitter.instruction(&format!("jb {}", done));                           // existing slots can be referenced without extending length
    ctx.emitter.instruction("mov r11, r9");                                     // start zero-filling at the previous logical end
    ctx.emitter.label(&fill_loop);
    ctx.emitter.instruction("cmp r11, rsi");                                    // have all gap slots before the referenced slot been initialized?
    ctx.emitter.instruction(&format!("jae {}", store_len));                     // stop filling before the referenced slot
    emit_zero_array_slot_x86_64(ctx, elem_size, "rax", "r11")?;
    ctx.emitter.instruction("add r11, 1");                                      // advance to the next gap slot
    ctx.emitter.instruction(&format!("jmp {}", fill_loop));                     // continue zero-filling until the referenced slot
    ctx.emitter.label(&store_len);
    ctx.emitter.instruction("lea r11, [rsi + 1]");                              // compute new logical length after materializing the reference slot
    ctx.emitter.instruction("mov QWORD PTR [rax], r11");                        // publish the extended indexed-array length
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits one zero-filled indexed-array slot on AArch64.
fn emit_zero_array_slot_aarch64(
    ctx: &mut FunctionContext<'_>,
    elem_size: i64,
    array_reg: &str,
    index_reg: &str,
) -> Result<()> {
    match elem_size {
        8 => {
            ctx.emitter.instruction(&format!("add x12, {}, #24", array_reg));   // compute the base address of pointer-sized indexed-array slots
            ctx.emitter
                .instruction(
                &format!("str xzr, [x12, {}, lsl #3]", index_reg)
            );                                                                  // initialize the missing by-reference slot to null
        }
        16 => {
            ctx.emitter.instruction(&format!("lsl x12, {}, #4", index_reg));    // scale the gap index by the two-word slot size
            ctx.emitter.instruction(&format!("add x12, {}, x12", array_reg));   // move to the selected two-word indexed-array slot
            ctx.emitter.instruction("add x12, x12, #24");                       // skip the indexed-array header before clearing the slot
            ctx.emitter.instruction("stp xzr, xzr, [x12]");                     // initialize both words of the missing by-reference slot
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_elem_addr element size {}",
                other
            )));
        }
    }
    Ok(())
}

/// Emits one zero-filled indexed-array slot on x86_64.
fn emit_zero_array_slot_x86_64(
    ctx: &mut FunctionContext<'_>,
    elem_size: i64,
    array_reg: &str,
    index_reg: &str,
) -> Result<()> {
    match elem_size {
        8 => {
            let clear_slot = format!(
                "mov QWORD PTR [{} + 24 + {} * 8], 0",
                array_reg, index_reg
            );
            ctx.emitter.instruction(&clear_slot);                               // initialize the missing by-reference slot to null
        }
        16 => {
            ctx.emitter.instruction(&format!("mov r12, {}", index_reg));        // copy the gap index before scaling for a two-word slot
            ctx.emitter.instruction("shl r12, 4");                              // scale the gap index by the two-word slot size
            ctx.emitter
                .instruction(
                &format!("mov QWORD PTR [{} + 24 + r12], 0", array_reg)
            );                                                                  // initialize the first word of the missing slot
            ctx.emitter
                .instruction(
                &format!("mov QWORD PTR [{} + 32 + r12], 0", array_reg)
            );                                                                  // initialize the second word of the missing slot
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_elem_addr element size {}",
                other
            )));
        }
    }
    Ok(())
}

/// Computes the final element-slot address on AArch64.
fn emit_array_elem_addr_result_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    elem_size: i64,
) -> Result<()> {
    // A spilled index beyond the unscaled frame range uses x9 as its address scratch.
    // Load it first so the array base is not replaced by the index's stack address.
    ctx.load_value_to_reg(index, "x10")?;
    ctx.load_value_to_reg(array, "x9")?;
    ctx.emitter.instruction("cmp x10, #0");                                     // keep negative by-reference offsets aligned with the materialized slot
    ctx.emitter.instruction("csel x10, xzr, x10, lt");                          // clamp unsupported negative offsets to the safe slot
    match elem_size {
        8 => {
            ctx.emitter.instruction("add x0, x9, #24");                         // compute the base address of pointer-sized indexed-array slots
            ctx.emitter.instruction("add x0, x0, x10, lsl #3");                 // return the selected by-reference element slot address
        }
        16 => {
            ctx.emitter.instruction("lsl x10, x10, #4");                        // scale the element index by the two-word slot size
            ctx.emitter.instruction("add x0, x9, #24");                         // compute the base address of two-word indexed-array slots
            ctx.emitter.instruction("add x0, x0, x10");                         // return the selected by-reference element slot address
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_elem_addr element size {}",
                other
            )));
        }
    }
    Ok(())
}

/// Computes the final element-slot address on x86_64.
fn emit_array_elem_addr_result_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    elem_size: i64,
) -> Result<()> {
    ctx.load_value_to_reg(array, "r10")?;
    ctx.load_value_to_reg(index, "r11")?;
    ctx.emitter.instruction("xor r12, r12");                                    // prepare the safe fallback offset for unsupported negative indexes
    ctx.emitter.instruction("cmp r11, 0");                                      // keep negative by-reference offsets aligned with the materialized slot
    ctx.emitter.instruction("cmovl r11, r12");                                  // clamp unsupported negative offsets to the safe slot
    match elem_size {
        8 => {
            ctx.emitter.instruction("lea rax, [r10 + 24 + r11 * 8]");           // return the selected pointer-sized by-reference slot address
        }
        16 => {
            ctx.emitter.instruction("shl r11, 4");                              // scale the element index by the two-word slot size
            ctx.emitter.instruction("lea rax, [r10 + 24 + r11]");               // return the selected two-word by-reference slot address
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_elem_addr element size {}",
                other
            )));
        }
    }
    Ok(())
}

/// Returns the local/ref-cell slot loaded by an array operand when it can be written back after growth.
fn source_load_local_slot(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<LocalSlotId>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if matches!(inst_ref.op, Op::LoadLocal | Op::LoadRefCell) {
        if let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate {
            return Ok(Some(slot));
        }
    }
    Ok(None)
}

/// Returns the runtime element-slot width for an indexed-array PHP type.
fn array_element_size(ty: &PhpType) -> Result<i64> {
    match ty {
        PhpType::Array(elem) => {
            if matches!(
                elem.codegen_repr(),
                PhpType::Str | PhpType::TaggedScalar | PhpType::Never
            ) {
                Ok(16)
            } else {
                Ok(8)
            }
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_new result PHP type {:?}",
            other
        ))),
    }
}

/// Stamps an indexed array as carrying inline tagged-scalar `{payload, tag}` slots.
fn emit_tagged_scalar_array_value_type_stamp(ctx: &mut FunctionContext<'_>, array_reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x10, [{}, #-8]", array_reg)); // load the packed indexed-array metadata before replacing value_type bits
            ctx.emitter.instruction("mov x11, #0x80ff");                        // preserve heap kind and persistent COW metadata only
            ctx.emitter.instruction("and x10, x10, x11");                       // clear stale indexed-array value_type bits
            ctx.emitter
                .instruction(
                &format!("mov x11, #{}", TAGGED_SCALAR_ARRAY_VALUE_TYPE)
            );                                                                  // value_type 11 = inline tagged-scalar slots
            ctx.emitter.instruction("lsl x11, x11, #8");                        // move the tagged-scalar value_type into the packed kind word
            ctx.emitter.instruction("orr x10, x10, x11");                       // combine stable metadata with the tagged-scalar value_type tag
            ctx.emitter.instruction(&format!("str x10, [{}, #-8]", array_reg)); // publish tagged-scalar indexed-array metadata
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(
                &format!("mov r10, QWORD PTR [{} - 8]", array_reg)
            );                                                                  // load the packed indexed-array metadata before replacing value_type bits
            ctx.emitter.instruction("mov r11, 0xffffffff000080ff");             // preserve heap marker, indexed-array kind, and persistent COW metadata
            ctx.emitter.instruction("and r10, r11");                            // clear stale indexed-array value_type bits
            ctx.emitter
                .instruction(
                &format!("or r10, 0x{:x}", TAGGED_SCALAR_ARRAY_VALUE_TYPE << 8)
            );                                                                  // add value_type 11 for inline tagged-scalar slots
            ctx.emitter
                .instruction(
                &format!("mov QWORD PTR [{} - 8], r10", array_reg)
            );                                                                  // publish tagged-scalar indexed-array metadata
        }
    }
}

/// Verifies that an array opcode receives an indexed array.
fn require_indexed_array(ty: PhpType, inst: &Instruction) -> Result<()> {
    if matches!(ty, PhpType::Array(_)) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        inst.op.name(),
        ty
    )))
}

/// Returns the capacity immediate attached to an array allocation.
fn expect_capacity(inst: &Instruction) -> Result<u32> {
    match inst.immediate {
        Some(Immediate::Capacity(capacity)) => Ok(capacity),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing capacity immediate",
            inst.op.name()
        ))),
    }
}

/// Lowers `Op::SlotDetach` — writes PHP null into `container[key]`, releasing whatever was there.
///
/// This is the nested-append lowering's ownership hand-off. After the bucket has been read into
/// the append temporary, it is owned twice (the container slot and the temporary), which would
/// make the upcoming push copy-on-write clone it. Nulling the slot drops the count back to one,
/// so the push mutates in place; the write-back that follows re-publishes the bucket into the
/// same slot. It cannot free the bucket: it only ever runs after the read has taken its
/// reference, so the count it decrements is at least two.
///
/// No new runtime helper and no new ABI knowledge: the two storage kinds go through the very
/// call sites `lower_hash_set` and `lower_array_set` already use, with a null payload
/// substituted for the value. `__rt_hash_set` may grow or rehash the table, and
/// `__rt_array_set_refcounted` may copy-on-write split the array, so both return a possibly-new
/// container pointer — which the receiver write-back below republishes.
pub(super) fn lower_slot_detach(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let container = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let container_ty = ctx.value_php_type(container)?.codegen_repr();
    let source_local = source_load_local_slot(ctx, container)?;
    if let Some(slot) = source_local {
        ctx.release_mutated_source_local_owner(slot, container)?;
    }
    match container_ty {
        PhpType::AssocArray { .. } => match ctx.emitter.target.arch {
            Arch::AArch64 => lower_slot_detach_hash_aarch64(ctx, container, key)?,
            Arch::X86_64 => lower_slot_detach_hash_x86_64(ctx, container, key)?,
        },
        PhpType::Array(_) => match ctx.emitter.target.arch {
            Arch::AArch64 => lower_slot_detach_indexed_aarch64(ctx, container, key)?,
            Arch::X86_64 => lower_slot_detach_indexed_x86_64(ctx, container, key)?,
        },
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "slot_detach container PHP type {:?}",
                other
            )));
        }
    }
    ctx.store_result_value(container)?;
    if let Some(slot) = source_local {
        ctx.store_container_writeback_to_local(slot, container)?;
    }
    ctx.writeback_global_array_source(container)?;
    Ok(())
}

/// Emits the AArch64 hash-storage slot detach: `__rt_hash_set(table, key, null)`.
///
/// Mirrors `lower_hash_set_aarch64`'s call sequence exactly, minus the value materialization:
/// PHP null is the `(value_lo = 0, value_hi = 0, value_tag = 8)` triple. The key is
/// materialized before the table is loaded because key normalization may call a helper, which
/// would clobber `x0`.
fn lower_slot_detach_hash_aarch64(
    ctx: &mut FunctionContext<'_>,
    hash: ValueId,
    key: ValueId,
) -> Result<()> {
    super::hashes::materialize_hash_key_aarch64(ctx, key)?;
    ctx.load_value_to_reg(hash, "x0")?;
    ctx.emitter.instruction("mov x3, xzr");                                     // value_lo = 0 (PHP null has no payload)
    ctx.emitter.instruction("mov x4, xzr");                                     // value_hi = 0
    abi::emit_load_int_immediate(ctx.emitter, "x5", 8);
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    Ok(())
}

/// Emits the x86_64 hash-storage slot detach: `__rt_hash_set(table, key, null)`.
///
/// `__rt_hash_set`'s SysV ABI is `rdi = table, rsi = key_lo, rdx = key_hi, rcx = value_lo,
/// r8 = value_hi, r9 = value_tag -> rax = table`.
fn lower_slot_detach_hash_x86_64(
    ctx: &mut FunctionContext<'_>,
    hash: ValueId,
    key: ValueId,
) -> Result<()> {
    super::hashes::materialize_hash_key_x86_64(ctx, key)?;
    ctx.load_value_to_reg(hash, "rdi")?;
    ctx.emitter.instruction("xor ecx, ecx");                                    // value_lo = 0 (PHP null has no payload)
    ctx.emitter.instruction("xor r8d, r8d");                                    // value_hi = 0
    abi::emit_load_int_immediate(ctx.emitter, "r9", 8);
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    Ok(())
}

/// Emits the AArch64 indexed-storage slot detach: `__rt_array_set_refcounted(array, index, 0)`.
///
/// A null payload makes the helper skip its element-type stamp and its retain (an incref of a
/// null pointer is a no-op), while still releasing the element it overwrites.
fn lower_slot_detach_indexed_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
) -> Result<()> {
    ctx.load_value_to_reg(array, "x0")?;
    ctx.load_value_to_reg(index, "x1")?;
    ctx.emitter.instruction("mov x2, xzr");                                     // payload = null: release the old element, store nothing
    abi::emit_call_label(ctx.emitter, "__rt_array_set_refcounted");
    Ok(())
}

/// Emits the x86_64 indexed-storage slot detach: `__rt_array_set_refcounted(array, index, 0)`.
fn lower_slot_detach_indexed_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
) -> Result<()> {
    ctx.load_value_to_reg(array, "rdi")?;
    ctx.load_value_to_reg(index, "rsi")?;
    ctx.emitter.instruction("xor edx, edx");                                    // payload = null: release the old element, store nothing
    abi::emit_call_label(ctx.emitter, "__rt_array_set_refcounted");
    Ok(())
}
