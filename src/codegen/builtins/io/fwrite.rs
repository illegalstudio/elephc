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
    emitter.comment("fwrite()");
    // -- evaluate fd argument --
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("str x0, [sp, #-16]!");                                 // push fd onto stack
    // -- evaluate data argument (string) --
    emit_expr(&args[1], emitter, ctx, data);
    // x1=data ptr, x2=data len after emit_expr
    emitter.instruction("ldr x0, [sp], #16");                                   // pop fd → x0
    // -- invoke write syscall --
    emitter.instruction("mov x16, #4");                                         // syscall 4 = write
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel, returns bytes written in x0
    Some(PhpType::Int)
}
