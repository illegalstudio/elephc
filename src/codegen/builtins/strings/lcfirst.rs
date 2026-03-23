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
    emitter.comment("lcfirst()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- copy string then lowercase the first character --
    emitter.instruction("bl __rt_strcopy");                                     // call runtime: copy string to mutable buffer
    emitter.instruction("cbz x2, 1f");                                          // skip if string is empty (length == 0)
    emitter.instruction("ldrb w9, [x1]");                                       // load first byte of copied string
    emitter.instruction("cmp w9, #65");                                         // compare with ASCII 'A' (start of uppercase range)
    emitter.instruction("b.lt 1f");                                             // skip if char < 'A' (not uppercase)
    emitter.instruction("cmp w9, #90");                                         // compare with ASCII 'Z' (end of uppercase range)
    emitter.instruction("b.gt 1f");                                             // skip if char > 'Z' (not uppercase)
    emitter.instruction("add w9, w9, #32");                                     // convert to lowercase by adding 32
    emitter.instruction("strb w9, [x1]");                                       // store lowercased byte back to string
    emitter.raw("1:");

    Some(PhpType::Str)
}
