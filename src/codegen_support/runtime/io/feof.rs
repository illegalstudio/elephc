//! Purpose:
//! Emits the `__rt_feof` runtime helper assembly for feof.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Native EOF is owned by `StreamState`; userspace wrappers still delegate
//!   to their `stream_eof` callback through the synthetic backend descriptor.

use crate::codegen_support::runtime::resources::layout::{
    STREAM_EOF_OFFSET, STREAM_PENDING_LEN_OFFSET, STREAM_PENDING_POS_OFFSET,
};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_stream_eof_known(handle)`: is this stream KNOWN to be finished?
///
/// Answers 1 only when the holding area is empty and the read has already posted `stream_eof()`'s
/// answer. It never asks the class — that is the whole point. A fill loop that asks before reading
/// makes the class see a question php never puts to it, and gets nothing in return: php fills, and
/// reads what it already knows.
pub fn emit_stream_eof_known(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_eof_known ---");
    emitter.label_global("__rt_stream_eof_known");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");
            emitter.instruction("stp x29, x30, [sp, #0]");
            emitter.instruction("mov x29, sp");
            emitter.instruction("bl __rt_stream_state");
            emitter.instruction("cbz x0, __rt_sek_no");                         // no state: nothing is known
            emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_LEN_OFFSET}]"));
            emitter.instruction(&format!("ldr x10, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
            emitter.instruction("subs x9, x9, x10");                            // what is still held
            emitter.instruction("b.gt __rt_sek_no");                            // bytes in hand: not at the end
            emitter.instruction(&format!("ldr x9, [x0, #{STREAM_EOF_OFFSET}]")); // what the read posted
            emitter.instruction("cmp x9, #0");
            emitter.instruction("cset x0, ne");
            emitter.instruction("b __rt_sek_done");
            emitter.label("__rt_sek_no");
            emitter.instruction("mov x0, #0");
            emitter.label("__rt_sek_done");
            emitter.instruction("ldp x29, x30, [sp, #0]");
            emitter.instruction("add sp, sp, #16");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("call __rt_stream_state");
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_sek_no_x86");                          // no state: nothing is known
            emitter.instruction(&format!("mov r9, QWORD PTR [rax + {STREAM_PENDING_LEN_OFFSET}]"));
            emitter.instruction(&format!("mov r10, QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}]"));
            emitter.instruction("sub r9, r10");                                 // what is still held
            emitter.instruction("cmp r9, 0");
            emitter.instruction("jg __rt_sek_no_x86");                          // bytes in hand: not at the end
            emitter.instruction(&format!("mov r9, QWORD PTR [rax + {STREAM_EOF_OFFSET}]")); // what the read posted
            emitter.instruction("xor eax, eax");
            emitter.instruction("test r9, r9");
            emitter.instruction("setne al");
            emitter.instruction("jmp __rt_sek_done_x86");
            emitter.label("__rt_sek_no_x86");
            emitter.instruction("xor eax, eax");
            emitter.label("__rt_sek_done_x86");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits a call to `__rt_feof`, stating who is asking.
///
/// `quiet` is not optional and not a default: the mode decides whether a wrapper with no
/// `stream_eof` is warned about, and a site that guessed would warn in the wrong function's name
/// or swallow a warning php prints. `false` means the PROGRAM called `feof()`; `true` means this
/// is elephc's own probe before a read.
pub fn emit_feof_call(emitter: &mut Emitter, quiet: bool) {
    let mode = i64::from(quiet);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x1, #{mode}"));                   // who is asking
            emitter.instruction("bl __rt_feof");
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rsi, {mode}"));                   // who is asking
            emitter.instruction("call __rt_feof");
        }
    }
}

/// Emits the `__rt_feof` runtime helper.
/// Dispatches to the target-specific implementation based on `emitter.target`.
/// Input: x0 = opaque stream handle
/// Output: x0 = 1 if EOF reached, 0 otherwise
///
/// The mode arrives in x1 / rsi and is what `emit_feof_call` writes: 0 when the PROGRAM called
/// `feof()`, 1 for elephc's OWN probes — the loops in `fgets`,
/// `readfile`, `stream_get_contents`, `stream_get_line`, `fpassthru` and `copy` that ask before
/// reading. php's readers ask no such question: they read, and the read asks the wrapper. So a
/// class with no `stream_eof` must not be warned about by a probe the program never wrote — the
/// read that follows refuses and names the caller. Only a `feof()` the PROGRAM called is loud.
///
/// One entry with an argument, not two labels sharing a body: see `emit_user_wrapper_feof` for why
/// the two-label spelling dead-strips itself on macOS.
pub fn emit_feof(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_feof_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: feof ---");
    emitter.label_global("__rt_feof");
    emitter.instruction("sub sp, sp, #48");                                     // preserve the stream handle, who is asking, and the caller frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable helper frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the opaque stream handle
    emitter.instruction("str x1, [sp, #24]");                                   // and who asked, across every call below
    emitter.instruction("bl __rt_stream_fd");                                   // resolve the backend descriptor for wrapper dispatch
    emitter.instruction("str x0, [sp, #8]");                                    // the probe below clobbers it
    emitter.instruction("mov w9, #0x4000");                                     // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
    emitter.instruction("lsl w9, w9, #16");                                     // shift into bits 30..16 to form 0x40000000
    emitter.instruction("cmp x0, x9");                                          // is the backend below the synthetic wrapper range?
    emitter.instruction("b.lo __rt_feof_stream_state");                         // native descriptors use authoritative StreamState EOF
    // The wrapper fd range ends at the allocated handle capacity, not a
    // fixed 256: a slot beyond the bound would be misread as a native fd.
    super::emit_load_handles_cap(emitter, "x10");
    emitter.instruction("add x10, x9, x10");                                    // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
    emitter.instruction("cmp x0, x10");                                         // is the backend above the synthetic wrapper range?
    emitter.instruction("b.hs __rt_feof_stream_state");                         // non-wrapper synthetic backends use StreamState EOF
    // -- a stream that still HOLDS bytes is not at its end, whatever the wrapper says --
    //
    // php's `feof()` answers false while the read buffer has anything left, because the next read
    // will come out of it. `stream_eof()` reports the WRAPPER's position, which a buffered read
    // has already moved past — so a bounded `fgets()` made `feof()` true and `fpassthru()` answer
    // zero while 22 bytes were still on the stream.
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("bl __rt_stream_state");                                // x0 = stable stream state, 0 when none
    emitter.instruction("cbz x0, __rt_feof_wrapper_call");                      // no state: nothing can be held
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_LEN_OFFSET}]")); // held byte count
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_PENDING_POS_OFFSET}]")); // how many were already handed out
    emitter.instruction("subs x9, x9, x10");                                    // what remains
    // -- and php never asks twice: the read already did --
    //
    // `stream_read()` is followed by `stream_eof()`, whose answer lives on the stream, and
    // `feof()` reads it rather than asking the class again. MEASURED: a `while (!feof($h))` loop
    // over a 10-byte wrapper asks the class TWICE — once before the first read, once after it —
    // and never again. A seek is what clears the answer.
    emitter.instruction("b.gt __rt_feof_holding");                              // holding bytes: not at end
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_EOF_OFFSET}]"));        // what the read remembered
    emitter.instruction("cbz x9, __rt_feof_wrapper_call");                      // nothing remembered: ask the class
    emitter.instruction("mov x0, #1");                                          // remembered: php answers from this
    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");
    emitter.label("__rt_feof_holding");
    emitter.instruction("mov x0, #0");                                          // holding bytes: not at end
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                     // release helper scratch storage
    emitter.instruction("ret");
    emitter.label("__rt_feof_wrapper_call");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the wrapper descriptor the probe clobbered
    emitter.instruction("ldr x9, [sp, #24]");                                   // who asked travels with the question
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame before wrapper tail dispatch
    emitter.instruction("add sp, sp, #48");                                     // release helper scratch storage
    emitter.instruction("mov x1, x9");                                          // and reaches the wrapper as its second argument
    emitter.instruction("b __rt_user_wrapper_feof");                            // wrapper backend delegates to stream_eof
    emitter.label("__rt_feof_stream_state");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle
    emitter.instruction("bl __rt_stream_eof_get");                              // read EOF from the stable StreamState
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                     // release helper scratch storage
    emitter.instruction("ret");                                                 // return the state-owned EOF predicate
}

/// x86_64 Linux implementation of `__rt_feof`.
/// Resolves wrappers by backend descriptor and native EOF by opaque handle.
/// Input: rdi = opaque stream handle
/// Output: eax = 1 if EOF reached, 0 otherwise
fn emit_feof_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: feof ---");
    emitter.label_global("__rt_feof");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 32");                                         // aligned storage for the handle and who is asking
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the opaque stream handle
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // and who asked, across every call below
    emitter.instruction("call __rt_stream_fd");                                 // resolve the backend descriptor for wrapper dispatch
    emitter.instruction("mov r9d, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rax, r9");                                         // is the backend below the synthetic wrapper range?
    emitter.instruction("jb __rt_feof_stream_state_x86");                       // native descriptors use authoritative StreamState EOF
    // The wrapper fd range ends at the allocated handle capacity, not a
    // fixed 256: a slot beyond the bound would be misread as a native fd.
    super::emit_load_handles_cap(emitter, "r10");
    emitter.instruction("add r10, r9");                                         // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
    emitter.instruction("cmp rax, r10");                                        // is the backend above the synthetic wrapper range?
    emitter.instruction("jae __rt_feof_stream_state_x86");                      // non-wrapper synthetic backends use StreamState EOF
    // Same probe as the AArch64 arm: bytes still held mean the stream is not at its end.
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the descriptor, which the probe clobbers
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("call __rt_stream_state");                              // rax = stable stream state, 0 when none
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_feof_wrapper_call_x86");                       // no state: nothing can be held
    emitter.instruction(&format!("mov r9, QWORD PTR [rax + {STREAM_PENDING_LEN_OFFSET}]")); // held byte count
    emitter.instruction(&format!("mov r10, QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}]")); // already handed out
    emitter.instruction("sub r9, r10");                                         // what remains
    emitter.instruction("cmp r9, 0");
    // See the AArch64 twin: php answers from what the read remembered.
    emitter.instruction("jg __rt_feof_holding_x86");                            // holding bytes: not at end
    emitter.instruction(&format!("mov r9, QWORD PTR [rax + {STREAM_EOF_OFFSET}]")); // what the read remembered
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_feof_wrapper_call_x86");                       // nothing remembered: ask the class
    emitter.instruction("mov eax, 1");                                          // remembered: php answers from this
    emitter.instruction("add rsp, 32");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
    emitter.label("__rt_feof_holding_x86");
    emitter.instruction("xor eax, eax");                                        // holding bytes: not at end
    emitter.instruction("add rsp, 32");                                         // release stream-handle storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
    emitter.label("__rt_feof_wrapper_call_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the wrapper descriptor the probe clobbered
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // who asked travels with the question
    emitter.instruction("add rsp, 32");                                         // release stream-handle storage before tail dispatch
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("mov rsi, r9");                                         // and reaches the wrapper as its second argument
    emitter.instruction("jmp __rt_user_wrapper_feof");                          // wrapper backend delegates to stream_eof
    emitter.label("__rt_feof_stream_state_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("call __rt_stream_eof_get");                            // read EOF from the stable StreamState
    emitter.instruction("add rsp, 32");                                         // release stream-handle storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the state-owned EOF predicate
}
