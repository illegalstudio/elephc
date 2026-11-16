//! Purpose:
//! Emits the `__rt_substr_count` runtime helper assembly for the PHP `substr_count` builtin.
//! Counts non-overlapping needle occurrences inside an already-sliced haystack window.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The helper receives the WINDOW, not the original subject: `substr_count()`'s `$offset`
//!   and `$length` are normalized (and their `ValueError`s raised) in the backend lowering,
//!   which then passes `haystack + offset` and the clamped length. That keeps the catchable
//!   diagnostics out of the runtime, where a fatal could not be caught.
//! - Matching is NON-OVERLAPPING, exactly like php-src: a hit advances the cursor by the full
//!   needle length, so `substr_count("aaaa", "aa")` is 2 rather than 3.
//! - A one-byte needle uses a word-at-a-time byte census; longer needles search the first
//!   byte with libc `memchr` and confirm each candidate with `memcmp`. Both paths preserve
//!   PHP's binary-safe byte semantics.
//! - The helper allocates nothing and calls nothing on the one-byte path; the general path
//!   keeps a small frame because the libc calls clobber caller-saved registers.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_substr_count` runtime helper for the `substr_count` builtin.
///
/// ABI (AArch64):
///   Input:  `x1` = window pointer, `x2` = window length, `x3` = needle pointer,
///           `x4` = needle length.
///   Output: `x0` = number of non-overlapping matches.
///
/// ABI (x86_64 System V):
///   Input:  `rdi` = window pointer, `rsi` = window length, `rdx` = needle pointer,
///           `rcx` = needle length.
///   Output: `rax` = number of non-overlapping matches.
///
/// An empty needle and a needle longer than the window both yield zero; the empty case
/// never reaches the helper because the lowering raises PHP's `ValueError` first, but the
/// guard keeps the loop from spinning if it ever did.
pub fn emit_substr_count(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_substr_count_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: substr_count ---");
    emitter.label_global("__rt_substr_count");

    emitter.instruction("mov x0, #0");                                          // start the match counter at zero
    emitter.instruction("cbz x4, __rt_substr_count_done");                      // an empty needle can never be counted
    emitter.instruction("cmp x4, x2");                                          // compare the needle length against the searchable window
    emitter.instruction("b.gt __rt_substr_count_done");                         // a needle longer than the window cannot match
    emitter.instruction("cmp x4, #1");                                          // is the needle a single byte?
    emitter.instruction("b.eq __rt_substr_count_byte");                         // use the word-at-a-time census for one-byte needles
    emitter.instruction("b __rt_substr_count_search");                          // use memchr/memcmp for longer needles

    // -- one-byte needle: count matching bytes sixteen at a time with NEON --
    emitter.label("__rt_substr_count_byte");
    emitter.instruction("ldrb w12, [x3]");                                      // load the single needle byte
    emitter.instruction("dup v1.16b, w12");                                     // broadcast the needle byte through the vector
    emitter.instruction("mov x13, x1");                                         // start the census at the window pointer
    emitter.instruction("mov x14, x2");                                         // count only the clamped window bytes
    emitter.instruction("cmp x14, #16");                                        // can the census read a full vector?
    emitter.instruction("b.lt __rt_substr_count_byte_tail");                    // short windows use the byte tail directly

    emitter.label("__rt_substr_count_byte_words");
    emitter.instruction("ldr q0, [x13]");                                       // load the next sixteen window bytes
    emitter.instruction("cmeq v0.16b, v0.16b, v1.16b");                         // mark every byte that matches the needle
    emitter.instruction("ushr v0.16b, v0.16b, #7");                             // normalize each match mask to one
    emitter.instruction("addv b0, v0.16b");                                     // sum this vector's matching bytes
    emitter.instruction("umov w17, v0.b[0]");                                   // move the vector sum into a scalar
    emitter.instruction("add x0, x0, x17");                                     // add this vector's matches to the running count
    emitter.instruction("add x13, x13, #16");                                   // advance to the next sixteen window bytes
    emitter.instruction("sub x14, x14, #16");                                   // account for the bytes just counted
    emitter.instruction("cmp x14, #16");                                        // is another full vector available?
    emitter.instruction("b.ge __rt_substr_count_byte_words");                   // continue the vector census while full vectors remain

    emitter.label("__rt_substr_count_byte_tail");
    emitter.instruction("cbz x14, __rt_substr_count_done");                     // stop when the window is exhausted
    emitter.instruction("ldrb w15, [x13]");                                     // load the next trailing window byte
    emitter.instruction("cmp w15, w12");                                        // does the trailing byte match the needle?
    emitter.instruction("cinc x0, x0, eq");                                     // count one match without branching
    emitter.instruction("add x13, x13, #1");                                    // advance to the next trailing byte
    emitter.instruction("sub x14, x14, #1");                                    // consume one trailing byte
    emitter.instruction("b __rt_substr_count_byte_tail");                       // continue the trailing byte census

    // -- longer needle: jump to first-byte candidates and confirm with memcmp --
    emitter.label("__rt_substr_count_search");
    emitter.instruction("sub sp, sp, #80");                                     // reserve the libc-preserved search state
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #64");                                    // establish a stable frame base
    emitter.instruction("stp x1, x2, [sp, #16]");                               // save the window pointer and length
    emitter.instruction("stp x3, x4, [sp, #32]");                               // save the needle pointer and length
    emitter.instruction("sub x9, x2, x4");                                      // last valid candidate start = window length - needle length
    emitter.instruction("str x9, [sp, #0]");                                    // publish the last valid candidate
    emitter.instruction("mov x9, #0");                                          // start scanning at the window start
    emitter.instruction("str x9, [sp, #8]");                                    // publish the current candidate offset
    emitter.instruction("ldrb w9, [x3]");                                       // load the needle's first byte
    emitter.instruction("str x9, [sp, #48]");                                   // save the first-byte search key
    emitter.instruction("str xzr, [sp, #56]");                                  // start the non-overlapping match counter at zero

    emitter.label("__rt_substr_count_search_loop");
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the window pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload the window length
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the current candidate offset
    emitter.instruction("add x0, x1, x9");                                      // search from the next candidate byte
    emitter.instruction("sub x2, x2, x9");                                      // remaining window bytes from the candidate
    emitter.instruction("ldr w1, [sp, #48]");                                   // pass the cached first needle byte
    emitter.bl_c("memchr");                                                    // find the next first-byte candidate
    emitter.instruction("cbz x0, __rt_substr_count_search_done");               // no first-byte candidate remains
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the window pointer for the offset
    emitter.instruction("sub x9, x0, x1");                                      // convert the candidate pointer into an offset
    emitter.instruction("str x9, [sp, #8]");                                    // publish the candidate offset
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the last valid candidate start
    emitter.instruction("cmp x9, x10");                                         // does the full needle still fit here?
    emitter.instruction("b.gt __rt_substr_count_search_done");                  // a candidate past the window cannot match
    emitter.instruction("add x0, x1, x9");                                      // compare the full needle at this candidate
    emitter.instruction("ldr x1, [sp, #32]");                                   // pass the needle pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // pass the needle length
    emitter.bl_c("memcmp");                                                    // compare the remaining needle bytes
    emitter.instruction("cbnz w0, __rt_substr_count_search_next");              // a mismatch advances one byte
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload the running match count
    emitter.instruction("add x0, x0, #1");                                      // record one more non-overlapping match
    emitter.instruction("str x0, [sp, #56]");                                   // publish the updated match count
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the matched candidate offset
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the needle length
    emitter.instruction("add x9, x9, x10");                                     // skip the whole matched needle
    emitter.instruction("str x9, [sp, #8]");                                    // publish the non-overlapping continuation
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the last valid candidate start
    emitter.instruction("cmp x9, x10");                                         // is another candidate still in range?
    emitter.instruction("b.le __rt_substr_count_search_loop");                  // continue while candidates remain
    emitter.instruction("b __rt_substr_count_search_done");                     // otherwise finish the search

    emitter.label("__rt_substr_count_search_next");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the failed candidate offset
    emitter.instruction("add x9, x9, #1");                                      // advance to the next candidate
    emitter.instruction("str x9, [sp, #8]");                                    // publish the advanced candidate
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the last valid candidate start
    emitter.instruction("cmp x9, x10");                                         // is another candidate still in range?
    emitter.instruction("b.le __rt_substr_count_search_loop");                  // continue while candidates remain

    emitter.label("__rt_substr_count_search_done");
    emitter.instruction("ldr x0, [sp, #56]");                                   // return the accumulated match count
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #80");                                     // release the search frame
    emitter.instruction("ret");                                                 // return the match count

    emitter.label("__rt_substr_count_done");
    emitter.instruction("ret");                                                 // return the accumulated match count
}

/// Emits `__rt_substr_count` for x86_64 Linux using the System V ABI.
fn emit_substr_count_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: substr_count ---");
    emitter.label_global("__rt_substr_count");

    emitter.instruction("xor eax, eax");                                        // start the match counter at zero
    emitter.instruction("test rcx, rcx");                                       // is the needle empty?
    emitter.instruction("jz __rt_substr_count_done_linux_x86_64");              // an empty needle can never be counted
    emitter.instruction("cmp rcx, rsi");                                        // compare the needle length against the searchable window
    emitter.instruction("jg __rt_substr_count_done_linux_x86_64");              // a needle longer than the window cannot match
    emitter.instruction("cmp rcx, 1");                                          // is the needle a single byte?
    emitter.instruction("je __rt_substr_count_byte_linux_x86_64");              // use the word-at-a-time census for one-byte needles
    emitter.instruction("jmp __rt_substr_count_search_linux_x86_64");           // use memchr/memcmp for longer needles

    // -- one-byte needle: count matching bytes eight at a time --
    emitter.label("__rt_substr_count_byte_linux_x86_64");
    emitter.instruction("movzx r8d, BYTE PTR [rdx]");                           // load the single needle byte
    emitter.instruction("mov rax, 0x0101010101010101");                         // materialize the byte-broadcast constant
    emitter.instruction("imul r8, rax");                                        // form the repeated-byte comparison word
    emitter.instruction("xor eax, eax");                                        // reset the match counter
    emitter.instruction("mov r9, rdi");                                         // start the census at the window pointer
    emitter.instruction("mov r10, rsi");                                        // count only the clamped window bytes
    emitter.instruction("cmp r10, 8");                                          // can the census read a full word?
    emitter.instruction("jl __rt_substr_count_byte_tail_linux_x86_64");         // short windows use the byte tail directly

    emitter.label("__rt_substr_count_byte_words_linux_x86_64");
    emitter.instruction("mov r11, QWORD PTR [r9]");                             // load the next eight window bytes
    emitter.instruction("xor r11, r8");                                         // mark every byte that differs from the needle
    emitter.instruction("mov rcx, 0x0101010101010101");                         // materialize the byte-borrow constant
    emitter.instruction("mov rdx, r11");                                        // preserve the differing-byte word
    emitter.instruction("sub r11, rcx");                                        // borrow across every matching zero byte
    emitter.instruction("not rdx");                                             // keep only zero-byte borrows
    emitter.instruction("and r11, rdx");                                        // isolate borrows that came from a zero byte
    emitter.instruction("mov rcx, 0x8080808080808080");                         // keep the high bit of each zero-byte borrow
    emitter.instruction("and r11, rcx");                                        // isolate one flag per matching byte
    emitter.instruction("shr r11, 7");                                          // move every byte flag into its low bit
    emitter.instruction("mov rcx, 0x0101010101010101");                         // materialize the horizontal-sum multiplier
    emitter.instruction("imul r11, rcx");                                       // sum the eight byte flags into the top byte
    emitter.instruction("shr r11, 56");                                         // extract the eight-byte match count
    emitter.instruction("add rax, r11");                                        // add this word's matches to the running count
    emitter.instruction("add r9, 8");                                           // advance to the next eight window bytes
    emitter.instruction("sub r10, 8");                                          // account for the eight bytes just counted
    emitter.instruction("cmp r10, 8");                                          // is another full word available?
    emitter.instruction("jge __rt_substr_count_byte_words_linux_x86_64");       // continue the word census while full words remain

    emitter.label("__rt_substr_count_byte_tail_linux_x86_64");
    emitter.instruction("test r10, r10");                                       // stop when the window is exhausted
    emitter.instruction("jz __rt_substr_count_done_linux_x86_64");              // no trailing bytes remain
    emitter.instruction("movzx r11d, BYTE PTR [r9]");                           // load the next trailing window byte
    emitter.instruction("cmp r11b, r8b");                                       // does the trailing byte match the needle?
    emitter.instruction("sete cl");                                             // materialize the byte comparison without branching
    emitter.instruction("movzx rcx, cl");                                       // widen the comparison flag
    emitter.instruction("add rax, rcx");                                        // add the trailing-byte match to the count
    emitter.instruction("add r9, 1");                                           // advance to the next trailing byte
    emitter.instruction("sub r10, 1");                                          // consume one trailing byte
    emitter.instruction("jmp __rt_substr_count_byte_tail_linux_x86_64");        // continue the trailing byte census

    // -- longer needle: jump to first-byte candidates and confirm with memcmp --
    emitter.label("__rt_substr_count_search_linux_x86_64");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                         // reserve the libc-preserved search state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the window pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the window length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the needle pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the needle length
    emitter.instruction("mov rax, rsi");                                        // copy the window length
    emitter.instruction("sub rax, rcx");                                        // last valid candidate start = window length - needle length
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // publish the last valid candidate
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // start scanning at the window start
    emitter.instruction("movzx eax, BYTE PTR [rdx]");                           // load the needle's first byte
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the first-byte search key
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // start the non-overlapping match counter at zero

    emitter.label("__rt_substr_count_search_loop_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the window pointer
    emitter.instruction("add rdi, QWORD PTR [rbp - 48]");                       // search from the current candidate byte
    emitter.instruction("mov esi, DWORD PTR [rbp - 56]");                       // pass the cached first needle byte
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the window length
    emitter.instruction("sub rdx, QWORD PTR [rbp - 48]");                       // remaining window bytes from the candidate
    emitter.bl_c("memchr");                                                    // find the next first-byte candidate
    emitter.instruction("test rax, rax");                                       // did memchr find another candidate?
    emitter.instruction("jz __rt_substr_count_search_done_linux_x86_64");       // no first-byte candidate remains
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the window pointer for the offset
    emitter.instruction("sub rax, r10");                                        // convert the candidate pointer into an offset
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // publish the candidate offset
    emitter.instruction("cmp rax, QWORD PTR [rbp - 40]");                       // does the full needle still fit here?
    emitter.instruction("jg __rt_substr_count_search_done_linux_x86_64");       // a candidate past the window cannot match
    emitter.instruction("add r10, rax");                                        // compare the full needle at this candidate
    emitter.instruction("mov rdi, r10");                                        // first memcmp argument = candidate bytes
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the needle pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // pass the needle length
    emitter.bl_c("memcmp");                                                    // compare the remaining needle bytes
    emitter.instruction("test eax, eax");                                       // did the full needle compare equal?
    emitter.instruction("jnz __rt_substr_count_search_next_linux_x86_64");      // a mismatch advances one byte
    emitter.instruction("add QWORD PTR [rbp - 64], 1");                         // record one more non-overlapping match
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the needle length
    emitter.instruction("add QWORD PTR [rbp - 48], r10");                       // skip the whole matched needle
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the non-overlapping continuation
    emitter.instruction("cmp rax, QWORD PTR [rbp - 40]");                       // is another candidate still in range?
    emitter.instruction("jle __rt_substr_count_search_loop_linux_x86_64");      // continue while candidates remain
    emitter.instruction("jmp __rt_substr_count_search_done_linux_x86_64");      // otherwise finish the search

    emitter.label("__rt_substr_count_search_next_linux_x86_64");
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // advance to the next candidate
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the advanced candidate
    emitter.instruction("cmp rax, QWORD PTR [rbp - 40]");                       // is another candidate still in range?
    emitter.instruction("jle __rt_substr_count_search_loop_linux_x86_64");      // continue while candidates remain

    emitter.label("__rt_substr_count_search_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // return the accumulated match count
    emitter.instruction("leave");                                               // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the match count

    emitter.label("__rt_substr_count_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return the accumulated match count
}
