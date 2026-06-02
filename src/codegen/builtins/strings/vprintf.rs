//! Purpose:
//! Emits PHP `vprintf($format, $values)` — `printf` with the arguments supplied
//! as an array. Formats through the `__rt_vsprintf` array→variadic bridge,
//! writes the result to stdout, and returns the byte count.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Identical to `vsprintf` up to the formatted string, then writes it to
//!   stdout (write syscall on AArch64, `write` syscall on x86_64) and returns
//!   the length, matching `printf`'s contract. Returns `PhpType::Int`.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a `vprintf($format, $values)` call: format via `__rt_vsprintf`, write
/// the result to stdout, return the byte count. Returns `Some(PhpType::Int)`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("vprintf()");
    emit_expr(&args[0], emitter, ctx, data); // format string → string-result pair
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // scratch slot for the format string
            emitter.instruction("stp x1, x2, [sp, #0]");                        // save the format ptr/len across the array evaluation
            emit_expr(&args[1], emitter, ctx, data); // arguments array → x0
            emitter.instruction("ldp x1, x2, [sp, #0]");                        // restore the format ptr/len
            emitter.instruction("add sp, sp, #16");                             // release the scratch slot
            abi::emit_call_label(emitter, "__rt_vsprintf");                     // x1 = formatted ptr, x2 = formatted len
            emitter.instruction("mov x0, #1");                                  // fd = stdout (x1/x2 already hold ptr/len)
            emitter.syscall(4);                                                 // write(1, formatted, len)
            emitter.instruction("mov x0, x2");                                  // return the byte count
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // scratch slot for the format string
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // save the format ptr across the array evaluation
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // save the format len across the array evaluation
            emit_expr(&args[1], emitter, ctx, data); // arguments array → rax
            emitter.instruction("mov rdi, rax");                                // array pointer → __rt_vsprintf first argument
            emitter.instruction("mov rax, QWORD PTR [rsp]");                    // restore the format ptr
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // restore the format len
            emitter.instruction("add rsp, 16");                                 // release the scratch slot
            abi::emit_call_label(emitter, "__rt_vsprintf");                     // rax = formatted ptr, rdx = formatted len
            emitter.instruction("mov rcx, rdx");                                // preserve the formatted byte count across the write syscall
            emitter.instruction("mov rsi, rax");                                // formatted pointer → SysV write buffer register
            emitter.instruction("mov rdx, rcx");                                // formatted length → SysV write byte-count register
            emitter.instruction("mov edi, 1");                                  // fd = stdout
            emitter.instruction("mov eax, 1");                                  // syscall 1 = write on Linux x86_64
            emitter.instruction("syscall");                                     // write the formatted bytes to stdout
            emitter.instruction("mov rax, rcx");                                // return the byte count
        }
    }
    Some(PhpType::Int)
}
