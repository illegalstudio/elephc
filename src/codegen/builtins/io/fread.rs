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
    emitter.comment("fread()");
    // -- evaluate fd argument --
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("str x0, [sp, #-16]!");                                 // push fd onto stack
    // -- evaluate length argument --
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("mov x1, x0");                                          // length → x1
    emitter.instruction("ldr x0, [sp], #16");                                   // pop fd → x0
    // -- call runtime to read from file --
    emitter.instruction("bl __rt_fread");                                       // read: x0=fd, x1=length → x1/x2=string
    Some(PhpType::Str)
}
