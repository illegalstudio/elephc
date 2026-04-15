use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::types::PhpType;

use super::prepare::IndexedAssignState;

pub(super) fn extend_indexed_array_if_needed(
    state: &IndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    if emitter.target.arch == Arch::X86_64 {
        extend_indexed_array_if_needed_linux_x86_64(state, emitter, ctx);
        return;
    }

    let skip_extend = ctx.next_label("array_assign_skip_extend");
    let extend_loop = ctx.next_label("array_assign_extend_loop");
    let extend_store_len = ctx.next_label("array_assign_store_len");
    emitter.instruction("cmp x9, x11");                                            // does this assignment extend the array beyond its original length?
    emitter.instruction(&format!("b.lo {}", skip_extend));                         // existing slots already keep the current array length
    emitter.instruction("mov x12, x11");                                           // start zero-filling at the previous logical end of the array
    emitter.label(&extend_loop);
    emitter.instruction("cmp x12, x9");                                            // have we filled every gap slot before the target index?
    emitter.instruction(&format!("b.ge {}", extend_store_len));                    // stop zero-filling once we reach the target index
    match &state.effective_store_ty {
        PhpType::Str => {
            emitter.instruction("lsl x13, x12, #4");                               // multiply the gap index by 16 for string slots
            emitter.instruction("add x13, x10, x13");                              // offset into the string data region
            emitter.instruction("add x13, x13, #24");                              // skip the 24-byte array header
            emitter.instruction("str xzr, [x13]");                                 // initialize the gap string pointer to null
            emitter.instruction("str xzr, [x13, #8]");                             // initialize the gap string length to zero
        }
        _ => {
            emitter.instruction("add x13, x10, #24");                              // compute the base of the pointer/scalar data region
            emitter.instruction("str xzr, [x13, x12, lsl #3]");                    // initialize the gap slot to zero/null
        }
    }
    emitter.instruction("add x12, x12, #1");                                       // advance to the next gap slot
    emitter.instruction(&format!("b {}", extend_loop));                            // continue zero-filling until the target index is reached
    emitter.label(&extend_store_len);
    emitter.instruction("add x12, x9, #1");                                        // new length = highest written index + 1
    emitter.instruction("str x12, [x10]");                                         // persist the extended logical length in the array header
    emitter.label(&skip_extend);
}

fn extend_indexed_array_if_needed_linux_x86_64(
    state: &IndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let skip_extend = ctx.next_label("array_assign_skip_extend");
    let extend_loop = ctx.next_label("array_assign_extend_loop");
    let extend_store_len = ctx.next_label("array_assign_store_len");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // reload the current indexed-array logical length because the preceding store path may clobber the caller-saved original-length register
    emitter.instruction("cmp r9, r11");                                         // does this indexed write extend the array beyond its original logical length?
    emitter.instruction(&format!("jb {}", skip_extend));                        // existing indexed-array slots already keep the current logical length
    emitter.instruction("mov r12, r11");                                        // start zero-filling at the previous logical end of the indexed array
    emitter.label(&extend_loop);
    emitter.instruction("cmp r12, r9");                                         // have we filled every indexed-array gap slot before the target index?
    emitter.instruction(&format!("jae {}", extend_store_len));                  // stop zero-filling once we reach the target indexed-array slot
    match &state.effective_store_ty {
        PhpType::Str => {
            emitter.instruction("mov r13, r12");                                // copy the gap index before scaling it into a 16-byte string-slot offset
            emitter.instruction("shl r13, 4");                                  // convert the gap index into the byte offset of the 16-byte string slot
            emitter.instruction("lea r13, [r10 + r13 + 24]");                   // compute the address of the indexed-array string gap slot
            emitter.instruction("mov QWORD PTR [r13], 0");                      // initialize the gap string pointer to null
            emitter.instruction("mov QWORD PTR [r13 + 8], 0");                  // initialize the gap string length to zero
        }
        _ => {
            emitter.instruction("mov QWORD PTR [r10 + 24 + r12 * 8], 0");       // initialize the scalar or pointer indexed-array gap slot to zero/null
        }
    }
    emitter.instruction("add r12, 1");                                          // advance to the next indexed-array gap slot that still needs zero-initialization
    emitter.instruction(&format!("jmp {}", extend_loop));                       // continue zero-filling until the target indexed-array slot is reached
    emitter.label(&extend_store_len);
    emitter.instruction("lea r12, [r9 + 1]");                                  // compute the new indexed-array logical length as the highest written index plus one
    emitter.instruction("mov QWORD PTR [r10], r12");                            // persist the extended indexed-array logical length in the array header
    emitter.label(&skip_extend);
}
