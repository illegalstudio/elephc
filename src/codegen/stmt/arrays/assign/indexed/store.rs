//! Purpose:
//! Lowers typed element storage into indexed array payloads.
//! Works as one phase of the indexed array assignment pipeline.
//!
//! Called from:
//! - `crate::codegen::stmt::arrays::assign::indexed`
//!
//! Key details:
//! - Each phase depends on the prepared state and must preserve registers needed by later phases.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::stmt::helpers;
use crate::types::PhpType;

use super::prepare::IndexedAssignState;
use super::super::ArrayAssignTarget;

/// Phase 3: emits the element store into the indexed array slot at the computed index.
/// For refcounted payloads, emits a decref of the previous slot contents before storing.
/// Uses the array header's slot width and kind word to address data region slots correctly.
///
/// # Arguments
/// * `target` - the array being assigned into; provides base address, slot width, kind word, and element type
/// * `state` - prepared indexed assignment state from phase 1; must contain computed index in `state.index` and array pointer in `state.data_ptr`
/// * `emitter` - target-specific instruction emitter
/// * `ctx` - codegen context (labels, locals, types)
///
/// # Notes
/// - Reads `state.index` and `state.data_ptr` as the slot address and array base.
/// - For refcounted types, emits `__rt_heap_free_safe` to release the previous slot.
/// - Preserves registers needed by subsequent phases after `store_indexed_array_value`.
pub(super) fn store_indexed_array_value(
    target: &ArrayAssignTarget<'_>,
    state: &IndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    if emitter.target.arch == Arch::X86_64 {
        store_indexed_array_value_linux_x86_64(target, state, emitter, ctx);
        return;
    }

    if state.stores_refcounted_pointer {
        emitter.instruction("cmp x9, x11");                                     // check whether this write overwrites an existing slot from the original array
        let skip_release = ctx.next_label("array_assign_skip_release");
        emitter.instruction(&format!("b.hs {}", skip_release));                 // skip release for writes past current length
        emitter.instruction("stp x0, x9, [sp, #-16]!");                         // preserve new nested pointer and index across decref call
        emitter.instruction("str x10, [sp, #-16]!");                            // preserve array pointer across decref call
        emitter.instruction("add x12, x10, #24");                               // compute base of array data region
        emitter.instruction("ldr x0, [x12, x9, lsl #3]");                       // load previous nested pointer from slot
        let previous_slot_ty = previous_indexed_slot_type(target, state);
        abi::emit_decref_if_refcounted(emitter, &previous_slot_ty);
        emitter.instruction("ldr x10, [sp], #16");                              // restore array pointer after decref
        emitter.instruction("ldp x0, x9, [sp], #16");                           // restore new nested pointer and index after decref
        emitter.label(&skip_release);
        helpers::stamp_indexed_array_value_type(emitter, "x10", &state.val_ty);
        emitter.instruction("add x12, x10, #24");                               // compute base of array data region
        emitter.instruction("str x0, [x12, x9, lsl #3]");                       // store pointer at data[index]
        return;
    }

    match &state.effective_store_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            emitter.instruction("add x12, x10, #24");                           // compute base of the scalar data region without clobbering the array pointer
            emitter.instruction("str x0, [x12, x9, lsl #3]");                   // store int-like payload at data[index]
        }
        PhpType::Float => {
            emitter.instruction("fmov x12, d0");                                // move float bits into an integer register for storage
            emitter.instruction("add x13, x10, #24");                           // skip 24-byte array header
            emitter.instruction("str x12, [x13, x9, lsl #3]");                  // store float bits at data[index]
        }
        PhpType::Str => {
            store_string_indexed_value(emitter, ctx, &state.val_ty);
        }
        _ => {}
    }
}

/// Emits the store for a string element value on ARM64. Handles release of the previous
/// string slot when overwriting an existing element within the original logical length,
/// then writes the new string pointer (x1) and length (x2) into the 16-byte string slot.
///
/// # Arguments
/// * `emitter` - ARM64 instruction emitter
/// * `ctx` - codegen context (labels, locals, types)
/// * `val_ty` - PHP type of the string value being stored; must be `PhpType::String`
///
/// # Notes
/// - Expects the array base pointer in x10 and target index in x9.
/// - Uses `state.index` (x9) and `state.data_ptr` (x10) from prepared state.
/// - On macOS: calls `__rt_heap_free_safe` via ABI helper to release previous slot.
fn store_string_indexed_value(emitter: &mut Emitter, ctx: &mut Context, val_ty: &PhpType) {
    emitter.instruction("cmp x9, x11");                                         // check whether this write overwrites an existing string slot
    let skip_release = ctx.next_label("array_assign_skip_release");
    emitter.instruction(&format!("b.hs {}", skip_release));                     // skip release for writes past current length
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // preserve new string ptr/len across old-string release
    emitter.instruction("stp x9, x10, [sp, #-16]!");                            // preserve index and array pointer across old-string release
    emitter.instruction("lsl x12, x9, #4");                                     // multiply index by 16 for string slots
    emitter.instruction("add x12, x10, x12");                                   // offset into array data region
    emitter.instruction("add x12, x12, #24");                                   // skip 24-byte array header
    emitter.instruction("ldr x0, [x12]");                                       // load previous string pointer from slot
    emitter.instruction("bl __rt_heap_free_safe");                              // release the overwritten string storage before replacing it
    emitter.instruction("ldp x9, x10, [sp], #16");                              // restore index and array pointer after old-string release
    emitter.instruction("ldp x1, x2, [sp], #16");                               // restore new string ptr/len after old-string release
    emitter.label(&skip_release);
    helpers::stamp_indexed_array_value_type(emitter, "x10", val_ty);
    emitter.instruction("lsl x12, x9, #4");                                     // multiply index by 16 without clobbering the logical index register
    emitter.instruction("add x12, x10, x12");                                   // offset into array data region without clobbering the array pointer
    emitter.instruction("add x12, x12, #24");                                   // skip 24-byte array header
    emitter.instruction("str x1, [x12]");                                       // store string pointer at slot
    emitter.instruction("str x2, [x12, #8]");                                   // store string length at slot+8
}

/// x86_64/Linux-specific value storage: emits equivalent store logic using System V ABI
/// register conventions and Intel syntax.
///
/// # Arguments
/// * `target` - the array being assigned into; provides slot width, kind word, and element type
/// * `state` - prepared indexed assignment state; must contain computed index in r9 and array base in r10
/// * `emitter` - x86_64 instruction emitter
/// * `ctx` - codegen context (labels, locals, types)
fn store_indexed_array_value_linux_x86_64(
    target: &ArrayAssignTarget<'_>,
    state: &IndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    if state.stores_refcounted_pointer {
        emitter.instruction("cmp r9, r11");                                     // check whether this write overwrites an existing indexed-array slot from the original logical length
        let skip_release = ctx.next_label("array_assign_skip_release");
        emitter.instruction(&format!("jae {}", skip_release));                  // skip release work for writes that extend the indexed array past its original logical length
        abi::emit_push_reg(emitter, "rax");                                       // preserve the new nested pointer across the decref helper call
        abi::emit_push_reg(emitter, "r9");                                        // preserve the target index across the decref helper call
        abi::emit_push_reg(emitter, "r10");                                       // preserve the indexed-array pointer across the decref helper call
        emitter.instruction("mov rax, QWORD PTR [r10 + 24 + r9 * 8]");          // load the previous nested pointer from the overwritten indexed-array slot
        let previous_slot_ty = previous_indexed_slot_type(target, state);
        abi::emit_decref_if_refcounted(emitter, &previous_slot_ty);
        abi::emit_pop_reg(emitter, "r10");                                        // restore the indexed-array pointer after releasing the previous nested payload
        abi::emit_pop_reg(emitter, "r9");                                         // restore the target index after releasing the previous nested payload
        abi::emit_pop_reg(emitter, "rax");                                        // restore the new nested pointer after releasing the previous nested payload
        emitter.label(&skip_release);
        abi::emit_push_reg(emitter, "rax");                                       // preserve the new nested pointer across the indexed-array value_type stamp helper, which uses caller-saved scratch registers
        helpers::stamp_indexed_array_value_type(emitter, "r10", &state.val_ty);
        abi::emit_pop_reg(emitter, "rax");                                        // restore the new nested pointer after the indexed-array value_type stamp helper clobbers scratch registers
        emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], rax");          // store the new nested pointer in the indexed-array slot after any needed release
        return;
    }

    match &state.effective_store_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], rax");      // store the scalar payload directly into the addressed indexed-array slot
        }
        PhpType::Float => {
            emitter.instruction("movq r12, xmm0");                              // move the floating-point payload bits into an integer scratch register for indexed-array storage
            emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], r12");      // store the floating-point payload bits in the addressed indexed-array slot
        }
        PhpType::Str => {
            store_string_indexed_value_linux_x86_64(emitter, ctx, &state.val_ty);
        }
        _ => {}
    }
}

/// Returns the PHP type of the slot being overwritten. When the array has been converted
/// to mixed, all previous slots are typed as `Mixed`; otherwise uses `target.elem_ty`.
///
/// # Arguments
/// * `target` - the array being assigned into
///
/// # Returns
/// `PhpType::Mixed` if the array has been converted to mixed layout; otherwise `target.elem_ty`.
fn previous_indexed_slot_type(
    target: &ArrayAssignTarget<'_>,
    state: &IndexedAssignState,
) -> PhpType {
    if state.converted_to_mixed {
        PhpType::Mixed
    } else {
        target.elem_ty.clone()
    }
}

/// x86_64/Linux-specific string store: emits equivalent logic using System V ABI register
/// conventions and Intel syntax.
///
/// # Arguments
/// * `emitter` - x86_64 instruction emitter
/// * `ctx` - codegen context (labels, locals, types)
/// * `val_ty` - PHP type of the string value being stored; must be `PhpType::String`
///
/// # Notes
/// - Expects the array base pointer in r10 and target index in r9.
/// - Uses `state.index` (r9) and `state.data_ptr` (r10) from prepared state.
/// - Checks r9 against r11 (logical length) to decide whether to release previous slot.
/// - Uses System V ABI: string pointer in rax, string length in rdx.
fn store_string_indexed_value_linux_x86_64(
    emitter: &mut Emitter,
    ctx: &mut Context,
    val_ty: &PhpType,
) {
    emitter.instruction("cmp r9, r11");                                         // check whether this write overwrites an existing indexed-array string slot
    let skip_release = ctx.next_label("array_assign_skip_release");
    emitter.instruction(&format!("jae {}", skip_release));                      // skip release work for writes that extend the indexed array past its original logical length
    abi::emit_push_reg_pair(emitter, "rax", "rdx");                              // preserve the new string pointer and length across the old-string release helper call
    abi::emit_push_reg(emitter, "r9");                                           // preserve the target index across the old-string release helper call
    abi::emit_push_reg(emitter, "r10");                                          // preserve the indexed-array pointer across the old-string release helper call
    emitter.instruction("mov rcx, r9");                                         // copy the target index before scaling it into a 16-byte indexed-array string-slot offset
    emitter.instruction("shl rcx, 4");                                          // convert the target index into the byte offset of the overwritten string slot
    emitter.instruction("lea rcx, [r10 + rcx + 24]");                           // compute the address of the overwritten indexed-array string slot
    emitter.instruction("mov rax, QWORD PTR [rcx]");                            // load the previous string pointer from the overwritten indexed-array slot
    abi::emit_call_label(emitter, "__rt_heap_free_safe");                        // release the overwritten owned string storage before replacing the indexed-array slot
    abi::emit_pop_reg(emitter, "r10");                                           // restore the indexed-array pointer after releasing the previous string payload
    abi::emit_pop_reg(emitter, "r9");                                            // restore the target index after releasing the previous string payload
    abi::emit_pop_reg_pair(emitter, "rax", "rdx");                               // restore the new string pointer and length after releasing the previous string payload
    emitter.label(&skip_release);
    abi::emit_push_reg_pair(emitter, "rax", "rdx");                              // preserve the new string pointer and length across the indexed-array value_type stamp helper, which uses caller-saved scratch registers
    helpers::stamp_indexed_array_value_type(emitter, "r10", val_ty);
    abi::emit_pop_reg_pair(emitter, "rax", "rdx");                               // restore the new string pointer and length after the indexed-array value_type stamp helper clobbers scratch registers
    emitter.instruction("mov rcx, r9");                                         // copy the target index before scaling it into a 16-byte indexed-array string-slot offset
    emitter.instruction("shl rcx, 4");                                          // convert the target index into the byte offset of the destination string slot
    emitter.instruction("lea rcx, [r10 + rcx + 24]");                           // compute the address of the destination indexed-array string slot
    emitter.instruction("mov QWORD PTR [rcx], rax");                            // store the new string pointer in the indexed-array slot
    emitter.instruction("mov QWORD PTR [rcx + 8], rdx");                        // store the new string length in the indexed-array slot
}
