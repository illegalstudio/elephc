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
    emitter.comment("date()");

    if args.len() == 2 {
        // -- evaluate timestamp argument first --
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("str x0, [sp, #-16]!");                             // push timestamp onto stack

        // -- evaluate format string --
        emit_expr(&args[0], emitter, ctx, data);
        // x1=format ptr, x2=format len

        // -- pop timestamp into x0 --
        emitter.instruction("ldr x0, [sp], #16");                               // pop timestamp from stack
    } else {
        // -- evaluate format string --
        emit_expr(&args[0], emitter, ctx, data);
        // x1=format ptr, x2=format len

        // -- use -1 to signal "use current time" --
        emitter.instruction("mov x0, #-1");                                     // timestamp -1 = use current time
    }

    // -- call runtime: x0=timestamp, x1=format ptr, x2=format len --
    emitter.instruction("bl __rt_date");                                        // format date → x1=result ptr, x2=result len

    Some(PhpType::Str)
}
