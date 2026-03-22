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
                // -- float absolute value --
                emitter.instruction("fabs d0, d0");                             // take absolute value of float in d0
                Some(PhpType::Float)
            } else {
                // -- integer absolute value via conditional negate --
                emitter.instruction("cmp x0, #0");                              // compare integer value against zero
                emitter.instruction("cneg x0, x0, lt");                         // negate x0 if it was negative (lt)
                Some(PhpType::Int)
            }
        }
        "floor" => {
            emitter.comment("floor()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            // -- convert int to float if needed, then round toward minus infinity --
            if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert signed int to float
            emitter.instruction("frintm d0, d0");                               // round toward minus infinity (floor)
            Some(PhpType::Float)
        }
        "ceil" => {
            emitter.comment("ceil()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            // -- convert int to float if needed, then round toward plus infinity --
            if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert signed int to float
            emitter.instruction("frintp d0, d0");                               // round toward plus infinity (ceil)
            Some(PhpType::Float)
        }
        "round" => {
            emitter.comment("round()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            // -- convert int to float if needed, then round to nearest with ties away from zero --
            if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert signed int to float
            emitter.instruction("frinta d0, d0");                               // round to nearest, ties away from zero
            Some(PhpType::Float)
        }
        "sqrt" => {
            emitter.comment("sqrt()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            // -- convert int to float if needed, then compute square root --
            if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert signed int to float
            emitter.instruction("fsqrt d0, d0");                                // compute square root of d0
            Some(PhpType::Float)
        }
        "pow" => {
            emitter.comment("pow()");
            // -- evaluate base, save it, evaluate exponent, call C pow() --
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert base to float if int
            emitter.instruction("str d0, [sp, #-16]!");                         // push base onto stack (pre-decrement)
            let t1 = emit_expr(&args[1], emitter, ctx, data);
            if t1 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert exponent to float if int
            emitter.instruction("fmov d1, d0");                                 // move exponent to d1 (second arg)
            emitter.instruction("ldr d0, [sp], #16");                           // pop base into d0 (first arg)
            emitter.instruction("bl _pow");                                     // call C library pow(base, exponent)
            Some(PhpType::Float)
        }
        "min" => {
            emitter.comment("min()");
            // -- evaluate first arg and push it onto the stack --
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 == PhpType::Float {
                emitter.instruction("str d0, [sp, #-16]!");                     // push first arg as float
            } else {
                emitter.instruction("str x0, [sp, #-16]!");                     // push first arg as int
            }
            let t1 = emit_expr(&args[1], emitter, ctx, data);
            if t0 == PhpType::Float || t1 == PhpType::Float {
                // -- float min: coerce both to float, use fmin --
                if t1 != PhpType::Float { emitter.instruction("scvtf d0, x0"); } // convert second arg to float
                if t0 == PhpType::Float {
                    emitter.instruction("ldr d1, [sp], #16");                   // pop first arg as float into d1
                } else {
                    emitter.instruction("ldr x9, [sp], #16");                   // pop first arg as int into x9
                    emitter.instruction("scvtf d1, x9");                        // convert first arg int to float
                }
                emitter.instruction("fmin d0, d1, d0");                         // d0 = minimum of d1 and d0
                Some(PhpType::Float)
            } else {
                // -- integer min: compare and conditionally select --
                emitter.instruction("ldr x1, [sp], #16");                       // pop first arg into x1
                emitter.instruction("cmp x1, x0");                              // compare first arg with second arg
                emitter.instruction("csel x0, x1, x0, lt");                     // select smaller value (x1 if x1 < x0)
                Some(PhpType::Int)
            }
        }
        "max" => {
            emitter.comment("max()");
            // -- evaluate first arg and push it onto the stack --
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 == PhpType::Float {
                emitter.instruction("str d0, [sp, #-16]!");                     // push first arg as float
            } else {
                emitter.instruction("str x0, [sp, #-16]!");                     // push first arg as int
            }
            let t1 = emit_expr(&args[1], emitter, ctx, data);
            if t0 == PhpType::Float || t1 == PhpType::Float {
                // -- float max: coerce both to float, use fmax --
                if t1 != PhpType::Float { emitter.instruction("scvtf d0, x0"); } // convert second arg to float
                if t0 == PhpType::Float {
                    emitter.instruction("ldr d1, [sp], #16");                   // pop first arg as float into d1
                } else {
                    emitter.instruction("ldr x9, [sp], #16");                   // pop first arg as int into x9
                    emitter.instruction("scvtf d1, x9");                        // convert first arg int to float
                }
                emitter.instruction("fmax d0, d1, d0");                         // d0 = maximum of d1 and d0
                Some(PhpType::Float)
            } else {
                // -- integer max: compare and conditionally select --
                emitter.instruction("ldr x1, [sp], #16");                       // pop first arg into x1
                emitter.instruction("cmp x1, x0");                              // compare first arg with second arg
                emitter.instruction("csel x0, x1, x0, gt");                     // select larger value (x1 if x1 > x0)
                Some(PhpType::Int)
            }
        }
        "intdiv" => {
            emitter.comment("intdiv()");
            // -- integer division: dividend / divisor --
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");                         // push dividend onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("ldr x1, [sp], #16");                           // pop dividend into x1
            emitter.instruction("sdiv x0, x1, x0");                             // x0 = x1 / x0 (signed integer divide)
            Some(PhpType::Int)
        }
        "fmod" => {
            emitter.comment("fmod()");
            // -- floating-point modulo: dividend - floor(dividend/divisor) * divisor --
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert dividend to float if int
            emitter.instruction("str d0, [sp, #-16]!");                         // push dividend onto stack
            let t1 = emit_expr(&args[1], emitter, ctx, data);
            if t1 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert divisor to float if int
            emitter.instruction("ldr d1, [sp], #16");                           // pop dividend into d1
            emitter.instruction("fdiv d2, d1, d0");                             // d2 = d1 / d0 (dividend / divisor)
            emitter.instruction("frintm d2, d2");                               // d2 = floor(d2) — truncate quotient
            emitter.instruction("fmsub d0, d2, d0, d1");                        // d0 = d1 - d2*d0 (remainder)
            Some(PhpType::Float)
        }
        "fdiv" => {
            emitter.comment("fdiv()");
            // -- floating-point division --
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert dividend to float if int
            emitter.instruction("str d0, [sp, #-16]!");                         // push dividend onto stack
            let t1 = emit_expr(&args[1], emitter, ctx, data);
            if t1 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert divisor to float if int
            emitter.instruction("ldr d1, [sp], #16");                           // pop dividend into d1
            emitter.instruction("fdiv d0, d1, d0");                             // d0 = d1 / d0 (float division)
            Some(PhpType::Float)
        }
        "rand" | "mt_rand" => {
            emitter.comment(&format!("{}()", name));
            if args.len() == 2 {
                // -- rand(min, max): generate random int in [min, max] --
                emit_expr(&args[0], emitter, ctx, data);
                emitter.instruction("str x0, [sp, #-16]!");                     // push min value onto stack
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("ldr x9, [sp], #16");                       // pop min value into x9
                emitter.instruction("sub x0, x0, x9");                          // x0 = max - min
                emitter.instruction("add x0, x0, #1");                          // x0 = range size (max - min + 1)
                emitter.instruction("str x9, [sp, #-16]!");                     // push min back for later use
                emitter.instruction("mov w0, w0");                              // zero-extend w0 to x0 (32-bit arg)
                emitter.instruction("bl _arc4random_uniform");                  // call arc4random_uniform(range) -> [0,range)
                emitter.instruction("ldr x9, [sp], #16");                       // pop min value back into x9
                emitter.instruction("add x0, x0, x9");                          // x0 = random + min (shift into range)
            } else {
                // -- rand() with no args: return non-negative random int --
                emitter.instruction("bl _arc4random");                          // call arc4random() -> random uint32
                emitter.instruction("lsr x0, x0, #1");                          // shift right by 1 to ensure non-negative
            }
            Some(PhpType::Int)
        }
        "random_int" => {
            emitter.comment("random_int()");
            // -- random_int(min, max): cryptographically secure random in [min, max] --
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");                         // push min value onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("ldr x9, [sp], #16");                           // pop min value into x9
            emitter.instruction("sub x0, x0, x9");                              // x0 = max - min
            emitter.instruction("add x0, x0, #1");                              // x0 = range size (max - min + 1)
            emitter.instruction("str x9, [sp, #-16]!");                         // push min back for later use
            emitter.instruction("mov w0, w0");                                  // zero-extend w0 to x0 (32-bit arg)
            emitter.instruction("bl _arc4random_uniform");                      // call arc4random_uniform(range) -> [0,range)
            emitter.instruction("ldr x9, [sp], #16");                           // pop min value back into x9
            emitter.instruction("add x0, x0, x9");                              // x0 = random + min (shift into range)
            Some(PhpType::Int)
        }
        _ => None,
    }
}
