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
    emitter.comment("ltrim()");

    if args.len() == 1 {
        emit_expr(&args[0], emitter, ctx, data);
        // -- strip whitespace from the left --
        emitter.instruction("bl __rt_ltrim");                                   // call runtime: trim whitespace from start of string
    } else {
        // -- ltrim with character mask --
        emit_expr(&args[0], emitter, ctx, data);
        emitter.instruction("str x1, [sp, #-16]!");                             // push string pointer onto stack
        emitter.instruction("str x2, [sp, #-16]!");                             // push string length onto stack
        emit_expr(&args[1], emitter, ctx, data);
        // -- mask string is in x1/x2, recover source string --
        emitter.instruction("mov x3, x1");                                      // move mask pointer to x3
        emitter.instruction("mov x4, x2");                                      // move mask length to x4
        emitter.instruction("ldr x2, [sp], #16");                               // pop source string length into x2
        emitter.instruction("ldr x1, [sp], #16");                               // pop source string pointer into x1
        emitter.instruction("bl __rt_ltrim_mask");                              // call runtime: trim mask chars from start
    }

    Some(PhpType::Str)
}
