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
    emitter.comment("file_put_contents()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push filename ptr and length onto the temporary stack while the data expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the data pointer into the third string-argument register pair slot
            emitter.instruction("mov x4, x2");                                  // move the data length into the fourth string-argument register pair slot
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the filename pointer and length after evaluating the data expression
            abi::emit_call_label(emitter, "__rt_file_put_contents");            // call the target-aware runtime helper that writes the string payload to disk
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the filename pointer and length while the data expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the data pointer into the third x86_64 string-argument slot
            emitter.instruction("mov rsi, rdx");                                // move the data length into the fourth x86_64 string-argument slot
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the filename pointer and length after evaluating the data expression
            abi::emit_call_label(emitter, "__rt_file_put_contents");            // call the target-aware runtime helper that writes the string payload to disk
        }
    }
    Some(PhpType::Int)
}
