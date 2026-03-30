use super::super::abi;
use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::super::expr::emit_expr;
use super::PhpType;
use crate::parser::ast::Expr;

pub(super) fn emit_echo_stmt(
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment("echo");
    let ty = emit_expr(expr, emitter, ctx, data);
    match &ty {
        PhpType::Void => {}
        PhpType::Bool => {
            let skip_label = ctx.next_label("echo_skip_false");
            emitter.instruction(&format!("cbz x0, {}", skip_label));            // branch to skip label if x0 is zero (false)
            abi::emit_write_stdout(emitter, &ty);
            emitter.label(&skip_label);
        }
        PhpType::Int => {
            let skip_label = ctx.next_label("echo_skip_null");
            emitter.instruction("movz x9, #0xFFFE");                            // load lowest 16 bits of null sentinel into x9
            emitter.instruction("movk x9, #0xFFFF, lsl #16");                   // insert bits 16-31 of null sentinel
            emitter.instruction("movk x9, #0xFFFF, lsl #32");                   // insert bits 32-47 of null sentinel
            emitter.instruction("movk x9, #0x7FFF, lsl #48");                   // insert bits 48-63 of null sentinel
            emitter.instruction("cmp x0, x9");                                  // compare integer value against null sentinel
            emitter.instruction(&format!("b.eq {}", skip_label));               // skip echo if value is the null sentinel
            abi::emit_write_stdout(emitter, &ty);
            emitter.label(&skip_label);
        }
        PhpType::Float => {
            abi::emit_write_stdout(emitter, &ty);
        }
        _ => {
            abi::emit_write_stdout(emitter, &ty);
        }
    }
}
