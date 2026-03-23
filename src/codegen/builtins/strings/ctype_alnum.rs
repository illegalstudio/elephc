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
    emitter.comment("ctype_alnum()");
    emit_expr(&args[0], emitter, ctx, data);
    let loop_label = ctx.next_label("ctype_loop");
    let next_label = ctx.next_label("ctype_next");
    let fail_label = ctx.next_label("ctype_fail");
    let pass_label = ctx.next_label("ctype_pass");
    let end_label = ctx.next_label("ctype_end");
    // -- return false for empty string --
    emitter.instruction(&format!("cbz x2, {}", fail_label));                    // empty string returns false
    emitter.instruction("mov x3, #0");                                          // x3 = loop index
    emitter.label(&loop_label);
    emitter.instruction("cmp x3, x2");                                          // check if index reached length
    emitter.instruction(&format!("b.ge {}", pass_label));                       // all bytes checked, pass
    emitter.instruction("ldrb w4, [x1, x3]");                                   // load byte at index
    // -- check if A-Z (65-90) --
    emitter.instruction("sub w5, w4, #65");                                     // w5 = byte - 'A'
    emitter.instruction("cmp w5, #25");                                         // check if in range A-Z
    emitter.instruction(&format!("b.ls {}", next_label));                       // branch if unsigned <= 25
    // -- check if a-z (97-122) --
    emitter.instruction("sub w5, w4, #97");                                     // w5 = byte - 'a'
    emitter.instruction("cmp w5, #25");                                         // check if in range a-z
    emitter.instruction(&format!("b.ls {}", next_label));                       // branch if unsigned <= 25
    // -- check if 0-9 (48-57) --
    emitter.instruction("sub w5, w4, #48");                                     // w5 = byte - '0'
    emitter.instruction("cmp w5, #9");                                          // check if in range 0-9
    emitter.instruction(&format!("b.hi {}", fail_label));                       // not alphanumeric, fail
    emitter.label(&next_label);
    emitter.instruction("add x3, x3, #1");                                      // increment index
    emitter.instruction(&format!("b {}", loop_label));                          // continue loop
    emitter.label(&fail_label);
    emitter.instruction("mov x0, #0");                                          // return false
    emitter.instruction(&format!("b {}", end_label));                           // jump to end
    emitter.label(&pass_label);
    emitter.instruction("mov x0, #1");                                          // return true
    emitter.label(&end_label);
    Some(PhpType::Bool)
}
