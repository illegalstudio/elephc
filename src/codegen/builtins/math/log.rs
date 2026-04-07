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
    emitter.comment("log()");
    if args.len() == 1 {
        // -- log($num) — natural logarithm --
        let ty = emit_expr(&args[0], emitter, ctx, data);
        if ty != PhpType::Float {
            emitter.instruction("scvtf d0, x0");                                // convert int to float
        }
        emitter.bl_c("log");                                         // call libc log(d0) → d0
    } else {
        // -- log($num, $base) — change of base: log($num) / log($base) --
        let ty = emit_expr(&args[0], emitter, ctx, data);
        if ty != PhpType::Float {
            emitter.instruction("scvtf d0, x0");                                // convert int to float
        }
        emitter.bl_c("log");                                         // log($num) → d0
        emitter.instruction("str d0, [sp, #-16]!");                             // save log($num) on stack
        let ty2 = emit_expr(&args[1], emitter, ctx, data);
        if ty2 != PhpType::Float {
            emitter.instruction("scvtf d0, x0");                                // convert int to float
        }
        emitter.bl_c("log");                                         // log($base) → d0
        emitter.instruction("fmov d1, d0");                                     // d1 = log($base)
        emitter.instruction("ldr d0, [sp], #16");                               // d0 = log($num), restore stack
        emitter.instruction("fdiv d0, d0, d1");                                 // d0 = log($num) / log($base)
    }
    Some(PhpType::Float)
}
