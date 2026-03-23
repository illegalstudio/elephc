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
    emitter.comment("fopen()");
    // -- evaluate filename (string) --
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // push filename ptr/len
    // -- evaluate mode (string) --
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("mov x3, x1");                                          // mode ptr → x3
    emitter.instruction("mov x4, x2");                                          // mode len → x4
    emitter.instruction("ldp x1, x2, [sp], #16");                               // pop filename ptr/len
    // -- call runtime to open file --
    emitter.instruction("bl __rt_fopen");                                       // open file: x1/x2=filename, x3/x4=mode → x0=fd
    Some(PhpType::Int)
}
