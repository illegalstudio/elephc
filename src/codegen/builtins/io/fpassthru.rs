//! Purpose:
//! Emits PHP `fpassthru` stream builtin calls over runtime file handles.
//! Validates the stream argument before streaming remaining bytes to stdout.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Returns the total number of bytes copied to stdout, or -1 on read failure.
//! - Normal descriptors delegate to `__rt_fpassthru`. Synthetic user-wrapper
//!   descriptors (`>= 0x40000000`) use a compiled, feof-gated loop emitted here
//!   that reads each chunk through `__rt_fread`, writes it to stdout, and
//!   releases it. feof is checked BEFORE each read so the loop never makes the
//!   EOF read whose empty `substr` result corrupts the caller's resource cell
//!   (see `stream_get_contents` for the full rationale).

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
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
    emitter.comment("fpassthru()");
    emit_stream_fd_arg("fpassthru", &args[0], emitter, ctx, data);
    let wrapper_label = ctx.next_label("fpt_wrapper");
    let loop_label = ctx.next_label("fpt_loop");
    let release_eof_label = ctx.next_label("fpt_release_eof");
    let wdone_label = ctx.next_label("fpt_done");
    let done_label = ctx.next_label("fpt_after");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov w9, #0x4000");                             // high half of USER_WRAPPER_FD_BASE
            emitter.instruction("lsl w9, w9, #16");                             // form 0x40000000 in w9
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", wrapper_label));            // wrappers stream via the feof-gated loop below
            abi::emit_call_label(emitter, "__rt_fpassthru");                    // normal fd: runtime helper streams the rest to stdout
            emitter.instruction(&format!("b {}", done_label));                  // skip the wrapper loop on the normal path

            emitter.label(&wrapper_label);
            emitter.instruction("sub sp, sp, #32");                             // scratch: [sp,#0]=fd, [sp,#8]=total, [sp,#16]=chunk ptr
            emitter.instruction("str x0, [sp, #0]");                            // save the synthetic wrapper fd
            emitter.instruction("str xzr, [sp, #8]");                           // bytes-copied total = 0
            emitter.label(&loop_label);
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the wrapper fd
            abi::emit_call_label(emitter, "__rt_feof");                         // check stream_eof FIRST (x0 = 1 at EOF)
            emitter.instruction(&format!("cbnz x0, {}", wdone_label));          // at EOF: stop without reading
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the wrapper fd
            emitter.instruction("mov x1, #4096");                               // request up to 4096 bytes
            abi::emit_call_label(emitter, "__rt_fread");                        // x1=chunk ptr, x2=len
            emitter.instruction(&format!("cbz x2, {}", release_eof_label));     // defensive: empty read also stops
            emitter.instruction("str x1, [sp, #16]");                           // save the chunk ptr for the later release
            emitter.instruction("ldr x9, [sp, #8]");                            // current total
            emitter.instruction("add x9, x9, x2");                              // add this chunk's length
            emitter.instruction("str x9, [sp, #8]");                            // store the updated total
            emitter.instruction("mov x0, #1");                                  // fd = stdout (x1=ptr, x2=len already in place)
            emitter.syscall(4);                                                 // write(1, chunk, len)
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the chunk ptr
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the owned chunk, then loop
            emitter.instruction(&format!("b {}", loop_label));                  // stream the next chunk
            emitter.label(&release_eof_label);
            emitter.instruction("mov x0, x1");                                  // the final (empty/uncopied) owned chunk
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release it (heap freed; non-heap skipped)
            emitter.label(&wdone_label);
            emitter.instruction("ldr x0, [sp, #8]");                            // return the total bytes copied to stdout
            emitter.instruction("add sp, sp, #32");                             // release the scratch frame
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rax, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", wrapper_label));             // wrappers stream via the feof-gated loop below
            emitter.instruction("mov rdi, rax");                                // normal fd: pass the descriptor to the helper
            abi::emit_call_label(emitter, "__rt_fpassthru");                    // runtime helper streams the rest to stdout
            emitter.instruction(&format!("jmp {}", done_label));                // skip the wrapper loop on the normal path

            emitter.label(&wrapper_label);
            emitter.instruction("sub rsp, 32");                                 // scratch: [rsp+0]=fd, [rsp+8]=total, [rsp+16]=chunk ptr
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // save the synthetic wrapper fd
            emitter.instruction("mov QWORD PTR [rsp + 8], 0");                  // bytes-copied total = 0
            emitter.label(&loop_label);
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // reload the wrapper fd
            abi::emit_call_label(emitter, "__rt_feof");                         // check stream_eof FIRST (rax = 1 at EOF)
            emitter.instruction("test rax, rax");                               // at EOF?
            emitter.instruction(&format!("jnz {}", wdone_label));               // at EOF: stop without reading
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // reload the wrapper fd
            emitter.instruction("mov rsi, 4096");                               // request up to 4096 bytes
            abi::emit_call_label(emitter, "__rt_fread");                        // rax=chunk ptr, rdx=len
            emitter.instruction("test rdx, rdx");                               // zero-length read?
            emitter.instruction(&format!("jz {}", release_eof_label));          // defensive: empty read also stops
            emitter.instruction("mov QWORD PTR [rsp + 16], rax");               // save the chunk ptr for the later release
            emitter.instruction("mov r8, QWORD PTR [rsp + 8]");                 // current total
            emitter.instruction("add r8, rdx");                                 // add this chunk's length
            emitter.instruction("mov QWORD PTR [rsp + 8], r8");                 // store the updated total
            emitter.instruction("mov rsi, rax");                                // buffer = chunk ptr
            emitter.instruction("mov edi, 1");                                  // fd = stdout (rdx=len already in place)
            abi::emit_call_label(emitter, "write");                            // write(1, chunk, len) via libc
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // reload the chunk ptr
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the owned chunk, then loop
            emitter.instruction(&format!("jmp {}", loop_label));                // stream the next chunk
            emitter.label(&release_eof_label);
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the final (empty/uncopied) chunk (rax=ptr)
            emitter.label(&wdone_label);
            emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                // return the total bytes copied to stdout
            emitter.instruction("add rsp, 32");                                 // release the scratch frame
            emitter.label(&done_label);
        }
    }
    Some(PhpType::Int)
}
