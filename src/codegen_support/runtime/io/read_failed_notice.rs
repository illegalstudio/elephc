//! Purpose:
//! Emits `__rt_read_failed_notice`, php's Notice for a read that FAILED after the stream was
//! successfully opened.
//!
//! Called from:
//! - `__rt_fread`'s failed-read path (`crate::codegen_support::runtime::io::fread`), which every
//!   php-level read of a real descriptor goes through — `fread()` itself and
//!   `stream_get_contents()`, which loops on it.
//!
//! Key details:
//! - php names the function, the number of bytes it ASKED for, the `errno` and the system's own
//!   text for it: `Notice: fread(): Read of 8192 bytes failed with errno=9 Bad file descriptor`.
//!   MEASURED on `php -n` 8.5.6 by reading a handle opened `"w"`.
//! - ⚠️ The byte count is the stream's CHUNK SIZE, never the caller's request. Measured, all four
//!   ways: `fread($h, 5)` on a default stream says 8192; `fread($h, 20000)` also says 8192;
//!   `stream_set_chunk_size($h, 100)` makes both say 100.
//! - A read that simply hits EOF is silent, and so is one that would block on a non-blocking
//!   descriptor — both are answered before this is reached.
//! - php has FOUR wordings for a failed WRITE, all MEASURED on `php -n` 8.5.6:
//!
//!   ```text
//!       plain fd opened "r"            fwrite(): Write of 4 bytes failed with errno=9 …
//!       socket whose peer is gone      fwrite(): Send of 4 bytes failed with errno=32 Broken pipe
//!       data:// in any mode            fwrite(): Stream is not writable
//!       php://temp, php://memory "r"   SILENT, just bool(false)
//!   ```
//!
//!   The THIRD is `__rt_not_writable_notice` below: php refuses on the wrapper's OPS, which the
//!   data wrapper has none of, so the recorded mode never enters into it — `data://` opened `"w"`
//!   is as unwritable as one opened `"r"`, and elephc answered that one `int(1)`.
//!
//!   The first is still missing. Its count is the PAYLOAD length, not the chunk — the opposite of
//!   the read half — and elephc's read-only mode gate refuses before the syscall, so the errno it
//!   would have to name (9, `Bad file descriptor`) is never produced. Adding it means the gate
//!   composing the errno itself, which is a different shape from every other diagnostic here.
//! - The function name travels in `_io_fail_fn_ptr` / `_io_fail_fn_len` rather than as an
//!   argument, because the failure is detected deep inside `__rt_fread` while the name belongs to
//!   whoever called it. The globals start out spelling `fread`, and
//!   `__rt_stream_get_contents` swaps its own name in around each `__rt_fread` call.

use crate::codegen_support::runtime::data::FGC_READ_FAILED_MID;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Head of the Notice, before the function name.
pub(crate) const READ_FAILED_NOTICE_HEAD: &str = "Notice: ";

/// Between the function name and the byte count, for a failed READ.
pub(crate) const READ_FAILED_NOTICE_MID: &str = "(): Read of ";

/// The name a zeroed `_io_fail_fn_ptr` stands for on the read side.
pub(crate) const READ_FN_NAME_FREAD: &str = "fread";

/// The name `__rt_stream_get_contents` announces around its own `__rt_fread` calls.
pub(crate) const READ_FN_NAME_STREAM_GET_CONTENTS: &str = "stream_get_contents";

/// Everything after the function name for a stream php will not write to AT ALL.
///
/// No count and no errno: php never reached a syscall, so there is nothing to number. Verbatim
/// from `_php_stream_write`, which refuses when the stream's ops have no write function.
pub(crate) const NOT_WRITABLE_NOTICE_TAIL: &str = "(): Stream is not writable\n";

/// The name a zeroed `_io_fail_fn_ptr` stands for on the write side.
pub(crate) const WRITE_FN_NAME_FWRITE: &str = "fwrite";

/// One half of the Notice: which verb it uses, what a zeroed name global means for it, and the
/// symbol it is entered by.
struct NoticeShape {
    label: &'static str,
    mid_symbol: &'static str,
    mid_len: usize,
    default_name_symbol: &'static str,
    default_name_len: usize,
}

/// A failed READ: `Notice: fread(): Read of 8192 bytes failed with errno=9 …`.
const READ_SHAPE: NoticeShape = NoticeShape {
    label: "__rt_read_failed_notice",
    mid_symbol: "_read_failed_notice_mid",
    mid_len: READ_FAILED_NOTICE_MID.len(),
    default_name_symbol: "_read_fn_name_fread",
    default_name_len: READ_FN_NAME_FREAD.len(),
};

/// Emits `__rt_read_failed_notice(count, errno)`.
///
/// # Input / Output
/// - AArch64: `x0` the byte count php names, `x1` the errno. No result.
/// - x86_64: `rdi` the byte count, `rsi` the errno. No result.
///
/// The shape is a parameter because the WRITE half wants the same body with one word changed. It
/// is not emitted yet: see the header comment for the four wordings php actually uses there.
///
/// The pieces go out one `__rt_diag_warning` call at a time, which is how every other composed
/// diagnostic in this runtime is written: the sink accumulates them, supplies php's leading blank
/// line, and flushes on the piece that ends in a newline, appending ` in FILE on line N`.
pub fn emit_read_failed_notice(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter, &READ_SHAPE),
        Arch::X86_64 => emit_x86_64(emitter, &READ_SHAPE),
    }
}

fn emit_aarch64(emitter: &mut Emitter, shape: &NoticeShape) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", shape.label));
    emitter.label_global(shape.label);
    emitter.instruction("sub sp, sp, #32");                                     // frame for the two numbers
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // park the byte count and the errno

    abi::emit_symbol_address(emitter, "x1", "_read_failed_notice_head");
    emitter.instruction(&format!("mov x2, #{}", READ_FAILED_NOTICE_HEAD.len()));
    emitter.instruction("bl __rt_diag_warning");                                // honours @ and the output-buffer scope
    abi::emit_symbol_address(emitter, "x9", "_io_fail_fn_ptr");
    emitter.instruction("ldr x1, [x9]");                                        // the php function whose read failed
    abi::emit_symbol_address(emitter, "x9", "_io_fail_fn_len");
    emitter.instruction("ldr x2, [x9]");
    emitter.instruction(&format!("cbnz x1, {}_named", shape.label));            // a caller announced itself
    abi::emit_symbol_address(emitter, "x1", shape.default_name_symbol);         // nobody did: it is the plain builtin
    emitter.instruction(&format!("mov x2, #{}", shape.default_name_len));
    emitter.label(&format!("{}_named", shape.label));
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x1", shape.mid_symbol);
    emitter.instruction(&format!("mov x2, #{}", shape.mid_len));
    emitter.instruction("bl __rt_diag_warning");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the byte count php names
    emitter.instruction("bl __rt_itoa");                                        // decimal digits into x1/x2
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x1", "_fgc_read_failed_mid");            // " bytes failed with errno="
    emitter.instruction(&format!("mov x2, #{}", FGC_READ_FAILED_MID.len()));
    emitter.instruction("bl __rt_diag_warning");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the errno
    emitter.instruction("bl __rt_itoa");
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x1", "_diag_space");
    emitter.instruction("mov x2, #1");
    emitter.instruction("bl __rt_diag_warning");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the errno again, for its text
    emitter.instruction("bl __rt_socket_strerror");                             // x0 = message pointer, x1 = its length
    emitter.instruction("mov x2, x1");                                          // the diagnostic sink reads x1/x2
    emitter.instruction("mov x1, x0");
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x1", "_diag_newline");
    emitter.instruction("mov x2, #1");
    emitter.instruction("bl __rt_diag_warning");                                // the newline is what flushes the line

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");
}

fn emit_x86_64(emitter: &mut Emitter, shape: &NoticeShape) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", shape.label));
    emitter.label_global(shape.label);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // frame for the two numbers, 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // park the byte count
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // park the errno

    abi::emit_symbol_address(emitter, "rdi", "_read_failed_notice_head");
    emitter.instruction(&format!("mov esi, {}", READ_FAILED_NOTICE_HEAD.len()));
    emitter.instruction("call __rt_diag_warning");                              // honours @ and the output-buffer scope
    abi::emit_symbol_address(emitter, "r9", "_io_fail_fn_ptr");
    emitter.instruction("mov rdi, QWORD PTR [r9]");                             // the php function whose read failed
    abi::emit_symbol_address(emitter, "r9", "_io_fail_fn_len");
    emitter.instruction("mov rsi, QWORD PTR [r9]");
    emitter.instruction("test rdi, rdi");
    emitter.instruction(&format!("jnz {}_named", shape.label));                 // a caller announced itself
    abi::emit_symbol_address(emitter, "rdi", shape.default_name_symbol);        // nobody did: it is the plain builtin
    emitter.instruction(&format!("mov esi, {}", shape.default_name_len));
    emitter.label(&format!("{}_named", shape.label));
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "rdi", shape.mid_symbol);
    emitter.instruction(&format!("mov esi, {}", shape.mid_len));
    emitter.instruction("call __rt_diag_warning");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the byte count php names
    emitter.instruction("call __rt_itoa");                                      // decimal digits into rax/rdx
    emitter.instruction("mov rdi, rax");
    emitter.instruction("mov rsi, rdx");
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "rdi", "_fgc_read_failed_mid");           // " bytes failed with errno="
    emitter.instruction(&format!("mov esi, {}", FGC_READ_FAILED_MID.len()));
    emitter.instruction("call __rt_diag_warning");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the errno
    emitter.instruction("call __rt_itoa");
    emitter.instruction("mov rdi, rax");
    emitter.instruction("mov rsi, rdx");
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "rdi", "_diag_space");
    emitter.instruction("mov esi, 1");
    emitter.instruction("call __rt_diag_warning");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the errno again, for its text
    emitter.instruction("call __rt_socket_strerror");                           // rax = message pointer, rdx = its length
    emitter.instruction("mov rdi, rax");
    emitter.instruction("mov rsi, rdx");
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "rdi", "_diag_newline");
    emitter.instruction("mov esi, 1");
    emitter.instruction("call __rt_diag_warning");                              // the newline is what flushes the line

    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}

/// Names the php function a failed read inside the next `__rt_fread` belongs to.
///
/// Emitted immediately before the call and undone by [`emit_clear_read_fn_name`] immediately
/// after, so a plain `fread()` later cannot inherit the name. Touches scratch registers only: the
/// stream handle is live in `x0` / `rdi` across this.
pub(crate) fn emit_announce_read_fn_name(emitter: &mut Emitter, symbol: &str, len: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", symbol);
            abi::emit_symbol_address(emitter, "x10", "_io_fail_fn_ptr");
            emitter.instruction("str x9, [x10]");
            emitter.instruction(&format!("mov x9, #{len}"));
            abi::emit_symbol_address(emitter, "x10", "_io_fail_fn_len");
            emitter.instruction("str x9, [x10]");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r9", symbol);
            abi::emit_symbol_address(emitter, "r10", "_io_fail_fn_ptr");
            emitter.instruction("mov QWORD PTR [r10], r9");
            emitter.instruction(&format!("mov r9, {len}"));
            abi::emit_symbol_address(emitter, "r10", "_io_fail_fn_len");
            emitter.instruction("mov QWORD PTR [r10], r9");
        }
    }
}

/// Puts the name back to the zero that stands for `fread`.
///
/// Emitted immediately after the `__rt_fread` call, where `x1`/`x2` and `rax`/`rdx` carry the
/// chunk this read returned — untouched here.
pub(crate) fn emit_clear_read_fn_name(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x10", "_io_fail_fn_ptr");
            emitter.instruction("str xzr, [x10]");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", "_io_fail_fn_ptr");
            emitter.instruction("mov QWORD PTR [r10], 0");
        }
    }
}

/// Emits `__rt_not_writable_notice()`: php's word for a stream whose wrapper cannot write at all.
///
/// Takes no arguments — the whole message is the function name and a fixed tail. The name comes
/// from the same `_io_fail_fn_ptr` pair the read half uses, so a builtin that announces itself
/// around its write is named correctly; a zeroed pair means the caller is `fwrite()` itself.
///
/// Clobbers only what `__rt_diag_warning` does. No result.
pub fn emit_not_writable_notice(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.blank();
            emitter.comment("--- runtime: not_writable_notice ---");
            emitter.label_global("__rt_not_writable_notice");
            emitter.instruction("sub sp, sp, #16");                             // frame for the saved linkage
            emitter.instruction("stp x29, x30, [sp, #0]");                      // save frame pointer and return address
            abi::emit_symbol_address(emitter, "x1", "_read_failed_notice_head");
            emitter.instruction(&format!("mov x2, #{}", READ_FAILED_NOTICE_HEAD.len()));
            emitter.instruction("bl __rt_diag_warning");                        // honours @ and the output-buffer scope
            abi::emit_symbol_address(emitter, "x9", "_io_fail_fn_ptr");
            emitter.instruction("ldr x1, [x9]");                                // the php function whose write was refused
            abi::emit_symbol_address(emitter, "x9", "_io_fail_fn_len");
            emitter.instruction("ldr x2, [x9]");
            emitter.instruction("cbnz x1, __rt_not_writable_notice_named");     // a caller announced itself
            abi::emit_symbol_address(emitter, "x1", "_write_fn_name_fwrite");   // nobody did: it is the plain builtin
            emitter.instruction(&format!("mov x2, #{}", WRITE_FN_NAME_FWRITE.len()));
            emitter.label("__rt_not_writable_notice_named");
            emitter.instruction("bl __rt_diag_warning");
            abi::emit_symbol_address(emitter, "x1", "_not_writable_notice_tail");
            emitter.instruction(&format!("mov x2, #{}", NOT_WRITABLE_NOTICE_TAIL.len()));
            emitter.instruction("bl __rt_diag_warning");                        // the newline is what flushes the line
            emitter.instruction("ldp x29, x30, [sp, #0]");                      // restore frame pointer and return address
            emitter.instruction("add sp, sp, #16");                             // release the frame
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.blank();
            emitter.comment("--- runtime: not_writable_notice ---");
            emitter.label_global("__rt_not_writable_notice");
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the helper frame pointer
            abi::emit_symbol_address(emitter, "rdi", "_read_failed_notice_head");
            emitter.instruction(&format!("mov esi, {}", READ_FAILED_NOTICE_HEAD.len()));
            emitter.instruction("call __rt_diag_warning");                      // honours @ and the output-buffer scope
            abi::emit_symbol_address(emitter, "r9", "_io_fail_fn_ptr");
            emitter.instruction("mov rdi, QWORD PTR [r9]");                     // the php function whose write was refused
            abi::emit_symbol_address(emitter, "r9", "_io_fail_fn_len");
            emitter.instruction("mov rsi, QWORD PTR [r9]");
            emitter.instruction("test rdi, rdi");
            emitter.instruction("jnz __rt_not_writable_notice_named_x86");      // a caller announced itself
            abi::emit_symbol_address(emitter, "rdi", "_write_fn_name_fwrite");  // nobody did: it is the plain builtin
            emitter.instruction(&format!("mov esi, {}", WRITE_FN_NAME_FWRITE.len()));
            emitter.label("__rt_not_writable_notice_named_x86");
            emitter.instruction("call __rt_diag_warning");
            abi::emit_symbol_address(emitter, "rdi", "_not_writable_notice_tail");
            emitter.instruction(&format!("mov esi, {}", NOT_WRITABLE_NOTICE_TAIL.len()));
            emitter.instruction("call __rt_diag_warning");                      // the newline is what flushes the line
            emitter.instruction("mov rsp, rbp");                                // release the frame from rbp
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");
        }
    }
}
