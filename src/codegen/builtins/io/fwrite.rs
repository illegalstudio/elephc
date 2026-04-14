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
    emitter.comment("fwrite()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // push the file descriptor while the data expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("ldr x0, [sp], #16");                           // restore the file descriptor into the write syscall register
            emitter.syscall(4);                                                 // write the elephc string payload to the requested file descriptor through the platform syscall path
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");                                 // preserve the file descriptor while the data expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the file descriptor into the first SysV libc write() argument register
            emitter.instruction("mov rsi, rax");                                // move the elephc string pointer into the second SysV libc write() argument register
            emitter.instruction("call write");                                  // write the requested elephc string payload through libc write()
        }
    }
    Some(PhpType::Int)
}
