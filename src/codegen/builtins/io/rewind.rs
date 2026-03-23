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
    emitter.comment("rewind()");
    // -- evaluate fd argument --
    emit_expr(&args[0], emitter, ctx, data);
    // -- call lseek(fd, 0, SEEK_SET) to reset position to beginning --
    emitter.instruction("mov x1, #0");                                          // offset = 0
    emitter.instruction("mov x2, #0");                                          // whence = SEEK_SET (0)
    emitter.instruction("mov x16, #199");                                       // syscall 199 = lseek
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel
    emitter.instruction("mov x0, #1");                                          // always return true
    Some(PhpType::Bool)
}
