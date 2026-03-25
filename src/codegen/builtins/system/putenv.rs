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
    emitter.comment("putenv()");
    // -- evaluate the KEY=VALUE string --
    emit_expr(&args[0], emitter, ctx, data);
    // -- copy string to heap so it persists (putenv keeps the pointer) --
    emitter.instruction("add x0, x2, #1");                                     // heap size = string len + 1 (null terminator)
    emitter.instruction("stp x1, x2, [sp, #-16]!");                            // save string ptr/len
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate persistent buffer → x0=heap_ptr
    emitter.instruction("ldp x1, x2, [sp], #16");                              // restore string ptr/len
    emitter.instruction("mov x3, x0");                                         // save heap ptr in x3
    // -- copy bytes to heap --
    emitter.instruction("mov x4, #0");                                         // copy index = 0
    let copy_loop = ctx.next_label("putenv_copy");
    let copy_done = ctx.next_label("putenv_copy_done");
    emitter.label(&copy_loop);
    emitter.instruction("cmp x4, x2");                                         // compare index with length
    emitter.instruction(&format!("b.ge {}", copy_done));                       // done if index >= length
    emitter.instruction("ldrb w5, [x1, x4]");                                  // load byte from source
    emitter.instruction("strb w5, [x3, x4]");                                  // store byte to heap
    emitter.instruction("add x4, x4, #1");                                     // increment index
    emitter.instruction(&format!("b {}", copy_loop));                          // continue copying
    emitter.label(&copy_done);
    emitter.instruction("strb wzr, [x3, x4]");                                 // null-terminate on heap
    // -- call putenv with heap-allocated string --
    emitter.instruction("mov x0, x3");                                         // pass heap cstr to putenv
    emitter.instruction("bl _putenv");                                          // set env var, returns 0 on success
    // -- convert return value to bool (0=success → true=1) --
    emitter.instruction("cmp x0, #0");                                          // check if putenv returned 0 (success)
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if success, 0 if failure
    Some(PhpType::Bool)
}
