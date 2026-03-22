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
    emitter.comment("str_pad()");
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // push input string
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("str x0, [sp, #-16]!");                                 // push target length
    // pad_string (arg 3, default " ")
    if args.len() >= 3 {
        emit_expr(&args[2], emitter, ctx, data);
        emitter.instruction("stp x1, x2, [sp, #-16]!");                         // push pad string
    } else {
        let (label, len) = data.add_string(b" ");
        emitter.instruction(&format!("adrp x1, {}@PAGE", label));               // load default pad string " "
        emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", label));         // resolve address
        emitter.instruction(&format!("mov x2, #{}", len));                      // pad string length = 1
        emitter.instruction("stp x1, x2, [sp, #-16]!");                         // push pad string
    }
    // pad_type (arg 4, default 1 = STR_PAD_RIGHT)
    if args.len() >= 4 {
        emit_expr(&args[3], emitter, ctx, data);
        emitter.instruction("mov x7, x0");                                      // pad type
    } else {
        emitter.instruction("mov x7, #1");                                      // STR_PAD_RIGHT
    }
    emitter.instruction("ldp x3, x4, [sp], #16");                               // pop pad string
    emitter.instruction("ldr x5, [sp], #16");                                   // pop target length
    emitter.instruction("ldp x1, x2, [sp], #16");                               // pop input string
    // x1/x2=input, x3/x4=pad_str, x5=target_len, x7=pad_type
    emitter.instruction("bl __rt_str_pad");                                     // call runtime: pad string
    Some(PhpType::Str)
}
