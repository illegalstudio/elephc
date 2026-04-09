use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::ArrayAssignTarget;

pub(super) fn emit_buffer_array_assign(
    target: &ArrayAssignTarget<'_>,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if target.is_ref {
        abi::load_at_offset(emitter, "x10", target.offset);                           // load ref slot that points at the buffer local
        emitter.instruction("ldr x10, [x10]");                                     // dereference the ref slot to get the buffer header pointer
    } else {
        abi::load_at_offset(emitter, "x10", target.offset);                           // load the buffer header pointer from the local slot
    }
    emitter.instruction("str x10, [sp, #-16]!");                                   // preserve the buffer pointer while evaluating the index
    emit_expr(index, emitter, ctx, data);
    emitter.instruction("str x0, [sp, #-16]!");                                    // preserve the computed element index across value evaluation
    let val_ty = emit_expr(value, emitter, ctx, data);
    match &val_ty {
        PhpType::Float => {
            emitter.instruction("str d0, [sp, #-16]!");                            // preserve the float payload across address computation
        }
        PhpType::Str => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                        // preserve unsupported string payload for consistent stack cleanup
        }
        _ => {
            emitter.instruction("str x0, [sp, #-16]!");                            // preserve scalar/pointer payload across address computation
        }
    }
    emitter.instruction("ldr x9, [sp, #16]");                                      // reload the target index without disturbing the saved value
    emitter.instruction("ldr x10, [sp, #32]");                                     // reload the buffer header pointer without disturbing the saved value
    let uaf_ok = ctx.next_label("buf_st_uaf_ok");
    emitter.instruction(&format!("cbnz x10, {}", uaf_ok));                         // skip fatal if buffer pointer is valid
    emitter.instruction("b __rt_buffer_use_after_free");                           // abort — buffer was freed
    emitter.label(&uaf_ok);
    let bounds_ok = ctx.next_label("buffer_store_ok");
    let oob_ok = ctx.next_label("buf_st_oob_ok");
    emitter.instruction("cmp x9, #0");                                             // reject negative buffer indexes
    emitter.instruction(&format!("b.ge {}", oob_ok));                              // skip fatal if index is non-negative
    emitter.instruction("b __rt_buffer_bounds_fail");                              // abort — negative index
    emitter.label(&oob_ok);
    emitter.instruction("ldr x11, [x10]");                                         // load the logical buffer length from the header
    emitter.instruction("cmp x9, x11");                                            // compare the target index against the logical length
    emitter.instruction(&format!("b.lo {}", bounds_ok));                           // continue once the write target is in bounds
    emitter.instruction("mov x0, x9");                                             // pass the out-of-bounds index to the runtime helper
    emitter.instruction("mov x1, x11");                                            // pass the logical buffer length to the runtime helper
    emitter.instruction("bl __rt_buffer_bounds_fail");                             // abort the program on invalid buffer writes
    emitter.label(&bounds_ok);
    emitter.instruction("ldr x12, [x10, #8]");                                     // load the element stride from the buffer header
    emitter.instruction("add x10, x10, #16");                                      // skip the buffer header to reach the payload base
    emitter.instruction("madd x10, x9, x12, x10");                                 // compute payload base + index*stride for the target element
    match &target.elem_ty {
        PhpType::Float => {
            emitter.instruction("ldr d0, [sp], #16");                              // restore the float payload before the direct store
            emitter.instruction("str d0, [x10]");                                  // store the float payload directly into the contiguous element slot
        }
        PhpType::Packed(_) => {
            emitter.comment("WARNING: packed buffer whole-element stores are not supported");
            emitter.instruction("add sp, sp, #16");                                // drop the preserved placeholder payload for unsupported packed stores
        }
        _ => {
            emitter.instruction("ldr x0, [sp], #16");                              // restore the scalar/pointer payload before the direct store
            emitter.instruction("str x0, [x10]");                                  // store the scalar/pointer payload directly into the contiguous element slot
        }
    }
    emitter.instruction("add sp, sp, #32");                                        // drop the preserved index and buffer pointer slots
}
