use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match name {
        "strlen" => {
            emitter.comment("strlen()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("mov x0, x2");
            Some(PhpType::Int)
        }
        "intval" => {
            emitter.comment("intval()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty == PhpType::Str {
                emitter.instruction("bl __rt_atoi");
            }
            Some(PhpType::Int)
        }
        "number_format" => {
            emitter.comment("number_format()");
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
            emitter.instruction("str d0, [sp, #-16]!");

            if args.len() >= 2 {
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("str x0, [sp, #-16]!");
            } else {
                emitter.instruction("str xzr, [sp, #-16]!");
            }

            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                emitter.instruction("ldrb w0, [x1]");
                emitter.instruction("str x0, [sp, #-16]!");
            } else {
                emitter.instruction("mov x0, #46"); // '.'
                emitter.instruction("str x0, [sp, #-16]!");
            }

            if args.len() >= 4 {
                emit_expr(&args[3], emitter, ctx, data);
                emitter.instruction("cbz x2, 1f");
                emitter.instruction("ldrb w0, [x1]");
                emitter.instruction("b 2f");
                emitter.raw("1:");
                emitter.instruction("mov x0, #0");
                emitter.raw("2:");
                emitter.instruction("str x0, [sp, #-16]!");
            } else {
                emitter.instruction("mov x0, #44"); // ','
                emitter.instruction("str x0, [sp, #-16]!");
            }

            emitter.instruction("ldr x3, [sp], #16");
            emitter.instruction("ldr x2, [sp], #16");
            emitter.instruction("ldr x1, [sp], #16");
            emitter.instruction("ldr d0, [sp], #16");
            emitter.instruction("bl __rt_number_format");
            Some(PhpType::Str)
        }
        _ => None,
    }
}
