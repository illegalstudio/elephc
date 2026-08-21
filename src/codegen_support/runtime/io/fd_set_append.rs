//! Purpose:
//! Emits `__rt_fd_set_append`, which turns an already-open descriptor into an appending one.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io::fopen_core`, for a `php://memory` or
//!   `php://temp` stream opened with an `a` mode.
//!
//! Key details:
//! - php's `a` modes send every write to the END of the stream, whatever `fseek()` did:
//!   `fopen("php://temp","a+")`, write `hello`, `fseek(0)`, write `world` answers `helloworld`,
//!   not `world`. A real file gets that from `O_APPEND` at `open()`; the in-memory backend is a
//!   `tmpfile()` descriptor created without any mode, so it silently OVERWROTE.
//! - Setting the flag on the descriptor rather than special-casing the write path means the
//!   append bookkeeping that already exists for files — `STREAM_APPEND_SKIP_OFFSET`, which is
//!   what keeps `ftell()` reporting php's logical cursor rather than the descriptor's — applies
//!   unchanged, instead of being reimplemented for a second backend.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_fd_set_append(fd) -> fd`, adding `O_APPEND` to an open descriptor.
///
/// The descriptor arrives and leaves in the INT RESULT register — `x0` on AArch64, `rax` on
/// x86_64 — because the helper is spliced into an open sequence that carries the fd there and
/// hands it straight to `box_stream_fd_or_false_result`. A failed `fcntl` is ignored: the stream
/// is still usable, it just will not append.
pub fn emit_fd_set_append(emitter: &mut Emitter) {
    let plat = emitter.platform;
    let append = plat.o_append();
    emitter.blank();
    emitter.comment("--- runtime: fd_set_append ---");
    emitter.label_global("__rt_fd_set_append");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // scratch for the descriptor
            emitter.instruction("str x0, [sp, #0]");                            // save it across both fcntl calls
            emitter.instruction("cmp x0, #0");                                  // a failed open answers a negative fd
            emitter.instruction("b.lt __rt_fdsa_done");                         // nothing to set on it
            emitter.instruction("mov x1, #3");                                  // F_GETFL
            emitter.instruction("mov x2, #0");                                  // unused third argument
            emitter.syscall(92);
            if plat.needs_cmp_before_error_branch() {
                emitter.instruction("cmp x0, #0");                              // Linux: a negative result means failure
            }
            emitter.instruction(&plat.branch_on_syscall_success("__rt_fdsa_getfl_ok")); // continue when F_GETFL worked
            emitter.instruction("b __rt_fdsa_done");                            // otherwise leave the stream as it is
            emitter.label("__rt_fdsa_getfl_ok");
            emitter.instruction(&format!("mov x9, #{append}"));                 // the O_APPEND flag bit
            emitter.instruction("orr x2, x0, x9");                              // add it to the current flags
            emitter.instruction("ldr x0, [sp, #0]");                            // the descriptor again
            emitter.instruction("mov x1, #4");                                  // F_SETFL
            emitter.syscall(92);
            emitter.label("__rt_fdsa_done");
            emitter.instruction("ldr x0, [sp, #0]");                            // answer the descriptor unchanged
            emitter.instruction("add sp, sp, #16");                             // release the scratch
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 16");
            emitter.instruction("mov QWORD PTR [rbp - 8], rax");                // save the descriptor
            emitter.instruction("cmp rax, 0");                                  // a failed open answers a negative fd
            emitter.instruction("jl __rt_fdsa_done_x86");                       // nothing to set on it
            emitter.instruction("mov rdi, rax");                                // fcntl's first argument
            emitter.instruction("mov esi, 3");                                  // F_GETFL
            emitter.instruction("xor edx, edx");                                // unused third argument
            emitter.instruction("mov eax, 72");                                 // Linux x86_64 syscall 72 = fcntl
            emitter.instruction("syscall");
            emitter.instruction("cmp rax, 0");
            emitter.instruction("jl __rt_fdsa_done_x86");                       // leave the stream as it is
            emitter.instruction(&format!("or rax, {append}"));                  // add the O_APPEND flag bit
            emitter.instruction("mov rdx, rax");                                // the updated flags
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // the descriptor again
            emitter.instruction("mov esi, 4");                                  // F_SETFL
            emitter.instruction("mov eax, 72");                                 // fcntl
            emitter.instruction("syscall");
            emitter.label("__rt_fdsa_done_x86");
            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                // answer the descriptor unchanged
            emitter.instruction("leave");
            emitter.instruction("ret");
        }
    }
}
