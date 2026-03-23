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
    emitter.comment("strstr()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- save haystack, evaluate needle --
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // push haystack ptr and length onto stack
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("mov x3, x1");                                          // move needle pointer to x3
    emitter.instruction("mov x4, x2");                                          // move needle length to x4
    emitter.instruction("ldp x1, x2, [sp], #16");                               // pop haystack ptr into x1, length into x2
    // -- find needle position in haystack --
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // push haystack again (needed after strpos call)
    emitter.instruction("bl __rt_strpos");                                      // call runtime: find needle position in haystack
    emitter.instruction("ldp x1, x2, [sp], #16");                               // pop saved haystack ptr and length
    // -- return substring from match position, or empty if not found --
    let found = ctx.next_label("strstr_found");
    emitter.instruction("cmp x0, #0");                                          // check if strpos returned a valid position
    emitter.instruction(&format!("b.ge {}", found));                            // branch to found if position >= 0
    emitter.instruction("mov x2, #0");                                          // set length to 0 (return empty string)
    let end = ctx.next_label("strstr_end");
    emitter.instruction(&format!("b {}", end));                                 // jump to end, skipping found logic
    emitter.label(&found);
    emitter.instruction("add x1, x1, x0");                                      // advance haystack ptr to match position
    emitter.instruction("sub x2, x2, x0");                                      // result length = haystack length - position
    emitter.label(&end);

    Some(PhpType::Str)
}
