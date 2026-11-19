//! Purpose:
//! Emits the `__rt_addcslashes` runtime helper for PHP's `addcslashes()`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` through the strings module.
//!
//! Key details:
//! - The helper builds a 256-byte escape table from the literal character list with the same
//!   byte-by-byte algorithm as php-src's `php_charmask()`, including its four `Invalid '..'`
//!   diagnostics and the three-dot shape where the range starts at the first dot.
//! - Escaped bytes use PHP's short C spellings for bell/backspace/tab/newline/vertical
//!   tab/form feed/carriage return, octal `\\ooo` for other non-printable bytes, and a
//!   backslash prefix for printable bytes.
//! - The worst-case four-bytes-per-input-byte result is reserved through
//!   `__rt_concat_reserve` before the first store; scratch-backed results are published
//!   through `__rt_concat_publish`. Both supported architectures share the same contract.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// The runtime warning PHP emits when a range marker has no character on its left.
pub(crate) const ADDCSLASHES_LEFT_WARNING: &str =
    "Warning: addcslashes(): Invalid '..'-range, no character to the left of '..'\n";

/// The runtime warning PHP emits when a range marker has no character on its right.
pub(crate) const ADDCSLASHES_RIGHT_WARNING: &str =
    "Warning: addcslashes(): Invalid '..'-range, no character to the right of '..'\n";

/// The runtime warning PHP emits when a range marker descends.
pub(crate) const ADDCSLASHES_INCREMENT_WARNING: &str =
    "Warning: addcslashes(): Invalid '..'-range, '..'-range needs to be incrementing\n";

/// The runtime warning PHP emits for a range marker it cannot classify further.
pub(crate) const ADDCSLASHES_PLAIN_WARNING: &str = "Warning: addcslashes(): Invalid '..'-range\n";

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
    emitter.instruction("str x19, [sp, #320]");                                 // save the callee-saved character-list cursor
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
    emitter.instruction("mov x19, #0");                                         // start scanning the character list at byte zero
    emitter.instruction("mov w15, #1");                                         // table entries are boolean markers, including for NUL
    emitter.label("__rt_addcslashes_mask_loop");
    emitter.instruction("cmp x19, x4");                                         // has every character-list byte been scanned?
    emitter.instruction("b.hs __rt_addcslashes_mask_done");                     // finish table construction
    emitter.instruction("ldrb w7, [x3, x19]");                                  // load the current character-list byte
    emitter.instruction("add x8, x19, #3");                                     // locate the candidate range end byte
    emitter.instruction("cmp x8, x4");                                          // does the four-byte range shape fit?
    emitter.instruction("b.hs __rt_addcslashes_mask_dot_check");                // a short tail cannot spell a range
    emitter.instruction("add x9, x19, #1");                                     // locate the first dot after the range start
    emitter.instruction("ldrb w9, [x3, x9]");                                   // load the byte after the range start
    emitter.instruction("cmp w9, #46");                                         // is the next byte the first dot?
    emitter.instruction("b.ne __rt_addcslashes_mask_dot_check");                // a non-dot byte cannot open a range
    emitter.instruction("add x9, x19, #2");                                     // locate the second dot
    emitter.instruction("ldrb w9, [x3, x9]");                                   // load the byte after the first dot
    emitter.instruction("cmp w9, #46");                                         // is the next byte the second dot?
    emitter.instruction("b.ne __rt_addcslashes_mask_dot_check");                // a single dot is not a range marker
    emitter.instruction("ldrb w9, [x3, x8]");                                   // load the candidate range end character
    emitter.instruction("cmp w9, w7");                                          // is the range ascending or singleton?
    emitter.instruction("b.lo __rt_addcslashes_mask_dot_check");                // PHP rejects a descending range
    emitter.instruction("mov w10, w7");                                         // start the inclusive range at the current byte
    emitter.label("__rt_addcslashes_range_loop");
    emitter.instruction("strb w15, [sp, x10]");                                 // mark this range character as escaped
    emitter.instruction("cmp w10, w9");                                         // has the range reached its final character?
    emitter.instruction("b.hs __rt_addcslashes_range_done");                    // the inclusive range is complete
    emitter.instruction("add w10, w10, #1");                                    // advance to the next range character
    emitter.instruction("b __rt_addcslashes_range_loop");                       // mark the remaining range characters
    emitter.label("__rt_addcslashes_range_done");
    emitter.instruction("add x19, x19, #4");                                    // skip the start, both dots, and the end byte
    emitter.instruction("b __rt_addcslashes_mask_loop");                        // continue scanning the character list
    emitter.label("__rt_addcslashes_mask_dot_check");
    emitter.instruction("cmp w7, #46");                                         // is the current byte a potential range marker?
    emitter.instruction("b.ne __rt_addcslashes_mask_single");                   // a non-dot byte marks one literal character
    emitter.instruction("add x9, x19, #1");                                     // locate the byte after the first dot
    emitter.instruction("cmp x9, x4");                                          // does a byte follow the first dot?
    emitter.instruction("b.hs __rt_addcslashes_mask_single");                   // a trailing dot is a literal character
    emitter.instruction("ldrb w9, [x3, x9]");                                   // load the byte after the first dot
    emitter.instruction("cmp w9, #46");                                         // is the second byte also a dot?
    emitter.instruction("b.ne __rt_addcslashes_mask_single");                   // a single dot is a literal character
    emitter.instruction("cbnz x19, __rt_addcslashes_dot_has_left");             // a range marker needs a character on its left
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_diag_addcslashes_range_left");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x2", ADDCSLASHES_LEFT_WARNING.len() as i64);
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_diag_warning"); // report a missing left range character
    emitter.instruction("ldr x3, [sp, #272]");                                  // reload the character-list pointer after the call
    emitter.instruction("ldr x4, [sp, #280]");                                  // reload the character-list length after the call
    emitter.instruction("add x19, x19, #1");                                    // skip the rejected marker's first dot
    emitter.instruction("b __rt_addcslashes_mask_loop");                        // continue scanning the character list
    emitter.label("__rt_addcslashes_dot_has_left");
    emitter.instruction("add x9, x19, #2");                                     // locate the byte after the second dot
    emitter.instruction("cmp x9, x4");                                          // does a range end byte follow the marker?
    emitter.instruction("b.lo __rt_addcslashes_dot_has_right");                 // the marker has both side characters
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_diag_addcslashes_range_right");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x2", ADDCSLASHES_RIGHT_WARNING.len() as i64);
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_diag_warning"); // report a missing right range character
    emitter.instruction("ldr x3, [sp, #272]");                                  // reload the character-list pointer after the call
    emitter.instruction("ldr x4, [sp, #280]");                                  // reload the character-list length after the call
    emitter.instruction("add x19, x19, #1");                                    // skip the rejected marker's first dot
    emitter.instruction("b __rt_addcslashes_mask_loop");                        // continue scanning the character list
    emitter.label("__rt_addcslashes_dot_has_right");
    emitter.instruction("ldrb w10, [x3, x9]");                                  // load the byte after the range marker
    emitter.instruction("sub x9, x19, #1");                                     // locate the character before the marker
    emitter.instruction("ldrb w11, [x3, x9]");                                  // load the character before the marker
    emitter.instruction("cmp w11, w10");                                        // does the rejected marker descend?
    emitter.instruction("b.ls __rt_addcslashes_dot_plain");                     // equal or ascending still is not a range
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_diag_addcslashes_range_increment");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x2", ADDCSLASHES_INCREMENT_WARNING.len() as i64);
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_diag_warning"); // report a descending range marker
    emitter.instruction("ldr x3, [sp, #272]");                                  // reload the character-list pointer after the call
    emitter.instruction("ldr x4, [sp, #280]");                                  // reload the character-list length after the call
    emitter.instruction("add x19, x19, #1");                                    // skip the rejected marker's first dot
    emitter.instruction("b __rt_addcslashes_mask_loop");                        // continue scanning the character list
    emitter.label("__rt_addcslashes_dot_plain");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_diag_addcslashes_range_plain");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x2", ADDCSLASHES_PLAIN_WARNING.len() as i64);
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_diag_warning"); // report an unclassifiable range marker
    emitter.instruction("ldr x3, [sp, #272]");                                  // reload the character-list pointer after the call
    emitter.instruction("ldr x4, [sp, #280]");                                  // reload the character-list length after the call
    emitter.instruction("add x19, x19, #1");                                    // skip the rejected marker's first dot
    emitter.instruction("b __rt_addcslashes_mask_loop");                        // continue scanning the character list
    emitter.label("__rt_addcslashes_mask_single");
    emitter.instruction("strb w15, [sp, x7]");                                  // mark this literal character as escaped
    emitter.instruction("add x19, x19, #1");                                    // advance to the next character-list byte
    emitter.instruction("b __rt_addcslashes_mask_loop");                        // scan the remaining character-list bytes
    emitter.label("__rt_addcslashes_mask_done");
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
    emitter.instruction("ldr x19, [sp, #320]");                                 // restore the callee-saved character-list cursor
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
    emitter.instruction("push r12");                                            // preserve the callee-saved character-list cursor
    emitter.instruction("push r13");                                            // preserve the callee-saved character-list pointer
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
    emitter.instruction("mov r13, QWORD PTR [rsp + 272]");                      // reload the character-list pointer
    emitter.instruction("mov rcx, QWORD PTR [rsp + 280]");                      // reload the character-list length
    emitter.instruction("xor r12d, r12d");                                      // start scanning the character list at byte zero
    emitter.label("__rt_addcslashes_mask_loop");
    emitter.instruction("cmp r12, rcx");                                        // has every character-list byte been scanned?
    emitter.instruction("jae __rt_addcslashes_mask_done");                      // finish table construction
    emitter.instruction("movzx eax, BYTE PTR [r13 + r12]");                     // load the current character-list byte
    emitter.instruction("lea r10, [r12 + 3]");                                  // locate the candidate range end byte
    emitter.instruction("cmp r10, rcx");                                        // does the four-byte range shape fit?
    emitter.instruction("jae __rt_addcslashes_mask_dot_check");                 // a short tail cannot spell a range
    emitter.instruction("movzx r11d, BYTE PTR [r13 + r12 + 1]");                // load the byte after the range start
    emitter.instruction("cmp r11b, 46");                                        // is the next byte the first dot?
    emitter.instruction("jne __rt_addcslashes_mask_dot_check");                 // a non-dot byte cannot open a range
    emitter.instruction("movzx r11d, BYTE PTR [r13 + r12 + 2]");                // load the byte after the first dot
    emitter.instruction("cmp r11b, 46");                                        // is the next byte the second dot?
    emitter.instruction("jne __rt_addcslashes_mask_dot_check");                 // a single dot is not a range marker
    emitter.instruction("movzx r11d, BYTE PTR [r13 + r10]");                    // load the candidate range end character
    emitter.instruction("cmp r11d, eax");                                       // is the range ascending or singleton?
    emitter.instruction("jb __rt_addcslashes_mask_dot_check");                  // PHP rejects a descending range
    emitter.instruction("mov r10d, eax");                                       // start the inclusive range at the current byte
    emitter.label("__rt_addcslashes_range_loop");
    emitter.instruction("mov BYTE PTR [rsp + r10], 1");                         // mark this range character as escaped
    emitter.instruction("cmp r10d, r11d");                                      // has the range reached its final character?
    emitter.instruction("jae __rt_addcslashes_range_done");                     // the inclusive range is complete
    emitter.instruction("add r10d, 1");                                         // advance to the next range character
    emitter.instruction("jmp __rt_addcslashes_range_loop");                     // mark the remaining range characters
    emitter.label("__rt_addcslashes_range_done");
    emitter.instruction("add r12, 4");                                          // skip the start, both dots, and the end byte
    emitter.instruction("jmp __rt_addcslashes_mask_loop");                      // continue scanning the character list
    emitter.label("__rt_addcslashes_mask_dot_check");
    emitter.instruction("cmp al, 46");                                          // is the current byte a potential range marker?
    emitter.instruction("jne __rt_addcslashes_mask_single");                    // a non-dot byte marks one literal character
    emitter.instruction("lea r10, [r12 + 1]");                                  // locate the byte after the first dot
    emitter.instruction("cmp r10, rcx");                                        // does a byte follow the first dot?
    emitter.instruction("jae __rt_addcslashes_mask_single");                    // a trailing dot is a literal character
    emitter.instruction("cmp BYTE PTR [r13 + r10], 46");                        // is the second byte also a dot?
    emitter.instruction("jne __rt_addcslashes_mask_single");                    // a single dot is a literal character
    emitter.instruction("test r12, r12");                                       // does the range marker have a character on its left?
    emitter.instruction("jz __rt_addcslashes_dot_no_left");                     // the marker starts the character list
    emitter.instruction("lea r10, [r12 + 2]");                                  // locate the byte after the second dot
    emitter.instruction("cmp r10, rcx");                                        // does a range end byte follow the marker?
    emitter.instruction("jae __rt_addcslashes_dot_no_right");                   // the marker ends the character list
    emitter.instruction("movzx r11d, BYTE PTR [r13 + r12 + 2]");                // load the byte after the range marker
    emitter.instruction("movzx r10d, BYTE PTR [r13 + r12 - 1]");                // load the character before the marker
    emitter.instruction("cmp r10d, r11d");                                      // does the rejected marker descend?
    emitter.instruction("jbe __rt_addcslashes_dot_plain");                      // equal or ascending still is not a range
    crate::codegen_support::abi::emit_symbol_address(emitter, "rdi", "_diag_addcslashes_range_increment");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "rsi", ADDCSLASHES_INCREMENT_WARNING.len() as i64);
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_diag_warning"); // report a descending range marker
    emitter.instruction("jmp __rt_addcslashes_after_warning");                  // resume scanning after the diagnostic
    emitter.label("__rt_addcslashes_dot_no_left");
    crate::codegen_support::abi::emit_symbol_address(emitter, "rdi", "_diag_addcslashes_range_left");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "rsi", ADDCSLASHES_LEFT_WARNING.len() as i64);
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_diag_warning"); // report a missing left range character
    emitter.instruction("jmp __rt_addcslashes_after_warning");                  // resume scanning after the diagnostic
    emitter.label("__rt_addcslashes_dot_no_right");
    crate::codegen_support::abi::emit_symbol_address(emitter, "rdi", "_diag_addcslashes_range_right");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "rsi", ADDCSLASHES_RIGHT_WARNING.len() as i64);
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_diag_warning"); // report a missing right range character
    emitter.instruction("jmp __rt_addcslashes_after_warning");                  // resume scanning after the diagnostic
    emitter.label("__rt_addcslashes_dot_plain");
    crate::codegen_support::abi::emit_symbol_address(emitter, "rdi", "_diag_addcslashes_range_plain");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "rsi", ADDCSLASHES_PLAIN_WARNING.len() as i64);
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_diag_warning"); // report an unclassifiable range marker
    emitter.label("__rt_addcslashes_after_warning");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 280]");                      // reload the character-list length after the call
    emitter.instruction("add r12, 1");                                          // skip the rejected marker's first dot
    emitter.instruction("jmp __rt_addcslashes_mask_loop");                      // continue scanning the character list
    emitter.label("__rt_addcslashes_mask_single");
    emitter.instruction("mov BYTE PTR [rsp + rax], 1");                         // mark this literal character as escaped
    emitter.instruction("add r12, 1");                                          // advance to the next character-list byte
    emitter.instruction("jmp __rt_addcslashes_mask_loop");                      // scan the remaining character-list bytes
    emitter.label("__rt_addcslashes_mask_done");
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
    emitter.instruction("add rsp, 352");                                        // release the byte lookup table and argument spills
    emitter.instruction("pop r13");                                             // restore the callee-saved character-list pointer
    emitter.instruction("pop r12");                                             // restore the callee-saved character-list cursor
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
