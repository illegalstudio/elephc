//! Purpose:
//! Emits PHP `fscanf` calls: reads one line from a stream and parses it with the
//! `sscanf` runtime, returning an array of matched fields.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - `fscanf($stream, $format)` = read a line via `__rt_fgets`, then
//!   `__rt_sscanf($line, $format)`. v1 implements the 2-argument array-returning
//!   form (the by-ref output-variable form is not supported, mirroring `sscanf`).
//! - `__rt_fgets` returns the line as (ptr, len) in the runtime string registers,
//!   exactly the position `__rt_sscanf` expects for its input string, so this
//!   reuses `sscanf`'s argument marshaling.
//! - At EOF `__rt_fgets` yields a zero-length line, so `fscanf` returns an empty
//!   array rather than PHP's `false`/`-1` (documented v1 divergence). Because the
//!   line read goes through `__rt_fgets`, which dispatches synthetic
//!   userspace-wrapper descriptors into the wrapper's `stream_read`, `fscanf` works
//!   on registered userspace-wrapper handles as well as real descriptors.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fscanf()");
    // -- read one line from the stream descriptor --
    emit_stream_fd_arg("fscanf", &args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(emitter, "__rt_fgets");                        // read one line: x1=ptr, x2=len
            // The line is now the sscanf input string (x1/x2). Marshal it and
            // the format string exactly like sscanf().
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the line while the format string is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // format pointer → secondary runtime string-argument pair
            emitter.instruction("mov x4, x2");                                  // format length → secondary runtime string-argument pair
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the line into the primary string-argument pair
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // move the descriptor into the first fgets argument register
            abi::emit_call_label(emitter, "__rt_fgets");                        // read one line: rax=ptr, rdx=len
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // push the line while the format string is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // format pointer → secondary string-argument pair
            emitter.instruction("mov rsi, rdx");                                // format length → secondary string-argument pair
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the line into the primary string-argument pair
        }
    }
    abi::emit_call_label(emitter, "__rt_sscanf");                               // parse the line per the format string into an array of fields
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
