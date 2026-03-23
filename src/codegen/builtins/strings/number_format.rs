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
    emitter.comment("number_format()");
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    // -- prepare the numeric value as a float --
    if t0 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }            // convert signed int to double-precision float
    emitter.instruction("str d0, [sp, #-16]!");                                 // push float value onto stack (pre-decrement sp by 16)

    // -- prepare decimals argument --
    if args.len() >= 2 {
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("str x0, [sp, #-16]!");                             // push decimal places count onto stack
    } else {
        emitter.instruction("str xzr, [sp, #-16]!");                            // push 0 decimals (default) onto stack
    }

    // -- prepare decimal point character --
    if args.len() >= 3 {
        emit_expr(&args[2], emitter, ctx, data);
        emitter.instruction("ldrb w0, [x1]");                                   // load first byte of decimal separator string
        emitter.instruction("str x0, [sp, #-16]!");                             // push decimal separator char onto stack
    } else {
        emitter.instruction("mov x0, #46");                                     // load ASCII '.' as default decimal separator
        emitter.instruction("str x0, [sp, #-16]!");                             // push default decimal separator onto stack
    }

    // -- prepare thousands separator character --
    if args.len() >= 4 {
        emit_expr(&args[3], emitter, ctx, data);
        emitter.instruction("cbz x2, 1f");                                      // if separator string is empty, jump to use zero
        emitter.instruction("ldrb w0, [x1]");                                   // load first byte of thousands separator string
        emitter.instruction("b 2f");                                            // skip over the zero-fallback
        emitter.raw("1:");
        emitter.instruction("mov x0, #0");                                      // use zero (no separator) for empty string
        emitter.raw("2:");
        emitter.instruction("str x0, [sp, #-16]!");                             // push thousands separator onto stack
    } else {
        emitter.instruction("mov x0, #44");                                     // load ASCII ',' as default thousands separator
        emitter.instruction("str x0, [sp, #-16]!");                             // push default thousands separator onto stack
    }

    // -- pop all args from stack into registers and call runtime --
    emitter.instruction("ldr x3, [sp], #16");                                   // pop thousands separator into x3
    emitter.instruction("ldr x2, [sp], #16");                                   // pop decimal separator into x2
    emitter.instruction("ldr x1, [sp], #16");                                   // pop decimal places count into x1
    emitter.instruction("ldr d0, [sp], #16");                                   // pop float value into d0
    emitter.instruction("bl __rt_number_format");                               // call runtime: format number as string

    Some(PhpType::Str)
}
