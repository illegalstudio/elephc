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
    emitter.comment("is_numeric()");
    let ty = emit_expr(&args[0], emitter, ctx, data);

    match ty {
        PhpType::Int | PhpType::Float => {
            // -- int and float are always numeric --
            emitter.instruction("mov x0, #1");                                  // return true for int/float types
        }
        PhpType::Str => {
            // -- check if string is numeric: optional leading -, digits, optional . with more digits --
            let loop_label = ctx.next_label("isnum_loop");
            let dot_label = ctx.next_label("isnum_dot");
            let frac_loop = ctx.next_label("isnum_frac");
            let fail_label = ctx.next_label("isnum_fail");
            let pass_label = ctx.next_label("isnum_pass");
            let end_label = ctx.next_label("isnum_end");

            // -- return false for empty string --
            emitter.instruction(&format!("cbz x2, {}", fail_label));            // empty string is not numeric
            emitter.instruction("mov x3, #0");                                  // x3 = loop index
            emitter.instruction("mov x5, #0");                                  // x5 = digit count

            // -- check for optional leading minus sign --
            emitter.instruction("ldrb w4, [x1]");                               // load first byte
            emitter.instruction("cmp w4, #45");                                 // check if '-'
            emitter.instruction(&format!("b.ne {}", loop_label));               // not minus, start digit loop
            emitter.instruction("add x3, x3, #1");                              // skip the minus sign
            emitter.instruction("cmp x3, x2");                                  // check if string is just "-"
            emitter.instruction(&format!("b.ge {}", fail_label));               // just "-" is not numeric

            // -- scan integer part: digits before optional dot --
            emitter.label(&loop_label);
            emitter.instruction("cmp x3, x2");                                  // check if index reached length
            emitter.instruction(&format!("b.ge {}", pass_label));               // end of string, check if we had digits
            emitter.instruction("ldrb w4, [x1, x3]");                           // load byte at index
            emitter.instruction("cmp w4, #46");                                 // check if '.'
            emitter.instruction(&format!("b.eq {}", dot_label));                // found dot, switch to fractional part
            emitter.instruction("sub w6, w4, #48");                             // w6 = byte - '0'
            emitter.instruction("cmp w6, #9");                                  // check if in range 0-9
            emitter.instruction(&format!("b.hi {}", fail_label));               // not a digit, fail
            emitter.instruction("add x5, x5, #1");                              // increment digit count
            emitter.instruction("add x3, x3, #1");                              // increment index
            emitter.instruction(&format!("b {}", loop_label));                  // continue loop

            // -- found a dot, scan fractional digits --
            emitter.label(&dot_label);
            emitter.instruction("add x3, x3, #1");                              // skip the dot
            emitter.label(&frac_loop);
            emitter.instruction("cmp x3, x2");                                  // check if index reached length
            emitter.instruction(&format!("b.ge {}", pass_label));               // end of string after dot
            emitter.instruction("ldrb w4, [x1, x3]");                           // load byte at index
            emitter.instruction("sub w6, w4, #48");                             // w6 = byte - '0'
            emitter.instruction("cmp w6, #9");                                  // check if in range 0-9
            emitter.instruction(&format!("b.hi {}", fail_label));               // not a digit after dot, fail
            emitter.instruction("add x5, x5, #1");                              // increment digit count
            emitter.instruction("add x3, x3, #1");                              // increment index
            emitter.instruction(&format!("b {}", frac_loop));                   // continue fractional loop

            // -- must have at least one digit to be numeric --
            emitter.label(&pass_label);
            emitter.instruction("cmp x5, #0");                                  // check if we found any digits
            emitter.instruction(&format!("b.eq {}", fail_label));               // no digits found, not numeric
            emitter.instruction("mov x0, #1");                                  // return true
            emitter.instruction(&format!("b {}", end_label));                   // jump to end

            emitter.label(&fail_label);
            emitter.instruction("mov x0, #0");                                  // return false

            emitter.label(&end_label);
        }
        _ => {
            // -- all other types are not numeric --
            emitter.instruction("mov x0, #0");                                  // return false for non-numeric types
        }
    }

    Some(PhpType::Bool)
}
