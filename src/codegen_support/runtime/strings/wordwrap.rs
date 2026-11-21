//! Purpose:
//! Emits the `__rt_wordwrap` runtime helper assembly implementing PHP's word-aware wordwrap.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Implements PHP's algorithm: lines break at the last space at/after the wrap width; an
//!   over-long word is left intact unless `cut_long_words` is set, in which case it is broken at
//!   the width. Existing `\n` bytes reset the current line length.
//! - The worst-case `textlen * (1 + break_len)` result is reserved through
//!   `__rt_concat_reserve` before the first store, so long inputs fall back to heap storage
//!   instead of running off the end of the 64 KiB concat scratch buffer.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_wordwrap` runtime helper entry point, dispatching to the
/// target-specific implementation. On x86_64, delegates to `emit_wordwrap_linux_x86_64`;
/// on ARM64 (the default fallback), emits the full word-aware wordwrap loop.
///
/// Input registers (ARM64): x1=source ptr, x2=source len, x3=width, x4=break ptr, x5=break len,
/// x6=cut_long_words flag (0/1).
/// Output registers (ARM64): x1=result ptr, x2=result len.
/// Output registers (x86_64): rax=result ptr, rdx=result len.
///
/// Reserves the worst-case `textlen * (1 + break_len)` result through `__rt_concat_reserve`
/// (concat scratch while it fits, owned heap storage otherwise) and finishes through
/// `__rt_concat_publish`. Clobbers every caller-saved register, because the reservation can
/// reach `__rt_heap_alloc`; a wrapped size bound reports PHP's allocation-overflow fatal.
pub fn emit_wordwrap(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_wordwrap_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: wordwrap (word-aware) ---");
    emitter.label_global("__rt_wordwrap");

    // -- set up stack frame and preserve callee-saved registers --
    // Frame (112 bytes): [sp+0]=result start ptr; [sp+16..95]=saved x19-x28; [sp+96]=x29/x30.
    emitter.instruction("sub sp, sp, #112");                                    // allocate the wordwrap stack frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // set the frame pointer
    emitter.instruction("stp x19, x20, [sp, #16]");                             // save callee-saved x19, x20
    emitter.instruction("stp x21, x22, [sp, #32]");                             // save callee-saved x21, x22
    emitter.instruction("stp x23, x24, [sp, #48]");                             // save callee-saved x23, x24
    emitter.instruction("stp x25, x26, [sp, #64]");                             // save callee-saved x25, x26
    emitter.instruction("stp x27, x28, [sp, #80]");                             // save callee-saved x27, x28

    // -- load inputs into callee-saved registers --
    emitter.instruction("mov x19, x1");                                         // x19 = source base pointer
    emitter.instruction("mov x20, x2");                                         // x20 = source length (textlen)
    emitter.instruction("mov x21, x3");                                         // x21 = wrap width
    emitter.instruction("mov x22, x4");                                         // x22 = break-string pointer
    emitter.instruction("mov x23, x5");                                         // x23 = break-string length
    emitter.instruction("mov x24, x6");                                         // x24 = cut_long_words flag (0/1)
    emitter.instruction("mov x25, #0");                                         // x25 = laststart (start index of current line)
    emitter.instruction("mov x26, #-1");                                        // x26 = lastspace index (-1 = no space on line yet)
    emitter.instruction("mov x27, #0");                                         // x27 = current scan index

    // -- reserve the worst-case wrapped result before writing anything --
    emitter.instruction("add x9, x23, #1");                                     // worst case is one break string inserted after every source byte
    emitter.instruction("umulh x10, x20, x9");                                  // capture the high half of the textlen * (1 + break length) product
    emitter.instruction("cbnz x10, __rt_wordwrap_size_overflow");               // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("mul x0, x20, x9");                                     // compute the worst-case wrapped result size
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the wrapped result
    emitter.instruction("mov x12, x0");                                         // x12 = output write pointer at the reserved payload start at the reserved payload start
    emitter.instruction("str x12, [sp, #0]");                                   // save the result start pointer for the final length

    // -- main scan loop --
    emitter.label("__rt_wordwrap_loop");
    emitter.instruction("cmp x27, x20");                                        // have all source bytes been scanned?
    emitter.instruction("b.ge __rt_wordwrap_tail");                             // yes → copy the trailing line and finish
    emitter.instruction("ldrb w9, [x19, x27]");                                 // load the current source byte

    // -- existing newline resets the current line --
    emitter.instruction("cmp w9, #10");                                         // is the current byte a '\n'?
    emitter.instruction("b.ne __rt_wordwrap_not_nl");                           // no → check for a space
    emitter.instruction("add x10, x27, #1");                                    // include the newline in the flushed range
    emitter.instruction("sub x10, x10, x25");                                   // count = (current + 1) - laststart
    emitter.instruction("add x9, x19, x25");                                    // source = base + laststart
    emitter.instruction("bl __rt_wordwrap_cpy");                                // copy the line including its newline to output
    emitter.instruction("add x25, x27, #1");                                    // laststart = current + 1
    emitter.instruction("mov x26, #-1");                                        // reset lastspace (no space on the new line)
    emitter.instruction("b __rt_wordwrap_next");                                // advance to the next byte

    emitter.label("__rt_wordwrap_not_nl");
    emitter.instruction("cmp w9, #32");                                         // is the current byte a space?
    emitter.instruction("b.ne __rt_wordwrap_other");                            // no → handle a regular character

    // -- space: break here if the line already reached the width --
    emitter.instruction("sub x10, x27, x25");                                   // line length so far = current - laststart
    emitter.instruction("cmp x10, x21");                                        // has the line reached the wrap width?
    emitter.instruction("b.lt __rt_wordwrap_mark_space");                       // no → just remember this space
    emitter.instruction("sub x10, x27, x25");                                   // count = current - laststart (exclude the space)
    emitter.instruction("add x9, x19, x25");                                    // source = base + laststart
    emitter.instruction("bl __rt_wordwrap_cpy");                                // copy the completed line (without the space)
    emitter.instruction("mov x10, x23");                                        // count = break-string length
    emitter.instruction("mov x9, x22");                                         // source = break-string pointer
    emitter.instruction("bl __rt_wordwrap_cpy");                                // copy the break string in place of the space
    emitter.instruction("add x25, x27, #1");                                    // laststart = current + 1 (skip the space)
    emitter.instruction("mov x26, #-1");                                        // reset lastspace
    emitter.instruction("b __rt_wordwrap_next");                                // advance to the next byte

    emitter.label("__rt_wordwrap_mark_space");
    emitter.instruction("mov x26, x27");                                        // lastspace = current
    emitter.instruction("b __rt_wordwrap_next");                                // advance to the next byte

    // -- regular character: break only when the line exceeds the width --
    emitter.label("__rt_wordwrap_other");
    emitter.instruction("sub x10, x27, x25");                                   // line length so far = current - laststart
    emitter.instruction("cmp x10, x21");                                        // is the line still under the wrap width?
    emitter.instruction("b.lt __rt_wordwrap_next");                             // yes → keep accumulating the word
    emitter.instruction("cmn x26, #1");                                         // is lastspace == -1 (no space on this line)?
    emitter.instruction("b.eq __rt_wordwrap_no_space");                         // yes → only a long word can be cut here

    // -- break at the last space seen on this line --
    emitter.instruction("sub x10, x26, x25");                                   // count = lastspace - laststart
    emitter.instruction("add x9, x19, x25");                                    // source = base + laststart
    emitter.instruction("bl __rt_wordwrap_cpy");                                // copy the line up to (not including) the space
    emitter.instruction("mov x10, x23");                                        // count = break-string length
    emitter.instruction("mov x9, x22");                                         // source = break-string pointer
    emitter.instruction("bl __rt_wordwrap_cpy");                                // copy the break string in place of the space
    emitter.instruction("add x25, x26, #1");                                    // laststart = lastspace + 1
    emitter.instruction("mov x26, #-1");                                        // reset lastspace
    emitter.instruction("b __rt_wordwrap_next");                                // advance to the next byte

    // -- long word with no space: break mid-word only when cut is requested --
    emitter.label("__rt_wordwrap_no_space");
    emitter.instruction("cbz x24, __rt_wordwrap_next");                         // cut disabled → leave the long word intact
    emitter.instruction("sub x10, x27, x25");                                   // count = current - laststart (a full width run)
    emitter.instruction("add x9, x19, x25");                                    // source = base + laststart
    emitter.instruction("bl __rt_wordwrap_cpy");                                // copy the width-long run of the word
    emitter.instruction("mov x10, x23");                                        // count = break-string length
    emitter.instruction("mov x9, x22");                                         // source = break-string pointer
    emitter.instruction("bl __rt_wordwrap_cpy");                                // copy the break string mid-word
    emitter.instruction("mov x25, x27");                                        // laststart = current (the remaining word continues)
    emitter.instruction("mov x26, #-1");                                        // reset lastspace

    emitter.label("__rt_wordwrap_next");
    emitter.instruction("add x27, x27, #1");                                    // current += 1
    emitter.instruction("b __rt_wordwrap_loop");                                // continue scanning

    // -- copy the final line (laststart .. textlen) --
    emitter.label("__rt_wordwrap_tail");
    emitter.instruction("sub x10, x20, x25");                                   // count = textlen - laststart
    emitter.instruction("add x9, x19, x25");                                    // source = base + laststart
    emitter.instruction("bl __rt_wordwrap_cpy");                                // copy the trailing line to output

    // -- finalize result pointer/length and publish the written bytes --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = result start pointer
    emitter.instruction("sub x2, x12, x1");                                     // x2 = result length = end - start
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results

    // -- restore callee-saved registers and return --
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore x19, x20
    emitter.instruction("ldp x21, x22, [sp, #32]");                             // restore x21, x22
    emitter.instruction("ldp x23, x24, [sp, #48]");                             // restore x23, x24
    emitter.instruction("ldp x25, x26, [sp, #64]");                             // restore x25, x26
    emitter.instruction("ldp x27, x28, [sp, #80]");                             // restore x27, x28
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the wrapped string in x1/x2

    // -- internal copy helper: copy x10 bytes from x9 to the output pointer x12 --
    // Clobbers x9, x10, x11 and advances x12. Uses x30 (caller saved it on the stack).
    // The cursor lives in x12, NOT in a callee-saved register: x28 is the
    // reserved runtime-context pointer in `--rt-ctx` builds, and the final
    // `bl __rt_concat_publish` below reads per-context state through it.
    emitter.label("__rt_wordwrap_cpy");
    emitter.instruction("cbz x10, __rt_wordwrap_cpy_ret");                      // nothing to copy
    emitter.label("__rt_wordwrap_cpy_loop");
    emitter.instruction("ldrb w11, [x9], #1");                                  // load a source byte and advance
    emitter.instruction("strb w11, [x12], #1");                                 // store it to output and advance
    emitter.instruction("subs x10, x10, #1");                                   // decrement the remaining byte count
    emitter.instruction("b.ne __rt_wordwrap_cpy_loop");                         // continue until all bytes are copied
    emitter.label("__rt_wordwrap_cpy_ret");
    emitter.instruction("ret");                                                 // return to the wrapping loop

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_wordwrap_size_overflow");
    emitter.instruction("b __rt_alloc_overflow");                               // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits the x86_64 Linux implementation of the word-aware `__rt_wordwrap` runtime helper.
///
/// Input registers: rax=source ptr, rdx=source len, rdi=width, rcx=break str ptr, r8=break str len,
/// r9=cut_long_words flag (0/1).
/// Output registers: rax=result ptr, rdx=result len.
///
/// Hot state lives in callee-saved registers (rbx=source base, r12=current, r13=laststart,
/// r15=output pointer); width, textlen, break ptr/len, cut, the result start AND the
/// lastspace index (`[rbp-96]`) are spilled. Writes wrapped output to `_concat_buf` /
/// `_concat_off` and advances `_concat_off` on completion.
///
/// `lastspace` lives in a spill slot rather than a register because this target has no
/// sixth callee-saved register left: `r14` is the reserved runtime-context pointer in
/// `--rt-ctx` builds, so no runtime helper may borrow it (the same resolution as the
/// integer-significand flag in `str_to_number`).
fn emit_wordwrap_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: wordwrap (word-aware) ---");
    emitter.label_global("__rt_wordwrap");

    // -- set up the frame and preserve callee-saved registers --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for spills
    emitter.instruction("push rbx");                                            // preserve callee-saved rbx (source base)
    emitter.instruction("push r12");                                            // preserve callee-saved r12 (current index)
    emitter.instruction("push r13");                                            // preserve callee-saved r13 (laststart)
    emitter.instruction("sub rsp, 8");                                          // placeholder for the former lastspace register: keeps every rbp-relative spill offset below unchanged
    emitter.instruction("push r15");                                            // preserve callee-saved r15 (output pointer)
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for the cold inputs

    // -- spill the cold inputs and initialize hot state --
    emitter.instruction("mov QWORD PTR [rbp - 72], rdi");                       // spill the wrap width
    emitter.instruction("mov QWORD PTR [rbp - 80], rcx");                       // spill the break-string pointer
    emitter.instruction("mov QWORD PTR [rbp - 88], r8");                        // spill the break-string length
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // spill the cut_long_words flag
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // spill the source length (textlen)
    emitter.instruction("mov rbx, rax");                                        // rbx = source base pointer
    emitter.instruction("xor r12, r12");                                        // r12 = current scan index = 0
    emitter.instruction("xor r13, r13");                                        // r13 = laststart = 0
    emitter.instruction("mov QWORD PTR [rbp - 96], -1");                        // lastspace = -1 (no space on line yet)

    // -- reserve the worst-case wrapped result before writing anything --
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // reload the break-string length before deriving the worst-case expansion factor
    emitter.instruction("add rax, 1");                                          // worst case is one break string inserted after every source byte
    emitter.instruction("imul rax, QWORD PTR [rbp - 56]");                      // compute the worst-case wrapped result size as textlen * (1 + break length)
    emitter.instruction("jo __rt_wordwrap_size_overflow_x86_64");               // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the wrapped result
    emitter.instruction("mov r15, rax");                                        // r15 = output write pointer at the reserved payload start
    emitter.instruction("mov QWORD PTR [rbp - 64], r15");                       // save the result start pointer for the final length

    // -- main scan loop --
    emitter.label("__rt_wordwrap_loop_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload textlen
    emitter.instruction("cmp r12, rax");                                        // have all source bytes been scanned?
    emitter.instruction("jge __rt_wordwrap_tail_x86_64");                       // yes → copy the trailing line and finish
    emitter.instruction("movzx eax, BYTE PTR [rbx + r12]");                     // load the current source byte

    // -- existing newline resets the current line --
    emitter.instruction("cmp al, 10");                                          // is the current byte a '\n'?
    emitter.instruction("jne __rt_wordwrap_not_nl_x86_64");                     // no → check for a space
    emitter.instruction("lea r10, [r12 + 1]");                                  // include the newline in the flushed range
    emitter.instruction("sub r10, r13");                                        // count = (current + 1) - laststart
    emitter.instruction("lea rsi, [rbx + r13]");                                // source = base + laststart
    emitter.instruction("call __rt_wordwrap_cpy_x86_64");                       // copy the line including its newline to output
    emitter.instruction("lea r13, [r12 + 1]");                                  // laststart = current + 1
    emitter.instruction("mov QWORD PTR [rbp - 96], -1");                        // reset lastspace
    emitter.instruction("jmp __rt_wordwrap_next_x86_64");                       // advance to the next byte

    emitter.label("__rt_wordwrap_not_nl_x86_64");
    emitter.instruction("cmp al, 32");                                          // is the current byte a space?
    emitter.instruction("jne __rt_wordwrap_other_x86_64");                      // no → handle a regular character

    // -- space: break here if the line already reached the width --
    emitter.instruction("mov r10, r12");                                        // line length so far = current ...
    emitter.instruction("sub r10, r13");                                        // ... - laststart
    emitter.instruction("cmp r10, QWORD PTR [rbp - 72]");                       // has the line reached the wrap width?
    emitter.instruction("jl __rt_wordwrap_mark_space_x86_64");                  // no → just remember this space
    emitter.instruction("mov r10, r12");                                        // count = current ...
    emitter.instruction("sub r10, r13");                                        // ... - laststart (exclude the space)
    emitter.instruction("lea rsi, [rbx + r13]");                                // source = base + laststart
    emitter.instruction("call __rt_wordwrap_cpy_x86_64");                       // copy the completed line (without the space)
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // count = break-string length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // source = break-string pointer
    emitter.instruction("call __rt_wordwrap_cpy_x86_64");                       // copy the break string in place of the space
    emitter.instruction("lea r13, [r12 + 1]");                                  // laststart = current + 1 (skip the space)
    emitter.instruction("mov QWORD PTR [rbp - 96], -1");                        // reset lastspace
    emitter.instruction("jmp __rt_wordwrap_next_x86_64");                       // advance to the next byte

    emitter.label("__rt_wordwrap_mark_space_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 96], r12");                       // lastspace = current
    emitter.instruction("jmp __rt_wordwrap_next_x86_64");                       // advance to the next byte

    // -- regular character: break only when the line exceeds the width --
    emitter.label("__rt_wordwrap_other_x86_64");
    emitter.instruction("mov r10, r12");                                        // line length so far = current ...
    emitter.instruction("sub r10, r13");                                        // ... - laststart
    emitter.instruction("cmp r10, QWORD PTR [rbp - 72]");                       // is the line still under the wrap width?
    emitter.instruction("jl __rt_wordwrap_next_x86_64");                        // yes → keep accumulating the word
    emitter.instruction("cmp QWORD PTR [rbp - 96], -1");                        // is lastspace == -1 (no space on this line)?
    emitter.instruction("je __rt_wordwrap_no_space_x86_64");                    // yes → only a long word can be cut here

    // -- break at the last space seen on this line --
    emitter.instruction("mov r10, QWORD PTR [rbp - 96]");                       // count = lastspace ...
    emitter.instruction("sub r10, r13");                                        // ... - laststart
    emitter.instruction("lea rsi, [rbx + r13]");                                // source = base + laststart
    emitter.instruction("call __rt_wordwrap_cpy_x86_64");                       // copy the line up to (not including) the space
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // count = break-string length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // source = break-string pointer
    emitter.instruction("call __rt_wordwrap_cpy_x86_64");                       // copy the break string in place of the space
    emitter.instruction("mov r13, QWORD PTR [rbp - 96]");                       // laststart = lastspace ...
    emitter.instruction("add r13, 1");                                          // ... + 1 (skip the space that was replaced by the break)
    emitter.instruction("mov QWORD PTR [rbp - 96], -1");                        // reset lastspace
    emitter.instruction("jmp __rt_wordwrap_next_x86_64");                       // advance to the next byte

    // -- long word with no space: break mid-word only when cut is requested --
    emitter.label("__rt_wordwrap_no_space_x86_64");
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // is cut_long_words disabled?
    emitter.instruction("je __rt_wordwrap_next_x86_64");                        // yes → leave the long word intact
    emitter.instruction("mov r10, r12");                                        // count = current ...
    emitter.instruction("sub r10, r13");                                        // ... - laststart (a full width run)
    emitter.instruction("lea rsi, [rbx + r13]");                                // source = base + laststart
    emitter.instruction("call __rt_wordwrap_cpy_x86_64");                       // copy the width-long run of the word
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // count = break-string length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // source = break-string pointer
    emitter.instruction("call __rt_wordwrap_cpy_x86_64");                       // copy the break string mid-word
    emitter.instruction("mov r13, r12");                                        // laststart = current (the remaining word continues)
    emitter.instruction("mov QWORD PTR [rbp - 96], -1");                        // reset lastspace

    emitter.label("__rt_wordwrap_next_x86_64");
    emitter.instruction("add r12, 1");                                          // current += 1
    emitter.instruction("jmp __rt_wordwrap_loop_x86_64");                       // continue scanning

    // -- copy the final line (laststart .. textlen) --
    emitter.label("__rt_wordwrap_tail_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // count = textlen ...
    emitter.instruction("sub r10, r13");                                        // ... - laststart
    emitter.instruction("lea rsi, [rbx + r13]");                                // source = base + laststart
    emitter.instruction("call __rt_wordwrap_cpy_x86_64");                       // copy the trailing line to output

    // -- finalize result pointer/length and publish the written bytes --
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // rax = result start pointer
    emitter.instruction("mov rdx, r15");                                        // rdx = output end pointer
    emitter.instruction("sub rdx, rax");                                        // rdx = result length = end - start
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed results

    // -- restore callee-saved registers and return --
    emitter.instruction("add rsp, 64");                                         // release the spill slots
    emitter.instruction("pop r15");                                             // restore r15
    emitter.instruction("add rsp, 8");                                          // release the former lastspace register placeholder
    emitter.instruction("pop r13");                                             // restore r13
    emitter.instruction("pop r12");                                             // restore r12
    emitter.instruction("pop rbx");                                             // restore rbx
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the wrapped string in rax/rdx

    // -- internal copy helper: copy r10 bytes from rsi to the output pointer r15 --
    // Clobbers rsi, r10, al and advances r15.
    emitter.label("__rt_wordwrap_cpy_x86_64");
    emitter.instruction("test r10, r10");                                       // nothing to copy?
    emitter.instruction("jz __rt_wordwrap_cpy_ret_x86_64");                     // yes → return immediately
    emitter.label("__rt_wordwrap_cpy_loop_x86_64");
    emitter.instruction("mov al, BYTE PTR [rsi]");                              // load a source byte
    emitter.instruction("mov BYTE PTR [r15], al");                              // store it to output
    emitter.instruction("add rsi, 1");                                          // advance the source cursor
    emitter.instruction("add r15, 1");                                          // advance the output cursor
    emitter.instruction("sub r10, 1");                                          // decrement the remaining byte count
    emitter.instruction("jnz __rt_wordwrap_cpy_loop_x86_64");                   // continue until all bytes are copied
    emitter.label("__rt_wordwrap_cpy_ret_x86_64");
    emitter.instruction("ret");                                                 // return to the wrapping loop

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_wordwrap_size_overflow_x86_64");
    emitter.instruction("jmp __rt_alloc_overflow");                             // unconditional branch keeps the fatal trampoline reachable from every caller
}
