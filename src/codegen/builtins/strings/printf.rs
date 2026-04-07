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
    emitter.comment("printf()");

    // printf = sprintf + echo
    let arg_count = args.len() - 1;

    // -- push args in reverse (same as sprintf) --
    for i in (1..args.len()).rev() {
        let ty = emit_expr(&args[i], emitter, ctx, data);
        match ty {
            PhpType::Int => {
                emitter.instruction("str x0, [sp, #-16]!");                     // push int value
                emitter.instruction("str xzr, [sp, #8]");                       // type tag 0 = int
            }
            PhpType::Float => {
                emitter.instruction("fmov x0, d0");                             // move float bits to int register
                emitter.instruction("str x0, [sp, #-16]!");                     // push float bits
                emitter.instruction("mov x0, #2");                              // type tag 2 = float
                emitter.instruction("str x0, [sp, #8]");                        // store type tag
            }
            PhpType::Bool => {
                emitter.instruction("str x0, [sp, #-16]!");                     // push bool value
                emitter.instruction("mov x0, #3");                              // type tag 3 = bool
                emitter.instruction("str x0, [sp, #8]");                        // store type tag
            }
            PhpType::Str => {
                emitter.instruction("str x1, [sp, #-16]!");                     // push string pointer
                emitter.instruction("lsl x0, x2, #8");                          // shift length left by 8
                emitter.instruction("orr x0, x0, #1");                          // set type tag = str
                emitter.instruction("str x0, [sp, #8]");                        // store tag|length
            }
            _ => {
                emitter.instruction("str xzr, [sp, #-16]!");                    // push zero
                emitter.instruction("str xzr, [sp, #8]");                       // type tag 0
            }
        }
    }

    // -- evaluate format string --
    emit_expr(&args[0], emitter, ctx, data);

    // -- call sprintf runtime --
    emitter.instruction(&format!("mov x0, #{}", arg_count));                    // argument count
    emitter.instruction("bl __rt_sprintf");                                     // format string → x1/x2

    // -- write result to stdout --
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);

    // -- return length written --
    emitter.instruction("mov x0, x2");                                          // return char count

    Some(PhpType::Int)
}
