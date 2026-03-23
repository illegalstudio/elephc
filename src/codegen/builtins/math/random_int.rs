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
    emitter.comment("random_int()");
    // -- random_int(min, max): cryptographically secure random in [min, max] --
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("str x0, [sp, #-16]!");                                 // push min value onto stack
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("ldr x9, [sp], #16");                                   // pop min value into x9
    emitter.instruction("sub x0, x0, x9");                                      // x0 = max - min
    emitter.instruction("add x0, x0, #1");                                      // x0 = range size (max - min + 1)
    emitter.instruction("str x9, [sp, #-16]!");                                 // push min back for later use
    emitter.instruction("mov w0, w0");                                          // zero-extend w0 to x0 (32-bit arg)
    emitter.instruction("bl _arc4random_uniform");                              // call arc4random_uniform(range) -> [0,range)
    emitter.instruction("ldr x9, [sp], #16");                                   // pop min value back into x9
    emitter.instruction("add x0, x0, x9");                                      // x0 = random + min (shift into range)
    Some(PhpType::Int)
}
