//! Purpose:
//! Emits the `__rt_grapheme_strrev` runtime helper for grapheme-aware string reversal.
//! The helper owns UTF-8 cluster scanning for the PHP 8.6 `grapheme_strrev()` builtin.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - Reverses cluster byte slices without reordering bytes inside each cluster, preserving NUL bytes and UTF-8 payloads.
//! - Returns a null pointer sentinel on malformed UTF-8 so codegen can box PHP `false`.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the grapheme-aware string reversal runtime helper for the active target.
///
/// Input is the standard PHP string pair (`x1`/`x2` on ARM64, `rax`/`rdx` on
/// x86_64). Output is a string pair on success, or a null pointer plus zero
/// length on malformed UTF-8. The implementation recognizes combining marks,
/// variation selectors, emoji modifiers, and zero-width-joiner sequences as
/// single clusters.
pub fn emit_grapheme_strrev(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_grapheme_strrev_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: grapheme_strrev ---");
    emitter.label_global("__rt_grapheme_strrev");

    // -- preserve caller return address for local decoder calls --
    emitter.instruction("sub sp, sp, #16");                                     // reserve a small aligned frame for the saved return address
    emitter.instruction("str x30, [sp, #8]");                                   // preserve the external caller return address across local decoder calls

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current concat-buffer write offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer for the reversed string
    emitter.instruction("mov x10, x9");                                         // preserve destination start for the returned string pointer
    emitter.instruction("mov x0, x1");                                          // keep the source string pointer in a decoder-friendly scratch register
    emitter.instruction("mov x11, x2");                                         // scan_end = source length in bytes

    // -- walk clusters from the end of the source to the beginning --
    emitter.label("__rt_grapheme_strrev_loop");
    emitter.instruction("cbz x11, __rt_grapheme_strrev_done");                  // finish once every source byte has been assigned to a reversed cluster
    emitter.instruction("mov x3, x11");                                         // decoder input end = current source scan boundary
    emitter.instruction("bl __rt_grapheme_prev_utf8");                          // decode the previous UTF-8 scalar before scan_end
    emitter.instruction("cbz x6, __rt_grapheme_strrev_fail");                   // malformed UTF-8 makes grapheme_strrev() return false
    emitter.instruction("mov x12, x4");                                         // cluster_start begins at the last decoded scalar
    emitter.instruction("mov x16, x5");                                         // current first scalar drives backward cluster extension rules

    // -- extend over combining marks, emoji modifiers, and ZWJ sequences --
    emitter.label("__rt_grapheme_cluster_extend");
    emitter.instruction("cbz x12, __rt_grapheme_cluster_copy");                 // source start reached: the current cluster cannot extend farther left
    emitter.instruction("mov x3, x12");                                         // decoder input end = current tentative cluster start
    emitter.instruction("bl __rt_grapheme_prev_utf8");                          // decode the scalar immediately before the tentative cluster
    emitter.instruction("cbz x6, __rt_grapheme_strrev_fail");                   // malformed UTF-8 makes grapheme_strrev() return false
    crate::codegen::abi::emit_load_int_immediate(emitter, "x17", 8205);
    emitter.instruction("cmp x16, x17");                                        // is the current first scalar a zero-width joiner?
    emitter.instruction("b.eq __rt_grapheme_include_prev");                     // include the scalar before a ZWJ in the same grapheme cluster
    emitter.instruction("cmp x5, x17");                                         // is the previous scalar a zero-width joiner?
    emitter.instruction("b.eq __rt_grapheme_include_prev");                     // include the ZWJ so the following emoji/text unit stays joined
    emit_extend_range_check(emitter, "x16", 0x0300, 0x036f);
    emit_extend_range_check(emitter, "x16", 0x1ab0, 0x1aff);
    emit_extend_range_check(emitter, "x16", 0x1dc0, 0x1dff);
    emit_extend_range_check(emitter, "x16", 0x20d0, 0x20ff);
    emit_extend_range_check(emitter, "x16", 0xfe00, 0xfe0f);
    emit_extend_range_check(emitter, "x16", 0xfe20, 0xfe2f);
    emit_extend_range_check(emitter, "x16", 0x1f3fb, 0x1f3ff);
    emitter.instruction("b __rt_grapheme_cluster_copy");                        // no left extension rule matched, so copy the current cluster

    emitter.label("__rt_grapheme_include_prev");
    emitter.instruction("mov x12, x4");                                         // extend the cluster start to include the previous scalar
    emitter.instruction("mov x16, x5");                                         // make the included scalar the new first scalar for continued checks
    emitter.instruction("b __rt_grapheme_cluster_extend");                      // keep extending while additional left-side rules apply

    // -- copy the chosen cluster byte range forward into the destination --
    emitter.label("__rt_grapheme_cluster_copy");
    emitter.instruction("mov x13, x12");                                        // copy_index = cluster_start
    emitter.label("__rt_grapheme_cluster_copy_loop");
    emitter.instruction("cmp x13, x11");                                        // have all bytes in the selected cluster been copied?
    emitter.instruction("b.ge __rt_grapheme_cluster_copied");                   // finish this cluster once copy_index reaches cluster_end
    emitter.instruction("ldrb w14, [x1, x13]");                                 // load the next source byte from the current grapheme cluster
    emitter.instruction("strb w14, [x9], #1");                                  // append the cluster byte to the reversed destination string
    emitter.instruction("add x13, x13, #1");                                    // advance to the next byte inside the selected cluster
    emitter.instruction("b __rt_grapheme_cluster_copy_loop");                   // continue copying this cluster as an intact byte slice
    emitter.label("__rt_grapheme_cluster_copied");
    emitter.instruction("mov x11, x12");                                        // next scan_end is the byte before this copied cluster
    emitter.instruction("b __rt_grapheme_strrev_loop");                         // continue with the previous source grapheme cluster

    // -- publish success --
    emitter.label("__rt_grapheme_strrev_done");
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // reload concat-buffer write offset for the final update
    emitter.instruction("add x8, x8, x2");                                      // advance offset by the unchanged source byte length
    emitter.instruction("str x8, [x6]");                                        // publish the updated concat-buffer write offset
    emitter.instruction("mov x1, x10");                                         // return pointer to the reversed string
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the external caller return address
    emitter.instruction("add sp, sp, #16");                                     // release the local decoder-call frame
    emitter.instruction("ret");                                                 // return the reversed string in x1/x2

    // -- publish failure as null pointer sentinel --
    emitter.label("__rt_grapheme_strrev_fail");
    emitter.instruction("mov x1, #0");                                          // null pointer sentinel means false for grapheme_strrev()
    emitter.instruction("mov x2, #0");                                          // failure length is zero
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the external caller return address
    emitter.instruction("add sp, sp, #16");                                     // release the local decoder-call frame on failure
    emitter.instruction("ret");                                                 // return the false sentinel to the codegen wrapper

    emit_prev_utf8_aarch64(emitter);
}

/// Emits a branch to the shared include label when `reg` falls inside one extend range.
fn emit_extend_range_check(emitter: &mut Emitter, reg: &str, start: u32, end: u32) {
    let next_label = format!("__rt_grapheme_extend_next_{start:x}_{end:x}");
    crate::codegen::abi::emit_load_int_immediate(emitter, "x17", i64::from(start));
    emitter.instruction(&format!("cmp {}, x17", reg));                          // compare the current first scalar with the extend-range lower bound
    emitter.instruction(&format!("b.lt {}", next_label));                       // values below this range do not match this extend rule
    crate::codegen::abi::emit_load_int_immediate(emitter, "x17", i64::from(end));
    emitter.instruction(&format!("cmp {}, x17", reg));                          // compare the current first scalar with the extend-range upper bound
    emitter.instruction("b.le __rt_grapheme_include_prev");                     // extend characters attach to the previous base scalar
    emitter.label(&next_label);
}

/// Emits the ARM64 local helper that decodes the previous UTF-8 scalar.
///
/// Input: `x0` source pointer, `x3` exclusive end index.
/// Output: `x4` scalar byte start, `x5` scalar value, `x6` success flag.
fn emit_prev_utf8_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_grapheme_prev_utf8");
    emitter.instruction("sub x4, x3, #1");                                      // start at the byte immediately before the exclusive end index
    emitter.instruction("ldrb w5, [x0, x4]");                                   // load the candidate final byte of the previous UTF-8 scalar
    emitter.instruction("tst x5, #128");                                        // check whether the byte is plain ASCII
    emitter.instruction("b.eq __rt_grapheme_prev_utf8_ascii");                  // ASCII bytes are complete one-byte scalars
    emitter.instruction("mov x6, #0");                                          // initialize continuation count while scanning back to the lead byte

    emitter.label("__rt_grapheme_prev_utf8_find_lead");
    emitter.instruction("and x7, x5, #192");                                    // isolate the UTF-8 high-bit class of the current byte
    emitter.instruction("cmp x7, #128");                                        // is the byte a continuation byte?
    emitter.instruction("b.ne __rt_grapheme_prev_utf8_lead");                   // a non-continuation byte is the candidate lead byte
    emitter.instruction("cbz x4, __rt_grapheme_prev_utf8_malformed");           // a continuation at byte zero has no valid lead byte
    emitter.instruction("cmp x6, #3");                                          // valid UTF-8 scalars have at most three continuation bytes
    emitter.instruction("b.ge __rt_grapheme_prev_utf8_malformed");              // too many continuation bytes means malformed UTF-8
    emitter.instruction("sub x4, x4, #1");                                      // step backward toward the scalar lead byte
    emitter.instruction("add x6, x6, #1");                                      // record one continuation byte following the candidate lead
    emitter.instruction("ldrb w5, [x0, x4]");                                   // load the next byte to test as the lead byte
    emitter.instruction("b __rt_grapheme_prev_utf8_find_lead");                 // keep scanning until a lead byte is found

    emitter.label("__rt_grapheme_prev_utf8_lead");
    emitter.instruction("cmp x5, #194");                                        // valid two-byte UTF-8 leads start at 0xC2
    emitter.instruction("b.lt __rt_grapheme_prev_utf8_malformed");              // reject overlong or continuation-like lead bytes
    emitter.instruction("cmp x5, #223");                                        // is this a two-byte UTF-8 lead byte?
    emitter.instruction("b.le __rt_grapheme_prev_utf8_two");                    // decode a two-byte scalar
    emitter.instruction("cmp x5, #239");                                        // is this a three-byte UTF-8 lead byte?
    emitter.instruction("b.le __rt_grapheme_prev_utf8_three");                  // decode a three-byte scalar
    emitter.instruction("cmp x5, #244");                                        // valid four-byte UTF-8 leads end at 0xF4
    emitter.instruction("b.le __rt_grapheme_prev_utf8_four");                   // decode a four-byte scalar
    emitter.instruction("b __rt_grapheme_prev_utf8_malformed");                 // lead bytes above 0xF4 are not valid UTF-8

    emitter.label("__rt_grapheme_prev_utf8_ascii");
    emitter.instruction("mov x6, #1");                                          // mark the one-byte ASCII scalar as valid
    emitter.instruction("ret");                                                 // return the ASCII scalar value and byte start

    emitter.label("__rt_grapheme_prev_utf8_two");
    emitter.instruction("cmp x6, #1");                                          // two-byte scalars require exactly one continuation byte
    emitter.instruction("b.ne __rt_grapheme_prev_utf8_malformed");              // mismatched continuation count means malformed UTF-8
    emitter.instruction("and x5, x5, #31");                                     // keep the payload bits from the two-byte lead
    emitter.instruction("add x7, x4, #1");                                      // point at the scalar continuation byte
    emitter.instruction("ldrb w8, [x0, x7]");                                   // load the continuation byte
    emitter.instruction("and x8, x8, #63");                                     // keep the continuation payload bits
    emitter.instruction("lsl x5, x5, #6");                                      // shift the lead payload into scalar position
    emitter.instruction("orr x5, x5, x8");                                      // combine lead and continuation payloads into the Unicode scalar
    emitter.instruction("mov x6, #1");                                          // mark the decoded scalar as valid
    emitter.instruction("ret");                                                 // return the decoded two-byte scalar

    emitter.label("__rt_grapheme_prev_utf8_three");
    emitter.instruction("cmp x6, #2");                                          // three-byte scalars require exactly two continuation bytes
    emitter.instruction("b.ne __rt_grapheme_prev_utf8_malformed");              // mismatched continuation count means malformed UTF-8
    emitter.instruction("and x5, x5, #15");                                     // keep the payload bits from the three-byte lead
    emitter.instruction("add x7, x4, #1");                                      // point at the first continuation byte
    emitter.instruction("ldrb w8, [x0, x7]");                                   // load the first continuation byte
    emitter.instruction("and x8, x8, #63");                                     // keep the first continuation payload bits
    emitter.instruction("lsl x5, x5, #6");                                      // make room for the first continuation payload
    emitter.instruction("orr x5, x5, x8");                                      // fold the first continuation payload into the scalar
    emitter.instruction("add x7, x4, #2");                                      // point at the second continuation byte
    emitter.instruction("ldrb w8, [x0, x7]");                                   // load the second continuation byte
    emitter.instruction("and x8, x8, #63");                                     // keep the second continuation payload bits
    emitter.instruction("lsl x5, x5, #6");                                      // make room for the second continuation payload
    emitter.instruction("orr x5, x5, x8");                                      // fold the second continuation payload into the scalar
    emitter.instruction("mov x6, #1");                                          // mark the decoded scalar as valid
    emitter.instruction("ret");                                                 // return the decoded three-byte scalar

    emitter.label("__rt_grapheme_prev_utf8_four");
    emitter.instruction("cmp x6, #3");                                          // four-byte scalars require exactly three continuation bytes
    emitter.instruction("b.ne __rt_grapheme_prev_utf8_malformed");              // mismatched continuation count means malformed UTF-8
    emitter.instruction("and x5, x5, #7");                                      // keep the payload bits from the four-byte lead
    emitter.instruction("add x7, x4, #1");                                      // point at the first continuation byte
    emitter.instruction("ldrb w8, [x0, x7]");                                   // load the first continuation byte
    emitter.instruction("and x8, x8, #63");                                     // keep the first continuation payload bits
    emitter.instruction("lsl x5, x5, #6");                                      // make room for the first continuation payload
    emitter.instruction("orr x5, x5, x8");                                      // fold the first continuation payload into the scalar
    emitter.instruction("add x7, x4, #2");                                      // point at the second continuation byte
    emitter.instruction("ldrb w8, [x0, x7]");                                   // load the second continuation byte
    emitter.instruction("and x8, x8, #63");                                     // keep the second continuation payload bits
    emitter.instruction("lsl x5, x5, #6");                                      // make room for the second continuation payload
    emitter.instruction("orr x5, x5, x8");                                      // fold the second continuation payload into the scalar
    emitter.instruction("add x7, x4, #3");                                      // point at the third continuation byte
    emitter.instruction("ldrb w8, [x0, x7]");                                   // load the third continuation byte
    emitter.instruction("and x8, x8, #63");                                     // keep the third continuation payload bits
    emitter.instruction("lsl x5, x5, #6");                                      // make room for the third continuation payload
    emitter.instruction("orr x5, x5, x8");                                      // fold the third continuation payload into the scalar
    emitter.instruction("mov x6, #1");                                          // mark the decoded scalar as valid
    emitter.instruction("ret");                                                 // return the decoded four-byte scalar

    emitter.label("__rt_grapheme_prev_utf8_malformed");
    emitter.instruction("mov x6, #0");                                          // report malformed UTF-8 to the caller
    emitter.instruction("ret");                                                 // return without a valid scalar
}

/// Emits the x86_64 Linux variant of the grapheme-aware reversal helper.
///
/// The algorithm mirrors the ARM64 variant while following the existing
/// x86_64 string ABI: input `rax`/`rdx`, output `rax`/`rdx`.
fn emit_grapheme_strrev_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: grapheme_strrev ---");
    emitter.label_global("__rt_grapheme_strrev");

    emitter.instruction("push rbx");                                            // preserve callee-saved source pointer storage
    emitter.instruction("push r12");                                            // preserve callee-saved source length storage
    emitter.instruction("push r13");                                            // preserve callee-saved destination cursor storage
    emitter.instruction("push r14");                                            // preserve callee-saved destination start storage
    emitter.instruction("push r15");                                            // preserve callee-saved cluster-end storage
    emitter.instruction("mov rbx, rax");                                        // keep the source string pointer stable across decoder calls
    emitter.instruction("mov r12, rdx");                                        // keep the source string length stable across decoder calls
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load current concat-buffer write offset
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r13, [r10 + r9]");                                 // compute destination pointer for the reversed string
    emitter.instruction("mov r14, r13");                                        // preserve destination start for the returned string pointer
    emitter.instruction("mov rcx, r12");                                        // scan_end = source length in bytes

    emitter.label("__rt_grapheme_strrev_loop_x");
    emitter.instruction("test rcx, rcx");                                       // has every source byte been assigned to a reversed cluster?
    emitter.instruction("jz __rt_grapheme_strrev_done_x");                      // finish once scan_end reaches the start of the source string
    emitter.instruction("mov r15, rcx");                                        // preserve cluster_end while extension checks decode earlier scalars
    emitter.instruction("call __rt_grapheme_prev_utf8_x");                      // decode the previous UTF-8 scalar before scan_end
    emitter.instruction("test rdx, rdx");                                       // did the UTF-8 decoder report success?
    emitter.instruction("jz __rt_grapheme_strrev_fail_x");                      // malformed UTF-8 makes grapheme_strrev() return false
    emitter.instruction("mov r10, r8");                                         // cluster_start begins at the last decoded scalar
    emitter.instruction("mov r11, r9");                                         // current first scalar drives backward cluster extension rules

    emitter.label("__rt_grapheme_cluster_extend_x");
    emitter.instruction("test r10, r10");                                       // source start reached?
    emitter.instruction("jz __rt_grapheme_cluster_copy_x");                     // the current cluster cannot extend farther left
    emitter.instruction("mov rcx, r10");                                        // decoder input end = current tentative cluster start
    emitter.instruction("call __rt_grapheme_prev_utf8_x");                      // decode the scalar immediately before the tentative cluster
    emitter.instruction("test rdx, rdx");                                       // did the UTF-8 decoder report success?
    emitter.instruction("jz __rt_grapheme_strrev_fail_x");                      // malformed UTF-8 makes grapheme_strrev() return false
    emitter.instruction("cmp r11, 8205");                                       // is the current first scalar a zero-width joiner?
    emitter.instruction("je __rt_grapheme_include_prev_x");                     // include the scalar before a ZWJ in the same grapheme cluster
    emitter.instruction("cmp r9, 8205");                                        // is the previous scalar a zero-width joiner?
    emitter.instruction("je __rt_grapheme_include_prev_x");                     // include the ZWJ so the following emoji/text unit stays joined
    emit_extend_range_check_x86_64(emitter, "r11", 0x0300, 0x036f);
    emit_extend_range_check_x86_64(emitter, "r11", 0x1ab0, 0x1aff);
    emit_extend_range_check_x86_64(emitter, "r11", 0x1dc0, 0x1dff);
    emit_extend_range_check_x86_64(emitter, "r11", 0x20d0, 0x20ff);
    emit_extend_range_check_x86_64(emitter, "r11", 0xfe00, 0xfe0f);
    emit_extend_range_check_x86_64(emitter, "r11", 0xfe20, 0xfe2f);
    emit_extend_range_check_x86_64(emitter, "r11", 0x1f3fb, 0x1f3ff);
    emitter.instruction("jmp __rt_grapheme_cluster_copy_x");                    // no left extension rule matched, so copy the current cluster

    emitter.label("__rt_grapheme_include_prev_x");
    emitter.instruction("mov r10, r8");                                         // extend the cluster start to include the previous scalar
    emitter.instruction("mov r11, r9");                                         // make the included scalar the new first scalar for continued checks
    emitter.instruction("jmp __rt_grapheme_cluster_extend_x");                  // keep extending while additional left-side rules apply

    emitter.label("__rt_grapheme_cluster_copy_x");
    emitter.instruction("mov rax, r10");                                        // copy_index = cluster_start
    emitter.label("__rt_grapheme_cluster_copy_loop_x");
    emitter.instruction("cmp rax, r15");                                        // have all bytes in the selected cluster been copied?
    emitter.instruction("jge __rt_grapheme_cluster_copied_x");                  // finish this cluster once copy_index reaches cluster_end
    emitter.instruction("mov dl, BYTE PTR [rbx + rax]");                        // load the next source byte from the current grapheme cluster
    emitter.instruction("mov BYTE PTR [r13], dl");                              // append the cluster byte to the reversed destination string
    emitter.instruction("add r13, 1");                                          // advance the destination cursor after copying one byte
    emitter.instruction("add rax, 1");                                          // advance to the next byte inside the selected cluster
    emitter.instruction("jmp __rt_grapheme_cluster_copy_loop_x");               // continue copying this cluster as an intact byte slice
    emitter.label("__rt_grapheme_cluster_copied_x");
    emitter.instruction("mov rcx, r10");                                        // next scan_end is the byte before this copied cluster
    emitter.instruction("jmp __rt_grapheme_strrev_loop_x");                     // continue with the previous source grapheme cluster

    emitter.label("__rt_grapheme_strrev_done_x");
    emitter.instruction("mov r8, QWORD PTR [rip + _concat_off]");               // reload concat-buffer write offset for the final update
    emitter.instruction("add r8, r12");                                         // advance offset by the unchanged source byte length
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r8");               // publish the updated concat-buffer write offset
    emitter.instruction("mov rax, r14");                                        // return pointer to the reversed string
    emitter.instruction("mov rdx, r12");                                        // return the unchanged source byte length
    emitter.instruction("pop r15");                                             // restore callee-saved cluster-end storage
    emitter.instruction("pop r14");                                             // restore callee-saved destination start storage
    emitter.instruction("pop r13");                                             // restore callee-saved destination cursor storage
    emitter.instruction("pop r12");                                             // restore callee-saved source length storage
    emitter.instruction("pop rbx");                                             // restore callee-saved source pointer storage
    emitter.instruction("ret");                                                 // return the reversed string in rax/rdx

    emitter.label("__rt_grapheme_strrev_fail_x");
    emitter.instruction("xor eax, eax");                                        // null pointer sentinel means false for grapheme_strrev()
    emitter.instruction("xor edx, edx");                                        // failure length is zero
    emitter.instruction("pop r15");                                             // restore callee-saved cluster-end storage on failure
    emitter.instruction("pop r14");                                             // restore callee-saved destination start storage on failure
    emitter.instruction("pop r13");                                             // restore callee-saved destination cursor storage on failure
    emitter.instruction("pop r12");                                             // restore callee-saved source length storage on failure
    emitter.instruction("pop rbx");                                             // restore callee-saved source pointer storage on failure
    emitter.instruction("ret");                                                 // return the false sentinel to the codegen wrapper

    emit_prev_utf8_x86_64(emitter);
}

/// Emits a x86_64 branch to the shared include label for one extend range.
fn emit_extend_range_check_x86_64(emitter: &mut Emitter, reg: &str, start: u32, end: u32) {
    let next_label = format!("__rt_grapheme_extend_next_x_{start:x}_{end:x}");
    emitter.instruction(&format!("cmp {}, {}", reg, start));                    // compare the current first scalar with the extend-range lower bound
    emitter.instruction(&format!("jl {}", next_label));                         // values below this range do not match this extend rule
    emitter.instruction(&format!("cmp {}, {}", reg, end));                      // compare the current first scalar with the extend-range upper bound
    emitter.instruction("jle __rt_grapheme_include_prev_x");                    // extend characters attach to the previous base scalar
    emitter.label(&next_label);
}

/// Emits the x86_64 local helper that decodes the previous UTF-8 scalar.
///
/// Input: `rbx` source pointer, `rcx` exclusive end index.
/// Output: `r8` scalar byte start, `r9` scalar value, `rdx` success flag.
fn emit_prev_utf8_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_grapheme_prev_utf8_x");
    emitter.instruction("lea r8, [rcx - 1]");                                   // start at the byte immediately before the exclusive end index
    emitter.instruction("movzx r9, BYTE PTR [rbx + r8]");                       // load the candidate final byte of the previous UTF-8 scalar
    emitter.instruction("test r9b, 128");                                       // check whether the byte is plain ASCII
    emitter.instruction("jz __rt_grapheme_prev_utf8_ascii_x");                  // ASCII bytes are complete one-byte scalars
    emitter.instruction("xor eax, eax");                                        // initialize continuation count while scanning back to the lead byte

    emitter.label("__rt_grapheme_prev_utf8_find_lead_x");
    emitter.instruction("mov rdx, r9");                                         // copy the candidate byte so the class mask does not destroy the scalar value
    emitter.instruction("and rdx, 192");                                        // isolate the UTF-8 high-bit class of the current byte
    emitter.instruction("cmp rdx, 128");                                        // is the byte a continuation byte?
    emitter.instruction("jne __rt_grapheme_prev_utf8_lead_x");                  // a non-continuation byte is the candidate lead byte
    emitter.instruction("test r8, r8");                                         // is the continuation byte at the start of the string?
    emitter.instruction("jz __rt_grapheme_prev_utf8_malformed_x");              // a continuation at byte zero has no valid lead byte
    emitter.instruction("cmp rax, 3");                                          // valid UTF-8 scalars have at most three continuation bytes
    emitter.instruction("jge __rt_grapheme_prev_utf8_malformed_x");             // too many continuation bytes means malformed UTF-8
    emitter.instruction("sub r8, 1");                                           // step backward toward the scalar lead byte
    emitter.instruction("add rax, 1");                                          // record one continuation byte following the candidate lead
    emitter.instruction("movzx r9, BYTE PTR [rbx + r8]");                       // load the next byte to test as the lead byte
    emitter.instruction("jmp __rt_grapheme_prev_utf8_find_lead_x");             // keep scanning until a lead byte is found

    emitter.label("__rt_grapheme_prev_utf8_lead_x");
    emitter.instruction("cmp r9, 194");                                         // valid two-byte UTF-8 leads start at 0xC2
    emitter.instruction("jl __rt_grapheme_prev_utf8_malformed_x");              // reject overlong or continuation-like lead bytes
    emitter.instruction("cmp r9, 223");                                         // is this a two-byte UTF-8 lead byte?
    emitter.instruction("jle __rt_grapheme_prev_utf8_two_x");                   // decode a two-byte scalar
    emitter.instruction("cmp r9, 239");                                         // is this a three-byte UTF-8 lead byte?
    emitter.instruction("jle __rt_grapheme_prev_utf8_three_x");                 // decode a three-byte scalar
    emitter.instruction("cmp r9, 244");                                         // valid four-byte UTF-8 leads end at 0xF4
    emitter.instruction("jle __rt_grapheme_prev_utf8_four_x");                  // decode a four-byte scalar
    emitter.instruction("jmp __rt_grapheme_prev_utf8_malformed_x");             // lead bytes above 0xF4 are not valid UTF-8

    emitter.label("__rt_grapheme_prev_utf8_ascii_x");
    emitter.instruction("mov rdx, 1");                                          // mark the one-byte ASCII scalar as valid
    emitter.instruction("ret");                                                 // return the ASCII scalar value and byte start

    emitter.label("__rt_grapheme_prev_utf8_two_x");
    emitter.instruction("cmp rax, 1");                                          // two-byte scalars require exactly one continuation byte
    emitter.instruction("jne __rt_grapheme_prev_utf8_malformed_x");             // mismatched continuation count means malformed UTF-8
    emitter.instruction("and r9, 31");                                          // keep the payload bits from the two-byte lead
    emitter.instruction("movzx rdx, BYTE PTR [rbx + r8 + 1]");                  // load the continuation byte
    emitter.instruction("and rdx, 63");                                         // keep the continuation payload bits
    emitter.instruction("shl r9, 6");                                           // shift the lead payload into scalar position
    emitter.instruction("or r9, rdx");                                          // combine lead and continuation payloads into the Unicode scalar
    emitter.instruction("mov rdx, 1");                                          // mark the decoded scalar as valid
    emitter.instruction("ret");                                                 // return the decoded two-byte scalar

    emitter.label("__rt_grapheme_prev_utf8_three_x");
    emitter.instruction("cmp rax, 2");                                          // three-byte scalars require exactly two continuation bytes
    emitter.instruction("jne __rt_grapheme_prev_utf8_malformed_x");             // mismatched continuation count means malformed UTF-8
    emitter.instruction("and r9, 15");                                          // keep the payload bits from the three-byte lead
    emitter.instruction("movzx rdx, BYTE PTR [rbx + r8 + 1]");                  // load the first continuation byte
    emitter.instruction("and rdx, 63");                                         // keep the first continuation payload bits
    emitter.instruction("shl r9, 6");                                           // make room for the first continuation payload
    emitter.instruction("or r9, rdx");                                          // fold the first continuation payload into the scalar
    emitter.instruction("movzx rdx, BYTE PTR [rbx + r8 + 2]");                  // load the second continuation byte
    emitter.instruction("and rdx, 63");                                         // keep the second continuation payload bits
    emitter.instruction("shl r9, 6");                                           // make room for the second continuation payload
    emitter.instruction("or r9, rdx");                                          // fold the second continuation payload into the scalar
    emitter.instruction("mov rdx, 1");                                          // mark the decoded scalar as valid
    emitter.instruction("ret");                                                 // return the decoded three-byte scalar

    emitter.label("__rt_grapheme_prev_utf8_four_x");
    emitter.instruction("cmp rax, 3");                                          // four-byte scalars require exactly three continuation bytes
    emitter.instruction("jne __rt_grapheme_prev_utf8_malformed_x");             // mismatched continuation count means malformed UTF-8
    emitter.instruction("and r9, 7");                                           // keep the payload bits from the four-byte lead
    emitter.instruction("movzx rdx, BYTE PTR [rbx + r8 + 1]");                  // load the first continuation byte
    emitter.instruction("and rdx, 63");                                         // keep the first continuation payload bits
    emitter.instruction("shl r9, 6");                                           // make room for the first continuation payload
    emitter.instruction("or r9, rdx");                                          // fold the first continuation payload into the scalar
    emitter.instruction("movzx rdx, BYTE PTR [rbx + r8 + 2]");                  // load the second continuation byte
    emitter.instruction("and rdx, 63");                                         // keep the second continuation payload bits
    emitter.instruction("shl r9, 6");                                           // make room for the second continuation payload
    emitter.instruction("or r9, rdx");                                          // fold the second continuation payload into the scalar
    emitter.instruction("movzx rdx, BYTE PTR [rbx + r8 + 3]");                  // load the third continuation byte
    emitter.instruction("and rdx, 63");                                         // keep the third continuation payload bits
    emitter.instruction("shl r9, 6");                                           // make room for the third continuation payload
    emitter.instruction("or r9, rdx");                                          // fold the third continuation payload into the scalar
    emitter.instruction("mov rdx, 1");                                          // mark the decoded scalar as valid
    emitter.instruction("ret");                                                 // return the decoded four-byte scalar

    emitter.label("__rt_grapheme_prev_utf8_malformed_x");
    emitter.instruction("xor edx, edx");                                        // report malformed UTF-8 to the caller
    emitter.instruction("ret");                                                 // return without a valid scalar
}
