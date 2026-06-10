//! Purpose:
//! Emits runtime dispatch helpers for synthetic user-wrapper file descriptors.
//! `__rt_user_wrapper_fclose`/`fread`/`fwrite`/`feof` translate a fopen-returned
//! synthetic fd into a call against the wrapper object's PHP-side method, looked
//! up through `_user_wrapper_vtable_<class_id>` (a fixed-slot table; the first
//! slots are stream_open, stream_close, stream_read, stream_write, stream_eof,
//! stream_tell, stream_seek, stream_flush, then stream_stat/url_stat and the
//! G1 surface — stream_lock at slot 11 and stream_truncate at slot 12 are wired
//! here; see `USER_WRAPPER_METHOD_NAMES` for the full slot order).
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::io`.
//! - The fread/fwrite/fclose/feof builtin emitters branch into these helpers
//!   when the resolved file descriptor is `>= 0x40000000`
//!   (`USER_WRAPPER_FD_BASE`).
//!
//! Key details:
//! - The synthetic fd encodes the handle slot in its low 6 bits
//!   (0x40000000 | slot_index). Slot zero is reserved-free when the table is
//!   empty; the helpers do not bounds-check beyond reading the handle table.
//! - The wrapper object is referenced through `_user_wrapper_handles[slot]`.
//!   When the slot is empty the helpers fall through to the "missing method"
//!   path: 0 bytes / `false` / EOF / NULL string, matching PHP's behavior when
//!   a wrapper method is absent.
//! - Wrapper methods follow the regular elephc method ABI: `$this` in the
//!   first int-arg register, then standard parameter packing. Returns are
//!   expected in their declared register form — string in the string-result
//!   pair, int/bool in the int-result register — so wrapper classes should
//!   declare `: string`/`: int`/`: bool` on the methods they implement.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

const FD_BASE_LOW16: u32 = 0x4000;
const FD_BASE: u32 = 0x40000000;
const VTABLE_SLOT_CLOSE: usize = 1;
const VTABLE_SLOT_READ: usize = 2;
const VTABLE_SLOT_WRITE: usize = 3;
const VTABLE_SLOT_EOF: usize = 4;
const VTABLE_SLOT_TELL: usize = 5;
const VTABLE_SLOT_SEEK: usize = 6;
const VTABLE_SLOT_FLUSH: usize = 7;
const VTABLE_SLOT_STAT: usize = 8;
const VTABLE_SLOT_LOCK: usize = 11;
const VTABLE_SLOT_TRUNCATE: usize = 12;

/// `__rt_user_wrapper_fclose`: invoke the wrapper's `stream_close` (if any)
/// and free the handle slot. Always returns 1 (`true`) once the slot is
/// freed, mirroring PHP's "fclose succeeded as far as we can tell" semantics
/// for wrappers; an empty slot still returns 1 so callers see consistent
/// fclose-after-fclose behavior.
pub fn emit_user_wrapper_fclose(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_fclose_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_fclose ---");
    emitter.label_global("__rt_user_wrapper_fclose");

    // Frame: 32 bytes. [sp, #0..16] saved x29/x30. [sp, #16..24] saved fd.
    emitter.instruction("sub sp, sp, #32");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the synthetic file descriptor

    emit_aarch64_handle_lookup(emitter, "__rt_uwfclose_clear");                 // resolve obj into x0, fall through to slot-clear on missing handles
    emit_aarch64_method_lookup(emitter, "__rt_uwfclose_clear", VTABLE_SLOT_CLOSE); // resolve stream_close method pointer into x11

    // -- call stream_close($this) --
    emitter.instruction("blr x11");                                             // invoke stream_close on the wrapper object

    emitter.label("__rt_uwfclose_clear");
    // -- free the handle slot so the synthetic fd cannot be reused stale --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the synthetic file descriptor
    emit_aarch64_slot_from_fd(emitter, "x0", "x9");                             // x9 = fd & 0x3f, the handle slot index
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_handles");
    emitter.instruction("str xzr, [x10, x9, lsl #3]");                          // clear the freed handle slot
    emitter.instruction("mov x0, #1");                                          // fclose() on a wrapper always reports success
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the inline fclose dispatch site
}

/// Emits the Linux x86_64 stream runtime helper for user wrapper fclose.
fn emit_user_wrapper_fclose_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_fclose ---");
    emitter.label_global("__rt_user_wrapper_fclose");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // helper frame for the wrapper dispatch
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the synthetic file descriptor

    emit_x86_handle_lookup(emitter, "__rt_uwfclose_clear_x86");                 // resolve obj into rdi, fall through on missing handles
    emit_x86_method_lookup(emitter, "__rt_uwfclose_clear_x86", VTABLE_SLOT_CLOSE); // resolve stream_close method pointer into r11

    // -- call stream_close($this) --
    emitter.instruction("call r11");                                            // invoke stream_close on the wrapper object

    emitter.label("__rt_uwfclose_clear_x86");
    // -- free the handle slot so the synthetic fd cannot be reused stale --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the synthetic file descriptor
    emit_x86_slot_from_fd(emitter, "rdi", "r9");                                // r9 = fd & 0x3f, the handle slot index
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_handles");          // handle table base
    emitter.instruction("mov QWORD PTR [r10 + r9 * 8], 0");                     // clear the freed handle slot
    emitter.instruction("mov eax, 1");                                          // fclose() on a wrapper always reports success
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the inline fclose dispatch site
}

/// `__rt_user_wrapper_fread`: invoke the wrapper's `stream_read($count)`
/// and return its declared string result (x1/x2 on ARM64, rax/rdx on x86_64).
/// When the method is absent, returns the empty string.
pub fn emit_user_wrapper_fread(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_fread_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_fread ---");
    emitter.label_global("__rt_user_wrapper_fread");

    // Frame: 32 bytes. [sp, #0..16] saved x29/x30, [sp, #16..24] saved fd,
    //   [sp, #24..32] saved requested length.
    emitter.instruction("sub sp, sp, #32");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the synthetic file descriptor
    emitter.instruction("str x1, [sp, #24]");                                   // save the requested read length across the helper call

    emit_aarch64_handle_lookup(emitter, "__rt_uwfread_empty");                  // resolve obj into x0, fall through to empty-string on missing handles
    emit_aarch64_method_lookup(emitter, "__rt_uwfread_empty", VTABLE_SLOT_READ); // resolve stream_read method pointer into x11

    // -- call stream_read($this, $count) → returns string in x1/x2 --
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the requested byte count
    emitter.instruction("blr x11");                                             // invoke stream_read on the wrapper object
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the wrapper's string result to the caller

    emitter.label("__rt_uwfread_empty");
    emitter.instruction("mov x1, #0");                                          // empty-string pointer for the missing stream_read fallback
    emitter.instruction("mov x2, #0");                                          // empty-string length for the missing stream_read fallback
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the empty-string result
}

/// Emits the Linux x86_64 stream runtime helper for user wrapper fread.
fn emit_user_wrapper_fread_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_fread ---");
    emitter.label_global("__rt_user_wrapper_fread");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // helper frame for the wrapper dispatch
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the synthetic file descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested read length

    emit_x86_handle_lookup(emitter, "__rt_uwfread_empty_x86");                  // resolve obj into rdi, fall through on missing handles
    emit_x86_method_lookup(emitter, "__rt_uwfread_empty_x86", VTABLE_SLOT_READ); // resolve stream_read method pointer into r11

    // -- call stream_read($this, $count) → returns string in rax/rdx --
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the requested byte count
    emitter.instruction("call r11");                                            // invoke stream_read on the wrapper object
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the wrapper's string result to the caller

    emitter.label("__rt_uwfread_empty_x86");
    emitter.instruction("xor eax, eax");                                        // empty-string pointer for the missing stream_read fallback
    emitter.instruction("xor edx, edx");                                        // empty-string length for the missing stream_read fallback
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the empty-string result
}

/// `__rt_user_wrapper_fwrite`: invoke the wrapper's `stream_write($data)`
/// and return its declared int result. When the method is absent, returns 0.
pub fn emit_user_wrapper_fwrite(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_fwrite_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_fwrite ---");
    emitter.label_global("__rt_user_wrapper_fwrite");

    // Frame: 32 bytes. [sp, #0..16] saved x29/x30. [sp, #16..24] data ptr.
    //   [sp, #24..32] data len.
    emitter.instruction("sub sp, sp, #32");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("stp x1, x2, [sp, #16]");                               // save the data string pointer/length across the helper call

    emit_aarch64_handle_lookup(emitter, "__rt_uwfwrite_zero");                  // resolve obj into x0, fall through to zero on missing handles
    emit_aarch64_method_lookup(emitter, "__rt_uwfwrite_zero", VTABLE_SLOT_WRITE); // resolve stream_write method pointer into x11

    // -- call stream_write($this, $data) → returns int in x0 --
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload data string ptr/len for the second argument pair
    emitter.instruction("blr x11");                                             // invoke stream_write on the wrapper object
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the wrapper's int result to the caller

    emitter.label("__rt_uwfwrite_zero");
    emitter.instruction("mov x0, #0");                                          // zero-byte fallback for the missing stream_write
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return 0 bytes written
}

/// Emits the Linux x86_64 stream runtime helper for user wrapper fwrite.
fn emit_user_wrapper_fwrite_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_fwrite ---");
    emitter.label_global("__rt_user_wrapper_fwrite");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // helper frame for the wrapper dispatch
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the data string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the data string length

    // rdi already holds the synthetic fd from the builtin call site; the
    // handle lookup expects the fd in rdi so no extra reload is needed.
    emit_x86_handle_lookup(emitter, "__rt_uwfwrite_zero_x86");                  // resolve obj into rdi, fall through on missing handles
    emit_x86_method_lookup(emitter, "__rt_uwfwrite_zero_x86", VTABLE_SLOT_WRITE); // resolve stream_write method pointer into r11

    // -- call stream_write($this, $data) → returns int in rax --
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload data string pointer as the second arg
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload data string length as the third arg
    emitter.instruction("call r11");                                            // invoke stream_write on the wrapper object
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the wrapper's int result to the caller

    emitter.label("__rt_uwfwrite_zero_x86");
    emitter.instruction("xor eax, eax");                                        // zero-byte fallback for the missing stream_write
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return 0 bytes written
}

/// `__rt_user_wrapper_feof`: invoke the wrapper's `stream_eof()` and return
/// its declared bool result. When the method is absent, returns 1 (EOF) so
/// callers that loop until feof terminate instead of spinning.
pub fn emit_user_wrapper_feof(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_feof_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_feof ---");
    emitter.label_global("__rt_user_wrapper_feof");

    emitter.instruction("sub sp, sp, #16");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    emit_aarch64_handle_lookup(emitter, "__rt_uwfeof_eof");                     // resolve obj into x0, fall through to EOF on missing handles
    emit_aarch64_method_lookup(emitter, "__rt_uwfeof_eof", VTABLE_SLOT_EOF);    // resolve stream_eof method pointer into x11

    emitter.instruction("blr x11");                                             // invoke stream_eof on the wrapper object
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the wrapper's bool result to the caller

    emitter.label("__rt_uwfeof_eof");
    emitter.instruction("mov x0, #1");                                          // report EOF when the wrapper does not implement stream_eof
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return EOF
}

/// Emits the Linux x86_64 stream runtime helper for user wrapper feof.
fn emit_user_wrapper_feof_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_feof ---");
    emitter.label_global("__rt_user_wrapper_feof");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    emit_x86_handle_lookup(emitter, "__rt_uwfeof_eof_x86");                     // resolve obj into rdi, fall through on missing handles
    emit_x86_method_lookup(emitter, "__rt_uwfeof_eof_x86", VTABLE_SLOT_EOF);    // resolve stream_eof method pointer into r11

    emitter.instruction("call r11");                                            // invoke stream_eof on the wrapper object
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the wrapper's bool result to the caller

    emitter.label("__rt_uwfeof_eof_x86");
    emitter.instruction("mov eax, 1");                                          // report EOF when the wrapper does not implement stream_eof
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return EOF
}

/// `__rt_user_wrapper_ftell`: invoke the wrapper's `stream_tell()` and return
/// its declared int result. When the method is absent, returns -1 — PHP's
/// ftell failure sentinel.
pub fn emit_user_wrapper_ftell(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_ftell_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_ftell ---");
    emitter.label_global("__rt_user_wrapper_ftell");

    emitter.instruction("sub sp, sp, #16");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    emit_aarch64_handle_lookup(emitter, "__rt_uwftell_fail");                   // resolve obj into x0, fall through to -1 on missing handles
    emit_aarch64_method_lookup(emitter, "__rt_uwftell_fail", VTABLE_SLOT_TELL); // resolve stream_tell method pointer into x11

    emitter.instruction("blr x11");                                             // invoke stream_tell on the wrapper object
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the wrapper's int result to the caller

    emitter.label("__rt_uwftell_fail");
    emitter.instruction("mov x0, #-1");                                         // ftell failure sentinel for missing handle/method
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return -1
}

/// Emits the Linux x86_64 stream runtime helper for user wrapper ftell.
fn emit_user_wrapper_ftell_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_ftell ---");
    emitter.label_global("__rt_user_wrapper_ftell");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    emit_x86_handle_lookup(emitter, "__rt_uwftell_fail_x86");                   // resolve obj into rdi, fall through on missing handles
    emit_x86_method_lookup(emitter, "__rt_uwftell_fail_x86", VTABLE_SLOT_TELL); // resolve stream_tell method pointer into r11

    emitter.instruction("call r11");                                            // invoke stream_tell on the wrapper object
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the wrapper's int result to the caller

    emitter.label("__rt_uwftell_fail_x86");
    emitter.instruction("mov rax, -1");                                         // ftell failure sentinel for missing handle/method
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return -1
}

/// `__rt_user_wrapper_fflush`: invoke the wrapper's `stream_flush()` and
/// return its declared bool result. When the method is absent, returns 1 —
/// fflush's "nothing to do, treat as success" convention.
pub fn emit_user_wrapper_fflush(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_fflush_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_fflush ---");
    emitter.label_global("__rt_user_wrapper_fflush");

    emitter.instruction("sub sp, sp, #16");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    emit_aarch64_handle_lookup(emitter, "__rt_uwfflush_ok");                    // resolve obj into x0, fall through to default-true on missing handles
    emit_aarch64_method_lookup(emitter, "__rt_uwfflush_ok", VTABLE_SLOT_FLUSH); // resolve stream_flush method pointer into x11

    emitter.instruction("blr x11");                                             // invoke stream_flush on the wrapper object
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the wrapper's bool result to the caller

    emitter.label("__rt_uwfflush_ok");
    emitter.instruction("mov x0, #1");                                          // report success when the wrapper does not implement stream_flush
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return true
}

/// Emits the Linux x86_64 stream runtime helper for user wrapper fflush.
fn emit_user_wrapper_fflush_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_fflush ---");
    emitter.label_global("__rt_user_wrapper_fflush");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    emit_x86_handle_lookup(emitter, "__rt_uwfflush_ok_x86");                    // resolve obj into rdi, fall through on missing handles
    emit_x86_method_lookup(emitter, "__rt_uwfflush_ok_x86", VTABLE_SLOT_FLUSH); // resolve stream_flush method pointer into r11

    emitter.instruction("call r11");                                            // invoke stream_flush on the wrapper object
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the wrapper's bool result to the caller

    emitter.label("__rt_uwfflush_ok_x86");
    emitter.instruction("mov eax, 1");                                          // report success when the wrapper does not implement stream_flush
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return true
}

/// `__rt_user_wrapper_fseek`: invoke the wrapper's `stream_seek($offset,
/// $whence)` and return 0 on success, -1 on failure. When the method is
/// absent, returns -1 — fseek's PHP failure sentinel.
pub fn emit_user_wrapper_fseek(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_fseek_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_fseek ---");
    emitter.label_global("__rt_user_wrapper_fseek");

    // Frame: 32 bytes. [sp, #0..16] saved x29/x30. [sp, #16..24] offset.
    //   [sp, #24..32] whence.
    emitter.instruction("sub sp, sp, #32");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("stp x1, x2, [sp, #16]");                               // save offset and whence across the helper call

    emit_aarch64_handle_lookup(emitter, "__rt_uwfseek_fail");                   // resolve obj into x0, fall through to -1 on missing handles
    emit_aarch64_method_lookup(emitter, "__rt_uwfseek_fail", VTABLE_SLOT_SEEK); // resolve stream_seek method pointer into x11

    // -- call stream_seek($this, $offset, $whence) → returns bool/int in x0 --
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload offset (x1) and whence (x2)
    emitter.instruction("blr x11");                                             // invoke stream_seek on the wrapper object
    emitter.instruction("cbz x0, __rt_uwfseek_fail");                           // stream_seek returned false → PHP -1 failure sentinel
    emitter.instruction("mov x0, #0");                                          // stream_seek succeeded → PHP fseek returns 0
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return success

    emitter.label("__rt_uwfseek_fail");
    emitter.instruction("mov x0, #-1");                                         // fseek failure sentinel for missing handle/method or false return
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return -1
}

/// Emits the Linux x86_64 stream runtime helper for user wrapper fseek.
fn emit_user_wrapper_fseek_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_fseek ---");
    emitter.label_global("__rt_user_wrapper_fseek");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // helper frame for the wrapper dispatch
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the offset across the helper call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the whence selector across the helper call

    emit_x86_handle_lookup(emitter, "__rt_uwfseek_fail_x86");                   // resolve obj into rdi, fall through on missing handles
    emit_x86_method_lookup(emitter, "__rt_uwfseek_fail_x86", VTABLE_SLOT_SEEK); // resolve stream_seek method pointer into r11

    // -- call stream_seek($this, $offset, $whence) → returns bool/int in rax --
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload offset
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload whence selector
    emitter.instruction("call r11");                                            // invoke stream_seek on the wrapper object
    emitter.instruction("test rax, rax");                                       // did stream_seek return false?
    emitter.instruction("jz __rt_uwfseek_fail_x86");                            // stream_seek returned false → PHP -1 failure sentinel
    emitter.instruction("xor eax, eax");                                        // stream_seek succeeded → PHP fseek returns 0
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return success

    emitter.label("__rt_uwfseek_fail_x86");
    emitter.instruction("mov rax, -1");                                         // fseek failure sentinel for missing handle/method or false return
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return -1
}

/// `__rt_user_wrapper_flock`: invoke the wrapper's `stream_lock($operation)`
/// (vtable slot 11) and return its declared bool result. When the handle or
/// method is absent, returns 0 (`false`) — PHP's `flock()` result for a
/// wrapper that does not implement locking.
pub fn emit_user_wrapper_flock(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_flock_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_flock ---");
    emitter.label_global("__rt_user_wrapper_flock");

    emitter.instruction("sub sp, sp, #16");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // The lock operation stays in x1 across both lookups (neither touches it).
    emit_aarch64_handle_lookup(emitter, "__rt_uwflock_false");                  // resolve obj into x0, fall through to false on missing handles
    emit_aarch64_method_lookup(emitter, "__rt_uwflock_false", VTABLE_SLOT_LOCK); // resolve stream_lock method pointer into x11

    // -- call stream_lock($this, $operation) → returns bool in x0 --
    emitter.instruction("blr x11");                                             // invoke stream_lock on the wrapper object
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the wrapper's bool result to the caller

    emitter.label("__rt_uwflock_false");
    emitter.instruction("mov x0, #0");                                          // false when the wrapper does not implement stream_lock
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return false
}

/// Emits the Linux x86_64 stream runtime helper for user wrapper flock.
fn emit_user_wrapper_flock_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_flock ---");
    emitter.label_global("__rt_user_wrapper_flock");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    // The lock operation stays in rsi across both lookups (neither touches it).
    emit_x86_handle_lookup(emitter, "__rt_uwflock_false_x86");                  // resolve obj into rdi, fall through on missing handles
    emit_x86_method_lookup(emitter, "__rt_uwflock_false_x86", VTABLE_SLOT_LOCK); // resolve stream_lock method pointer into r11

    // -- call stream_lock($this, $operation) → returns bool in rax --
    emitter.instruction("call r11");                                            // invoke stream_lock on the wrapper object
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the wrapper's bool result to the caller

    emitter.label("__rt_uwflock_false_x86");
    emitter.instruction("xor eax, eax");                                        // false when the wrapper does not implement stream_lock
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return false
}

/// `__rt_user_wrapper_ftruncate`: invoke the wrapper's
/// `stream_truncate($new_size)` (vtable slot 12) and return its declared bool
/// result. When the handle or method is absent, returns 0 (`false`) — PHP's
/// `ftruncate()` result for a wrapper that does not implement truncation.
pub fn emit_user_wrapper_ftruncate(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_ftruncate_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_ftruncate ---");
    emitter.label_global("__rt_user_wrapper_ftruncate");

    emitter.instruction("sub sp, sp, #16");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // The new size stays in x1 across both lookups (neither touches it).
    emit_aarch64_handle_lookup(emitter, "__rt_uwftrunc_false");                 // resolve obj into x0, fall through to false on missing handles
    emit_aarch64_method_lookup(emitter, "__rt_uwftrunc_false", VTABLE_SLOT_TRUNCATE); // resolve stream_truncate method pointer into x11

    // -- call stream_truncate($this, $new_size) → returns bool in x0 --
    emitter.instruction("blr x11");                                             // invoke stream_truncate on the wrapper object
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the wrapper's bool result to the caller

    emitter.label("__rt_uwftrunc_false");
    emitter.instruction("mov x0, #0");                                          // false when the wrapper does not implement stream_truncate
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return false
}

/// Emits the Linux x86_64 stream runtime helper for user wrapper ftruncate.
fn emit_user_wrapper_ftruncate_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_ftruncate ---");
    emitter.label_global("__rt_user_wrapper_ftruncate");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    // The new size stays in rsi across both lookups (neither touches it).
    emit_x86_handle_lookup(emitter, "__rt_uwftrunc_false_x86");                 // resolve obj into rdi, fall through on missing handles
    emit_x86_method_lookup(emitter, "__rt_uwftrunc_false_x86", VTABLE_SLOT_TRUNCATE); // resolve stream_truncate method pointer into r11

    // -- call stream_truncate($this, $new_size) → returns bool in rax --
    emitter.instruction("call r11");                                            // invoke stream_truncate on the wrapper object
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the wrapper's bool result to the caller

    emitter.label("__rt_uwftrunc_false_x86");
    emitter.instruction("xor eax, eax");                                        // false when the wrapper does not implement stream_truncate
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return false
}

/// `__rt_box_wrapper_stat_result`: normalize a wrapper stat method's
/// type-erased return value (in x0/rax) into a boxed Mixed cell (returned in
/// x0/rax). Shared by `__rt_user_wrapper_fstat` (stream_stat) and
/// `__rt_user_wrapper_url_stat` (url_stat).
///
/// The vtable erases the method's static return type, so the shape is inspected
/// at runtime via the heap-kind byte: `0` (scalar `false`/null) → boxed
/// `false`; kind 5 (already a boxed Mixed cell, e.g. an `array|false` return) →
/// returned verbatim; kind 3 (associative hash — the usual stat array) → boxed
/// as a tag-5 Mixed; kind 2 (indexed array) → boxed as a tag-4 Mixed; any other
/// shape → boxed `false`. `__rt_mixed_from_value` *retains* the array pointer
/// while the method already returned an owned reference, so the array shapes
/// `__rt_decref_any` once after boxing to transfer (not duplicate) ownership
/// into the Mixed cell.
pub fn emit_box_wrapper_stat_result(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_box_wrapper_stat_result_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: box_wrapper_stat_result ---");
    emitter.label_global("__rt_box_wrapper_stat_result");

    // Frame: 32 bytes. [sp,#0..16] saved x29/x30, [sp,#16] raw array pointer,
    //   [sp,#24] boxed Mixed result (held across the balancing decref).
    emitter.instruction("sub sp, sp, #32");                                     // frame for the boxing helper calls
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    emitter.instruction("cbz x0, __rt_bwsr_false");                             // scalar false/null return → boxed false
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the returned value's heap-kind word
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low heap-kind byte
    emitter.instruction("cmp x9, #5");                                          // already a boxed Mixed cell (e.g. array|false return)?
    emitter.instruction("b.eq __rt_bwsr_ret");                                  // return it verbatim — ownership transfers to the caller
    emitter.instruction("mov x1, x0");                                          // raw array/hash pointer → mixed payload low word
    emitter.instruction("cmp x9, #3");                                          // associative hash (the usual string-keyed stat array)?
    emitter.instruction("mov x0, #5");                                          // runtime tag 5 = associative array
    emitter.instruction("b.eq __rt_bwsr_box");                                  // box the hash pointer as an associative Mixed
    emitter.instruction("cmp x9, #2");                                          // indexed array?
    emitter.instruction("mov x0, #4");                                          // runtime tag 4 = indexed array
    emitter.instruction("b.eq __rt_bwsr_box");                                  // box the indexed-array pointer as a Mixed
    emitter.instruction("b __rt_bwsr_false");                                   // unexpected shape → boxed false

    emitter.label("__rt_bwsr_box");                                             // x0 = tag (4/5), x1 = raw array pointer
    emitter.instruction("str x1, [sp, #16]");                                   // save the raw array pointer for the balancing release
    emitter.instruction("mov x2, #0");                                          // array-pointer mixed payloads use no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the array pointer (retains it) → x0 = Mixed cell
    emitter.instruction("str x0, [sp, #24]");                                   // save the boxed Mixed result across the release
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the raw array pointer
    emitter.instruction("bl __rt_decref_any");                                  // release the method's transferred ref (the box retained its own)
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed Mixed result for return

    emitter.label("__rt_bwsr_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed Mixed stat array

    emitter.label("__rt_bwsr_false");
    emitter.instruction("mov x0, #3");                                          // runtime tag 3 = bool for the boxed-false fallback
    emitter.instruction("mov x1, #0");                                          // false payload low word
    emitter.instruction("mov x2, #0");                                          // false payload high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box PHP false for the missing/failed stat case
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return boxed false
}

/// Emits the Linux x86_64 stream runtime helper for box wrapper stat result.
fn emit_box_wrapper_stat_result_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: box_wrapper_stat_result ---");
    emitter.label_global("__rt_box_wrapper_stat_result");

    // Frame: [rbp-8] raw array pointer, [rbp-16] boxed Mixed result. push rbp
    // then sub rsp,16 leaves rsp 16-aligned for the helper calls.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // spill slots for the raw pointer and boxed result

    emitter.instruction("test rax, rax");                                       // scalar false/null return?
    emitter.instruction("jz __rt_bwsr_false_x86");                              // → boxed false
    emitter.instruction("mov r9, QWORD PTR [rax - 8]");                         // load the returned value's heap-kind word
    emitter.instruction("and r9, 0xff");                                        // isolate the low heap-kind byte
    emitter.instruction("cmp r9, 5");                                           // already a boxed Mixed cell (e.g. array|false return)?
    emitter.instruction("je __rt_bwsr_ret_x86");                                // return it verbatim — ownership transfers to the caller
    emitter.instruction("mov rdi, rax");                                        // raw array/hash pointer → mixed payload low word (before rax is reused for the tag)
    emitter.instruction("cmp r9, 3");                                           // associative hash (the usual string-keyed stat array)?
    emitter.instruction("mov eax, 5");                                          // runtime tag 5 = associative array
    emitter.instruction("je __rt_bwsr_box_x86");                                // box the hash pointer as an associative Mixed
    emitter.instruction("cmp r9, 2");                                           // indexed array?
    emitter.instruction("mov eax, 4");                                          // runtime tag 4 = indexed array
    emitter.instruction("je __rt_bwsr_box_x86");                                // box the indexed-array pointer as a Mixed
    emitter.instruction("jmp __rt_bwsr_false_x86");                             // unexpected shape → boxed false

    emitter.label("__rt_bwsr_box_x86");                                         // rax = tag (4/5), rdi = raw array pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the raw array pointer for the balancing release
    emitter.instruction("xor esi, esi");                                        // array-pointer mixed payloads use no high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the array pointer (retains it) → rax = Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the boxed Mixed result across the release
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the raw array pointer (decref_any reads rax)
    emitter.instruction("call __rt_decref_any");                                // release the method's transferred ref (the box retained its own)
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed Mixed result for return

    emitter.label("__rt_bwsr_ret_x86");
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed stat array

    emitter.label("__rt_bwsr_false_x86");
    emitter.instruction("mov eax, 3");                                          // runtime tag 3 = bool for the boxed-false fallback
    emitter.instruction("xor edi, edi");                                        // false payload low word
    emitter.instruction("xor esi, esi");                                        // false payload high word
    emitter.instruction("call __rt_mixed_from_value");                          // box PHP false for the missing/failed stat case
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return boxed false
}

/// `__rt_user_wrapper_fstat`: invoke the wrapper's `stream_stat()` (vtable slot
/// 8) and return its result as a boxed Mixed cell that `fstat()` returns
/// verbatim (so `fstat($f)['size']` reads through `__rt_mixed_array_get`). The
/// raw return is normalized by `__rt_box_wrapper_stat_result`. A missing
/// handle/method boxes `false`, matching PHP's `fstat()` failure.
pub fn emit_user_wrapper_fstat(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_wrapper_fstat_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_fstat ---");
    emitter.label_global("__rt_user_wrapper_fstat");

    emitter.instruction("sub sp, sp, #16");                                     // helper frame for the wrapper dispatch
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    emit_aarch64_handle_lookup(emitter, "__rt_uwfstat_false");                  // resolve obj into x0, fall through to boxed false on missing handles
    emit_aarch64_method_lookup(emitter, "__rt_uwfstat_false", VTABLE_SLOT_STAT); // resolve stream_stat method pointer into x11

    // -- call stream_stat($this) → x0 = raw return, normalized to a Mixed --
    emitter.instruction("blr x11");                                             // invoke stream_stat on the wrapper object
    emitter.instruction("bl __rt_box_wrapper_stat_result");                     // normalize the type-erased return into a boxed Mixed
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed Mixed stat array

    emitter.label("__rt_uwfstat_false");
    emitter.instruction("mov x0, #0");                                          // null return → box_wrapper_stat_result yields boxed false
    emitter.instruction("bl __rt_box_wrapper_stat_result");                     // produce boxed false for the missing handle/method case
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return boxed false
}

/// Emits the Linux x86_64 stream runtime helper for user wrapper fstat.
fn emit_user_wrapper_fstat_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_wrapper_fstat ---");
    emitter.label_global("__rt_user_wrapper_fstat");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer (leaves rsp 16-aligned for the call)
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    emit_x86_handle_lookup(emitter, "__rt_uwfstat_false_x86");                  // resolve obj into rdi, fall through on missing handles
    emit_x86_method_lookup(emitter, "__rt_uwfstat_false_x86", VTABLE_SLOT_STAT); // resolve stream_stat method pointer into r11

    // -- call stream_stat($this) → rax = raw return, normalized to a Mixed --
    emitter.instruction("call r11");                                            // invoke stream_stat on the wrapper object
    emitter.instruction("call __rt_box_wrapper_stat_result");                   // normalize the type-erased return into a boxed Mixed
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed stat array

    emitter.label("__rt_uwfstat_false_x86");
    emitter.instruction("xor eax, eax");                                        // null return → box_wrapper_stat_result yields boxed false
    emitter.instruction("call __rt_box_wrapper_stat_result");                   // produce boxed false for the missing handle/method case
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return boxed false
}

/// AArch64: compute `dst = src - USER_WRAPPER_FD_BASE`, leaving the resulting
/// handle slot index in `dst`. The shift-and-subtract sequence keeps the
/// constant out of an immediate field (it does not fit a 12-bit cmp/sub).
fn emit_aarch64_slot_from_fd(emitter: &mut Emitter, src: &str, dst: &str) {
    emitter.instruction(&format!("mov w{}, #{:#x}", dst.trim_start_matches('x'), FD_BASE_LOW16)); // load the high half of USER_WRAPPER_FD_BASE
    emitter.instruction(&format!("lsl {}, {}, #16", dst, dst));                 // shift into bits 30..16 to form 0x40000000
    emitter.instruction(&format!("sub {}, {}, {}", dst, src, dst));             // dst = fd - USER_WRAPPER_FD_BASE → handle slot index
}

/// AArch64: load the wrapper object pointer for the synthetic fd that lives
/// in `x0` on entry. Leaves the obj pointer in `x0`. On a missing slot
/// (cleared after fclose) jumps to `missing_label`.
fn emit_aarch64_handle_lookup(emitter: &mut Emitter, missing_label: &str) {
    emit_aarch64_slot_from_fd(emitter, "x0", "x9");                             // x9 = handle slot index
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_handles");
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // obj = _user_wrapper_handles[slot]
    emitter.instruction(&format!("cbz x0, {}", missing_label));                 // slot empty (already fclose'd or never registered): take the fallback
}

/// AArch64: resolve the method pointer for vtable slot `vtable_slot` of the
/// class of the object currently held in `x0`. Leaves the method pointer in
/// `x11`. On a missing method (`0` slot) jumps to `missing_label`.
fn emit_aarch64_method_lookup(emitter: &mut Emitter, missing_label: &str, vtable_slot: usize) {
    emitter.instruction("ldr x10, [x0]");                                       // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x11", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x11, [x11, x10, lsl #3]");                         // per-class user-wrapper vtable for the resolved class
    emitter.instruction(&format!("ldr x11, [x11, #{}]", vtable_slot * 8));      // load the requested wrapper method pointer
    emitter.instruction(&format!("cbz x11, {}", missing_label));                // method absent: take the fallback path
}

/// x86_64: compute `dst = src - USER_WRAPPER_FD_BASE`, leaving the resulting
/// handle slot index in `dst`.
fn emit_x86_slot_from_fd(emitter: &mut Emitter, src: &str, dst: &str) {
    emitter.instruction(&format!("mov {}, {}", dst, src));                      // copy the synthetic fd into the scratch register
    emitter.instruction(&format!("sub {}, {:#x}", dst, FD_BASE));               // dst = fd - USER_WRAPPER_FD_BASE → handle slot index
}

/// x86_64: load the wrapper object pointer for the synthetic fd that lives
/// in `rdi` on entry. Leaves the obj pointer in `rdi`. On a missing slot
/// jumps to `missing_label`.
fn emit_x86_handle_lookup(emitter: &mut Emitter, missing_label: &str) {
    emit_x86_slot_from_fd(emitter, "rdi", "r9");                                // r9 = handle slot index
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_handles");          // handle table base
    emitter.instruction("mov rdi, QWORD PTR [r10 + r9 * 8]");                   // obj = _user_wrapper_handles[slot]
    emitter.instruction("test rdi, rdi");                                       // is the slot empty?
    emitter.instruction(&format!("jz {}", missing_label));                      // slot empty: take the fallback
}

/// x86_64: resolve the method pointer for vtable slot `vtable_slot` of the
/// class of the object currently held in `rdi`. Leaves the method pointer
/// in `r11`. On a missing method jumps to `missing_label`.
fn emit_x86_method_lookup(emitter: &mut Emitter, missing_label: &str, vtable_slot: usize) {
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r11", "_user_wrapper_vtable_ptrs");      // base of the per-class user-wrapper vtable pointer table
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // per-class user-wrapper vtable for the resolved class
    emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {}]", vtable_slot * 8)); // load the requested wrapper method pointer
    emitter.instruction("test r11, r11");                                       // is the method missing?
    emitter.instruction(&format!("jz {}", missing_label));                      // method absent: take the fallback
}
