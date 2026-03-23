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
    emitter.comment("ctype_space()");
    emit_expr(&args[0], emitter, ctx, data);
    let loop_label = ctx.next_label("ctype_loop");
    let next_label = ctx.next_label("ctype_next");
    let fail_label = ctx.next_label("ctype_fail");
    let pass_label = ctx.next_label("ctype_pass");
    let end_label = ctx.next_label("ctype_end");
    // -- return false for empty string --
    emitter.instruction(&format!("cbz x2, {}", fail_label));             // empty string returns false
    emitter.instruction("mov x3, #0");                                    // x3 = loop index
    emitter.label(&loop_label);
    emitter.instruction("cmp x3, x2");                                    // check if index reached length
    emitter.instruction(&format!("b.ge {}", pass_label));                 // all bytes checked, pass
    emitter.instruction("ldrb w4, [x1, x3]");                            // load byte at index
    // -- check space (32) --
    emitter.instruction("cmp w4, #32");                                   // check if space
    emitter.instruction(&format!("b.eq {}", next_label));                 // is space, next byte
    // -- check tab (9) --
    emitter.instruction("cmp w4, #9");                                    // check if tab
    emitter.instruction(&format!("b.eq {}", next_label));                 // is tab, next byte
    // -- check newline (10) --
    emitter.instruction("cmp w4, #10");                                   // check if newline
    emitter.instruction(&format!("b.eq {}", next_label));                 // is newline, next byte
    // -- check carriage return (13) --
    emitter.instruction("cmp w4, #13");                                   // check if carriage return
    emitter.instruction(&format!("b.eq {}", next_label));                 // is CR, next byte
    // -- check vertical tab (11) --
    emitter.instruction("cmp w4, #11");                                   // check if vertical tab
    emitter.instruction(&format!("b.eq {}", next_label));                 // is VT, next byte
    // -- check form feed (12) --
    emitter.instruction("cmp w4, #12");                                   // check if form feed
    emitter.instruction(&format!("b.ne {}", fail_label));                 // not whitespace, fail
    emitter.label(&next_label);
    emitter.instruction("add x3, x3, #1");                                // increment index
    emitter.instruction(&format!("b {}", loop_label));                    // continue loop
    emitter.label(&fail_label);
    emitter.instruction("mov x0, #0");                                    // return false
    emitter.instruction(&format!("b {}", end_label));                     // jump to end
    emitter.label(&pass_label);
    emitter.instruction("mov x0, #1");                                    // return true
    emitter.label(&end_label);
    Some(PhpType::Bool)
}
