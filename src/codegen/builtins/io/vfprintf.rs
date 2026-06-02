//! Purpose:
//! Emits PHP `vfprintf($stream, $format, $values)` — `fprintf` with the
//! arguments supplied as an array. Formats through the `__rt_vsprintf`
//! array→variadic bridge and writes the result to the stream via `__rt_fwrite`
//! (so write filters and userspace wrappers apply, exactly like `fwrite`).
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - The descriptor and the format string are stashed in the local frame so
//!   they survive the `__rt_vsprintf` call (which uses its own frame for the
//!   per-element records). Returns `PhpType::Int` (bytes written).

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits a `vfprintf($stream, $format, $values)` call: format via
/// `__rt_vsprintf`, then `__rt_fwrite` the result to the stream descriptor.
/// Returns `Some(PhpType::Int)` (bytes written).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("vfprintf()");
    // args[0] = stream, args[1] = format, args[2] = values array.
    emit_stream_fd_arg("vfprintf", &args[0], emitter, ctx, data); // fd → int-result reg
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // frame: [sp,#0]=fd, [sp,#8..24]=format ptr/len
            emitter.instruction("str x0, [sp, #0]");                            // stash the descriptor across the format + vsprintf calls
            emit_expr(&args[1], emitter, ctx, data); // format → x1/x2
            emitter.instruction("stp x1, x2, [sp, #8]");                        // stash the format ptr/len across the array evaluation
            emit_expr(&args[2], emitter, ctx, data); // values array → x0
            emitter.instruction("ldp x1, x2, [sp, #8]");                        // restore the format ptr/len
            abi::emit_call_label(emitter, "__rt_vsprintf");                     // x1 = formatted ptr, x2 = formatted len
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the descriptor (x1/x2 hold the payload)
            abi::emit_call_label(emitter, "__rt_fwrite");                       // write the formatted bytes, applying any write filter
            emitter.instruction("add sp, sp, #32");                             // release the frame (x0 = bytes written)
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 32");                                 // frame: [rsp]=fd, [rsp+8..24]=format ptr/len
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // stash the descriptor across the format + vsprintf calls
            emit_expr(&args[1], emitter, ctx, data); // format → rax/rdx
            emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // stash the format ptr across the array evaluation
            emitter.instruction("mov QWORD PTR [rsp + 16], rdx");               // stash the format len across the array evaluation
            emit_expr(&args[2], emitter, ctx, data); // values array → rax
            emitter.instruction("mov rdi, rax");                                // array pointer → __rt_vsprintf first argument
            emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                // restore the format ptr
            emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");               // restore the format len
            abi::emit_call_label(emitter, "__rt_vsprintf");                     // rax = formatted ptr, rdx = formatted len
            emitter.instruction("mov rsi, rax");                                // formatted pointer → __rt_fwrite buffer arg (rdx=len in place)
            emitter.instruction("mov rdi, QWORD PTR [rsp]");                    // reload the descriptor → __rt_fwrite fd arg
            abi::emit_call_label(emitter, "__rt_fwrite");                       // write the formatted bytes, applying any write filter
            emitter.instruction("add rsp, 32");                                 // release the frame (rax = bytes written)
        }
    }
    Some(PhpType::Int)
}
