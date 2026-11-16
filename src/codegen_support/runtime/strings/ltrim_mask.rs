//! Purpose:
//! Emits the `__rt_ltrim_mask` runtime helper assembly.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Builds a 256-byte membership set from PHP's charlist using `php_charmask`'s
//!   rules: only an increasing `start..end` range is expanded, and an invalid
//!   range leaves its bytes literal instead of consuming the range.
//! - Invalid ranges call `__rt_trim_warning` with the caller-visible function
//!   name; the trim wrapper enables the warning on its first scan only.
//! - Trim helpers scan byte ranges without copying unless the returned pointer/length slice changes.

use crate::codegen_support::abi;
use super::trim_warnings;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_ltrim_mask` runtime helper for the current AArch64 target.
///
/// Input: x1=str_ptr, x2=str_len, x3=mask_ptr, x4=mask_len, x5=name_ptr, x6=name_len, x7=warn; output: x1=adjusted_ptr, x2=adjusted_len.
pub fn emit_ltrim_mask(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ltrim_mask_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ltrim_mask ---");
    emitter.label_global("__rt_ltrim_mask");
    // -- build a 256-byte membership set from the PHP charlist --
    emitter.instruction("sub sp, sp, #352");                                    // reserve the membership set and the warning-call spill slots
    emitter.instruction("str x5, [sp, #256]");                                  // save the PHP-visible function name pointer
    emitter.instruction("str x6, [sp, #264]");                                  // save the PHP-visible function name length
    emitter.instruction("str x7, [sp, #272]");                                  // save whether this scan emits invalid-range warnings
    emitter.instruction("mov x8, sp");                                          // set base for zeroing and later membership tests
    emitter.instruction("mov x9, #32");                                         // clear 32 machine words (256 bytes)
    emitter.label("__rt_ltrim_mask_zero");
    emitter.instruction("str xzr, [x8], #8");                                   // clear one eight-byte word of the membership set
    emitter.instruction("subs x9, x9, #1");                                     // decrement the remaining word count
    emitter.instruction("b.ne __rt_ltrim_mask_zero");                           // repeat until the whole set is cleared
    emitter.instruction("mov x8, sp");                                          // restore the membership set base after zeroing
    emitter.instruction("mov x9, #0");                                          // start at the first charlist byte
    emitter.label("__rt_ltrim_mask_expand");
    emitter.instruction("cmp x9, x4");                                          // have we consumed the whole charlist?
    emitter.instruction("b.hs __rt_ltrim_mask_scan");                           // scan the string once the charlist is fully expanded
    emitter.instruction("add x12, x3, x9");                                     // point at the current charlist byte
    emitter.instruction("ldrb w10, [x12]");                                     // load the current charlist byte
    emitter.instruction("add x11, x9, #3");                                     // index just past a possible start..end range
    emitter.instruction("cmp x11, x4");                                         // is a complete start..end range still present?
    emitter.instruction("b.hs __rt_ltrim_mask_dot_pair");                       // an incomplete range cannot consume four bytes
    emitter.instruction("ldrb w11, [x12, #1]");                                 // load the first range marker
    emitter.instruction("cmp w11, #46");                                        // is the first range marker a dot?
    emitter.instruction("b.ne __rt_ltrim_mask_literal");                        // not a range: mark the current byte literally
    emitter.instruction("ldrb w11, [x12, #2]");                                 // load the second range marker
    emitter.instruction("cmp w11, #46");                                        // is the second range marker a dot?
    emitter.instruction("b.ne __rt_ltrim_mask_literal");                        // not a range: mark the current byte literally
    emitter.instruction("ldrb w14, [x12, #3]");                                 // load the range end byte
    emitter.instruction("cmp w14, w10");                                        // is the range incrementing?
    emitter.instruction("b.lo __rt_ltrim_mask_decreasing");                     // a decreasing range leaves its bytes literal
    emitter.label("__rt_ltrim_mask_range");
    emitter.instruction("mov w15, #1");                                         // membership marker for one charlist byte
    emitter.instruction("strb w15, [x8, x10]");                                 // mark the current range byte as trimmable
    emitter.instruction("cmp w10, w14");                                        // have we reached the range end?
    emitter.instruction("b.eq __rt_ltrim_mask_range_done");                     // finish this inclusive range
    emitter.instruction("add w10, w10, #1");                                    // advance to the next byte in the range
    emitter.instruction("b __rt_ltrim_mask_range");                             // continue marking the inclusive range
    emitter.label("__rt_ltrim_mask_range_done");
    emitter.instruction("add x9, x9, #4");                                      // consume the valid start..end range
    emitter.instruction("b __rt_ltrim_mask_expand");                            // parse the next charlist byte
    emitter.label("__rt_ltrim_mask_decreasing");
    emitter.instruction("mov w15, #1");                                         // membership marker for the decreasing-range start
    emitter.instruction("strb w15, [x8, x10]");                                 // mark only the start byte, as php_charmask does
    emitter.instruction("add x9, x9, #1");                                      // let the dots and end byte be reprocessed literally
    emitter.instruction("b __rt_ltrim_mask_expand");                            // parse the next charlist byte
    emitter.label("__rt_ltrim_mask_dot_pair");
    emitter.instruction("cmp w10, #46");                                        // is the current byte a dot?
    emitter.instruction("b.ne __rt_ltrim_mask_literal");                        // a non-dot byte is a literal
    emitter.instruction("add x11, x9, #1");                                     // index of the byte after the current dot
    emitter.instruction("cmp x11, x4");                                         // is there a byte after the current dot?
    emitter.instruction("b.hs __rt_ltrim_mask_literal");                        // a trailing dot is a literal
    emitter.instruction("ldrb w11, [x12, #1]");                                 // load the byte after the current dot
    emitter.instruction("cmp w11, #46");                                        // does the current dot start an invalid .. pair?
    emitter.instruction("b.ne __rt_ltrim_mask_literal");                        // a lone dot is a literal
    emitter.instruction("ldr x15, [sp, #272]");                                 // load the warning-enable flag for this scan
    emitter.instruction("cbz x15, __rt_ltrim_mask_dot_advance");                // silent scans skip the invalid-range diagnostic
    emitter.instruction("cbz x9, __rt_ltrim_mask_warn_left");                   // a pair at the first byte has no left character
    emitter.instruction("add x11, x9, #2");                                     // index of the byte after the invalid pair
    emitter.instruction("cmp x11, x4");                                         // is there a byte to the right of the pair?
    emitter.instruction("b.hs __rt_ltrim_mask_warn_right");                     // an end-of-mask pair has no right character
    emitter.instruction("ldrb w15, [x12, #-1]");                                // load the character to the left of the pair
    emitter.instruction("ldrb w14, [x12, #2]");                                 // load the character to the right of the pair
    emitter.instruction("cmp w15, w14");                                        // is the range in incrementing order?
    emitter.instruction("b.hi __rt_ltrim_mask_warn_order");                     // a decreasing pair reports the incrementing warning
    emitter.instruction("b __rt_ltrim_mask_warn_generic");                      // every other malformed pair reports the generic warning
    emitter.label("__rt_ltrim_mask_warn_left");
    emitter.instruction("str x30, [sp, #280]");                                 // preserve the return address across the warning call
    emitter.instruction("str x1, [sp, #288]");                                  // preserve the source pointer across the warning call
    emitter.instruction("str x2, [sp, #296]");                                  // preserve the source length across the warning call
    emitter.instruction("str x3, [sp, #304]");                                  // preserve the mask pointer across the warning call
    emitter.instruction("str x4, [sp, #312]");                                  // preserve the mask length across the warning call
    emitter.instruction("str x8, [sp, #320]");                                  // preserve the membership set base across the warning call
    emitter.instruction("str x9, [sp, #328]");                                  // preserve the mask index across the warning call
    emitter.instruction("ldr x0, [sp, #256]");                                  // load the PHP-visible function name pointer
    emitter.instruction("ldr x1, [sp, #264]");                                  // load the PHP-visible function name length
    abi::emit_symbol_address(emitter, "x2", trim_warnings::RANGE_LEFT_SYMBOL);
    emitter.instruction(&format!("mov x3, #{}", trim_warnings::RANGE_LEFT_LEN)); // pass the invalid-range message length
    emitter.instruction("bl __rt_trim_warning");                                // emit the PHP invalid-range warning
    emitter.instruction("ldr x30, [sp, #280]");                                 // restore the return address after the warning
    emitter.instruction("ldr x1, [sp, #288]");                                  // restore the source pointer after the warning
    emitter.instruction("ldr x2, [sp, #296]");                                  // restore the source length after the warning
    emitter.instruction("ldr x3, [sp, #304]");                                  // restore the mask pointer after the warning
    emitter.instruction("ldr x4, [sp, #312]");                                  // restore the mask length after the warning
    emitter.instruction("ldr x8, [sp, #320]");                                  // restore the membership set base after the warning
    emitter.instruction("ldr x9, [sp, #328]");                                  // restore the mask index after the warning
    emitter.instruction("add x9, x9, #1");                                      // advance past the invalid pair
    emitter.instruction("b __rt_ltrim_mask_expand");                            // parse the next charlist byte
    emitter.label("__rt_ltrim_mask_warn_right");
    emitter.instruction("str x30, [sp, #280]");                                 // preserve the return address across the warning call
    emitter.instruction("str x1, [sp, #288]");                                  // preserve the source pointer across the warning call
    emitter.instruction("str x2, [sp, #296]");                                  // preserve the source length across the warning call
    emitter.instruction("str x3, [sp, #304]");                                  // preserve the mask pointer across the warning call
    emitter.instruction("str x4, [sp, #312]");                                  // preserve the mask length across the warning call
    emitter.instruction("str x8, [sp, #320]");                                  // preserve the membership set base across the warning call
    emitter.instruction("str x9, [sp, #328]");                                  // preserve the mask index across the warning call
    emitter.instruction("ldr x0, [sp, #256]");                                  // load the PHP-visible function name pointer
    emitter.instruction("ldr x1, [sp, #264]");                                  // load the PHP-visible function name length
    abi::emit_symbol_address(emitter, "x2", trim_warnings::RANGE_RIGHT_SYMBOL);
    emitter.instruction(&format!("mov x3, #{}", trim_warnings::RANGE_RIGHT_LEN)); // pass the invalid-range message length
    emitter.instruction("bl __rt_trim_warning");                                // emit the PHP invalid-range warning
    emitter.instruction("ldr x30, [sp, #280]");                                 // restore the return address after the warning
    emitter.instruction("ldr x1, [sp, #288]");                                  // restore the source pointer after the warning
    emitter.instruction("ldr x2, [sp, #296]");                                  // restore the source length after the warning
    emitter.instruction("ldr x3, [sp, #304]");                                  // restore the mask pointer after the warning
    emitter.instruction("ldr x4, [sp, #312]");                                  // restore the mask length after the warning
    emitter.instruction("ldr x8, [sp, #320]");                                  // restore the membership set base after the warning
    emitter.instruction("ldr x9, [sp, #328]");                                  // restore the mask index after the warning
    emitter.instruction("add x9, x9, #1");                                      // advance past the invalid pair
    emitter.instruction("b __rt_ltrim_mask_expand");                            // parse the next charlist byte
    emitter.label("__rt_ltrim_mask_warn_order");
    emitter.instruction("str x30, [sp, #280]");                                 // preserve the return address across the warning call
    emitter.instruction("str x1, [sp, #288]");                                  // preserve the source pointer across the warning call
    emitter.instruction("str x2, [sp, #296]");                                  // preserve the source length across the warning call
    emitter.instruction("str x3, [sp, #304]");                                  // preserve the mask pointer across the warning call
    emitter.instruction("str x4, [sp, #312]");                                  // preserve the mask length across the warning call
    emitter.instruction("str x8, [sp, #320]");                                  // preserve the membership set base across the warning call
    emitter.instruction("str x9, [sp, #328]");                                  // preserve the mask index across the warning call
    emitter.instruction("ldr x0, [sp, #256]");                                  // load the PHP-visible function name pointer
    emitter.instruction("ldr x1, [sp, #264]");                                  // load the PHP-visible function name length
    abi::emit_symbol_address(emitter, "x2", trim_warnings::RANGE_ORDER_SYMBOL);
    emitter.instruction(&format!("mov x3, #{}", trim_warnings::RANGE_ORDER_LEN)); // pass the invalid-range message length
    emitter.instruction("bl __rt_trim_warning");                                // emit the PHP invalid-range warning
    emitter.instruction("ldr x30, [sp, #280]");                                 // restore the return address after the warning
    emitter.instruction("ldr x1, [sp, #288]");                                  // restore the source pointer after the warning
    emitter.instruction("ldr x2, [sp, #296]");                                  // restore the source length after the warning
    emitter.instruction("ldr x3, [sp, #304]");                                  // restore the mask pointer after the warning
    emitter.instruction("ldr x4, [sp, #312]");                                  // restore the mask length after the warning
    emitter.instruction("ldr x8, [sp, #320]");                                  // restore the membership set base after the warning
    emitter.instruction("ldr x9, [sp, #328]");                                  // restore the mask index after the warning
    emitter.instruction("add x9, x9, #1");                                      // advance past the invalid pair
    emitter.instruction("b __rt_ltrim_mask_expand");                            // parse the next charlist byte
    emitter.label("__rt_ltrim_mask_warn_generic");
    emitter.instruction("str x30, [sp, #280]");                                 // preserve the return address across the warning call
    emitter.instruction("str x1, [sp, #288]");                                  // preserve the source pointer across the warning call
    emitter.instruction("str x2, [sp, #296]");                                  // preserve the source length across the warning call
    emitter.instruction("str x3, [sp, #304]");                                  // preserve the mask pointer across the warning call
    emitter.instruction("str x4, [sp, #312]");                                  // preserve the mask length across the warning call
    emitter.instruction("str x8, [sp, #320]");                                  // preserve the membership set base across the warning call
    emitter.instruction("str x9, [sp, #328]");                                  // preserve the mask index across the warning call
    emitter.instruction("ldr x0, [sp, #256]");                                  // load the PHP-visible function name pointer
    emitter.instruction("ldr x1, [sp, #264]");                                  // load the PHP-visible function name length
    abi::emit_symbol_address(emitter, "x2", trim_warnings::RANGE_GENERIC_SYMBOL);
    emitter.instruction(&format!("mov x3, #{}", trim_warnings::RANGE_GENERIC_LEN)); // pass the invalid-range message length
    emitter.instruction("bl __rt_trim_warning");                                // emit the PHP invalid-range warning
    emitter.instruction("ldr x30, [sp, #280]");                                 // restore the return address after the warning
    emitter.instruction("ldr x1, [sp, #288]");                                  // restore the source pointer after the warning
    emitter.instruction("ldr x2, [sp, #296]");                                  // restore the source length after the warning
    emitter.instruction("ldr x3, [sp, #304]");                                  // restore the mask pointer after the warning
    emitter.instruction("ldr x4, [sp, #312]");                                  // restore the mask length after the warning
    emitter.instruction("ldr x8, [sp, #320]");                                  // restore the membership set base after the warning
    emitter.instruction("ldr x9, [sp, #328]");                                  // restore the mask index after the warning
    emitter.instruction("add x9, x9, #1");                                      // advance past the invalid pair
    emitter.instruction("b __rt_ltrim_mask_expand");                            // parse the next charlist byte
    emitter.label("__rt_ltrim_mask_dot_advance");
    emitter.instruction("add x9, x9, #1");                                      // skip the first dot of the invalid pair without marking it
    emitter.instruction("b __rt_ltrim_mask_expand");                            // let the second dot be reprocessed literally
    emitter.label("__rt_ltrim_mask_literal");
    emitter.instruction("mov w15, #1");                                         // membership marker for one literal byte
    emitter.instruction("strb w15, [x8, x10]");                                 // mark the literal byte as trimmable
    emitter.instruction("add x9, x9, #1");                                      // consume one literal charlist byte
    emitter.instruction("b __rt_ltrim_mask_expand");                            // parse the next charlist byte
    emitter.label("__rt_ltrim_mask_scan");
    emitter.instruction("cbz x2, __rt_ltrim_mask_done");                        // stop once the string slice is empty
    emitter.instruction("ldrb w10, [x1]");                                      // load the current leading source byte
    emitter.instruction("ldrb w11, [x8, x10]");                                 // test the byte against the expanded charlist set
    emitter.instruction("cbz w11, __rt_ltrim_mask_done");                       // stop at the first byte outside the mask
    emitter.instruction("add x1, x1, #1");                                      // advance past the trimmed leading byte
    emitter.instruction("sub x2, x2, #1");                                      // shrink the remaining source length
    emitter.instruction("b __rt_ltrim_mask_scan");                              // continue trimming from the new front
    emitter.label("__rt_ltrim_mask_done");
    emitter.instruction("add sp, sp, #352");                                    // release the membership set and warning spill slots
    emitter.instruction("ret");                                                 // return the adjusted borrowed source slice
}

/// Emits the `__rt_ltrim_mask` runtime helper for x86_64 Linux.
///
/// Input: rax=str_ptr, rdx=str_len, rdi=mask_ptr, rsi=mask_len, r8=name_ptr, r9=name_len, r10=warn; output: rax=adjusted_ptr, rdx=adjusted_len.
fn emit_ltrim_mask_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ltrim_mask ---");
    emitter.label_global("__rt_ltrim_mask");
    // -- build a 256-byte membership set from the PHP charlist --
    emitter.instruction("sub rsp, 344");                                        // reserve the membership set and warning spill slots with call alignment
    emitter.instruction("mov QWORD PTR [rsp + 320], r8");                       // save the PHP-visible function name pointer
    emitter.instruction("mov QWORD PTR [rsp + 328], r9");                       // save the PHP-visible function name length
    emitter.instruction("mov QWORD PTR [rsp + 336], r10");                      // save whether this scan emits invalid-range warnings
    emitter.instruction("mov r10, rax");                                        // preserve the borrowed source pointer while clearing the set
    emitter.instruction("mov r11, rdx");                                        // preserve the borrowed source length while clearing the set
    emitter.instruction("xor eax, eax");                                        // zero the membership-set bytes
    emitter.instruction("mov rcx, 32");                                         // clear 32 machine words (256 bytes)
    emitter.instruction("mov r8, rsp");                                         // set cursor for the membership-set clearing loop
    emitter.label("__rt_ltrim_mask_zero_linux_x86_64");
    emitter.instruction("mov QWORD PTR [r8], rax");                             // clear one eight-byte word of the membership set
    emitter.instruction("add r8, 8");                                           // advance the clearing cursor
    emitter.instruction("dec rcx");                                             // decrement the remaining word count
    emitter.instruction("jnz __rt_ltrim_mask_zero_linux_x86_64");               // repeat until the whole set is cleared
    emitter.instruction("mov rax, r10");                                        // restore the borrowed source pointer
    emitter.instruction("mov rdx, r11");                                        // restore the borrowed source length
    emitter.instruction("xor r8, r8");                                          // start at the first charlist byte
    emitter.instruction("mov r9, rsp");                                         // set base for membership tests
    emitter.label("__rt_ltrim_mask_expand_linux_x86_64");
    emitter.instruction("cmp r8, rsi");                                         // have we consumed the whole charlist?
    emitter.instruction("jae __rt_ltrim_mask_scan_linux_x86_64");               // scan the string once the charlist is fully expanded
    emitter.instruction("movzx r10d, BYTE PTR [rdi + r8]");                     // load the current charlist byte
    emitter.instruction("mov rcx, r8");                                         // copy the current charlist index
    emitter.instruction("add rcx, 3");                                          // index just past a possible start..end range
    emitter.instruction("cmp rcx, rsi");                                        // is a complete start..end range still present?
    emitter.instruction("jae __rt_ltrim_mask_dot_pair_linux_x86_64");           // an incomplete range cannot consume four bytes
    emitter.instruction("cmp BYTE PTR [rdi + r8 + 1], 46");                     // is the first range marker a dot?
    emitter.instruction("jne __rt_ltrim_mask_literal_linux_x86_64");            // not a range: mark the current byte literally
    emitter.instruction("cmp BYTE PTR [rdi + r8 + 2], 46");                     // is the second range marker a dot?
    emitter.instruction("jne __rt_ltrim_mask_literal_linux_x86_64");            // not a range: mark the current byte literally
    emitter.instruction("movzx r11d, BYTE PTR [rdi + r8 + 3]");                 // load the range end byte
    emitter.instruction("cmp r10b, r11b");                                      // is the range incrementing?
    emitter.instruction("jbe __rt_ltrim_mask_range_linux_x86_64");              // an increasing range marks every byte inclusively
    emitter.instruction("mov BYTE PTR [r9 + r10], 1");                          // mark only the decreasing-range start, as php_charmask does
    emitter.instruction("inc r8");                                              // let the dots and end byte be reprocessed literally
    emitter.instruction("jmp __rt_ltrim_mask_expand_linux_x86_64");             // parse the next charlist byte
    emitter.label("__rt_ltrim_mask_range_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9 + r10], 1");                          // mark the current range byte as trimmable
    emitter.instruction("cmp r10b, r11b");                                      // have we reached the range end?
    emitter.instruction("je __rt_ltrim_mask_range_done_linux_x86_64");          // finish this inclusive range
    emitter.instruction("inc r10");                                             // advance to the next byte in the range
    emitter.instruction("jmp __rt_ltrim_mask_range_linux_x86_64");              // continue marking the inclusive range
    emitter.label("__rt_ltrim_mask_range_done_linux_x86_64");
    emitter.instruction("add r8, 4");                                           // consume the valid start..end range
    emitter.instruction("jmp __rt_ltrim_mask_expand_linux_x86_64");             // parse the next charlist byte
    emitter.label("__rt_ltrim_mask_dot_pair_linux_x86_64");
    emitter.instruction("cmp r10b, 46");                                        // is the current byte a dot?
    emitter.instruction("jne __rt_ltrim_mask_literal_linux_x86_64");            // a non-dot byte is a literal
    emitter.instruction("mov rcx, r8");                                         // copy the current charlist index
    emitter.instruction("inc rcx");                                             // index of the byte after the current dot
    emitter.instruction("cmp rcx, rsi");                                        // is there a byte after the current dot?
    emitter.instruction("jae __rt_ltrim_mask_literal_linux_x86_64");            // a trailing dot is a literal
    emitter.instruction("cmp BYTE PTR [rdi + r8 + 1], 46");                     // does the current dot start an invalid .. pair?
    emitter.instruction("jne __rt_ltrim_mask_literal_linux_x86_64");            // a lone dot is a literal
    emitter.instruction("cmp QWORD PTR [rsp + 336], 0");                        // does this scan emit invalid-range warnings?
    emitter.instruction("je __rt_ltrim_mask_dot_advance_linux_x86_64");         // silent scans skip the invalid-range diagnostic
    emitter.instruction("test r8, r8");                                         // is the invalid pair at the first charlist byte?
    emitter.instruction("jz __rt_ltrim_mask_warn_left_linux_x86_64");           // a pair at the first byte has no left character
    emitter.instruction("mov rcx, r8");                                         // copy the current charlist index
    emitter.instruction("add rcx, 2");                                          // index of the byte after the invalid pair
    emitter.instruction("cmp rcx, rsi");                                        // is there a byte to the right of the pair?
    emitter.instruction("jae __rt_ltrim_mask_warn_right_linux_x86_64");         // an end-of-mask pair has no right character
    emitter.instruction("movzx r10d, BYTE PTR [rdi + r8 - 1]");                 // load the character to the left of the pair
    emitter.instruction("movzx r11d, BYTE PTR [rdi + r8 + 2]");                 // load the character to the right of the pair
    emitter.instruction("cmp r10b, r11b");                                      // is the range in incrementing order?
    emitter.instruction("ja __rt_ltrim_mask_warn_order_linux_x86_64");          // a decreasing pair reports the incrementing warning
    emitter.instruction("jmp __rt_ltrim_mask_warn_generic_linux_x86_64");       // every other malformed pair reports the generic warning
    emitter.label("__rt_ltrim_mask_warn_left_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rsp + 256], rax");                      // preserve the source pointer across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 264], rdx");                      // preserve the source length across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 272], rdi");                      // preserve the mask pointer across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 280], rsi");                      // preserve the mask length across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 288], r8");                       // preserve the mask index across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 296], r9");                       // preserve the membership set base across the warning call
    emitter.instruction("mov rdi, QWORD PTR [rsp + 320]");                      // load the PHP-visible function name pointer
    emitter.instruction("mov rsi, QWORD PTR [rsp + 328]");                      // load the PHP-visible function name length
    abi::emit_symbol_address(emitter, "rdx", trim_warnings::RANGE_LEFT_SYMBOL);
    emitter.instruction(&format!("mov rcx, {}", trim_warnings::RANGE_LEFT_LEN)); // pass the invalid-range message length
    emitter.instruction("call __rt_trim_warning");                              // emit the PHP invalid-range warning
    emitter.instruction("mov rax, QWORD PTR [rsp + 256]");                      // restore the source pointer after the warning
    emitter.instruction("mov rdx, QWORD PTR [rsp + 264]");                      // restore the source length after the warning
    emitter.instruction("mov rdi, QWORD PTR [rsp + 272]");                      // restore the mask pointer after the warning
    emitter.instruction("mov rsi, QWORD PTR [rsp + 280]");                      // restore the mask length after the warning
    emitter.instruction("mov r8, QWORD PTR [rsp + 288]");                       // restore the mask index after the warning
    emitter.instruction("mov r9, QWORD PTR [rsp + 296]");                       // restore the membership set base after the warning
    emitter.instruction("inc r8");                                              // advance past the invalid pair
    emitter.instruction("jmp __rt_ltrim_mask_expand_linux_x86_64");             // parse the next charlist byte
    emitter.label("__rt_ltrim_mask_warn_right_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rsp + 256], rax");                      // preserve the source pointer across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 264], rdx");                      // preserve the source length across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 272], rdi");                      // preserve the mask pointer across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 280], rsi");                      // preserve the mask length across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 288], r8");                       // preserve the mask index across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 296], r9");                       // preserve the membership set base across the warning call
    emitter.instruction("mov rdi, QWORD PTR [rsp + 320]");                      // load the PHP-visible function name pointer
    emitter.instruction("mov rsi, QWORD PTR [rsp + 328]");                      // load the PHP-visible function name length
    abi::emit_symbol_address(emitter, "rdx", trim_warnings::RANGE_RIGHT_SYMBOL);
    emitter.instruction(&format!("mov rcx, {}", trim_warnings::RANGE_RIGHT_LEN)); // pass the invalid-range message length
    emitter.instruction("call __rt_trim_warning");                              // emit the PHP invalid-range warning
    emitter.instruction("mov rax, QWORD PTR [rsp + 256]");                      // restore the source pointer after the warning
    emitter.instruction("mov rdx, QWORD PTR [rsp + 264]");                      // restore the source length after the warning
    emitter.instruction("mov rdi, QWORD PTR [rsp + 272]");                      // restore the mask pointer after the warning
    emitter.instruction("mov rsi, QWORD PTR [rsp + 280]");                      // restore the mask length after the warning
    emitter.instruction("mov r8, QWORD PTR [rsp + 288]");                       // restore the mask index after the warning
    emitter.instruction("mov r9, QWORD PTR [rsp + 296]");                       // restore the membership set base after the warning
    emitter.instruction("inc r8");                                              // advance past the invalid pair
    emitter.instruction("jmp __rt_ltrim_mask_expand_linux_x86_64");             // parse the next charlist byte
    emitter.label("__rt_ltrim_mask_warn_order_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rsp + 256], rax");                      // preserve the source pointer across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 264], rdx");                      // preserve the source length across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 272], rdi");                      // preserve the mask pointer across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 280], rsi");                      // preserve the mask length across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 288], r8");                       // preserve the mask index across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 296], r9");                       // preserve the membership set base across the warning call
    emitter.instruction("mov rdi, QWORD PTR [rsp + 320]");                      // load the PHP-visible function name pointer
    emitter.instruction("mov rsi, QWORD PTR [rsp + 328]");                      // load the PHP-visible function name length
    abi::emit_symbol_address(emitter, "rdx", trim_warnings::RANGE_ORDER_SYMBOL);
    emitter.instruction(&format!("mov rcx, {}", trim_warnings::RANGE_ORDER_LEN)); // pass the invalid-range message length
    emitter.instruction("call __rt_trim_warning");                              // emit the PHP invalid-range warning
    emitter.instruction("mov rax, QWORD PTR [rsp + 256]");                      // restore the source pointer after the warning
    emitter.instruction("mov rdx, QWORD PTR [rsp + 264]");                      // restore the source length after the warning
    emitter.instruction("mov rdi, QWORD PTR [rsp + 272]");                      // restore the mask pointer after the warning
    emitter.instruction("mov rsi, QWORD PTR [rsp + 280]");                      // restore the mask length after the warning
    emitter.instruction("mov r8, QWORD PTR [rsp + 288]");                       // restore the mask index after the warning
    emitter.instruction("mov r9, QWORD PTR [rsp + 296]");                       // restore the membership set base after the warning
    emitter.instruction("inc r8");                                              // advance past the invalid pair
    emitter.instruction("jmp __rt_ltrim_mask_expand_linux_x86_64");             // parse the next charlist byte
    emitter.label("__rt_ltrim_mask_warn_generic_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rsp + 256], rax");                      // preserve the source pointer across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 264], rdx");                      // preserve the source length across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 272], rdi");                      // preserve the mask pointer across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 280], rsi");                      // preserve the mask length across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 288], r8");                       // preserve the mask index across the warning call
    emitter.instruction("mov QWORD PTR [rsp + 296], r9");                       // preserve the membership set base across the warning call
    emitter.instruction("mov rdi, QWORD PTR [rsp + 320]");                      // load the PHP-visible function name pointer
    emitter.instruction("mov rsi, QWORD PTR [rsp + 328]");                      // load the PHP-visible function name length
    abi::emit_symbol_address(emitter, "rdx", trim_warnings::RANGE_GENERIC_SYMBOL);
    emitter.instruction(&format!("mov rcx, {}", trim_warnings::RANGE_GENERIC_LEN)); // pass the invalid-range message length
    emitter.instruction("call __rt_trim_warning");                              // emit the PHP invalid-range warning
    emitter.instruction("mov rax, QWORD PTR [rsp + 256]");                      // restore the source pointer after the warning
    emitter.instruction("mov rdx, QWORD PTR [rsp + 264]");                      // restore the source length after the warning
    emitter.instruction("mov rdi, QWORD PTR [rsp + 272]");                      // restore the mask pointer after the warning
    emitter.instruction("mov rsi, QWORD PTR [rsp + 280]");                      // restore the mask length after the warning
    emitter.instruction("mov r8, QWORD PTR [rsp + 288]");                       // restore the mask index after the warning
    emitter.instruction("mov r9, QWORD PTR [rsp + 296]");                       // restore the membership set base after the warning
    emitter.instruction("inc r8");                                              // advance past the invalid pair
    emitter.instruction("jmp __rt_ltrim_mask_expand_linux_x86_64");             // parse the next charlist byte
    emitter.label("__rt_ltrim_mask_dot_advance_linux_x86_64");
    emitter.instruction("inc r8");                                              // skip the first dot of the invalid pair without marking it
    emitter.instruction("jmp __rt_ltrim_mask_expand_linux_x86_64");             // let the second dot be reprocessed literally
    emitter.label("__rt_ltrim_mask_literal_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9 + r10], 1");                          // mark the literal byte as trimmable
    emitter.instruction("inc r8");                                              // consume one literal charlist byte
    emitter.instruction("jmp __rt_ltrim_mask_expand_linux_x86_64");             // parse the next charlist byte
    emitter.label("__rt_ltrim_mask_scan_linux_x86_64");
    emitter.instruction("test rdx, rdx");                                       // stop once the string slice is empty
    emitter.instruction("jz __rt_ltrim_mask_done_linux_x86_64");                // return immediately when there are no source bytes left
    emitter.instruction("movzx ecx, BYTE PTR [rax]");                           // load the current leading source byte
    emitter.instruction("cmp BYTE PTR [r9 + rcx], 0");                          // test the byte against the expanded charlist set
    emitter.instruction("je __rt_ltrim_mask_done_linux_x86_64");                // stop at the first byte outside the mask
    emitter.instruction("inc rax");                                             // advance past the trimmed leading byte
    emitter.instruction("dec rdx");                                             // shrink the remaining source length
    emitter.instruction("jmp __rt_ltrim_mask_scan_linux_x86_64");               // continue trimming from the new front
    emitter.label("__rt_ltrim_mask_done_linux_x86_64");
    emitter.instruction("add rsp, 344");                                        // release the membership set and warning spill slots
    emitter.instruction("ret");                                                 // return the adjusted borrowed source slice
}
