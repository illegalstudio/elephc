use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("unset()");
    if let crate::parser::ast::ExprKind::Variable(name) = &args[0].kind {
        let var = ctx.variables.get(name).expect("undefined variable");
        let offset = var.stack_offset;
        // -- set variable to null sentinel value (0x7FFFFFFFFFFFFFFFE) --
        emitter.instruction("movz x0, #0xFFFE");                                // load null sentinel bits [15:0]
        emitter.instruction("movk x0, #0xFFFF, lsl #16");                       // load null sentinel bits [31:16]
        emitter.instruction("movk x0, #0xFFFF, lsl #32");                       // load null sentinel bits [47:32]
        emitter.instruction("movk x0, #0x7FFF, lsl #48");                       // load null sentinel bits [63:48]
        emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));          // store null sentinel to variable's stack slot
        ctx.variables.get_mut(name).unwrap().ty = PhpType::Void;
    }
    Some(PhpType::Void)
}
