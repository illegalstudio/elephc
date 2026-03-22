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
    emitter.comment("in_array()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- save needle, evaluate array --
    emitter.instruction("str x0, [sp, #-16]!");                         // push needle value onto stack
    emit_expr(&args[1], emitter, ctx, data);
    let found_label = ctx.next_label("in_array_found");
    let end_label = ctx.next_label("in_array_end");
    let done_label = ctx.next_label("in_array_done");
    // -- set up loop to search array for needle --
    emitter.instruction("ldr x9, [x0]");                                // load array length into x9
    emitter.instruction("add x10, x0, #24");                            // x10 = pointer to array data (past 24-byte header)
    emitter.instruction("ldr x11, [sp], #16");                          // pop needle value into x11
    emitter.instruction("mov x12, #0");                                 // initialize loop counter to 0
    let loop_label = ctx.next_label("in_array_loop");
    emitter.label(&loop_label);
    // -- compare each element against needle --
    emitter.instruction("cmp x12, x9");                                 // check if counter reached array length
    emitter.instruction(&format!("b.ge {}", end_label));                // exit loop if all elements checked
    emitter.instruction("ldr x13, [x10, x12, lsl #3]");                 // load element at index x12 (offset = x12 * 8)
    emitter.instruction("cmp x13, x11");                                // compare element with needle
    emitter.instruction(&format!("b.eq {}", found_label));              // branch to found if element matches
    emitter.instruction("add x12, x12, #1");                            // increment loop counter
    emitter.instruction(&format!("b {}", loop_label));                  // jump back to loop start
    // -- needle found --
    emitter.label(&found_label);
    emitter.instruction("mov x0, #1");                                  // set return value to 1 (true)
    emitter.instruction(&format!("b {}", done_label));                  // jump to done
    // -- needle not found --
    emitter.label(&end_label);
    emitter.instruction("mov x0, #0");                                  // set return value to 0 (false)
    emitter.label(&done_label);

    Some(PhpType::Int)
}
