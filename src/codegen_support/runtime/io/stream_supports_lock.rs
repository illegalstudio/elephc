//! Purpose:
//! Emits `__rt_stream_supports_lock`, which answers `stream_supports_lock()` from the
//! wrapper a stream was opened through.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The `stream_supports_lock()` lowering.
//!
//! Key details:
//! - php-src answers from the stream's ops: a descriptor-backed stream carries the lock
//!   option, the memory and output wrappers do not. Measured against `php -n` 8.5.6, that
//!   makes exactly `php://memory`, `php://temp`, `php://output` and `php://input` answer
//!   false, while a file, `tmpfile()`, `php://stdout` and `STDIN` answer true.
//! - Those four are told apart the way `stream_type_name` tells them apart: wrapper id
//!   `php` plus the byte after `php://`. A descriptor test cannot do it, because elephc
//!   backs `php://memory` with a real temporary descriptor.

use crate::codegen_support::runtime::resources::layout::{
    STREAM_OWNERSHIP_FLAGS_OFFSET, STREAM_STATE_FLAG_UNLINK_ON_CLOSE,
    STREAM_URI_LEN_OFFSET, STREAM_URI_PTR_OFFSET, STREAM_WRAPPER_ID_OFFSET,
};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Wrapper id recorded for a `php://` stream.
const WRAPPER_ID_PHP: u64 = 6;
/// The `data://` wrapper id, which reports no lock option and is not local.
const WRAPPER_ID_DATA: u64 = 7;

/// Emits `__rt_stream_supports_lock(handle) -> 1 lockable / 0 not`.
pub fn emit_stream_supports_lock(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 form.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_supports_lock ---");
    emitter.label_global("__rt_stream_supports_lock");
    // The frame carries the incoming handle and a stat buffer: the socket test below needs the
    // DESCRIPTOR, which only the handle can produce, and `__rt_stream_state` consumes x0.
    let stat_buf = emitter.platform.stat_buf_size(emitter.target.arch);
    let buf_off = 32;
    let frame = (buf_off + stat_buf + 15) & !15;
    let mode_off = buf_off + emitter.platform.stat_mode_offset(emitter.target.arch);
    emitter.instruction(&format!("sub sp, sp, #{}", frame));                    // frame for the linkage, the handle and a stat buffer
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // keep the handle for the descriptor lookup
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_ssl_yes");                                // no state: keep the permissive answer
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_WRAPPER_ID_OFFSET}]")); // which wrapper opened it
    emitter.instruction(&format!("cmp x9, #{WRAPPER_ID_DATA}"));
    emitter.instruction("b.eq __rt_ssl_no");                                    // data:// carries its payload in the URI: nothing to lock
    emitter.instruction(&format!("cmp x9, #{WRAPPER_ID_PHP}"));
    emitter.instruction("b.ne __rt_ssl_yes");                                   // otherwise only the php:// wrappers lack the lock option
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_URI_PTR_OFFSET}]"));   // the recorded URI
    emitter.instruction(&format!("ldr x11, [x0, #{STREAM_URI_LEN_OFFSET}]"));   // and its length
    emitter.instruction("cbz x10, __rt_ssl_yes");                               // no URI to classify
    emitter.instruction("cmp x11, #7");                                         // "php://" plus the naming byte
    emitter.instruction("b.lt __rt_ssl_yes");
    emitter.instruction("ldrb w12, [x10, #6]");                                 // the first byte of the php:// wrapper name
    emitter.instruction("cmp w12, #0x6D");                                      // 'm' as in memory
    emitter.instruction("b.eq __rt_ssl_no");
    emitter.instruction("cmp w12, #0x74");                                      // 't' as in temp
    emitter.instruction("b.eq __rt_ssl_no");
    emitter.instruction("cmp w12, #0x6F");                                      // 'o' as in output
    emitter.instruction("b.eq __rt_ssl_no");
    emitter.instruction("cmp w12, #0x69");                                      // 'i' as in input
    emitter.instruction("b.eq __rt_ssl_no");
    emitter.label("__rt_ssl_yes");
    // A socket has no locking option in php, and no wrapper id says so: `stream_socket_pair()`
    // and `fopen()` are recorded the same way. Only the descriptor knows.
    emitter.instruction("ldr x0, [sp, #16]");                                   // the handle this call was given
    emitter.instruction("bl __rt_stream_fd");                                   // its backend descriptor
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.lt __rt_ssl_yes_final");                             // nothing to stat: keep the permissive answer
    emitter.instruction(&format!("add x1, sp, #{}", buf_off));                  // the stat buffer
    emitter.syscall(339);                                                       // fstat(fd, buf)
    emitter.instruction("cbnz x0, __rt_ssl_yes_final");                         // fstat refused: keep the answer
    emitter.instruction(&emitter.platform.stat_mode_load_instr("w9", "sp", mode_off));
    emitter.instruction("and w9, w9, #0xF000");                                 // S_IFMT
    emitter.instruction("cmp w9, #0xC000");                                     // S_IFSOCK
    emitter.instruction("b.eq __rt_ssl_no");                                    // a socket stream cannot lock
    emitter.label("__rt_ssl_yes_final");
    emitter.instruction("mov x0, #1");                                          // stdin/stdout/stderr, fd, and every other wrapper lock
    emitter.instruction("b __rt_ssl_ret");
    emitter.label("__rt_ssl_no");
    emitter.instruction("mov x0, #0");                                          // the memory and output wrappers do not
    emitter.label("__rt_ssl_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame));                    // release the helper frame
    emitter.instruction("ret");
}

/// Emits the Linux x86_64 form.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_supports_lock ---");
    emitter.label_global("__rt_stream_supports_lock");
    // See the AArch64 arm: the socket test needs the descriptor, so the handle is kept.
    let stat_buf = emitter.platform.stat_buf_size(emitter.target.arch);
    let handle_off = 8;
    let buf_off = 16 + stat_buf;
    let frame = (buf_off + 15) & !15;
    let mode_off = buf_off - emitter.platform.stat_mode_offset(emitter.target.arch);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction(&format!("sub rsp, {}", frame));                        // room for the handle and a stat buffer
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", handle_off)); // keep the handle for the descriptor lookup
    emitter.instruction("call __rt_stream_state");                              // resolve the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_ssl_yes_x86");                                 // no state: keep the permissive answer
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rax + {STREAM_WRAPPER_ID_OFFSET}]"
    ));                                                                         // which wrapper opened it
    emitter.instruction(&format!("cmp r9, {WRAPPER_ID_DATA}"));
    emitter.instruction("je __rt_ssl_no_x86");                                  // data:// carries its payload in the URI: nothing to lock
    emitter.instruction(&format!("cmp r9, {WRAPPER_ID_PHP}"));
    emitter.instruction("jne __rt_ssl_yes_x86");                                // only the php:// wrappers lack the lock option
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_URI_PTR_OFFSET}]"
    ));                                                                         // the recorded URI
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {STREAM_URI_LEN_OFFSET}]"
    ));                                                                         // and its length
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_ssl_yes_x86");                                 // no URI to classify
    emitter.instruction("cmp r11, 7");                                          // "php://" plus the naming byte
    emitter.instruction("jl __rt_ssl_yes_x86");
    emitter.instruction("movzx r9d, BYTE PTR [r10 + 6]");                       // the first byte of the php:// wrapper name
    emitter.instruction("cmp r9d, 0x6D");                                       // 'm' as in memory
    emitter.instruction("je __rt_ssl_no_x86");
    emitter.instruction("cmp r9d, 0x74");                                       // 't' as in temp
    emitter.instruction("je __rt_ssl_no_x86");
    emitter.instruction("cmp r9d, 0x6F");                                       // 'o' as in output
    emitter.instruction("je __rt_ssl_no_x86");
    emitter.instruction("cmp r9d, 0x69");                                       // 'i' as in input
    emitter.instruction("je __rt_ssl_no_x86");
    emitter.label("__rt_ssl_yes_x86");
    // See the AArch64 arm: only the descriptor can say whether this is a socket.
    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", handle_off)); // the handle this call was given
    emitter.instruction("call __rt_stream_fd");                                 // its backend descriptor
    emitter.instruction("cmp rax, 0");
    emitter.instruction("jl __rt_ssl_yes_final_x86");                           // nothing to stat: keep the permissive answer
    emitter.instruction("mov rdi, rax");                                        // fd → first libc fstat() argument
    emitter.instruction(&format!("lea rsi, [rbp - {}]", buf_off));              // the stat buffer
    emitter.instruction("call fstat");
    emitter.instruction("cmp eax, 0");
    emitter.instruction("jne __rt_ssl_yes_final_x86");                          // fstat refused: keep the answer
    emitter.instruction(&format!("mov r9d, DWORD PTR [rbp - {}]", mode_off));   // st_mode
    emitter.instruction("and r9d, 0xF000");                                     // S_IFMT
    emitter.instruction("cmp r9d, 0xC000");                                     // S_IFSOCK
    emitter.instruction("je __rt_ssl_no_x86");                                  // a socket stream cannot lock
    emitter.label("__rt_ssl_yes_final_x86");
    emitter.instruction("mov eax, 1");                                          // stdin/stdout/stderr, fd, and every other wrapper lock
    emitter.instruction("jmp __rt_ssl_ret_x86");
    emitter.label("__rt_ssl_no_x86");
    emitter.instruction("xor eax, eax");                                        // the memory and output wrappers do not
    emitter.label("__rt_ssl_ret_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}

/// Emits `__rt_stream_own_its_file(handle)`: marks a stream as owning the file its URI names.
///
/// `tmpfile()` is the only caller. php keeps that file LINKED while the handle lives and removes
/// it on close, so the close path needs to know which streams to clean up after; the flag is what
/// tells it. Everything else opened a file it did not create.
pub fn emit_stream_own_its_file(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: this stream owns the file its URI names ---");
    emitter.label_global("__rt_stream_own_its_file");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");
            emitter.instruction("str x30, [sp, #8]");
            emitter.instruction("bl __rt_stream_state");                        // resolve the owning stream state
            emitter.instruction("cbz x0, __rt_soif_ret");                       // a stale handle owns nothing
            emitter.instruction(&format!(
                "ldr x9, [x0, #{}]", STREAM_OWNERSHIP_FLAGS_OFFSET
            ));
            emitter.instruction(&format!(
                "orr x9, x9, #{}", STREAM_STATE_FLAG_UNLINK_ON_CLOSE
            ));
            emitter.instruction(&format!(
                "str x9, [x0, #{}]", STREAM_OWNERSHIP_FLAGS_OFFSET
            ));
            emitter.label("__rt_soif_ret");
            emitter.instruction("ldr x30, [sp, #8]");
            emitter.instruction("add sp, sp, #16");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("call __rt_stream_state");                      // resolve the owning stream state
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_soif_ret_x86");                        // a stale handle owns nothing
            emitter.instruction(&format!(
                "or QWORD PTR [rax + {}], {}",
                STREAM_OWNERSHIP_FLAGS_OFFSET, STREAM_STATE_FLAG_UNLINK_ON_CLOSE
            ));
            emitter.label("__rt_soif_ret_x86");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_stream_unlink_if_owned(handle)`: removes the file a stream owns, once.
///
/// Only `tmpfile()` marks a stream this way. The flag is cleared before the unlink so the two
/// callers — an explicit `fclose()`, which closes its own descriptor, and the backend close every
/// other destruction goes through — cannot both remove a path that a later stream may have been
/// given by the operating system.
pub fn emit_stream_unlink_if_owned(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: remove the file a stream owns ---");
    emitter.label_global("__rt_stream_unlink_if_owned");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");
            emitter.instruction("str x30, [sp, #8]");
            emitter.instruction("bl __rt_stream_state");                        // resolve the owning stream state
            emitter.instruction("cbz x0, __rt_suio_ret");                       // a stale handle owns nothing
            emitter.instruction(&format!(
                "ldr x9, [x0, #{}]", STREAM_OWNERSHIP_FLAGS_OFFSET
            ));
            emitter.instruction(&format!(
                "tst x9, #{}", STREAM_STATE_FLAG_UNLINK_ON_CLOSE
            ));                                                                 // does this stream own its file?
            emitter.instruction("b.eq __rt_suio_ret");
            emitter.instruction(&format!(
                "bic x9, x9, #{}", STREAM_STATE_FLAG_UNLINK_ON_CLOSE
            ));                                                                 // exactly once, whichever caller arrives first
            emitter.instruction(&format!(
                "str x9, [x0, #{}]", STREAM_OWNERSHIP_FLAGS_OFFSET
            ));
            emitter.instruction(&format!("ldr x1, [x0, #{}]", STREAM_URI_PTR_OFFSET));
            emitter.instruction(&format!("ldr x2, [x0, #{}]", STREAM_URI_LEN_OFFSET));
            emitter.instruction("cbz x1, __rt_suio_ret");                       // nothing recorded: nothing to remove
            emitter.instruction("bl __rt_cstr");                                // the path as a C string
            emitter.bl_c("unlink");                                             // a missing file is not an error here
            emitter.label("__rt_suio_ret");
            emitter.instruction("ldr x30, [sp, #8]");
            emitter.instruction("add sp, sp, #16");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("call __rt_stream_state");                      // resolve the owning stream state
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_suio_ret_x86");                        // a stale handle owns nothing
            emitter.instruction(&format!(
                "test QWORD PTR [rax + {}], {}",
                STREAM_OWNERSHIP_FLAGS_OFFSET, STREAM_STATE_FLAG_UNLINK_ON_CLOSE
            ));                                                                 // does this stream own its file?
            emitter.instruction("jz __rt_suio_ret_x86");
            emitter.instruction(&format!(
                "and QWORD PTR [rax + {}], {}",
                STREAM_OWNERSHIP_FLAGS_OFFSET, !STREAM_STATE_FLAG_UNLINK_ON_CLOSE as i64
            ));                                                                 // exactly once, whichever caller arrives first
            emitter.instruction(&format!(
                "mov r10, QWORD PTR [rax + {}]", STREAM_URI_PTR_OFFSET
            ));
            emitter.instruction(&format!(
                "mov rdx, QWORD PTR [rax + {}]", STREAM_URI_LEN_OFFSET
            ));
            emitter.instruction("test r10, r10");
            emitter.instruction("jz __rt_suio_ret_x86");                        // nothing recorded: nothing to remove
            emitter.instruction("mov rax, r10");
            emitter.instruction("call __rt_cstr");                              // the path as a C string
            emitter.instruction("mov rdi, rax");
            emitter.instruction("call unlink");                                 // a missing file is not an error here
            emitter.label("__rt_suio_ret_x86");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}
