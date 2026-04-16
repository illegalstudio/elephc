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
    emitter.comment("ord()");
    emit_expr(&args[0], emitter, ctx, data);
    let empty_label = ctx.next_label("ord_empty");
    let done_label = ctx.next_label("ord_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x2, {empty_label}"));             // return zero when ord() receives an empty string
            emitter.instruction("ldrb w0, [x1]");                               // load the first byte of the string as an unsigned integer code point
            emitter.instruction(&format!("b {done_label}"));                    // skip the empty-string fallback after loading the first byte
        }
        Arch::X86_64 => {
            emitter.instruction("test rdx, rdx");                               // return zero when ord() receives an empty string
            emitter.instruction(&format!("jz {empty_label}"));                  // branch to the empty-string fallback when the string length is zero
            emitter.instruction("movzx eax, BYTE PTR [rax]");                   // load the first byte of the string as an unsigned integer code point
            emitter.instruction(&format!("jmp {done_label}"));                  // skip the empty-string fallback after loading the first byte
        }
    }
    emitter.label(&empty_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #0");                                  // return zero when ord() receives an empty string
        }
        Arch::X86_64 => {
            emitter.instruction("xor eax, eax");                                // return zero when ord() receives an empty string
        }
    }
    emitter.label(&done_label);

    Some(PhpType::Int)
}
