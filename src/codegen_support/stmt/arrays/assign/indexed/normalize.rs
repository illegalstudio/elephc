//! Purpose:
//! Lowers runtime layout normalization before indexed array element writes.
//! Works as one phase of the indexed array assignment pipeline.
//!
//! Called from:
//! - `crate::codegen_support::stmt::arrays::assign::indexed`
//!
//! Key details:
//! - Each phase depends on the prepared state and must preserve registers needed by later phases.

use crate::codegen_support::context::Context;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::stmt::helpers;
use crate::types::PhpType;

use super::prepare::IndexedAssignState;

/// Normalizes the array's runtime storage layout on the first indexed write (when length
/// is zero). Sets the slot width and packed kind word according to `effective_store_ty`
/// so subsequent operations use the correct data region offsets.
///
/// # Arguments
/// * `state` - prepared indexed assignment state from the prepare phase; carries
///   `effective_store_ty` and `val_ty` used to configure the runtime header
/// * `emitter` - target-specific instruction emitter
/// * `ctx` - codegen context (labels, locals, types)
pub(super) fn normalize_indexed_array_layout(
    state: &IndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    if emitter.target.arch == Arch::X86_64 {
        normalize_indexed_array_layout_linux_x86_64(state, emitter, ctx);
        return;
    }

    let skip_normalize = ctx.next_label("array_assign_skip_normalize");
    emitter.instruction("cmp x11, #0");                                         // is this the first indexed write into the array?
    emitter.instruction(&format!("b.ne {}", skip_normalize));                   // keep the existing storage layout once the array already has elements
    match &state.effective_store_ty {
        PhpType::Str => {
            emitter.instruction("mov x12, #16");                                // string arrays need 16-byte slots for ptr+len payloads
            emitter.instruction("str x12, [x10, #16]");                         // persist the string slot size in the array header
            helpers::stamp_indexed_array_value_type(emitter, "x10", &state.val_ty);
        }
        PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            emitter.instruction("mov x12, #8");                                 // nested heap pointers use ordinary 8-byte slots
            emitter.instruction("str x12, [x10, #16]");                         // persist the pointer-sized slot width in the array header
        }
        _ => {
            emitter.instruction("mov x12, #8");                                 // scalar indexed arrays use ordinary 8-byte slots
            emitter.instruction("str x12, [x10, #16]");                         // persist the scalar slot width in the array header
            emitter.instruction("ldr x12, [x10, #-8]");                         // load the packed array kind word from the heap header
            emitter.instruction("mov x14, #0x80ff");                            // preserve the indexed-array kind and persistent COW flag
            emitter.instruction("and x12, x12, x14");                           // clear stale value_type bits while keeping the persistent container metadata
            emitter.instruction("str x12, [x10, #-8]");                         // persist the scalar-oriented packed kind word
        }
    }
    emitter.label(&skip_normalize);
}

/// Emits runtime storage layout normalization for x86_64/Linux targets.
///
/// On the first indexed write (when the array length is zero), this function sets the
/// slot width and packed kind word in the array header so subsequent operations use the
/// correct data region offsets. The logic branches on `effective_store_ty`:
/// - `PhpType::Str`: 16-byte slots (pointer + length), string type stamped in header
/// - Heap pointer types (`Mixed`, `Array`, `AssocArray`, `Object`): 8-byte slots
/// - Scalar types: 8-byte slots, preserving the x86_64 heap marker and container kind bits
///
/// # Arguments
/// * `state` - prepared indexed assignment state from the prepare phase
/// * `emitter` - target-specific instruction emitter
/// * `ctx` - codegen context (labels, locals, types)
///
/// # Register usage
/// - `r10`: heap pointer to indexed-array header
/// - `r11`: array length (checked against zero)
/// - `r12`: slot width temporaries
/// - `r14`: packed kind word copy for bit manipulation
fn normalize_indexed_array_layout_linux_x86_64(
    state: &IndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let skip_normalize = ctx.next_label("array_assign_skip_normalize");
    emitter.instruction("cmp r11, 0");                                          // is this the first indexed write into the array?
    emitter.instruction(&format!("jne {}", skip_normalize));                    // keep the existing storage layout once the indexed array already has elements
    match &state.effective_store_ty {
        PhpType::Str => {
            emitter.instruction("mov r12, 16");                                 // string indexed arrays need 16-byte slots for pointer-plus-length payloads
            emitter.instruction("mov QWORD PTR [r10 + 16], r12");               // persist the string-slot width in the indexed-array header
            helpers::stamp_indexed_array_value_type(emitter, "r10", &state.val_ty);
        }
        PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            emitter.instruction("mov r12, 8");                                  // nested heap pointers still use ordinary 8-byte indexed-array slots
            emitter.instruction("mov QWORD PTR [r10 + 16], r12");               // persist the pointer-sized slot width in the indexed-array header
        }
        _ => {
            emitter.instruction("mov r12, 8");                                  // scalar indexed arrays use ordinary 8-byte slots
            emitter.instruction("mov QWORD PTR [r10 + 16], r12");               // persist the scalar slot width in the indexed-array header
            emitter.instruction("mov r12, QWORD PTR [r10 - 8]");                // load the packed indexed-array kind word from the heap header
            emitter.instruction("mov r14, r12");                                // copy the packed indexed-array kind word so the x86_64 heap marker and low container bits can be preserved independently
            emitter.instruction("and r12, 0x80ff");                             // keep the low indexed-array kind and persistent copy-on-write flag bits while clearing stale value_type bits
            emitter.instruction("and r14, -65536");                             // keep the high x86_64 heap-marker bits while clearing the low container-kind payload lane
            emitter.instruction("or r12, r14");                                 // combine the preserved x86_64 heap marker bits with the stable scalar container metadata
            emitter.instruction("mov QWORD PTR [r10 - 8], r12");                // persist the scalar-oriented packed kind word back into the heap header
        }
    }
    emitter.label(&skip_normalize);
}
