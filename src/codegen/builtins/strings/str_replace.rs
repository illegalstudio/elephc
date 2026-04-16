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
    emitter.comment("str_replace()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the search string while evaluating the replacement and subject strings
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the replacement string while evaluating the subject string
            emit_expr(&args[2], emitter, ctx, data);
            emitter.instruction("mov x5, x1");                                  // move the subject pointer into the third runtime string-argument pair
            emitter.instruction("mov x6, x2");                                  // move the subject length into the third runtime string-argument pair
            emitter.instruction("ldp x3, x4, [sp], #16");                       // restore the replacement string into the secondary runtime string-argument pair
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the search string into the primary runtime string-argument pair
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the search string while evaluating the replacement and subject strings
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the replacement string while evaluating the subject string
            emit_expr(&args[2], emitter, ctx, data);
            emitter.instruction("mov rcx, rax");                                // move the subject pointer into the third x86_64 runtime string-argument pair
            emitter.instruction("mov r8, rdx");                                 // move the subject length into the third x86_64 runtime string-argument pair
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the replacement string into the secondary x86_64 runtime string-argument pair
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the search string into the primary x86_64 string-helper input registers
        }
    }
    abi::emit_call_label(emitter, "__rt_str_replace");                          // replace every search-string occurrence inside the subject through the target-aware runtime helper

    Some(PhpType::Str)
}
