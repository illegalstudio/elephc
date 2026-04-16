use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
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
            match emitter.target.arch {
                Arch::X86_64 => {
                    emitter.instruction("cmp rax, 0");                          // test the boolean payload in the x86_64 integer result register before deciding whether print_r() should print anything
                    emitter.instruction(&format!("je {}", skip));               // skip the print_r() write path entirely when the boolean payload is false on x86_64
                }
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #0");                          // test the boolean payload in the AArch64 integer result register before deciding whether print_r() should print anything
                    emitter.instruction(&format!("cbz x0, {}", skip));          // skip the print_r() write path entirely when the boolean payload is false on AArch64
                }
            }
            abi::emit_write_stdout(emitter, &ty);
            emitter.label(&skip);
        }
        PhpType::Void => {
            // print_r(null) prints nothing
        }
        PhpType::Array(elem_ty) => {
            // -- print "Array\n" --
            let (lbl, len) = data.add_string(b"Array\n");
            abi::emit_symbol_address(emitter, abi::string_result_regs(emitter).0, &lbl); // materialize the borrowed \"Array\\n\" string pointer in the active target string-result pointer register
            abi::emit_load_int_immediate(emitter, abi::string_result_regs(emitter).1, len as i64); // materialize the borrowed \"Array\\n\" string length in the paired target string-result length register
            abi::emit_write_stdout(emitter, &PhpType::Str);                     // print the synthetic array label through the shared target-aware string stdout helper
            let _ = elem_ty;
        }
        _ => {
            // print_r for int, float, string — same as echo
            abi::emit_write_stdout(emitter, &ty);
        }
    }
    Some(PhpType::Void)
}
