//! Purpose:
//! Emits `__rt_readfile_wrapper`, the `readfile()` path for `scheme://` URLs
//! backed by a registered userspace stream wrapper. Opens the URL through
//! `__rt_fopen` (which dispatches the wrapper and returns a synthetic fd),
//! streams it to stdout with `__rt_fpassthru`, closes it, and returns the byte
//! count — matching `__rt_readfile`'s count / `-2`-open-failure convention.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - The `readfile()` builtin, after `__rt_path_is_wrapper` confirms the path's
//!   scheme matches a registered wrapper.
//!
//! Key details:
//! - The fd returned by `__rt_fopen` for a registered wrapper scheme is a
//!   synthetic handle (`>= USER_WRAPPER_FD_BASE`), so the close goes through
//!   `__rt_user_wrapper_fclose`. `__rt_fpassthru` is already wrapper-fd-aware
//!   (its feof-first drain handles synthetic descriptors).
//! - The mode is the shared single-byte `_meta_mode_r` ("r") read-only string.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_readfile_wrapper(path_ptr, path_len) -> byte_count | -2`.
///
/// Inputs: x1 = path pointer, x2 = path length (AArch64); rax = path pointer,
/// rdx = path length (x86_64) — the elephc string ABI shared with `__rt_readfile`.
/// Output: x0 / rax = bytes streamed to stdout, or `-2` when the wrapper's
/// `stream_open` refuses the URL (boxed to PHP `false` by the readfile builtin).
pub fn emit_readfile_wrapper(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_readfile_wrapper_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: readfile_wrapper ---");
    emitter.label_global("__rt_readfile_wrapper");

    // Frame: 48 bytes. [sp,#0..16] x29/x30, [sp,#16] fd, [sp,#24] byte count,
    //   [sp,#32] current chunk pointer (across the per-chunk release).
    emitter.instruction("sub sp, sp, #48");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- fopen(path, "r"): path already in x1/x2, mode in x3/x4 --
    abi::emit_symbol_address(emitter, "x3", "_meta_mode_r");
    emitter.instruction("mov x4, #1");                                          // strlen("r")
    emitter.instruction("bl __rt_fopen");                                       // x0 = synthetic wrapper fd, or -1 on failure
    emitter.instruction("cmp x0, #0");                                          // did fopen() fail (negative fd)?
    emitter.instruction("b.lt __rt_rfw_fail");                                  // → -2 open-failure sentinel
    emitter.instruction("str x0, [sp, #16]");                                   // save the synthetic fd across the drain/close
    emitter.instruction("str xzr, [sp, #24]");                                  // bytes-copied total = 0

    // -- feof-gated drain: check stream_eof BEFORE each read so the loop never
    //    makes the EOF read whose empty result would cross the method boundary --
    emitter.label("__rt_rfw_loop");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the wrapper fd
    emitter.instruction("bl __rt_feof");                                        // check stream_eof first (x0 = 1 at EOF)
    emitter.instruction("cbnz x0, __rt_rfw_done");                              // at EOF: stop without reading
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the wrapper fd
    emitter.instruction("mov x1, #4096");                                       // request up to 4096 bytes
    emitter.instruction("bl __rt_fread");                                       // x1 = chunk ptr, x2 = len
    emitter.instruction("cbz x2, __rt_rfw_release_eof");                        // defensive: empty read also stops
    emitter.instruction("str x1, [sp, #32]");                                   // save the chunk ptr for the later release
    emitter.instruction("ldr x9, [sp, #24]");                                   // current byte total
    emitter.instruction("add x9, x9, x2");                                      // add this chunk's length
    emitter.instruction("str x9, [sp, #24]");                                   // store the updated total
    emitter.instruction("mov x0, #1");                                          // fd = stdout (x1=ptr, x2=len already in place)
    emitter.syscall(4);                                                         // write(1, chunk, len)
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the chunk ptr
    emitter.instruction("bl __rt_decref_any");                                  // release the owned chunk, then loop
    emitter.instruction("b __rt_rfw_loop");                                     // stream the next chunk

    emitter.label("__rt_rfw_release_eof");
    emitter.instruction("mov x0, x1");                                          // the final (empty/uncopied) owned chunk
    emitter.instruction("bl __rt_decref_any");                                  // release it (heap freed; non-heap skipped)

    emitter.label("__rt_rfw_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the synthetic fd
    emitter.instruction("bl __rt_user_wrapper_fclose");                         // run the wrapper's stream_close and free the handle slot
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the byte count for return
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the byte count

    emitter.label("__rt_rfw_fail");
    emitter.instruction("mov x0, #-2");                                         // open-failure sentinel (readfile builtin boxes -2 → false)
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return -2
}

/// Emits the Linux x86_64 stream runtime helper for readfile wrapper.
fn emit_readfile_wrapper_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: readfile_wrapper ---");
    emitter.label_global("__rt_readfile_wrapper");

    // Frame: [rbp-8] fd, [rbp-16] byte count, [rbp-24] current chunk pointer.
    // push rbp then sub rsp,48 keeps rsp 16-aligned for the helper calls.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // spill slots for fd/total/chunk

    // -- fopen(path, "r"): path already in rax/rdx, mode in rdi/rsi --
    abi::emit_symbol_address(emitter, "rdi", "_meta_mode_r");                   // mode pointer "r" (secondary string-arg slot)
    emitter.instruction("mov rsi, 1");                                          // strlen("r")
    emitter.instruction("call __rt_fopen");                                     // rax = synthetic wrapper fd, or -1 on failure
    emitter.instruction("cmp rax, 0");                                          // did fopen() fail (negative fd)?
    emitter.instruction("jl __rt_rfw_fail_x86");                                // → -2 open-failure sentinel
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the synthetic fd across the drain/close
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // bytes-copied total = 0

    // -- feof-gated drain: check stream_eof BEFORE each read --
    emitter.label("__rt_rfw_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the wrapper fd
    emitter.instruction("call __rt_feof");                                      // check stream_eof first (rax = 1 at EOF)
    emitter.instruction("test rax, rax");                                       // at EOF?
    emitter.instruction("jnz __rt_rfw_done_x86");                               // at EOF: stop without reading
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the wrapper fd
    emitter.instruction("mov rsi, 4096");                                       // request up to 4096 bytes
    emitter.instruction("call __rt_fread");                                     // rax = chunk ptr, rdx = len
    emitter.instruction("test rdx, rdx");                                       // zero-length read?
    emitter.instruction("jz __rt_rfw_release_eof_x86");                         // defensive: empty read also stops
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the chunk ptr for the later release
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // current byte total
    emitter.instruction("add r8, rdx");                                         // add this chunk's length
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // store the updated total
    emitter.instruction("mov rsi, rax");                                        // buffer = chunk ptr
    emitter.instruction("mov edi, 1");                                          // fd = stdout (rdx=len already in place)
    emitter.instruction("call write");                                          // write(1, chunk, len) via libc
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the chunk ptr
    emitter.instruction("call __rt_decref_any");                                // release the owned chunk, then loop
    emitter.instruction("jmp __rt_rfw_loop_x86");                               // stream the next chunk

    emitter.label("__rt_rfw_release_eof_x86");
    emitter.instruction("call __rt_decref_any");                                // release the final (empty/uncopied) chunk (rax=ptr)

    emitter.label("__rt_rfw_done_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the synthetic fd
    emitter.instruction("call __rt_user_wrapper_fclose");                       // run the wrapper's stream_close and free the handle slot
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the byte count for return
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the byte count

    emitter.label("__rt_rfw_fail_x86");
    emitter.instruction("mov rax, -2");                                         // open-failure sentinel (readfile builtin boxes -2 → false)
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return -2
}
