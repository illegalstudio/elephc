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
    emitter.comment("intdiv()");
    // -- integer division: dividend / divisor --
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("str x0, [sp, #-16]!");                                 // push dividend onto stack
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("ldr x1, [sp], #16");                                   // pop dividend into x1

    // -- division by zero guard --
    let zero_label = ctx.next_label("intdiv_zero");
    let done_label = ctx.next_label("intdiv_done");
    emitter.instruction(&format!("cbz x0, {zero_label}"));                      // if divisor is 0, branch to error
    emitter.instruction("sdiv x0, x1, x0");                                     // x0 = x1 / x0 (signed integer divide)
    emitter.instruction(&format!("b {done_label}"));                            // skip error path

    // -- fatal error: division by zero --
    emitter.label(&zero_label);
    let (err_label, err_len) = data.add_string(b"Fatal error: division by zero\n");
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.adrp("x1", &format!("{}", err_label));               // load page of error message
    emitter.add_lo12("x1", "x1", &format!("{}", err_label));         // resolve error message address
    emitter.instruction(&format!("mov x2, #{}", err_len));                      // message length
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit code 1
    emitter.syscall(1);

    emitter.label(&done_label);
    Some(PhpType::Int)
}
