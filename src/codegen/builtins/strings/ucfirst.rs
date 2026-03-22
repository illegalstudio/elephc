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
    emitter.comment("ucfirst()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- copy string then uppercase the first character --
    emitter.instruction("bl __rt_strcopy");                             // call runtime: copy string to mutable buffer
    emitter.instruction("cbz x2, 1f");                                  // skip if string is empty (length == 0)
    emitter.instruction("ldrb w9, [x1]");                               // load first byte of copied string
    emitter.instruction("cmp w9, #97");                                 // compare with ASCII 'a' (start of lowercase range)
    emitter.instruction("b.lt 1f");                                     // skip if char < 'a' (not lowercase)
    emitter.instruction("cmp w9, #122");                                // compare with ASCII 'z' (end of lowercase range)
    emitter.instruction("b.gt 1f");                                     // skip if char > 'z' (not lowercase)
    emitter.instruction("sub w9, w9, #32");                             // convert to uppercase by subtracting 32
    emitter.instruction("strb w9, [x1]");                               // store uppercased byte back to string
    emitter.raw("1:");

    Some(PhpType::Str)
}
