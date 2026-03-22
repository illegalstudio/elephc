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
        "abs" => {
            emitter.comment("abs()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty == PhpType::Float {
                emitter.instruction("fabs d0, d0");
                Some(PhpType::Float)
            } else {
                emitter.instruction("cmp x0, #0");
                emitter.instruction("cneg x0, x0, lt");
                Some(PhpType::Int)
            }
        }
        "floor" => {
            emitter.comment("floor()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
            emitter.instruction("frintm d0, d0");
            Some(PhpType::Float)
        }
        "ceil" => {
            emitter.comment("ceil()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
            emitter.instruction("frintp d0, d0");
            Some(PhpType::Float)
        }
        "round" => {
            emitter.comment("round()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
            emitter.instruction("frinta d0, d0");
            Some(PhpType::Float)
        }
        "sqrt" => {
            emitter.comment("sqrt()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
            emitter.instruction("fsqrt d0, d0");
            Some(PhpType::Float)
        }
        "pow" => {
            emitter.comment("pow()");
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
            emitter.instruction("str d0, [sp, #-16]!");
            let t1 = emit_expr(&args[1], emitter, ctx, data);
            if t1 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
            emitter.instruction("fmov d1, d0");
            emitter.instruction("ldr d0, [sp], #16");
            emitter.instruction("bl _pow");
            Some(PhpType::Float)
        }
        "min" => {
            emitter.comment("min()");
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 == PhpType::Float {
                emitter.instruction("str d0, [sp, #-16]!");
            } else {
                emitter.instruction("str x0, [sp, #-16]!");
            }
            let t1 = emit_expr(&args[1], emitter, ctx, data);
            if t0 == PhpType::Float || t1 == PhpType::Float {
                if t1 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
                if t0 == PhpType::Float {
                    emitter.instruction("ldr d1, [sp], #16");
                } else {
                    emitter.instruction("ldr x9, [sp], #16");
                    emitter.instruction("scvtf d1, x9");
                }
                emitter.instruction("fmin d0, d1, d0");
                Some(PhpType::Float)
            } else {
                emitter.instruction("ldr x1, [sp], #16");
                emitter.instruction("cmp x1, x0");
                emitter.instruction("csel x0, x1, x0, lt");
                Some(PhpType::Int)
            }
        }
        "max" => {
            emitter.comment("max()");
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 == PhpType::Float {
                emitter.instruction("str d0, [sp, #-16]!");
            } else {
                emitter.instruction("str x0, [sp, #-16]!");
            }
            let t1 = emit_expr(&args[1], emitter, ctx, data);
            if t0 == PhpType::Float || t1 == PhpType::Float {
                if t1 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
                if t0 == PhpType::Float {
                    emitter.instruction("ldr d1, [sp], #16");
                } else {
                    emitter.instruction("ldr x9, [sp], #16");
                    emitter.instruction("scvtf d1, x9");
                }
                emitter.instruction("fmax d0, d1, d0");
                Some(PhpType::Float)
            } else {
                emitter.instruction("ldr x1, [sp], #16");
                emitter.instruction("cmp x1, x0");
                emitter.instruction("csel x0, x1, x0, gt");
                Some(PhpType::Int)
            }
        }
        "intdiv" => {
            emitter.comment("intdiv()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("ldr x1, [sp], #16");
            emitter.instruction("sdiv x0, x1, x0");
            Some(PhpType::Int)
        }
        "fmod" => {
            emitter.comment("fmod()");
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
            emitter.instruction("str d0, [sp, #-16]!");
            let t1 = emit_expr(&args[1], emitter, ctx, data);
            if t1 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
            emitter.instruction("ldr d1, [sp], #16");
            emitter.instruction("fdiv d2, d1, d0");
            emitter.instruction("frintm d2, d2");
            emitter.instruction("fmsub d0, d2, d0, d1");
            Some(PhpType::Float)
        }
        "fdiv" => {
            emitter.comment("fdiv()");
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
            emitter.instruction("str d0, [sp, #-16]!");
            let t1 = emit_expr(&args[1], emitter, ctx, data);
            if t1 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
            emitter.instruction("ldr d1, [sp], #16");
            emitter.instruction("fdiv d0, d1, d0");
            Some(PhpType::Float)
        }
        "rand" | "mt_rand" => {
            emitter.comment(&format!("{}()", name));
            if args.len() == 2 {
                emit_expr(&args[0], emitter, ctx, data);
                emitter.instruction("str x0, [sp, #-16]!");
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("ldr x9, [sp], #16");
                emitter.instruction("sub x0, x0, x9");
                emitter.instruction("add x0, x0, #1");
                emitter.instruction("str x9, [sp, #-16]!");
                emitter.instruction("mov w0, w0");
                emitter.instruction("bl _arc4random_uniform");
                emitter.instruction("ldr x9, [sp], #16");
                emitter.instruction("add x0, x0, x9");
            } else {
                emitter.instruction("bl _arc4random");
                emitter.instruction("lsr x0, x0, #1");
            }
            Some(PhpType::Int)
        }
        "random_int" => {
            emitter.comment("random_int()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("ldr x9, [sp], #16");
            emitter.instruction("sub x0, x0, x9");
            emitter.instruction("add x0, x0, #1");
            emitter.instruction("str x9, [sp, #-16]!");
            emitter.instruction("mov w0, w0");
            emitter.instruction("bl _arc4random_uniform");
            emitter.instruction("ldr x9, [sp], #16");
            emitter.instruction("add x0, x0, x9");
            Some(PhpType::Int)
        }
        _ => None,
    }
}
