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
    emitter.comment("readline()");
    if args.len() == 1 {
        // -- evaluate and print prompt string --
        emit_expr(&args[0], emitter, ctx, data);
        emitter.instruction("mov x0, #1");                                      // fd = stdout
        emitter.syscall(4);
    }
    // -- read a line from stdin --
    emitter.instruction("mov x0, #0");                                          // fd = stdin (fd 0)
    emitter.instruction("bl __rt_fgets");                                       // read line: x0=fd → x1/x2=string
    Some(PhpType::Str)
}
