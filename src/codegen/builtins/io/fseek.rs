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
    emitter.comment("fseek()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the file descriptor while the seek offset expression is evaluated
    emit_expr(&args[1], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the seek offset while the optional whence expression is evaluated
    if args.len() >= 3 {
        emit_expr(&args[2], emitter, ctx, data);
    } else {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #0");                              // default whence = SEEK_SET for the AArch64 lseek path
            }
            Arch::X86_64 => {
                emitter.instruction("xor eax, eax");                            // default whence = SEEK_SET for the x86_64 lseek path
            }
        }
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x2, x0");                                  // move the whence selector into the third lseek syscall argument register
            abi::emit_pop_reg(emitter, "x1");                                   // restore the seek offset into the second lseek syscall argument register
            abi::emit_pop_reg(emitter, "x0");                                   // restore the file descriptor into the first lseek syscall argument register
            emitter.syscall(199);                                               // reposition the file offset through the platform syscall path
            emitter.instruction("cmp x0, #0");                                  // did lseek succeed with a non-negative resulting file offset?
            emitter.instruction("cset x0, ge");                                 // materialize success as 1 and failure as 0 before mapping to PHP fseek semantics
            emitter.instruction("sub x0, x0, #1");                              // map success→0 and failure→-1 to match PHP fseek()
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdx, rax");                                // move the whence selector into the third SysV lseek() argument register
            abi::emit_pop_reg(emitter, "rsi");                                  // restore the seek offset into the second SysV lseek() argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the file descriptor into the first SysV lseek() argument register
            emitter.instruction("call lseek");                                  // reposition the file offset through libc lseek() on linux-x86_64
            emitter.instruction("cmp rax, 0");                                  // did libc lseek() succeed with a non-negative resulting file offset?
            emitter.instruction("setge al");                                    // materialize success as 1 and failure as 0 before mapping to PHP fseek semantics
            emitter.instruction("movzx rax, al");                               // widen the boolean success flag into the standard x86_64 integer result register
            emitter.instruction("sub rax, 1");                                  // map success→0 and failure→-1 to match PHP fseek()
        }
    }
    Some(PhpType::Int)
}
