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
    emitter.comment("array_keys()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- read source array length and allocate result array --
    emitter.instruction("ldr x9, [x0]");                                        // load source array length into x9
    emitter.instruction("str x9, [sp, #-16]!");                                 // push array length onto stack (for loop bound)
    emitter.instruction("mov x0, x9");                                          // pass length as capacity for new array
    emitter.instruction("mov x1, #8");                                          // element size = 8 bytes (integer keys)
    emitter.instruction("bl __rt_array_new");                                   // call runtime: allocate new array
    emitter.instruction("str x0, [sp, #-16]!");                                 // push new array pointer onto stack
    emitter.instruction("str xzr, [sp, #-16]!");                                // push loop counter (0) onto stack
    let loop_label = ctx.next_label("akeys_loop");
    let end_label = ctx.next_label("akeys_end");
    emitter.label(&loop_label);
    // -- loop: push each index as a key into result array --
    emitter.instruction("ldr x12, [sp]");                                       // load current loop counter from stack
    emitter.instruction("ldr x9, [sp, #32]");                                   // load array length from stack (2 slots above)
    emitter.instruction("cmp x12, x9");                                         // compare counter with array length
    emitter.instruction(&format!("b.ge {}", end_label));                        // exit loop if counter >= length
    emitter.instruction("ldr x0, [sp, #16]");                                   // load result array pointer from stack
    emitter.instruction("mov x1, x12");                                         // pass current index as value to push
    emitter.instruction("bl __rt_array_push_int");                              // call runtime: push index into result array
    emitter.instruction("ldr x12, [sp]");                                       // reload loop counter from stack
    emitter.instruction("add x12, x12, #1");                                    // increment loop counter
    emitter.instruction("str x12, [sp]");                                       // store updated counter back to stack
    emitter.instruction(&format!("b {}", loop_label));                          // jump back to loop start
    emitter.label(&end_label);
    // -- clean up stack and return result array --
    emitter.instruction("add sp, sp, #16");                                     // drop loop counter from stack
    emitter.instruction("ldr x0, [sp], #16");                                   // pop result array pointer into x0
    emitter.instruction("add sp, sp, #16");                                     // drop saved array length from stack

    Some(PhpType::Array(Box::new(PhpType::Int)))
}
