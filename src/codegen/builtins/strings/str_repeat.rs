use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("str_repeat()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- save string, evaluate repeat count --
    let (str_ptr_reg, str_len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, str_ptr_reg, str_len_reg);                 // preserve the source string while the repeat-count expression is evaluated
    emit_expr(&args[1], emitter, ctx, data);

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x3, x0");                                  // move the repeat count into the third AArch64 string-helper argument register
            abi::emit_pop_reg_pair(emitter, "x1", "x2");                        // restore the source string into the AArch64 runtime string-argument registers
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // move the repeat count into the extra x86_64 runtime argument register used by str_repeat()
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the source string into the standard x86_64 string input registers expected by string helpers
        }
    }

    abi::emit_call_label(emitter, "__rt_str_repeat");                           // call the target-aware runtime helper that repeats the source string into concat storage

    Some(PhpType::Str)
}
