//! Purpose:
//! Emits the `__rt_pcre_to_posix` compatibility helper used by preg runtimes.
//! The helper preserves PCRE syntax and only materializes a C string for the PCRE2 wrapper,
//! escaping any NUL byte the pattern contains so the C string still holds the whole pattern.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - The symbol name is retained so existing preg emitters keep one call path,
//!   but no PCRE escapes are rewritten before PCRE2 sees the pattern -- except a NUL byte.
//! - php hands PCRE2 the pattern WITH its length, so a raw NUL is an ordinary character to match
//!   (`"~[\s\0]+~"`, which Laravel's `Str::trim()` builds). The wrapper takes a C string, which a
//!   NUL truncates: the class lost its closing bracket and the whole call failed to compile.
//!   `\x00` means the same thing to PCRE2 in and out of a character class, so each NUL becomes
//!   that escape; a NUL an odd run of backslashes already escapes gets only `x00`, so the
//!   backslash in front of it becomes the escape's own. The interpreter does the same in
//!   `pattern_with_escaped_nul_bytes`.
//! - A pattern without a NUL takes the unchanged `__rt_cstr` path. One with a NUL is expanded
//!   (at most four bytes per input byte) into the runtime-heap buffer `__rt_cstr` itself keeps in
//!   `_cstr_buf_dynamic`, which the next long C-string conversion reclaims.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_pcre_to_posix`: a tail-call to `__rt_cstr`, or a NUL-escaping copy.
///
/// Despite the historic helper name, this no longer translates PCRE to POSIX
/// syntax. It copies the stripped pattern bytes into a C-string buffer and returns the buffer
/// pointer in the platform result register so `elephc_pcre2_v1_compile()` receives the original
/// PCRE pattern.
///
/// # Inputs
/// - AArch64: `x1` pattern pointer, `x2` length. x86_64: `rax` pattern pointer, `rdx` length.
///
/// # Outputs
/// - `x0` / `rax`: pointer to the NUL-terminated pattern.
pub(crate) fn emit_pcre_to_posix(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 helper.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pcre_pattern_to_cstr ---");
    emitter.label_global("__rt_pcre_to_posix");
    emitter.instruction("mov x9, x1");                                          // scan cursor = pattern start
    emitter.instruction("mov x10, x2");                                         // bytes left to scan
    emitter.label("__rt_pcre_to_posix_scan");
    emitter.instruction("cbz x10, __rt_pcre_to_posix_plain");                   // no NUL found: the plain copy is exact
    emitter.instruction("ldrb w11, [x9], #1");                                  // load one pattern byte and advance
    emitter.instruction("cbz w11, __rt_pcre_to_posix_escape");                  // a NUL byte needs the escaping copy
    emitter.instruction("sub x10, x10, #1");                                    // one byte consumed
    emitter.instruction("b __rt_pcre_to_posix_scan");                           // keep scanning
    emitter.label("__rt_pcre_to_posix_plain");
    emitter.instruction("b __rt_cstr");                                         // tail-call the shared C-string copier while preserving the caller return address

    // -- escaping copy: frame slots src=0, len=8, buffer=16, saved fp/lr=32/40 --
    emitter.label("__rt_pcre_to_posix_escape");
    emitter.instruction("sub sp, sp, #48");                                     // reserve saved input, buffer pointer and aligned frame storage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame
    emitter.instruction("stp x1, x2, [sp, #0]");                                // retain the pattern bytes and length across the allocation
    emitter.instruction("lsr x9, x2, #60");                                     // a length this large cannot be expanded 4x without wrapping
    emitter.instruction("cbz x9, __rt_pcre_to_posix_size_ok");                  // the ordinary path skips the cross-atom fatal branch
    emitter.instruction("b __rt_heap_allocation_failed");                         // fail like every other oversized request
    emitter.label("__rt_pcre_to_posix_size_ok");
    emitter.instruction("lsl x0, x2, #2");                                      // worst case: every byte becomes a four-byte escape
    emitter.instruction("add x0, x0, #1");                                      // plus the trailing C terminator
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the expansion buffer from the runtime heap
    emitter.instruction("str x0, [sp, #16]");                                   // keep the buffer while the previous one is reclaimed
    abi::emit_symbol_address(emitter, "x9", "_cstr_buf_dynamic");               // the persistent dynamic C-string scratch slot
    emitter.instruction("ldr x10, [x9]");                                       // previous dynamic scratch allocation
    emitter.instruction("str x0, [x9]");                                        // publish the new buffer before releasing the stale one
    emitter.instruction("cbz x10, __rt_pcre_to_posix_fill");                    // first long input has nothing to reclaim
    emitter.instruction("mov x0, x10");                                         // pass the superseded raw runtime-heap buffer
    emitter.instruction("bl __rt_heap_free_safe");                              // return it without touching static storage
    emitter.label("__rt_pcre_to_posix_fill");
    emitter.instruction("ldr x9, [sp, #16]");                                   // destination cursor
    emitter.instruction("mov x10, x9");                                         // remember the buffer base for the return value
    emitter.instruction("ldp x1, x11, [sp, #0]");                               // source cursor and remaining byte count
    emitter.instruction("mov x13, #0");                                         // pending-backslash flag: an odd run of backslashes precedes the cursor
    emitter.label("__rt_pcre_to_posix_loop");
    emitter.instruction("cbz x11, __rt_pcre_to_posix_done");                    // terminate once every byte is copied
    emitter.instruction("ldrb w12, [x1], #1");                                  // load one pattern byte and advance
    emitter.instruction("sub x11, x11, #1");                                    // one byte consumed
    emitter.instruction("cbz w12, __rt_pcre_to_posix_nul");                     // a NUL becomes an escape
    emitter.instruction("strb w12, [x9], #1");                                  // ordinary bytes are copied as they are
    emitter.instruction("cmp w12, #92");                                        // a backslash ?
    emitter.instruction("cset w14, eq");                                        // w14 = byte is a backslash
    emitter.instruction("bic w13, w14, w13");                                   // pending = backslash AND NOT pending (two backslashes cancel)
    emitter.instruction("b __rt_pcre_to_posix_loop");                           // next byte
    emitter.label("__rt_pcre_to_posix_nul");
    emitter.instruction("cbnz x13, __rt_pcre_to_posix_escaped");                // an unpaired backslash already opens the escape
    emitter.instruction("mov w14, #92");                                        // '\\'
    emitter.instruction("strb w14, [x9], #1");                                  // open the escape
    emitter.label("__rt_pcre_to_posix_escaped");
    emitter.instruction("mov w14, #120");                                       // 'x'
    emitter.instruction("strb w14, [x9], #1");                                  // write the hex-escape marker
    emitter.instruction("mov w14, #48");                                        // '0'
    emitter.instruction("strb w14, [x9], #1");                                  // first hex digit
    emitter.instruction("strb w14, [x9], #1");                                  // second hex digit
    emitter.instruction("mov x13, #0");                                         // the escape consumed any pending backslash
    emitter.instruction("b __rt_pcre_to_posix_loop");                           // next byte
    emitter.label("__rt_pcre_to_posix_done");
    emitter.instruction("strb wzr, [x9]");                                      // terminate the C string
    emitter.instruction("mov x0, x10");                                         // return the buffer base
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the NUL-free C pattern
}

/// Emits the x86_64 helper.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pcre_pattern_to_cstr ---");
    emitter.label_global("__rt_pcre_to_posix");
    emitter.instruction("mov r8, rax");                                         // scan cursor = pattern start
    emitter.instruction("mov rcx, rdx");                                        // bytes left to scan
    emitter.label("__rt_pcre_to_posix_scan");
    emitter.instruction("test rcx, rcx");                                       // any bytes left ?
    emitter.instruction("jz __rt_pcre_to_posix_plain");                         // no NUL found: the plain copy is exact
    emitter.instruction("cmp BYTE PTR [r8], 0");                                // a NUL byte ?
    emitter.instruction("je __rt_pcre_to_posix_escape");                        // a NUL byte needs the escaping copy
    emitter.instruction("add r8, 1");                                           // advance the scan cursor
    emitter.instruction("sub rcx, 1");                                          // one byte consumed
    emitter.instruction("jmp __rt_pcre_to_posix_scan");                         // keep scanning
    emitter.label("__rt_pcre_to_posix_plain");
    emitter.instruction("jmp __rt_cstr");                                       // tail-call the x86_64 C-string copier while preserving the caller return address

    // -- escaping copy: frame slots src=0, len=8, buffer=16 --
    emitter.label("__rt_pcre_to_posix_escape");
    emitter.instruction("push rbp");                                            // preserve the caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 32");                                         // saved input, buffer pointer and aligned storage
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // retain the pattern bytes across the allocation
    emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                        // retain the pattern length across the allocation
    emitter.instruction("mov r8, rdx");                                         // copy the length for the overflow guard
    emitter.instruction("shr r8, 60");                                          // a length this large cannot be expanded 4x without wrapping
    emitter.instruction("jz __rt_pcre_to_posix_size_ok");                       // the ordinary path skips the cross-atom fatal branch
    emitter.instruction("jmp __rt_heap_allocation_failed");                       // fail like every other oversized request
    emitter.label("__rt_pcre_to_posix_size_ok");
    emitter.instruction("mov rax, rdx");                                        // worst case: every byte becomes a four-byte escape
    emitter.instruction("shl rax, 2");                                          // length times four
    emitter.instruction("add rax, 1");                                          // plus the trailing C terminator
    emitter.instruction("call __rt_heap_alloc");                                // allocate the expansion buffer from the runtime heap
    emitter.instruction("mov QWORD PTR [rsp + 16], rax");                       // keep the buffer while the previous one is reclaimed
    abi::emit_symbol_address(emitter, "r8", "_cstr_buf_dynamic");               // the persistent dynamic C-string scratch slot
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // previous dynamic scratch allocation
    emitter.instruction("mov QWORD PTR [r8], rax");                             // publish the new buffer before releasing the stale one
    emitter.instruction("test r9, r9");                                         // is there a previous allocation ?
    emitter.instruction("jz __rt_pcre_to_posix_fill");                          // first long input has nothing to reclaim
    emitter.instruction("mov rax, r9");                                         // pass the superseded raw runtime-heap buffer
    emitter.instruction("call __rt_heap_free_safe");                            // return it without touching static storage
    emitter.label("__rt_pcre_to_posix_fill");
    emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                        // destination cursor
    emitter.instruction("mov r9, r8");                                          // remember the buffer base for the return value
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // source cursor
    emitter.instruction("mov rcx, QWORD PTR [rsp + 8]");                        // remaining byte count
    emitter.instruction("xor edx, edx");                                        // pending-backslash flag: an odd run of backslashes precedes the cursor
    emitter.label("__rt_pcre_to_posix_loop");
    emitter.instruction("test rcx, rcx");                                       // any bytes left ?
    emitter.instruction("jz __rt_pcre_to_posix_done");                          // terminate once every byte is copied
    emitter.instruction("movzx r11d, BYTE PTR [r10]");                          // load one pattern byte
    emitter.instruction("add r10, 1");                                          // advance the source cursor
    emitter.instruction("sub rcx, 1");                                          // one byte consumed
    emitter.instruction("test r11b, r11b");                                     // a NUL ?
    emitter.instruction("jz __rt_pcre_to_posix_nul");                           // a NUL becomes an escape
    emitter.instruction("mov BYTE PTR [r8], r11b");                             // ordinary bytes are copied as they are
    emitter.instruction("add r8, 1");                                           // advance the destination cursor
    emitter.instruction("cmp r11b, 92");                                        // a backslash ?
    emitter.instruction("jne __rt_pcre_to_posix_clear");                        // any other byte ends a run of backslashes
    emitter.instruction("test rdx, rdx");                                       // was a backslash already pending ?
    emitter.instruction("jnz __rt_pcre_to_posix_clear");                        // two backslashes cancel
    emitter.instruction("mov edx, 1");                                          // an unpaired backslash is now pending
    emitter.instruction("jmp __rt_pcre_to_posix_loop");                         // next byte
    emitter.label("__rt_pcre_to_posix_clear");
    emitter.instruction("xor edx, edx");                                        // nothing is pending
    emitter.instruction("jmp __rt_pcre_to_posix_loop");                         // next byte
    emitter.label("__rt_pcre_to_posix_nul");
    emitter.instruction("test rdx, rdx");                                       // does an unpaired backslash already open the escape ?
    emitter.instruction("jnz __rt_pcre_to_posix_escaped");                      // yes: only the hex digits follow
    emitter.instruction("mov BYTE PTR [r8], 92");                               // open the escape with a backslash
    emitter.instruction("add r8, 1");                                           // advance the destination cursor
    emitter.label("__rt_pcre_to_posix_escaped");
    emitter.instruction("mov BYTE PTR [r8], 120");                              // 'x'
    emitter.instruction("mov BYTE PTR [r8 + 1], 48");                           // first hex digit '0'
    emitter.instruction("mov BYTE PTR [r8 + 2], 48");                           // second hex digit '0'
    emitter.instruction("add r8, 3");                                           // advance past the hex escape
    emitter.instruction("xor edx, edx");                                        // the escape consumed any pending backslash
    emitter.instruction("jmp __rt_pcre_to_posix_loop");                         // next byte
    emitter.label("__rt_pcre_to_posix_done");
    emitter.instruction("mov BYTE PTR [r8], 0");                                // terminate the C string
    emitter.instruction("mov rax, r9");                                         // return the buffer base
    emitter.instruction("add rsp, 32");                                         // release the helper locals
    emitter.instruction("pop rbp");                                             // restore the caller frame
    emitter.instruction("ret");                                                 // return the NUL-free C pattern
}
