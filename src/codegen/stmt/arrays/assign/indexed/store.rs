use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::codegen::stmt::helpers;
use crate::types::PhpType;

use super::prepare::IndexedAssignState;
use super::super::ArrayAssignTarget;

pub(super) fn store_indexed_array_value(
    target: &ArrayAssignTarget<'_>,
    state: &IndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    if state.stores_refcounted_pointer {
        emitter.instruction("cmp x9, x11");                                        // check whether this write overwrites an existing slot from the original array
        let skip_release = ctx.next_label("array_assign_skip_release");
        emitter.instruction(&format!("b.hs {}", skip_release));                    // skip release for writes past current length
        emitter.instruction("stp x0, x9, [sp, #-16]!");                            // preserve new nested pointer and index across decref call
        emitter.instruction("str x10, [sp, #-16]!");                               // preserve array pointer across decref call
        emitter.instruction("add x12, x10, #24");                                  // compute base of array data region
        emitter.instruction("ldr x0, [x12, x9, lsl #3]");                          // load previous nested pointer from slot
        abi::emit_decref_if_refcounted(emitter, &target.elem_ty);
        emitter.instruction("ldr x10, [sp], #16");                                 // restore array pointer after decref
        emitter.instruction("ldp x0, x9, [sp], #16");                              // restore new nested pointer and index after decref
        emitter.label(&skip_release);
        helpers::stamp_indexed_array_value_type(emitter, "x10", &state.val_ty);
        emitter.instruction("add x12, x10, #24");                                  // compute base of array data region
        emitter.instruction("str x0, [x12, x9, lsl #3]");                          // store pointer at data[index]
        return;
    }

    match &state.effective_store_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            emitter.instruction("add x12, x10, #24");                              // compute base of the scalar data region without clobbering the array pointer
            emitter.instruction("str x0, [x12, x9, lsl #3]");                      // store int-like payload at data[index]
        }
        PhpType::Float => {
            emitter.instruction("fmov x12, d0");                                   // move float bits into an integer register for storage
            emitter.instruction("add x13, x10, #24");                              // skip 24-byte array header
            emitter.instruction("str x12, [x13, x9, lsl #3]");                     // store float bits at data[index]
        }
        PhpType::Str => {
            store_string_indexed_value(emitter, ctx, &state.val_ty);
        }
        _ => {}
    }
}

fn store_string_indexed_value(emitter: &mut Emitter, ctx: &mut Context, val_ty: &PhpType) {
    emitter.instruction("cmp x9, x11");                                            // check whether this write overwrites an existing string slot
    let skip_release = ctx.next_label("array_assign_skip_release");
    emitter.instruction(&format!("b.hs {}", skip_release));                        // skip release for writes past current length
    emitter.instruction("stp x1, x2, [sp, #-16]!");                                // preserve new string ptr/len across old-string release
    emitter.instruction("stp x9, x10, [sp, #-16]!");                               // preserve index and array pointer across old-string release
    emitter.instruction("lsl x12, x9, #4");                                        // multiply index by 16 for string slots
    emitter.instruction("add x12, x10, x12");                                      // offset into array data region
    emitter.instruction("add x12, x12, #24");                                      // skip 24-byte array header
    emitter.instruction("ldr x0, [x12]");                                          // load previous string pointer from slot
    emitter.instruction("bl __rt_heap_free_safe");                                 // release the overwritten string storage before replacing it
    emitter.instruction("ldp x9, x10, [sp], #16");                                 // restore index and array pointer after old-string release
    emitter.instruction("ldp x1, x2, [sp], #16");                                  // restore new string ptr/len after old-string release
    emitter.label(&skip_release);
    helpers::stamp_indexed_array_value_type(emitter, "x10", val_ty);
    emitter.instruction("lsl x12, x9, #4");                                        // multiply index by 16 without clobbering the logical index register
    emitter.instruction("add x12, x10, x12");                                      // offset into array data region without clobbering the array pointer
    emitter.instruction("add x12, x12, #24");                                      // skip 24-byte array header
    emitter.instruction("str x1, [x12]");                                          // store string pointer at slot
    emitter.instruction("str x2, [x12, #8]");                                      // store string length at slot+8
}
