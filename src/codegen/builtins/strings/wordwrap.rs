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
    emitter.comment("wordwrap()");
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // push string
    // width (arg 2, default 75)
    if args.len() >= 2 {
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("mov x3, x0");                                      // width
    } else {
        emitter.instruction("mov x3, #75");                                     // default width
    }
    // break string (arg 3, default "\n")
    if args.len() >= 3 {
        emit_expr(&args[2], emitter, ctx, data);
        emitter.instruction("mov x4, x1");                                      // break ptr
        emitter.instruction("mov x5, x2");                                      // break len
    } else {
        let (label, len) = data.add_string(b"\n");
        emitter.adrp("x4", &format!("{}", label));               // load default break "\n"
        emitter.add_lo12("x4", "x4", &format!("{}", label));         // resolve address
        emitter.instruction(&format!("mov x5, #{}", len));                      // break length = 1
    }
    emitter.instruction("ldp x1, x2, [sp], #16");                               // pop input string
    emitter.instruction("bl __rt_wordwrap");                                    // call runtime: wrap text at word boundaries
    Some(PhpType::Str)
}
