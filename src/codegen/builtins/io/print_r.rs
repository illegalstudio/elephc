use crate::codegen::abi;
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
    emitter.comment("print_r()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    match &ty {
        PhpType::Bool => {
            // print_r(true) prints "1", print_r(false) prints nothing
            let skip = ctx.next_label("pr_skip");
            emitter.instruction("cmp x0, #0");                                  // test boolean value
            emitter.instruction(&format!("cbz x0, {}", skip));                  // skip if false
            abi::emit_write_stdout(emitter, &ty);
            emitter.label(&skip);
        }
        PhpType::Void => {
            // print_r(null) prints nothing
        }
        PhpType::Array(elem_ty) => {
            // -- print "Array\n" --
            let (lbl, len) = data.add_string(b"Array\n");
            emitter.adrp("x1", &format!("{}", lbl));             // load "Array\n" page
            emitter.add_lo12("x1", "x1", &format!("{}", lbl));       // resolve address
            emitter.instruction(&format!("mov x2, #{}", len));                  // string length
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.syscall(4);
            let _ = elem_ty;
        }
        _ => {
            // print_r for int, float, string — same as echo
            abi::emit_write_stdout(emitter, &ty);
        }
    }
    Some(PhpType::Void)
}
