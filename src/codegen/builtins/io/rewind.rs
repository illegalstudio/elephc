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
    emitter.comment("rewind()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, #0");                                  // offset = 0 for the AArch64 rewind() lseek syscall
            emitter.instruction("mov x2, #0");                                  // whence = SEEK_SET for the AArch64 rewind() lseek syscall
            emitter.syscall(199);                                               // reset the file position through the platform lseek syscall path
            emitter.instruction("mov x0, #1");                                  // rewind() reports true after issuing the seek on AArch64
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // move the file descriptor into the first SysV lseek() argument register
            emitter.instruction("xor esi, esi");                                // offset = 0 for the linux-x86_64 rewind() lseek() call
            emitter.instruction("xor edx, edx");                                // whence = SEEK_SET for the linux-x86_64 rewind() lseek() call
            emitter.instruction("call lseek");                                  // reset the file position through libc lseek() on linux-x86_64
            emitter.instruction("mov rax, 1");                                  // rewind() reports true after issuing the seek on linux-x86_64
        }
    }
    Some(PhpType::Bool)
}
