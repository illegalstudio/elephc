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
    emitter.comment("substr()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- save string and evaluate offset --
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // push string ptr and length onto stack
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("str x0, [sp, #-16]!");                                 // push offset value onto stack
    if args.len() >= 3 {
        emit_expr(&args[2], emitter, ctx, data);
        emitter.instruction("mov x3, x0");                                      // move length argument to x3
    } else {
        emitter.instruction("mov x3, #-1");                                     // set sentinel -1: use all remaining characters
    }
    // -- restore offset and string from stack --
    emitter.instruction("ldr x0, [sp], #16");                                   // pop offset into x0
    emitter.instruction("ldp x1, x2, [sp], #16");                               // pop string ptr into x1, length into x2
    // -- handle negative offset --
    emitter.instruction("cmp x0, #0");                                          // check if offset is negative
    emitter.instruction("b.ge 1f");                                             // skip adjustment if offset >= 0
    emitter.instruction("add x0, x2, x0");                                      // convert negative offset: offset = length + offset
    emitter.instruction("cmp x0, #0");                                          // check if adjusted offset is still negative
    emitter.instruction("csel x0, xzr, x0, lt");                                // clamp to 0 if offset went below zero
    emitter.raw("1:");
    // -- clamp offset to string length --
    emitter.instruction("cmp x0, x2");                                          // compare offset to string length
    emitter.instruction("csel x0, x2, x0, gt");                                 // clamp offset to length if it exceeds it
    // -- adjust pointer and compute result length --
    emitter.instruction("add x1, x1, x0");                                      // advance string pointer by offset bytes
    emitter.instruction("sub x2, x2, x0");                                      // remaining = length - offset
    // -- apply optional length argument --
    emitter.instruction("cmn x3, #1");                                          // test if x3 == -1 (no length arg given)
    emitter.instruction("b.eq 2f");                                             // skip length clamping if no length arg
    emitter.instruction("cmp x3, #0");                                          // check if length arg is negative
    emitter.instruction("csel x3, xzr, x3, lt");                                // clamp negative length to 0
    emitter.instruction("cmp x3, x2");                                          // compare length arg to remaining chars
    emitter.instruction("csel x2, x3, x2, lt");                                 // result length = min(length arg, remaining)
    emitter.raw("2:");

    Some(PhpType::Str)
}
