use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::codegen::stmt::helpers;
use crate::types::PhpType;

use super::prepare::IndexedAssignState;

pub(super) fn normalize_indexed_array_layout(
    state: &IndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let skip_normalize = ctx.next_label("array_assign_skip_normalize");
    emitter.instruction("cmp x11, #0");                                            // is this the first indexed write into the array?
    emitter.instruction(&format!("b.ne {}", skip_normalize));                      // keep the existing storage layout once the array already has elements
    match &state.effective_store_ty {
        PhpType::Str => {
            emitter.instruction("mov x12, #16");                                   // string arrays need 16-byte slots for ptr+len payloads
            emitter.instruction("str x12, [x10, #16]");                            // persist the string slot size in the array header
            helpers::stamp_indexed_array_value_type(emitter, "x10", &state.val_ty);
        }
        PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            emitter.instruction("mov x12, #8");                                    // nested heap pointers use ordinary 8-byte slots
            emitter.instruction("str x12, [x10, #16]");                            // persist the pointer-sized slot width in the array header
        }
        _ => {
            emitter.instruction("mov x12, #8");                                    // scalar indexed arrays use ordinary 8-byte slots
            emitter.instruction("str x12, [x10, #16]");                            // persist the scalar slot width in the array header
            emitter.instruction("ldr x12, [x10, #-8]");                            // load the packed array kind word from the heap header
            emitter.instruction("mov x14, #0x80ff");                               // preserve the indexed-array kind and persistent COW flag
            emitter.instruction("and x12, x12, x14");                              // clear stale value_type bits while keeping the persistent container metadata
            emitter.instruction("str x12, [x10, #-8]");                            // persist the scalar-oriented packed kind word
        }
    }
    emitter.label(&skip_normalize);
}
