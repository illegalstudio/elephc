use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::types::PhpType;

use super::prepare::IndexedAssignState;

pub(super) fn extend_indexed_array_if_needed(
    state: &IndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
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
