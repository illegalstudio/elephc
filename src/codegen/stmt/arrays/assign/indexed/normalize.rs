use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::stmt::helpers;
use crate::types::PhpType;

use super::prepare::IndexedAssignState;

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
