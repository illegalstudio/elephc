//! Purpose:
//! Emits `__rt_pcre2_exec_at`, the cursor-continuing regex search every global `preg_*` loop
//! uses.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - A global search continues from a cursor inside the subject. Handing PCRE2 the SUFFIX
//!   (`cursor` as a fresh subject) made every continuation look like the start of the subject,
//!   so `\b`, lookbehind and `^` under `m` saw no preceding byte: `preg_match_all('/\b\w/',
//!   'hello world')` counted all ten letters instead of two. php matches the whole subject with a
//!   start offset, which is what this helper does, through the shim's `REG_STARTEND` bounds.
//! - The offset pairs come back RELATIVE TO THE CURSOR, as the suffix search reported them, so
//!   the callers' prefix, advance and capture arithmetic is unchanged. A capture that did not
//!   participate keeps its `-1`.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// The shim's `REG_STARTEND` execution flag.
const REG_STARTEND: i64 = 128;

/// __rt_pcre2_exec_at: search a compiled pattern from a cursor inside the whole subject.
/// Input:  x0=compiled handle, x1=subject base, x2=offset-pair count (at least 1),
///         x3=offset-pair buffer, x4=cursor (base <= cursor <= base + len), x5=subject length
/// Output: x0=shim status (0: matched), pairs rebased to the cursor
pub(crate) fn emit_pcre2_exec_at(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_pcre2_exec_at_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: pcre2_exec_at ---");
    emitter.label_global("__rt_pcre2_exec_at");
    emitter.instruction("sub sp, sp, #48");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set the helper frame pointer
    emitter.instruction("sub x9, x4, x1");                                      // the cursor's byte offset in the whole subject
    emitter.instruction("str x9, [sp, #0]");                                    // keep that offset to rebase the result
    emitter.instruction("str x2, [sp, #8]");                                    // keep the pair count across the shim call
    emitter.instruction("str x3, [sp, #16]");                                   // keep the pair buffer across the shim call
    emitter.instruction("str x9, [x3]");                                        // REG_STARTEND start bound: the cursor
    emitter.instruction("str x5, [x3, #8]");                                    // REG_STARTEND end bound: the whole subject
    emitter.instruction(&format!("mov x4, #{}", REG_STARTEND));                 // search from the offset, with the whole subject visible
    emitter.bl_c("elephc_pcre2_v1_exec");                                       // run the match; offsets come back absolute
    emitter.instruction("cbnz w0, __rt_pcre2_exec_at_done");                    // no match leaves nothing to rebase
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the cursor offset
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the pair count
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the pair buffer
    emitter.label("__rt_pcre2_exec_at_rebase");
    emitter.instruction("cbz x10, __rt_pcre2_exec_at_done");                    // every pair has been rebased
    emitter.instruction("ldr x12, [x11]");                                      // load this pair's start
    emitter.instruction("cmp x12, #0");                                         // did this capture participate?
    emitter.instruction("b.lt __rt_pcre2_exec_at_next");                        // an unset capture keeps its -1 pair
    emitter.instruction("sub x12, x12, x9");                                    // start relative to the cursor
    emitter.instruction("str x12, [x11]");                                      // store the rebased start
    emitter.instruction("ldr x12, [x11, #8]");                                  // load this pair's end
    emitter.instruction("sub x12, x12, x9");                                    // end relative to the cursor
    emitter.instruction("str x12, [x11, #8]");                                  // store the rebased end
    emitter.label("__rt_pcre2_exec_at_next");
    emitter.instruction("add x11, x11, #16");                                   // advance to the next pair
    emitter.instruction("sub x10, x10, #1");                                    // one pair fewer to rebase
    emitter.instruction("b __rt_pcre2_exec_at_rebase");                         // continue rebasing
    emitter.label("__rt_pcre2_exec_at_done");
    emitter.instruction("mov w0, w0");                                          // widen the 32-bit status so callers may test all of x0
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the shim status in x0
}

/// Linux x86_64 twin of `__rt_pcre2_exec_at` (SysV argument registers).
/// Input:  rdi=compiled handle, rsi=subject base, rdx=offset-pair count (at least 1),
///         rcx=offset-pair buffer, r8=cursor, r9=subject length
/// Output: eax=shim status (0: matched), pairs rebased to the cursor
fn emit_pcre2_exec_at_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pcre2_exec_at ---");
    emitter.label_global("__rt_pcre2_exec_at");
    emitter.instruction("push rbp");                                            // save the caller frame pointer (re-aligns the stack)
    emitter.instruction("mov rbp, rsp");                                        // set the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots
    emitter.instruction("mov r10, r8");                                         // copy the cursor
    emitter.instruction("sub r10, rsi");                                        // the cursor's byte offset in the whole subject
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // keep that offset to rebase the result
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // keep the pair count across the shim call
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // keep the pair buffer across the shim call
    emitter.instruction("mov QWORD PTR [rcx], r10");                            // REG_STARTEND start bound: the cursor
    emitter.instruction("mov QWORD PTR [rcx + 8], r9");                         // REG_STARTEND end bound: the whole subject
    emitter.instruction(&format!("mov r8d, {}", REG_STARTEND));                 // search from the offset, with the whole subject visible
    emitter.bl_c("elephc_pcre2_v1_exec");                                       // run the match; offsets come back absolute
    emitter.instruction("test eax, eax");                                       // did the shim find a match?
    emitter.instruction("jnz __rt_pcre2_exec_at_done_linux_x86_64");            // no match leaves nothing to rebase
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the cursor offset
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the pair count
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the pair buffer
    emitter.label("__rt_pcre2_exec_at_rebase_linux_x86_64");
    emitter.instruction("test rdx, rdx");                                       // is any pair left to rebase?
    emitter.instruction("jz __rt_pcre2_exec_at_done_linux_x86_64");             // every pair has been rebased
    emitter.instruction("mov r11, QWORD PTR [rcx]");                            // load this pair's start
    emitter.instruction("test r11, r11");                                       // did this capture participate?
    emitter.instruction("js __rt_pcre2_exec_at_next_linux_x86_64");             // an unset capture keeps its -1 pair
    emitter.instruction("sub r11, r10");                                        // start relative to the cursor
    emitter.instruction("mov QWORD PTR [rcx], r11");                            // store the rebased start
    emitter.instruction("mov r11, QWORD PTR [rcx + 8]");                        // load this pair's end
    emitter.instruction("sub r11, r10");                                        // end relative to the cursor
    emitter.instruction("mov QWORD PTR [rcx + 8], r11");                        // store the rebased end
    emitter.label("__rt_pcre2_exec_at_next_linux_x86_64");
    emitter.instruction("add rcx, 16");                                         // advance to the next pair
    emitter.instruction("sub rdx, 1");                                          // one pair fewer to rebase
    emitter.instruction("jmp __rt_pcre2_exec_at_rebase_linux_x86_64");          // continue rebasing
    emitter.label("__rt_pcre2_exec_at_done_linux_x86_64");
    emitter.instruction("leave");                                               // restore the caller frame
    emitter.instruction("ret");                                                 // return the shim status in eax
}
