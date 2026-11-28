//! Purpose:
//! Emits the `__rt_opendir_glob` runtime helper, which opens a synthetic
//! directory stream over a libc `glob` match list. The Phase 6
//! `glob://` stream wrapper routes here from `__rt_opendir` when the
//! caller passes `opendir("glob://pattern")`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_opendir`'s scheme probe when the path begins with `glob://`.
//!
//! Key details:
//! - Memoir owns the native glob envelope and the embedded libc state.
//! - Returns a real file descriptor obtained via `dup(2)` so the
//!   PHP-visible resource value never collides with a real directory
//!   fd from libc `opendir`. The descriptor is stashed in
//!   `_glob_handles[fd]` keyed by fd; `__rt_readdir`, `__rt_closedir`,
//!   and `__rt_rewinddir` probe that table before falling through to
//!   the libc DIR* path.
//! - A glob failure returns -1; the caller's `box_socket_result` lowers
//!   that to PHP false.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use elephc_memoir::{buffers::stream_tables, interop::glob_storage};

/// opendir_glob: open a synthetic dir stream over a glob match list.
/// Input:  AArch64 x1 = address pointer (including "glob://"), x2 = address length
///         x86_64  rax = address pointer (including "glob://"), rdx = address length
///         (matches the elephc string-arg convention so the dispatcher can
///         tail-branch here without shuffling registers)
/// Output: a real file descriptor (dup of stderr) or -1 on failure
pub fn emit_opendir_glob(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_opendir_glob_linux_x86_64(emitter);
        return;
    }

    let pathv_off = emitter.platform.glob_pathv_offset();
    emitter.blank();
    emitter.comment("--- runtime: opendir_glob ---");
    emitter.label_global("__rt_opendir_glob");

    // Frame (80 bytes): [0..16) saved x29/x30, [16) addr ptr, [24) addr len,
    //   [32) c_string ptr from __rt_cstr, [40) glob_state struct ptr,
    //   [48..80) padding.
    emitter.instruction("sub sp, sp, #80");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #16]");                                   // save the address pointer (elephc string ABI: x1)
    emitter.instruction("str x2, [sp, #24]");                                   // save the address length (elephc string ABI: x2)

    // -- skip the "glob://" prefix and NUL-terminate the pattern --
    emitter.instruction("add x1, x1, #7");                                      // pattern pointer = addr + 7
    emitter.instruction("sub x2, x2, #7");                                      // pattern length = addr_len - 7
    emitter.instruction("bl __rt_cstr");                                        // x0 = NUL-terminated pattern
    emitter.instruction("str x0, [sp, #32]");                                   // save the pattern c_string across the heap alloc

    // -- allocate the glob_state struct on the heap --
    glob_storage::emit_allocate(emitter);
    emitter.instruction("str x0, [sp, #40]");                                   // save the struct pointer

    // -- call libc glob(pattern, 0, NULL, &struct.glob_t) --
    emitter.instruction("ldr x0, [sp, #32]");                                   // pattern c_string
    emitter.instruction("mov x1, #0");                                          // flags = 0
    emitter.instruction("mov x2, #0");                                          // errfunc = NULL
    emitter.instruction("ldr x3, [sp, #40]");                                   // struct pointer
    glob_storage::emit_glob_address(emitter, "x3", "x3");
    emitter.bl_c("glob");                                                       // x0 = retcode (0 success, non-zero failure)
    emitter.instruction("cbnz w0, __rt_opendir_glob_fail");                     // glob failed → bail

    // -- populate the glob_state metadata fields --
    glob_storage::emit_initialize(emitter, 40, pathv_off);

    // -- dup(2) to mint a fresh fd we can hand out as the PHP resource value --
    emitter.instruction("mov x0, #2");                                          // duplicate stderr (always available)
    emitter.bl_c("dup");                                                        // x0 = new fd (-1 on failure)
    emitter.instruction("cmp w0, #0");                                          // did dup fail?
    emitter.instruction("b.lt __rt_opendir_glob_fail");                         // dup failed → bail
    emitter.instruction("cmp w0, #255");                                        // out-of-range for the 256-slot table?
    emitter.instruction("b.gt __rt_opendir_glob_reject_fd");                    // can't register this fd

    // -- register the struct in _glob_handles[fd] --
    emitter.instruction("ldr x10, [sp, #40]");                                  // struct pointer
    stream_tables::GLOB_HANDLES.emit_store_indexed_word(emitter, "x10", "x0", "x9");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the frame
    emitter.instruction("ret");                                                 // return the freshly-minted fd

    emitter.label("__rt_opendir_glob_reject_fd");
    emitter.bl_c("close");
    emitter.label("__rt_opendir_glob_fail");
    glob_storage::emit_release(emitter, 40);
    emitter.instruction("mov x0, #-1");                                         // -1 signals a failed glob:// open
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for opendir glob.
fn emit_opendir_glob_linux_x86_64(emitter: &mut Emitter) {
    let pathv_off = emitter.platform.glob_pathv_offset();
    emitter.blank();
    emitter.comment("--- runtime: opendir_glob ---");
    emitter.label_global("__rt_opendir_glob");

    // Frame (rbp-relative): [-8) addr ptr, [-16) addr len, [-24) c_string ptr,
    //   [-32) glob_state struct ptr.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // helper frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the address pointer (elephc x86_64 string ABI: rax)
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the address length (elephc x86_64 string ABI: rdx)

    // -- skip the "glob://" prefix and NUL-terminate the pattern --
    emitter.instruction("add rax, 7");                                          // pattern pointer = addr + 7
    emitter.instruction("sub rdx, 7");                                          // pattern length = addr_len - 7
    emitter.instruction("call __rt_cstr");                                      // rax = NUL-terminated pattern
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the pattern c_string

    // -- allocate the glob_state struct on the heap --
    glob_storage::emit_allocate(emitter);
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the struct pointer

    // -- call libc glob(pattern, 0, NULL, &struct.glob_t) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pattern c_string
    emitter.instruction("xor esi, esi");                                        // flags = 0
    emitter.instruction("xor edx, edx");                                        // errfunc = NULL
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // struct pointer
    glob_storage::emit_glob_address(emitter, "rcx", "rcx");
    emitter.instruction("call glob");                                           // rax = retcode (0 success, non-zero failure)
    emitter.instruction("test eax, eax");                                       // glob failed?
    emitter.instruction("jnz __rt_opendir_glob_fail_x86");                      // bail on failure

    // -- populate the glob_state metadata fields --
    glob_storage::emit_initialize(emitter, 32, pathv_off);

    // -- dup(2) to mint a fresh fd we can hand out as the PHP resource value --
    emitter.instruction("mov edi, 2");                                          // duplicate stderr (always available)
    emitter.instruction("call dup");                                            // rax = new fd (-1 on failure)
    emitter.instruction("test eax, eax");                                       // did dup fail?
    emitter.instruction("js __rt_opendir_glob_fail_x86");                       // negative → bail
    emitter.instruction("cmp eax, 255");                                        // out-of-range for the 256-slot table?
    emitter.instruction("jg __rt_opendir_glob_reject_fd_x86");                  // can't register this fd

    // -- register the struct in _glob_handles[fd] --
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // struct pointer
    stream_tables::GLOB_HANDLES.emit_store_indexed_word(emitter, "r10", "rax", "r9");
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the freshly-minted fd

    emitter.label("__rt_opendir_glob_reject_fd_x86");
    emitter.instruction("mov rdi, rax");                                        // pass the unregistrable descriptor to libc close
    emitter.bl_c("close");
    emitter.label("__rt_opendir_glob_fail_x86");
    glob_storage::emit_release(emitter, 32);
    emitter.instruction("mov rax, -1");                                         // -1 signals a failed glob:// open
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
