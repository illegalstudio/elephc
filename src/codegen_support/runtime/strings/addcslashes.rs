//! Purpose:
//! Emits the `__rt_addcslashes` runtime helper for PHP's `addcslashes()`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` through the strings module.
//!
//! Key details:
//! - The helper builds a 256-byte escape table from the literal character list, expanding
//!   ascending `a..z` ranges exactly like PHP and ignoring descending ranges.
//! - Escaped bytes use PHP's short C spellings for bell/backspace/tab/newline/vertical
//!   tab/form feed/carriage return, octal `\\ooo` for other non-printable bytes, and a
//!   backslash prefix for printable bytes.
//! - The worst-case four-bytes-per-input-byte result is reserved through
//!   `__rt_concat_reserve` before the first store; scratch-backed results are published
//!   through `__rt_concat_publish`. Both supported architectures share the same contract.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// The runtime warning PHP emits when a character-list range descends.
pub(crate) const ADDCSLASHES_RANGE_WARNING: &str =
    "Warning: addcslashes(): Invalid '..'-range, '..'-range needs to be incrementing\n";

/// Emits the `__rt_addcslashes` runtime helper for PHP's `addcslashes()`.
pub fn emit_addcslashes(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_addcslashes_linux_x86_64(emitter);
        return;
    }
    emit_addcslashes_aarch64(emitter);
}

/// Emits the AArch64 variant. Input: x1/x2 string, x3/x4 character list; output: x1/x2.
fn emit_addcslashes_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: addcslashes ---");
    emitter.label_global("__rt_addcslashes");
    emitter.instruction("sub sp, sp, #352");                                    // reserve a byte lookup table, argument spills, and the frame footer
    emitter.instruction("stp x29, x30, [sp, #336]");                            // save the caller frame pointer and return address
    emitter.instruction("add x29, sp, #336");                                   // establish the helper frame pointer
    emitter.instruction("stp x1, x2, [sp, #256]");                              // save the input pointer and length across helper calls
    emitter.instruction("stp x3, x4, [sp, #272]");                              // save the character-list pointer and length
    emitter.instruction("mov x9, #0");                                          // zero the first table word
    emitter.instruction("mov x10, #0");                                         // start zeroing at table offset zero
    emitter.label("__rt_addcslashes_zero_table");
    emitter.instruction("add x11, sp, x10");                                    // address the next sixteen table bytes
    emitter.instruction("stp x9, x9, [x11]");                                   // clear two eight-byte table words
    emitter.instruction("add x10, x10, #16");                                   // advance to the next table block
    emitter.instruction("cmp x10, #256");                                       // has the complete byte table been cleared?
    emitter.instruction("b.lo __rt_addcslashes_zero_table");                    // clear the remaining table blocks
    emitter.instruction("ldr x3, [sp, #272]");                                  // reload the character-list pointer
    emitter.instruction("ldr x4, [sp, #280]");                                  // reload the character-list length
    emitter.instruction("mov x5, #0");                                          // start scanning the character list at byte zero
    emitter.instruction("mov x6, #-1");                                         // no preceding character exists before the first mask byte
    emitter.instruction("mov w9, #0");                                          // start with no descending-range warnings
    emitter.instruction("str w9, [sp, #288]");                                  // initialize the warning counter
    emitter.label("__rt_addcslashes_mask_loop");
    emitter.instruction("cmp x5, x4");                                          // has every character-list byte been scanned?
    emitter.instruction("b.hs __rt_addcslashes_mask_done");                     // finish table construction
    emitter.instruction("ldrb w7, [x3, x5]");                                   // load the current character-list byte
    emitter.instruction("mov w15, #1");                                         // table entries are boolean markers, including for NUL
    emitter.instruction("cmp w7, #46");                                         // is this byte the first dot of a range?
    emitter.instruction("b.ne __rt_addcslashes_mask_single");                   // a non-dot byte marks one literal character
    emitter.instruction("add x8, x5, #2");                                      // locate the range end byte after the two dots
    emitter.instruction("cmp x8, x4");                                          // does the range end byte exist?
    emitter.instruction("b.hs __rt_addcslashes_mask_single");                   // a trailing dot is a literal character
    emitter.instruction("add x9, x5, #1");                                      // locate the second dot
    emitter.instruction("ldrb w9, [x3, x9]");                                   // load the second dot byte
    emitter.instruction("cmp w9, #46");                                         // is the second byte also a dot?
    emitter.instruction("b.ne __rt_addcslashes_mask_single");                   // a single dot is a literal character
    emitter.instruction("cmn x6, #1");                                          // is there a preceding range start character?
    emitter.instruction("b.eq __rt_addcslashes_mask_single");                   // a leading range marker has no start character
    emitter.instruction("ldrb w9, [x3, x8]");                                   // load the range end character
    emitter.instruction("cmp w6, w9");                                          // is the range ascending?
    emitter.instruction("b.hi __rt_addcslashes_skip_range");                    // PHP rejects descending ranges
    emitter.instruction("mov w10, w6");                                         // start the inclusive range at its first character
    emitter.label("__rt_addcslashes_range_loop");
    emitter.instruction("strb w15, [sp, x10]");                                 // mark this range character as escaped
    emitter.instruction("cmp w10, w9");                                         // has the range reached its final character?
    emitter.instruction("b.hs __rt_addcslashes_range_done");                    // the inclusive range is complete
    emitter.instruction("add w10, w10, #1");                                    // advance to the next range character
    emitter.instruction("b __rt_addcslashes_range_loop");                       // mark the remaining range characters
    emitter.label("__rt_addcslashes_range_done");
    emitter.instruction("add x5, x5, #3");                                      // skip the start character, both dots, and the end character
    emitter.instruction("mov x6, #-1");                                         // the next range needs a fresh start character
    emitter.instruction("b __rt_addcslashes_mask_loop");                        // continue scanning the character list
    emitter.label("__rt_addcslashes_skip_range");
    emitter.instruction("add x8, x5, #1");                                      // locate the first dot of the rejected range
    emitter.instruction("ldrb w10, [x3, x8]");                                  // load the first dot as a literal character
    emitter.instruction("strb w15, [sp, x10]");                                 // PHP still escapes the range syntax bytes
    emitter.instruction("add x8, x5, #2");                                      // locate the second dot
    emitter.instruction("ldrb w10, [x3, x8]");                                  // load the second dot as a literal character
    emitter.instruction("strb w15, [sp, x10]");                                 // mark the second dot as escaped
    emitter.instruction("strb w15, [sp, x9]");                                  // mark the range end character as escaped
    emitter.instruction("ldr w10, [sp, #288]");                                 // load the descending-range warning count
    emitter.instruction("add w10, w10, #1");                                    // count this rejected range
    emitter.instruction("str w10, [sp, #288]");                                 // retain the updated warning count
    emitter.instruction("add x5, x5, #3");                                      // skip the rejected range after marking its literals
    emitter.instruction("mov x6, #-1");                                         // the next range needs a fresh start character
    emitter.instruction("b __rt_addcslashes_mask_loop");                        // continue scanning the character list
    emitter.label("__rt_addcslashes_mask_single");
    emitter.instruction("strb w15, [sp, x7]");                                   // mark this literal character as escaped
    emitter.instruction("mov x6, x7");                                          // remember it as a possible range start
    emitter.instruction("add x5, x5, #1");                                      // advance to the next character-list byte
    emitter.instruction("b __rt_addcslashes_mask_loop");                        // scan the remaining character-list bytes
    emitter.label("__rt_addcslashes_mask_done");
    emitter.instruction("ldr w9, [sp, #288]");                                  // load the descending-range warning count
    emitter.instruction("cbz w9, __rt_addcslashes_no_range_warning");           // keep silent for valid or absent ranges
    emitter.label("__rt_addcslashes_range_warning_loop");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_diag_addcslashes_range");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x2", ADDCSLASHES_RANGE_WARNING.len() as i64);
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_diag_warning");  // report one descending-range warning
    emitter.instruction("ldr w9, [sp, #288]");                                  // reload the warning count across the diagnostic call
    emitter.instruction("sub w9, w9, #1");                                      // consume one reported range
    emitter.instruction("str w9, [sp, #288]");                                  // retain the remaining warning count
    emitter.instruction("cbnz w9, __rt_addcslashes_range_warning_loop");        // report every rejected range
    emitter.label("__rt_addcslashes_no_range_warning");
    emitter.instruction("ldr x2, [sp, #264]");                                  // reload the input length for the worst-case reservation
    emitter.instruction("adds x9, x2, x2");                                     // compute twice the input length and record overflow
    emitter.instruction("b.cs __rt_addcslashes_size_overflow");                 // reject a wrapped destination size
    emitter.instruction("adds x0, x9, x9");                                     // compute the four-bytes-per-byte worst case
    emitter.instruction("b.cs __rt_addcslashes_size_overflow");                 // reject a wrapped destination size
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the escaped result
    emitter.instruction("mov x9, x0");                                          // use the reservation start as the write cursor
    emitter.instruction("mov x10, x0");                                         // remember the result start for the final slice
    emitter.instruction("ldr x1, [sp, #256]");                                  // reload the input pointer
    emitter.instruction("ldr x2, [sp, #264]");                                  // reload the input length
    emitter.instruction("mov x11, x2");                                         // track how many input bytes remain
    emitter.label("__rt_addcslashes_input_loop");
    emitter.instruction("cbz x11, __rt_addcslashes_done");                      // finish when every input byte has been copied
    emitter.instruction("ldrb w12, [x1], #1");                                  // load the next input byte and advance the source cursor
    emitter.instruction("sub x11, x11, #1");                                    // decrement the remaining input byte count
    emitter.instruction("ldrb w13, [sp, x12]");                                 // look up whether this byte belongs to the escape set
    emitter.instruction("cbz w13, __rt_addcslashes_copy_byte");                 // ordinary bytes are copied unchanged
    emitter.instruction("mov w14, #92");                                        // load the backslash escape prefix
    emitter.instruction("strb w14, [x9], #1");                                  // write the escape prefix
    emitter.instruction("cmp w12, #7");                                         // is the byte the bell control character?
    emitter.instruction("b.eq __rt_addcslashes_esc_bell");                      // use PHP's short \a spelling
    emitter.instruction("cmp w12, #8");                                         // is the byte the backspace control character?
    emitter.instruction("b.eq __rt_addcslashes_esc_backspace");                 // use PHP's short \b spelling
    emitter.instruction("cmp w12, #9");                                         // is the byte the horizontal tab?
    emitter.instruction("b.eq __rt_addcslashes_esc_tab");                       // use PHP's short \t spelling
    emitter.instruction("cmp w12, #10");                                        // is the byte a newline?
    emitter.instruction("b.eq __rt_addcslashes_esc_newline");                   // use PHP's short \n spelling
    emitter.instruction("cmp w12, #11");                                        // is the byte a vertical tab?
    emitter.instruction("b.eq __rt_addcslashes_esc_vtab");                      // use PHP's short \v spelling
    emitter.instruction("cmp w12, #12");                                        // is the byte a form feed?
    emitter.instruction("b.eq __rt_addcslashes_esc_formfeed");                  // use PHP's short \f spelling
    emitter.instruction("cmp w12, #13");                                        // is the byte a carriage return?
    emitter.instruction("b.eq __rt_addcslashes_esc_carriage");                  // use PHP's short \r spelling
    emitter.instruction("cmp w12, #32");                                        // is the byte below the printable ASCII range?
    emitter.instruction("b.lo __rt_addcslashes_octal");                         // PHP prints non-printable bytes as octal escapes
    emitter.instruction("cmp w12, #126");                                       // is the byte above the printable ASCII range?
    emitter.instruction("b.hi __rt_addcslashes_octal");                         // PHP prints non-printable bytes as octal escapes
    emitter.instruction("strb w12, [x9], #1");                                  // write the escaped printable byte after its backslash
    emitter.instruction("b __rt_addcslashes_input_loop");                       // continue with the next input byte
    emitter.label("__rt_addcslashes_esc_bell");
    emitter.instruction("mov w12, #97");                                        // select the letter a
    emitter.instruction("b __rt_addcslashes_write_short_escape");               // write the selected short escape letter
    emitter.label("__rt_addcslashes_esc_backspace");
    emitter.instruction("mov w12, #98");                                        // select the letter b
    emitter.instruction("b __rt_addcslashes_write_short_escape");               // write the selected short escape letter
    emitter.label("__rt_addcslashes_esc_tab");
    emitter.instruction("mov w12, #116");                                       // select the letter t
    emitter.instruction("b __rt_addcslashes_write_short_escape");               // write the selected short escape letter
    emitter.label("__rt_addcslashes_esc_newline");
    emitter.instruction("mov w12, #110");                                       // select the letter n
    emitter.instruction("b __rt_addcslashes_write_short_escape");               // write the selected short escape letter
    emitter.label("__rt_addcslashes_esc_vtab");
    emitter.instruction("mov w12, #118");                                       // select the letter v
    emitter.instruction("b __rt_addcslashes_write_short_escape");               // write the selected short escape letter
    emitter.label("__rt_addcslashes_esc_formfeed");
    emitter.instruction("mov w12, #102");                                       // select the letter f
    emitter.instruction("b __rt_addcslashes_write_short_escape");               // write the selected short escape letter
    emitter.label("__rt_addcslashes_esc_carriage");
    emitter.instruction("mov w12, #114");                                       // select the letter r
    emitter.label("__rt_addcslashes_write_short_escape");
    emitter.instruction("strb w12, [x9], #1");                                  // write the short escape letter
    emitter.instruction("b __rt_addcslashes_input_loop");                       // continue with the next input byte
    emitter.label("__rt_addcslashes_octal");
    emitter.instruction("lsr w13, w12, #6");                                    // extract the high octal digit
    emitter.instruction("and w13, w13, #7");                                    // mask the high octal digit to three bits
    emitter.instruction("add w13, w13, #48");                                   // convert the high digit to ASCII
    emitter.instruction("strb w13, [x9], #1");                                  // write the high octal digit
    emitter.instruction("lsr w13, w12, #3");                                    // extract the middle octal digit
    emitter.instruction("and w13, w13, #7");                                    // mask the middle octal digit to three bits
    emitter.instruction("add w13, w13, #48");                                   // convert the middle digit to ASCII
    emitter.instruction("strb w13, [x9], #1");                                  // write the middle octal digit
    emitter.instruction("and w13, w12, #7");                                    // extract the low octal digit
    emitter.instruction("add w13, w13, #48");                                   // convert the low digit to ASCII
    emitter.instruction("strb w13, [x9], #1");                                  // write the low octal digit
    emitter.instruction("b __rt_addcslashes_input_loop");                       // continue with the next input byte
    emitter.label("__rt_addcslashes_copy_byte");
    emitter.instruction("strb w12, [x9], #1");                                  // copy an ordinary byte unchanged
    emitter.instruction("b __rt_addcslashes_input_loop");                       // continue with the next input byte
    emitter.label("__rt_addcslashes_done");
    emitter.instruction("mov x1, x10");                                         // return the escaped result pointer
    emitter.instruction("sub x2, x9, x10");                                     // return the escaped result length
    emitter.instruction("bl __rt_concat_publish");                              // publish the written length for scratch-backed results
    emitter.instruction("ldp x29, x30, [sp, #336]");                            // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #352");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return the escaped string pair
    emitter.label("__rt_addcslashes_size_overflow");
    emitter.instruction("b __rt_alloc_overflow");                               // report the shared allocation-overflow fatal error
}

/// Emits the x86_64 Linux variant. Input: rdi/rsi string, rdx/rcx character list; output: rax/rdx.
fn emit_addcslashes_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: addcslashes ---");
    emitter.label_global("__rt_addcslashes");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame base
    emitter.instruction("sub rsp, 352");                                        // reserve the byte lookup table, argument spills, and alignment
    emitter.instruction("mov QWORD PTR [rsp + 256], rdi");                      // save the input pointer across helper calls
    emitter.instruction("mov QWORD PTR [rsp + 264], rsi");                      // save the input length across helper calls
    emitter.instruction("mov QWORD PTR [rsp + 272], rdx");                      // save the character-list pointer
    emitter.instruction("mov QWORD PTR [rsp + 280], rcx");                      // save the character-list length
    emitter.instruction("xor eax, eax");                                        // use zero to clear the byte lookup table
    emitter.instruction("xor r8d, r8d");                                        // start clearing at table offset zero
    emitter.label("__rt_addcslashes_zero_table");
    emitter.instruction("mov QWORD PTR [rsp + r8], rax");                       // clear one eight-byte table word
    emitter.instruction("add r8, 8");                                           // advance to the next table word
    emitter.instruction("cmp r8, 256");                                         // has the complete byte table been cleared?
    emitter.instruction("jb __rt_addcslashes_zero_table");                      // clear the remaining table words
    emitter.instruction("mov rdx, QWORD PTR [rsp + 272]");                      // reload the character-list pointer
    emitter.instruction("mov rcx, QWORD PTR [rsp + 280]");                      // reload the character-list length
    emitter.instruction("xor r8d, r8d");                                        // start scanning the character list at byte zero
    emitter.instruction("mov r9, -1");                                          // no preceding character exists before the first mask byte
    emitter.instruction("mov DWORD PTR [rsp + 288], 0");                        // initialize the warning counter
    emitter.label("__rt_addcslashes_mask_loop");
    emitter.instruction("cmp r8, rcx");                                         // has every character-list byte been scanned?
    emitter.instruction("jae __rt_addcslashes_mask_done");                      // finish table construction
    emitter.instruction("movzx eax, BYTE PTR [rdx + r8]");                      // load the current character-list byte
    emitter.instruction("cmp al, 46");                                          // is this byte the first dot of a range?
    emitter.instruction("jne __rt_addcslashes_mask_single");                    // a non-dot byte marks one literal character
    emitter.instruction("lea r10, [r8 + 2]");                                   // locate the range end byte after the two dots
    emitter.instruction("cmp r10, rcx");                                        // does the range end byte exist?
    emitter.instruction("jae __rt_addcslashes_mask_single");                    // a trailing dot is a literal character
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r8 + 1]");                 // load the second dot byte
    emitter.instruction("cmp r11b, 46");                                        // is the second byte also a dot?
    emitter.instruction("jne __rt_addcslashes_mask_single");                    // a single dot is a literal character
    emitter.instruction("cmp r9, -1");                                          // is there a preceding range start character?
    emitter.instruction("je __rt_addcslashes_mask_single");                     // a leading range marker has no start character
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r10]");                    // load the range end character
    emitter.instruction("cmp r9d, r11d");                                       // is the range ascending?
    emitter.instruction("ja __rt_addcslashes_skip_range");                      // PHP rejects descending ranges
    emitter.instruction("mov r10d, r9d");                                       // start the inclusive range at its first character
    emitter.label("__rt_addcslashes_range_loop");
    emitter.instruction("mov BYTE PTR [rsp + r10], 1");                         // mark this range character as escaped
    emitter.instruction("cmp r10d, r11d");                                      // has the range reached its final character?
    emitter.instruction("jae __rt_addcslashes_range_done");                     // the inclusive range is complete
    emitter.instruction("add r10d, 1");                                         // advance to the next range character
    emitter.instruction("jmp __rt_addcslashes_range_loop");                     // mark the remaining range characters
    emitter.label("__rt_addcslashes_range_done");
    emitter.instruction("add r8, 3");                                           // skip the start character, both dots, and the end character
    emitter.instruction("mov r9, -1");                                          // the next range needs a fresh start character
    emitter.instruction("jmp __rt_addcslashes_mask_loop");                      // continue scanning the character list
    emitter.label("__rt_addcslashes_skip_range");
    emitter.instruction("lea r10, [r8 + 1]");                                   // locate the first dot of the rejected range
    emitter.instruction("movzx eax, BYTE PTR [rdx + r10]");                     // load the first dot as a literal character
    emitter.instruction("mov BYTE PTR [rsp + rax], 1");                         // PHP still escapes the range syntax bytes
    emitter.instruction("lea r10, [r8 + 2]");                                   // locate the second dot
    emitter.instruction("movzx eax, BYTE PTR [rdx + r10]");                     // load the second dot as a literal character
    emitter.instruction("mov BYTE PTR [rsp + rax], 1");                         // mark the second dot as escaped
    emitter.instruction("mov BYTE PTR [rsp + r11], 1");                         // mark the range end character as escaped
    emitter.instruction("add DWORD PTR [rsp + 288], 1");                        // count this rejected range
    emitter.instruction("add r8, 3");                                           // skip the rejected range after marking its literals
    emitter.instruction("mov r9, -1");                                          // the next range needs a fresh start character
    emitter.instruction("jmp __rt_addcslashes_mask_loop");                      // continue scanning the character list
    emitter.label("__rt_addcslashes_mask_single");
    emitter.instruction("mov BYTE PTR [rsp + rax], 1");                         // mark this literal character as escaped
    emitter.instruction("mov r9, rax");                                         // remember it as a possible range start
    emitter.instruction("add r8, 1");                                           // advance to the next character-list byte
    emitter.instruction("jmp __rt_addcslashes_mask_loop");                      // scan the remaining character-list bytes
    emitter.label("__rt_addcslashes_mask_done");
    emitter.instruction("cmp DWORD PTR [rsp + 288], 0");                        // did the mask contain a descending range?
    emitter.instruction("je __rt_addcslashes_no_range_warning");                // keep silent for valid or absent ranges
    emitter.label("__rt_addcslashes_range_warning_loop");
    crate::codegen_support::abi::emit_symbol_address(emitter, "rdi", "_diag_addcslashes_range");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "rsi", ADDCSLASHES_RANGE_WARNING.len() as i64);
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_diag_warning"); // report one descending-range warning
    emitter.instruction("sub DWORD PTR [rsp + 288], 1");                        // consume one reported range
    emitter.instruction("jne __rt_addcslashes_range_warning_loop");             // report every rejected range
    emitter.label("__rt_addcslashes_no_range_warning");
    emitter.instruction("mov rdx, QWORD PTR [rsp + 264]");                      // reload the input length for the reservation
    emitter.instruction("mov rax, rdx");                                        // start computing the four-bytes-per-byte worst case
    emitter.instruction("add rax, rax");                                        // compute twice the input length
    emitter.instruction("jc __rt_addcslashes_size_overflow");                   // reject a wrapped destination size
    emitter.instruction("add rax, rax");                                        // compute the four-bytes-per-byte worst case
    emitter.instruction("jc __rt_addcslashes_size_overflow");                   // reject a wrapped destination size
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the escaped result
    emitter.instruction("mov r9, rax");                                         // use the reservation start as the write cursor
    emitter.instruction("mov r10, rax");                                        // remember the result start for the final slice
    emitter.instruction("mov r11, QWORD PTR [rsp + 256]");                      // reload the input pointer
    emitter.instruction("mov rcx, QWORD PTR [rsp + 264]");                      // track how many input bytes remain
    emitter.label("__rt_addcslashes_input_loop");
    emitter.instruction("test rcx, rcx");                                       // has every input byte been copied?
    emitter.instruction("jz __rt_addcslashes_done");                            // finish the escaped result
    emitter.instruction("movzx eax, BYTE PTR [r11]");                           // load the next input byte
    emitter.instruction("add r11, 1");                                          // advance the source cursor
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining input byte count
    emitter.instruction("cmp BYTE PTR [rsp + rax], 0");                         // does this byte belong to the escape set?
    emitter.instruction("je __rt_addcslashes_copy_byte");                       // ordinary bytes are copied unchanged
    emitter.instruction("mov BYTE PTR [r9], 92");                               // write the backslash escape prefix
    emitter.instruction("add r9, 1");                                           // advance past the escape prefix
    emitter.instruction("cmp al, 7");                                           // is the byte the bell control character?
    emitter.instruction("je __rt_addcslashes_esc_bell");                        // use PHP's short \a spelling
    emitter.instruction("cmp al, 8");                                           // is the byte the backspace control character?
    emitter.instruction("je __rt_addcslashes_esc_backspace");                   // use PHP's short \b spelling
    emitter.instruction("cmp al, 9");                                           // is the byte the horizontal tab?
    emitter.instruction("je __rt_addcslashes_esc_tab");                         // use PHP's short \t spelling
    emitter.instruction("cmp al, 10");                                          // is the byte a newline?
    emitter.instruction("je __rt_addcslashes_esc_newline");                     // use PHP's short \n spelling
    emitter.instruction("cmp al, 11");                                          // is the byte a vertical tab?
    emitter.instruction("je __rt_addcslashes_esc_vtab");                        // use PHP's short \v spelling
    emitter.instruction("cmp al, 12");                                          // is the byte a form feed?
    emitter.instruction("je __rt_addcslashes_esc_formfeed");                    // use PHP's short \f spelling
    emitter.instruction("cmp al, 13");                                          // is the byte a carriage return?
    emitter.instruction("je __rt_addcslashes_esc_carriage");                    // use PHP's short \r spelling
    emitter.instruction("cmp al, 32");                                          // is the byte below the printable ASCII range?
    emitter.instruction("jb __rt_addcslashes_octal");                           // PHP prints non-printable bytes as octal escapes
    emitter.instruction("cmp al, 126");                                         // is the byte above the printable ASCII range?
    emitter.instruction("ja __rt_addcslashes_octal");                           // PHP prints non-printable bytes as octal escapes
    emitter.instruction("mov BYTE PTR [r9], al");                               // write the escaped printable byte after its backslash
    emitter.instruction("add r9, 1");                                           // advance past the escaped printable byte
    emitter.instruction("jmp __rt_addcslashes_input_loop");                     // continue with the next input byte
    emitter.label("__rt_addcslashes_esc_bell");
    emitter.instruction("mov al, 97");                                          // select the letter a
    emitter.instruction("jmp __rt_addcslashes_write_short_escape");             // write the selected short escape letter
    emitter.label("__rt_addcslashes_esc_backspace");
    emitter.instruction("mov al, 98");                                          // select the letter b
    emitter.instruction("jmp __rt_addcslashes_write_short_escape");             // write the selected short escape letter
    emitter.label("__rt_addcslashes_esc_tab");
    emitter.instruction("mov al, 116");                                         // select the letter t
    emitter.instruction("jmp __rt_addcslashes_write_short_escape");             // write the selected short escape letter
    emitter.label("__rt_addcslashes_esc_newline");
    emitter.instruction("mov al, 110");                                         // select the letter n
    emitter.instruction("jmp __rt_addcslashes_write_short_escape");             // write the selected short escape letter
    emitter.label("__rt_addcslashes_esc_vtab");
    emitter.instruction("mov al, 118");                                         // select the letter v
    emitter.instruction("jmp __rt_addcslashes_write_short_escape");             // write the selected short escape letter
    emitter.label("__rt_addcslashes_esc_formfeed");
    emitter.instruction("mov al, 102");                                         // select the letter f
    emitter.instruction("jmp __rt_addcslashes_write_short_escape");             // write the selected short escape letter
    emitter.label("__rt_addcslashes_esc_carriage");
    emitter.instruction("mov al, 114");                                         // select the letter r
    emitter.label("__rt_addcslashes_write_short_escape");
    emitter.instruction("mov BYTE PTR [r9], al");                               // write the short escape letter
    emitter.instruction("add r9, 1");                                           // advance past the short escape letter
    emitter.instruction("jmp __rt_addcslashes_input_loop");                     // continue with the next input byte
    emitter.label("__rt_addcslashes_octal");
    emitter.instruction("mov r8d, eax");                                        // copy the byte before extracting octal digits
    emitter.instruction("shr r8d, 6");                                          // extract the high octal digit
    emitter.instruction("and r8d, 7");                                          // mask the high octal digit to three bits
    emitter.instruction("add r8d, 48");                                         // convert the high digit to ASCII
    emitter.instruction("mov BYTE PTR [r9], r8b");                              // write the high octal digit
    emitter.instruction("add r9, 1");                                           // advance past the high digit
    emitter.instruction("mov r8d, eax");                                        // reload the byte for the middle octal digit
    emitter.instruction("shr r8d, 3");                                          // extract the middle octal digit
    emitter.instruction("and r8d, 7");                                          // mask the middle octal digit to three bits
    emitter.instruction("add r8d, 48");                                         // convert the middle digit to ASCII
    emitter.instruction("mov BYTE PTR [r9], r8b");                              // write the middle octal digit
    emitter.instruction("add r9, 1");                                           // advance past the middle digit
    emitter.instruction("and eax, 7");                                          // extract the low octal digit
    emitter.instruction("add eax, 48");                                         // convert the low digit to ASCII
    emitter.instruction("mov BYTE PTR [r9], al");                               // write the low octal digit
    emitter.instruction("add r9, 1");                                           // advance past the low digit
    emitter.instruction("jmp __rt_addcslashes_input_loop");                     // continue with the next input byte
    emitter.label("__rt_addcslashes_copy_byte");
    emitter.instruction("mov BYTE PTR [r9], al");                               // copy an ordinary byte unchanged
    emitter.instruction("add r9, 1");                                           // advance past the ordinary byte
    emitter.instruction("jmp __rt_addcslashes_input_loop");                     // continue with the next input byte
    emitter.label("__rt_addcslashes_done");
    emitter.instruction("mov rax, r10");                                        // return the escaped result pointer
    emitter.instruction("mov rdx, r9");                                         // return the escaped result length start
    emitter.instruction("sub rdx, r10");                                        // compute the escaped result length
    emitter.instruction("call __rt_concat_publish");                            // publish the written length for scratch-backed results
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the escaped string pair
    emitter.label("__rt_addcslashes_size_overflow");
    emitter.instruction("jmp __rt_alloc_overflow");                             // report the shared allocation-overflow fatal error
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Emits the helper on every supported target and keeps the architecture-specific entry.
    #[test]
    fn addcslashes_emits_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_addcslashes(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_addcslashes:"), "{name}: {asm}");
            if name == "macos-aarch64" {
                eprintln!("{asm}");
            }
        }
    }
}
