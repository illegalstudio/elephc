//! Purpose:
//! Emits `__rt_copy_wrapper`, the `copy()` path for a source or destination served by a
//! registered userspace stream wrapper.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The `copy()` builtin, when `__rt_path_is_wrapper` matches either path.
//!
//! Key details:
//! - `__rt_copy` is a PATH implementation — it stats both ends and moves bytes with syscalls — so
//!   it cannot reach a wrapper at all. MEASURED on `php -n` 8.5.6:
//!   `copy("cc://a", "out.txt")` answers `bool(true)` and copies, where elephc warned
//!   `Failed to open stream: No such file or directory` and answered `bool(false)`. The comment in
//!   `filesystem_ops` claiming the runtime "reaches every REGISTERED wrapper on its own" was
//!   simply wrong.
//! - The route is `__rt_readfile_wrapper`'s, with the destination in place of stdout: open both
//!   ends through `__rt_fopen` (which dispatches the wrapper and answers a synthetic fd), drain
//!   the source with the same feof-gated loop, and write each chunk with `__rt_fwrite`.
//! - EITHER end may be a plain file, so the closes pick by fd range rather than assuming a
//!   wrapper: a synthetic fd (`>= USER_WRAPPER_FD_BASE`) closes through
//!   `__rt_user_wrapper_fclose`, a native one through `close(2)`.
//! - php opens its own ends BINARY, so the wrapper sees `rb` and `wb`.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// The synthetic descriptor base shared with `user_wrapper`.
const FD_BASE: u32 = 0x40000000;

/// Emits `__rt_copy_wrapper(src_ptr, src_len, dst_ptr, dst_len) -> 1 | 0`.
///
/// Inputs: AArch64 x1/x2 = source path, x3/x4 = destination path; x86_64 rax/rdx and rdi/rsi,
/// the same pairing `__rt_copy` takes.
pub fn emit_copy_wrapper(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// The AArch64 route.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: copy through a userspace wrapper ---");
    emitter.label_global("__rt_copy_wrapper");
    // Frame: [0]=src fd [8]=dst fd [16]=chunk ptr [24]=dst path ptr [32]=dst path len
    //        [64]=src path ptr [72]=src path len — both paths outlive the stats and the opens
    emitter.instruction("sub sp, sp, #80");                                     // reserve the copy frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("stp x3, x4, [sp, #24]");                               // the destination path outlives the first open
    emitter.instruction("stp x1, x2, [sp, #64]");                               // and the source path outlives its own stat

    // -- php stats BOTH ends before it opens either --
    //
    // MEASURED on `php -n` 8.5.6, wrapper to wrapper: `url_stat(src, 0)`, `url_stat(dst, 2)`,
    // then the two opens. The flags differ because the destination is the one php expects may
    // not exist — 2 is STREAM_URL_STAT_QUIET. A path with no registered scheme matches nothing
    // and the helper answers 0, so an end served by the filesystem produces no call at all,
    // which is exactly what php's trace shows for a disk-to-wrapper copy.
    //
    // The boxed answer is released: this stat is asked for the CALL, not for its value, and the
    // cache the helper fills is php's own — it caches this stat too.
    emitter.instruction("ldp x0, x1, [sp, #64]");                               // the source path
    emitter.instruction("mov x2, #0");                                          // php's flags for the source
    emitter.instruction("bl __rt_user_wrapper_url_stat");
    emitter.instruction("bl __rt_decref_any");                                  // the answer is not what this asks for
    emitter.instruction("ldp x0, x1, [sp, #24]");                               // the destination path
    emitter.instruction("mov x2, #2");                                          // STREAM_URL_STAT_QUIET
    emitter.instruction("bl __rt_user_wrapper_url_stat");
    emitter.instruction("bl __rt_decref_any");

    // -- open the source "rb" --
    emitter.instruction("ldp x1, x2, [sp, #64]");                               // reload it past the stats
    abi::emit_symbol_address(emitter, "x3", "_meta_mode_rb");
    emitter.instruction("mov x4, #2");                                          // strlen("rb")
    emitter.instruction("bl __rt_fopen");                                       // x0 = fd, or negative on refusal
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.lt __rt_cpw_fail");                                  // the source could not be opened
    emitter.instruction("str x0, [sp, #0]");                                    // keep the source fd

    // -- open the destination "wb" --
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // the destination path
    abi::emit_symbol_address(emitter, "x3", "_meta_mode_wb");
    emitter.instruction("mov x4, #2");                                          // strlen("wb")
    emitter.instruction("bl __rt_fopen");
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.lt __rt_cpw_close_src");                             // destination refused: release the source
    emitter.instruction("str x0, [sp, #8]");                                    // keep the destination fd

    // -- READ, then ask: php reads once before it believes any EOF --
    //
    // The `feof`-first shape `__rt_readfile_wrapper` uses skipped the read entirely for a
    // wrapper whose `stream_eof()` answers true from the start, where php still performs one
    // `stream_read()`. Asking after the read is also what the rest of the wrapper protocol does.
    emitter.label("__rt_cpw_loop");
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("mov x1, #4096");                                       // one chunk at a time
    emitter.instruction("bl __rt_fread");                                       // x1 = chunk ptr, x2 = length
    emitter.instruction("cbz x2, __rt_cpw_release_eof");                        // an empty read also stops
    emitter.instruction("str x1, [sp, #16]");                                   // the chunk is owned until released
    emitter.instruction("mov x9, x1");                                          // the write clobbers x1/x2
    emitter.instruction("mov x10, x2");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the destination fd
    emitter.instruction("mov x1, x9");
    emitter.instruction("mov x2, x10");
    emitter.instruction("bl __rt_fwrite");                                      // reaches the wrapper's stream_write
    emitter.instruction("ldr x0, [sp, #16]");
    emitter.instruction("bl __rt_decref_any");                                  // release the chunk
    emitter.instruction("ldr x0, [sp, #0]");
    super::feof::emit_feof_call(emitter, true);                                 // elephc's own probe: never warns, the read does
    emitter.instruction("cbnz x0, __rt_cpw_done");
    emitter.instruction("b __rt_cpw_loop");

    emitter.label("__rt_cpw_release_eof");
    emitter.instruction("mov x0, x1");                                          // the final uncopied chunk
    emitter.instruction("bl __rt_decref_any");

    emitter.label("__rt_cpw_done");
    emitter.instruction("ldr x0, [sp, #8]");
    emitter.instruction("mov x9, #1");
    abi::emit_symbol_address(emitter, "x10", "_uw_pending_flush");
    emitter.instruction("str x9, [x10]");
    emitter.instruction("bl __rt_cpw_close_one");                               // close the destination, which was written
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("mov x9, #0");
    abi::emit_symbol_address(emitter, "x10", "_uw_pending_flush");
    emitter.instruction("str x9, [x10]");
    emitter.instruction("bl __rt_cpw_close_one");                               // then the source, which was only read
    emitter.instruction("mov x0, #1");                                          // php answers true
    emitter.instruction("b __rt_cpw_ret");

    emitter.label("__rt_cpw_close_src");
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("mov x9, #0");
    abi::emit_symbol_address(emitter, "x10", "_uw_pending_flush");
    emitter.instruction("str x9, [x10]");
    emitter.instruction("bl __rt_cpw_close_one");                               // the source was opened; do not leak it
    emitter.label("__rt_cpw_fail");
    emitter.instruction("mov x0, #0");                                          // php answers false

    emitter.label("__rt_cpw_ret");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the copy frame
    emitter.instruction("ret");

    // -- close one end, picking by fd range --
    emitter.blank();
    emitter.comment("--- runtime: close one end of a wrapper copy ---");
    emitter.label_global("__rt_cpw_close_one");
    emitter.instruction("stp x29, x30, [sp, #-16]!");
    emitter.instruction("mov x29, sp");
    emitter.instruction(&format!("mov x9, #{:#x}", FD_BASE));
    emitter.instruction("cmp x0, x9");
    emitter.instruction("b.lt __rt_cpw_close_native");                          // a plain file closes with close(2)
    emitter.instruction("bl __rt_user_wrapper_fclose");                         // stream_flush + stream_close, slot freed
    emitter.instruction("b __rt_cpw_close_ret");
    emitter.label("__rt_cpw_close_native");
    emitter.syscall(6);                                                         // close(fd)
    emitter.label("__rt_cpw_close_ret");
    emitter.instruction("ldp x29, x30, [sp], #16");
    emitter.instruction("ret");
}

/// The x86_64 route.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: copy through a userspace wrapper ---");
    emitter.label_global("__rt_copy_wrapper");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 64");                                         // src fd / dst fd / chunk / both paths
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // the destination path outlives the first open
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // and the source path outlives its own stat
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");

    // -- php stats BOTH ends before it opens either; see the AArch64 arm --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // the source path
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");
    emitter.instruction("xor edx, edx");                                        // php's flags for the source
    emitter.instruction("call __rt_user_wrapper_url_stat");
    emitter.instruction("call __rt_decref_any");                                // the answer is not what this asks for
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // the destination path
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");
    emitter.instruction("mov rdx, 2");                                          // STREAM_URL_STAT_QUIET
    emitter.instruction("call __rt_user_wrapper_url_stat");
    emitter.instruction("call __rt_decref_any");

    // -- open the source "rb" --
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload it past the stats
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");
    abi::emit_symbol_address(emitter, "rdi", "_meta_mode_rb");
    emitter.instruction("mov rsi, 2");                                          // strlen("rb")
    emitter.instruction("call __rt_fopen");                                     // rax = fd, or negative on refusal
    emitter.instruction("cmp rax, 0");
    emitter.instruction("jl __rt_cpw_fail_x86");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the source fd

    // -- open the destination "wb" --
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the destination path
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");
    abi::emit_symbol_address(emitter, "rdi", "_meta_mode_wb");
    emitter.instruction("mov rsi, 2");                                          // strlen("wb")
    emitter.instruction("call __rt_fopen");
    emitter.instruction("cmp rax, 0");
    emitter.instruction("jl __rt_cpw_close_src_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // keep the destination fd

    // -- READ, then ask; see the AArch64 arm --
    emitter.label("__rt_cpw_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    emitter.instruction("mov rsi, 4096");                                       // one chunk at a time
    emitter.instruction("call __rt_fread");                                     // rax = chunk ptr, rdx = length
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_cpw_release_eof_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // the chunk is owned until released
    emitter.instruction("mov r10, rax");
    emitter.instruction("mov r11, rdx");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the destination fd
    emitter.instruction("mov rsi, r10");
    emitter.instruction("mov rdx, r11");
    emitter.instruction("call __rt_fwrite");                                    // reaches the wrapper's stream_write
    // `__rt_decref_any` reads RAX on this target, not rdi — passing it in rdi released whatever
    // `__rt_fwrite` had left in rax, which the heap-range check then rejected, and every copied
    // chunk leaked.
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");
    emitter.instruction("call __rt_decref_any");                                // release the chunk
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    super::feof::emit_feof_call(emitter, true);                                 // elephc's own probe: never warns, the read does
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_cpw_done_x86");
    emitter.instruction("jmp __rt_cpw_loop_x86");

    emitter.label("__rt_cpw_release_eof_x86");
    emitter.instruction("call __rt_decref_any");                                // the final uncopied chunk, already in rax

    emitter.label("__rt_cpw_done_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");
    abi::emit_symbol_address(emitter, "r10", "_uw_pending_flush");
    emitter.instruction("mov QWORD PTR [r10], 1");
    emitter.instruction("call __rt_cpw_close_one");                             // close the destination, which was written
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    abi::emit_symbol_address(emitter, "r10", "_uw_pending_flush");
    emitter.instruction("mov QWORD PTR [r10], 0");
    emitter.instruction("call __rt_cpw_close_one");                             // then the source, which was only read
    emitter.instruction("mov eax, 1");                                          // php answers true
    emitter.instruction("jmp __rt_cpw_ret_x86");

    emitter.label("__rt_cpw_close_src_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    abi::emit_symbol_address(emitter, "r10", "_uw_pending_flush");
    emitter.instruction("mov QWORD PTR [r10], 0");
    emitter.instruction("call __rt_cpw_close_one");                             // the source was opened; do not leak it
    emitter.label("__rt_cpw_fail_x86");
    emitter.instruction("xor eax, eax");                                        // php answers false

    emitter.label("__rt_cpw_ret_x86");
    emitter.instruction("leave");
    emitter.instruction("ret");

    emitter.blank();
    emitter.comment("--- runtime: close one end of a wrapper copy ---");
    emitter.label_global("__rt_cpw_close_one");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction(&format!("mov r10, {:#x}", FD_BASE));
    emitter.instruction("cmp rdi, r10");
    emitter.instruction("jl __rt_cpw_close_native_x86");                        // a plain file closes with close(2)
    emitter.instruction("call __rt_user_wrapper_fclose");                       // stream_flush + stream_close, slot freed
    emitter.instruction("jmp __rt_cpw_close_ret_x86");
    emitter.label("__rt_cpw_close_native_x86");
    emitter.instruction("call close");                                          // close the native descriptor, as the stream teardown does
    emitter.label("__rt_cpw_close_ret_x86");
    emitter.instruction("leave");
    emitter.instruction("ret");
}
