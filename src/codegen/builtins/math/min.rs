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
    emitter.comment("min()");

    // -- evaluate first arg --
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    let mut any_float = t0 == PhpType::Float;

    // -- check all arg types for float promotion --
    // We need to know upfront if any arg is float so we use a consistent register
    // For simplicity, we'll track float dynamically per pair

    for i in 1..args.len() {
        // -- push current minimum onto stack --
        if any_float {
            if i == 1 && t0 != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                                // convert first arg int to float
            }
            emitter.instruction("str d0, [sp, #-16]!");                             // push current min as float
        } else {
            emitter.instruction("str x0, [sp, #-16]!");                             // push current min as int
        }

        let ti = emit_expr(&args[i], emitter, ctx, data);

        if any_float || ti == PhpType::Float {
            // -- float comparison path --
            if ti != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                                // convert new arg to float
            }
            if !any_float {
                // Previous was int on stack, need to convert
                emitter.instruction("ldr x9, [sp], #16");                           // pop previous min as int
                emitter.instruction("scvtf d1, x9");                                // convert previous min to float
            } else {
                emitter.instruction("ldr d1, [sp], #16");                           // pop previous min as float
            }
            emitter.instruction("fmin d0, d1, d0");                                 // d0 = minimum of d1 and d0
            any_float = true;
        } else {
            // -- integer comparison path --
            emitter.instruction("ldr x1, [sp], #16");                               // pop previous min into x1
            emitter.instruction("cmp x1, x0");                                      // compare previous min with new arg
            emitter.instruction("csel x0, x1, x0, lt");                             // select smaller value
        }
    }

    if any_float {
        Some(PhpType::Float)
    } else {
        Some(PhpType::Int)
    }
}
