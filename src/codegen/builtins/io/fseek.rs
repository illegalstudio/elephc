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
    emitter.comment("fseek()");
    // -- evaluate fd argument --
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("str x0, [sp, #-16]!");                                 // push fd onto stack
    // -- evaluate offset argument --
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("str x0, [sp, #-16]!");                                 // push offset onto stack
    // -- evaluate whence argument (default SEEK_SET=0) --
    if args.len() >= 3 {
        emit_expr(&args[2], emitter, ctx, data);
        emitter.instruction("mov x2, x0");                                      // whence → x2
    } else {
        emitter.instruction("mov x2, #0");                                      // default whence = SEEK_SET (0)
    }
    emitter.instruction("ldr x1, [sp], #16");                                   // pop offset → x1
    emitter.instruction("ldr x0, [sp], #16");                                   // pop fd → x0
    // -- invoke lseek syscall --
    emitter.instruction("mov x16, #199");                                       // syscall 199 = lseek
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel, returns new position in x0
    // -- map lseek result to PHP fseek convention: 0=success, -1=failure --
    emitter.instruction("cmp x0, #0");                                          // check if lseek returned an error (negative)
    emitter.instruction("cset x0, ge");                                         // x0 = 1 if >= 0 (success), 0 if < 0 (failure)
    emitter.instruction("sub x0, x0, #1");                                      // map: 1 → 0 (success), 0 → -1 (failure)
    Some(PhpType::Int)
}
