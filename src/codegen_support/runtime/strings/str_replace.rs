//! Purpose:
//! Emits the `__rt_str_replace`, `__rt_str_replace_loop` runtime helper assembly for str replace.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers scan or transform byte ranges and return target ABI pointer/length pairs for generated call sites.
//! - The destination is sized before the first store: at most `subject_len / search_len`
//!   replacements can fire, so `subject_len + (subject_len / search_len) * replacement_len`
//!   bounds the result. That bound goes through `__rt_concat_reserve`, so an expanding
//!   replacement falls back to heap storage instead of running off the end of the 64 KiB
//!   concat scratch buffer.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_str_replace` runtime helper.
///
/// Replaces all occurrences of `search` with `replace` in `subject`, returning
/// the result as a pointer/length pair in the concat buffer.
///
/// # Input (ARM64 calling convention)
/// - `x1/x2`: search string pointer and length
/// - `x3/x4`: replacement string pointer and length
/// - `x5/x6`: subject string pointer and length
///
/// # Output (ARM64 calling convention)
/// - `x1`: result string pointer
/// - `x2`: result string length
///
/// # Side effects
/// - Reserves the bounded destination through `__rt_concat_reserve` and publishes the written
///   length through `__rt_concat_publish`, which advances `_concat_off` only for scratch-backed
///   results. Clobbers every caller-saved register, because the reservation can reach
///   `__rt_heap_alloc`. A wrapped size bound reports PHP's allocation-overflow fatal.
///
/// # Dispatch
/// On x86_64 Linux, delegates to `emit_str_replace_linux_x86_64`.
pub fn emit_str_replace(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_replace_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_replace ---");
    emitter.label_global("__rt_str_replace");

    // -- set up stack frame (80 bytes) --
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer

    // -- save input arguments to stack --
    emitter.instruction("stp x1, x2, [sp]");                                    // save search string ptr and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save replacement string ptr and length
    emitter.instruction("stp x5, x6, [sp, #32]");                               // save subject string ptr and length

    // -- reserve the bounded destination before the first store --
    emitter.instruction("mov x0, x6");                                          // an empty search never matches, so the subject length alone bounds the result
    emitter.instruction("cbz x2, __rt_str_replace_reserve");                    // skip the expansion arithmetic when the search string is empty
    emitter.instruction("udiv x9, x6, x2");                                     // at most subject_len / search_len replacements can fire
    emitter.instruction("umulh x10, x9, x4");                                   // capture the high half of the replacement-count * replacement-length product
    emitter.instruction("cbnz x10, __rt_str_replace_size_overflow");            // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("mul x9, x9, x4");                                      // total replacement bytes the loop can ever emit
    emitter.instruction("adds x0, x6, x9");                                     // upper bound = subject length plus all emitted replacement bytes
    emitter.instruction("b.cs __rt_str_replace_size_overflow");                 // reject a wrapped bound instead of reserving a too-small destination
    emitter.label("__rt_str_replace_reserve");
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the replaced string
    emitter.instruction("mov x12, x0");                                         // compute destination pointer
    emitter.instruction("str x12, [sp, #48]");                                  // save result start pointer

    // -- initialize subject scan index --
    emitter.instruction("mov x13, #0");                                         // subject index = 0

    // -- main loop: scan subject for search string --
    emitter.label("__rt_str_replace_loop");
    emitter.instruction("ldp x5, x6, [sp, #32]");                               // reload subject ptr and length
    emitter.instruction("cmp x13, x6");                                         // check if past end of subject
    emitter.instruction("b.ge __rt_str_replace_done");                          // if done, finalize result

    // -- check if search string matches at current position --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload search ptr and length
    emitter.instruction("cbz x2, __rt_str_replace_copy_byte");                  // empty search = never matches, copy byte
    emitter.instruction("sub x14, x6, x13");                                    // remaining = subject_len - current_pos
    emitter.instruction("cmp x2, x14");                                         // check if search fits in remaining
    emitter.instruction("b.gt __rt_str_replace_copy_byte");                     // search longer than remaining, copy byte

    // -- compare search string at current position --
    emitter.instruction("mov x15, #0");                                         // match comparison index = 0
    emitter.label("__rt_str_replace_match");
    emitter.instruction("cmp x15, x2");                                         // check if all search bytes matched
    emitter.instruction("b.ge __rt_str_replace_found");                         // full match found
    emitter.instruction("add x16, x13, x15");                                   // compute subject index = pos + match_idx
    emitter.instruction("ldrb w17, [x5, x16]");                                 // load subject byte at computed index
    emitter.instruction("ldrb w9, [x1, x15]");                                  // load search byte (w9: x18 is reserved on Apple)
    emitter.instruction("cmp w17, w9");                                         // compare subject and search bytes
    emitter.instruction("b.ne __rt_str_replace_copy_byte");                     // mismatch, just copy current byte
    emitter.instruction("add x15, x15, #1");                                    // advance match index
    emitter.instruction("b __rt_str_replace_match");                            // continue matching

    // -- match found: copy replacement string --
    emitter.label("__rt_str_replace_found");
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload replacement ptr and length
    emitter.instruction("mov x15, #0");                                         // replacement copy index = 0
    emitter.label("__rt_str_replace_rep_copy");
    emitter.instruction("cmp x15, x4");                                         // check if all replacement bytes copied
    emitter.instruction("b.ge __rt_str_replace_rep_done");                      // done copying replacement
    emitter.instruction("ldrb w17, [x3, x15]");                                 // load replacement byte at index
    emitter.instruction("strb w17, [x12], #1");                                 // store to dest, advance dest ptr
    emitter.instruction("add x15, x15, #1");                                    // advance replacement index
    emitter.instruction("b __rt_str_replace_rep_copy");                         // continue copying replacement
    emitter.label("__rt_str_replace_rep_done");
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload search ptr and length
    emitter.instruction("add x13, x13, x2");                                    // skip past matched search in subject
    emitter.instruction("b __rt_str_replace_loop");                             // continue scanning subject

    // -- no match: copy single byte from subject --
    emitter.label("__rt_str_replace_copy_byte");
    emitter.instruction("ldp x5, x6, [sp, #32]");                               // reload subject ptr and length
    emitter.instruction("ldrb w17, [x5, x13]");                                 // load subject byte at current position
    emitter.instruction("strb w17, [x12], #1");                                 // store to dest, advance dest ptr
    emitter.instruction("add x13, x13, #1");                                    // advance subject index by 1
    emitter.instruction("b __rt_str_replace_loop");                             // continue scanning

    // -- finalize: compute result length and publish the written bytes --
    emitter.label("__rt_str_replace_done");
    emitter.instruction("ldr x1, [sp, #48]");                                   // load result start pointer
    emitter.instruction("sub x2, x12, x1");                                     // result length = dest_end - dest_start
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_str_replace_size_overflow");
    emitter.instruction("b __rt_alloc_overflow");                               // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits the x86_64 Linux variant of the `__rt_str_replace` runtime helper.
///
/// Implements the same semantic as `emit_str_replace` for the System V AMD64 ABI.
/// Uses a frame pointer in `rbp` with 80 bytes of spill slots to preserve the three
/// input strings, concat-buffer bookkeeping, and the scanning cursor across the loop.
///
/// # Input (System V AMD64 calling convention)
/// - `rdi/rsi`: search string pointer and length
/// - `rdx/rcx`: replacement string pointer and length
/// - `r8/r9`: subject string pointer and length (extra arguments passed in r8, r9)
///
/// # Output (System V AMD64 calling convention)
/// - `rax`: result string pointer in `_concat_buf`
/// - `rdx`: result string length
///
/// # Side effects
/// - Advances the `_concat_off` global write offset by the result length.
fn emit_str_replace_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_replace ---");
    emitter.label_global("__rt_str_replace");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving str_replace() spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved search, replacement, and subject strings
    emitter.instruction("sub rsp, 80");                                         // reserve aligned spill slots for the three input strings plus concat-buffer bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the search string pointer across the replacement loop
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the search string length across the replacement loop
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the replacement string pointer across the replacement loop
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // preserve the replacement string length across the replacement loop
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // preserve the subject string pointer across the replacement loop
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // preserve the subject string length across the replacement loop

    // -- reserve the bounded destination before the first store --
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the search-string length to decide how much expansion is possible
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // an empty search never matches, so the subject length alone bounds the result
    emitter.instruction("test r10, r10");                                       // is the search string empty?
    emitter.instruction("jz __rt_str_replace_reserve_linux_x86_64");            // skip the expansion arithmetic when the search string is empty
    emitter.instruction("xor rdx, rdx");                                        // clear the high dividend word before the unsigned division
    emitter.instruction("div r10");                                             // at most subject_len / search_len replacements can fire
    emitter.instruction("imul rax, QWORD PTR [rbp - 32]");                      // total replacement bytes the loop can ever emit
    emitter.instruction("jo __rt_str_replace_size_overflow_linux_x86_64");      // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("add rax, QWORD PTR [rbp - 48]");                       // upper bound = subject length plus all emitted replacement bytes
    emitter.instruction("jc __rt_str_replace_size_overflow_linux_x86_64");      // reject a wrapped bound instead of reserving a too-small destination
    emitter.label("__rt_str_replace_reserve_linux_x86_64");
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the replaced string
    emitter.instruction("mov r11, rax");                                        // compute the destination pointer where the replaced string begins
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // preserve the replaced-string start pointer for the final string return pair
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // start scanning the subject string from byte offset zero

    emitter.label("__rt_str_replace_loop_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the current subject-string byte offset before testing loop completion
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // reload the subject-string length before testing loop completion
    emitter.instruction("cmp r9, r8");                                          // have we already consumed every byte of the subject string?
    emitter.instruction("jge __rt_str_replace_done_linux_x86_64");              // stop once the current subject offset reaches the subject length
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the search-string length before testing whether a match can fit at the current offset
    emitter.instruction("test r10, r10");                                       // is the search string empty at the current replacement step?
    emitter.instruction("jz __rt_str_replace_copy_byte_linux_x86_64");          // copy the current subject byte verbatim when the search string is empty
    emitter.instruction("mov rcx, r8");                                         // copy the subject length before computing how many bytes remain at the current offset
    emitter.instruction("sub rcx, r9");                                         // compute the remaining subject-string bytes from the current offset to the end
    emitter.instruction("cmp r10, rcx");                                        // can the search string fit entirely inside the remaining subject tail?
    emitter.instruction("jg __rt_str_replace_copy_byte_linux_x86_64");          // copy the current subject byte verbatim when the search string is longer than the remaining tail
    emitter.instruction("xor rcx, rcx");                                        // start comparing the search string from byte index zero at the current subject offset

    emitter.label("__rt_str_replace_match_linux_x86_64");
    emitter.instruction("cmp rcx, r10");                                        // have all search-string bytes matched at the current subject offset?
    emitter.instruction("jge __rt_str_replace_found_linux_x86_64");             // jump to replacement copying once the full search string matches
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the subject string pointer before loading the next candidate byte
    emitter.instruction("mov rdx, r9");                                         // copy the current subject offset before indexing into the subject string
    emitter.instruction("add rdx, rcx");                                        // compute the subject byte offset for the current search-byte comparison
    emitter.instruction("movzx eax, BYTE PTR [rax + rdx]");                     // load the candidate subject byte at the current match position
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the search string pointer before loading the next search byte
    emitter.instruction("movzx edx, BYTE PTR [rdx + rcx]");                     // load the search byte at the current match position
    emitter.instruction("cmp eax, edx");                                        // compare the current subject and search bytes at the match position
    emitter.instruction("jne __rt_str_replace_copy_byte_linux_x86_64");         // copy the current subject byte verbatim on the first mismatching search byte
    emitter.instruction("add rcx, 1");                                          // advance to the next search-byte comparison after a successful byte match
    emitter.instruction("jmp __rt_str_replace_match_linux_x86_64");             // continue matching the remaining search bytes at the current subject offset

    emitter.label("__rt_str_replace_found_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the replacement-string length before copying the replacement bytes
    emitter.instruction("xor r8d, r8d");                                        // start copying the replacement string from byte index zero

    emitter.label("__rt_str_replace_rep_linux_x86_64");
    emitter.instruction("cmp r8, rcx");                                         // have all replacement-string bytes been copied into the concat buffer?
    emitter.instruction("jge __rt_str_replace_rep_done_linux_x86_64");          // advance the subject cursor once the full replacement string has been emitted
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the replacement string pointer before loading the next replacement byte
    emitter.instruction("mov dl, BYTE PTR [rax + r8]");                         // load the current replacement byte
    emitter.instruction("mov BYTE PTR [r11], dl");                              // store the current replacement byte into the concat-buffer destination
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination after storing one replacement byte
    emitter.instruction("add r8, 1");                                           // advance to the next replacement byte after a successful copy
    emitter.instruction("jmp __rt_str_replace_rep_linux_x86_64");               // continue copying replacement bytes until the full replacement string is emitted

    emitter.label("__rt_str_replace_rep_done_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the current subject offset before skipping the matched search string
    emitter.instruction("add r9, QWORD PTR [rbp - 16]");                        // skip past the fully matched search string inside the subject string
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // preserve the updated subject offset for the next replacement-loop iteration
    emitter.instruction("jmp __rt_str_replace_loop_linux_x86_64");              // continue scanning the subject string after emitting the replacement bytes

    emitter.label("__rt_str_replace_copy_byte_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the subject string pointer before copying the current unmatched subject byte
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the current subject offset before copying the unmatched subject byte
    emitter.instruction("mov dl, BYTE PTR [rax + r9]");                         // load the current unmatched subject byte
    emitter.instruction("mov BYTE PTR [r11], dl");                              // copy the unmatched subject byte into the concat-buffer destination
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination after copying one unmatched subject byte
    emitter.instruction("add r9, 1");                                           // advance the subject offset by one after copying the unmatched subject byte
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // preserve the updated subject offset for the next replacement-loop iteration
    emitter.instruction("jmp __rt_str_replace_loop_linux_x86_64");              // continue scanning the subject string after copying the unmatched byte

    emitter.label("__rt_str_replace_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // return the reserved start pointer of the replaced string in the primary x86_64 string result register
    emitter.instruction("mov rdx, r11");                                        // copy the destination end pointer so the final replaced-string length can be derived
    emitter.instruction("sub rdx, rax");                                        // derive the replaced-string length from the destination start/end pointers
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("add rsp, 80");                                         // release the str_replace() spill slots before returning the replaced string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the replaced string in the standard x86_64 string result registers

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_str_replace_size_overflow_linux_x86_64");
    emitter.instruction("jmp __rt_alloc_overflow");                             // unconditional branch keeps the fatal trampoline reachable from every caller
}
