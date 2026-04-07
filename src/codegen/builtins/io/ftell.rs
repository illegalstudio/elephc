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
    emitter.comment("ftell()");
    // -- evaluate fd argument --
    emit_expr(&args[0], emitter, ctx, data);
    // -- call lseek(fd, 0, SEEK_CUR) to get current position --
    emitter.instruction("mov x1, #0");                                          // offset = 0
    emitter.instruction("mov x2, #1");                                          // whence = SEEK_CUR (1)
    emitter.syscall(199);
    Some(PhpType::Int)
}
