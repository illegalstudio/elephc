use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_pop()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let elem_ty = match &arr_ty {
        PhpType::Array(t) => *t.clone(),
        _ => PhpType::Int,
    };

    let empty_label = ctx.next_label("array_pop_empty");
    let end_label = ctx.next_label("array_pop_end");

    // -- check if array is empty --
    emitter.instruction("ldr x9, [x0]");                                        // load current array length into x9
    emitter.instruction(&format!("cbz x9, {}", empty_label));                   // if length == 0, jump to empty handler

    // -- decrement array length to remove last element --
    emitter.instruction("sub x9, x9, #1");                                      // decrement length by 1
    emitter.instruction("str x9, [x0]");                                        // store decremented length back to array header
    match &elem_ty {
        PhpType::Int => {
            // -- load the popped integer element --
            emitter.instruction("add x0, x0, #24");                             // advance past array header (24 bytes) to data area
            emitter.instruction("ldr x0, [x0, x9, lsl #3]");                    // load int at index x9 (offset = x9 * 8 bytes)
        }
        PhpType::Str => {
            // -- load the popped string element (ptr + len) --
            emitter.instruction("lsl x10, x9, #4");                             // multiply index by 16 (each string entry = 16 bytes)
            emitter.instruction("add x0, x0, x10");                             // advance pointer by element offset
            emitter.instruction("add x0, x0, #24");                             // skip past array header to data area
            emitter.instruction("ldr x1, [x0]");                                // load string pointer from element
            emitter.instruction("ldr x2, [x0, #8]");                            // load string length from element + 8
        }
        _ => {}
    }
    emitter.instruction(&format!("b {}", end_label));                           // skip empty handler

    // -- empty array: return null sentinel --
    emitter.label(&empty_label);
    emitter.instruction("movz x0, #0xFFFE");                                    // load null sentinel bits [15:0]
    emitter.instruction("movk x0, #0xFFFF, lsl #16");                           // load null sentinel bits [31:16]
    emitter.instruction("movk x0, #0xFFFF, lsl #32");                           // load null sentinel bits [47:32]
    emitter.instruction("movk x0, #0x7FFF, lsl #48");                           // load null sentinel bits [63:48] = 0x7FFFFFFFFFFFFFFE

    emitter.label(&end_label);

    Some(elem_ty)
}
