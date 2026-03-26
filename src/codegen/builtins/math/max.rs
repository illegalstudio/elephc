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
    emitter.comment("max()");

    // -- evaluate first arg --
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    let mut any_float = t0 == PhpType::Float;

    for i in 1..args.len() {
        // -- push current maximum onto stack --
        if any_float {
            if i == 1 && t0 != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                                // convert first arg int to float
            }
            emitter.instruction("str d0, [sp, #-16]!");                             // push current max as float
        } else {
            emitter.instruction("str x0, [sp, #-16]!");                             // push current max as int
        }

        let ti = emit_expr(&args[i], emitter, ctx, data);

        if any_float || ti == PhpType::Float {
            // -- float comparison path --
            if ti != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                                // convert new arg to float
            }
            if !any_float {
                // Previous was int on stack, need to convert
                emitter.instruction("ldr x9, [sp], #16");                           // pop previous max as int
                emitter.instruction("scvtf d1, x9");                                // convert previous max to float
            } else {
                emitter.instruction("ldr d1, [sp], #16");                           // pop previous max as float
            }
            emitter.instruction("fmax d0, d1, d0");                                 // d0 = maximum of d1 and d0
            any_float = true;
        } else {
            // -- integer comparison path --
            emitter.instruction("ldr x1, [sp], #16");                               // pop previous max into x1
            emitter.instruction("cmp x1, x0");                                      // compare previous max with new arg
            emitter.instruction("csel x0, x1, x0, gt");                             // select larger value
        }
    }

    if any_float {
        Some(PhpType::Float)
    } else {
        Some(PhpType::Int)
    }
}
