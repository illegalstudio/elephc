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
    emitter.comment("sprintf()");

    // Strategy: evaluate all args, push them on stack with type tags,
    // then call __rt_sprintf which processes the format string.
    //
    // Stack layout per argument (16 bytes each):
    //   [sp + 0] = value (x0 for int/bool, d0 bits for float, x1 for str ptr)
    //   [sp + 8] = type_tag | (for str: length in upper bits)
    // Type tags: 0=int, 1=str (with length in bits 8+), 2=float, 3=bool

    let arg_count = args.len() - 1; // exclude format string

    // -- evaluate and push arguments in reverse order --
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
                emitter.instruction("str x0, [sp, #-16]!");                     // push bool value (0 or 1)
                emitter.instruction("mov x0, #3");                              // type tag 3 = bool
                emitter.instruction("str x0, [sp, #8]");                        // store type tag
            }
            PhpType::Str => {
                // Pack: value=ptr, tag = 1 | (len << 8)
                emitter.instruction("str x1, [sp, #-16]!");                     // push string pointer
                emitter.instruction("lsl x0, x2, #8");                          // shift length left by 8
                emitter.instruction("orr x0, x0, #1");                          // set type tag bit 0 = str
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
    // x1=fmt_ptr, x2=fmt_len

    // -- call runtime: x1/x2=format, x0=arg_count, args on stack --
    emitter.instruction(&format!("mov x0, #{}", arg_count));                    // number of format arguments
    emitter.instruction("bl __rt_sprintf");                                     // call runtime sprintf
    // x1=result_ptr, x2=result_len returned
    // runtime cleans up the stack (pops arg_count * 16 bytes)

    Some(PhpType::Str)
}
