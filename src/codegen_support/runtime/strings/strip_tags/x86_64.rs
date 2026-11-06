//! Purpose:
//! Linux x86_64 `__rt_strip_tags` state machine for PHP 8.5 `strip_tags()`.
//!
//! Called from:
//! - `super::emit_strip_tags()` when the target is x86_64.
//!
//! Key details:
//! - ABI: `rdi`/`rsi` = subject pointer/length, `rdx`/`rcx` = allow pointer/length.
//! - Result: `rax`/`rdx` = stripped pointer/length, published through
//!   `__rt_concat_publish`.
//! - Zero allow length is treated as no allow-list.

use crate::codegen_support::emit::Emitter;


/// Emits the Linux x86_64 `__rt_strip_tags` helper.
pub(super) fn emit_strip_tags_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    // --- runtime: strip_tags ---
    emitter.comment("--- runtime: strip_tags ---");
    emitter.label_global("__rt_strip_tags");

    // -- preserve the caller and initialize per-call scanner state --
    emitter.instruction("push rbp");                                            // preserve the caller frame before allocating scanner state
    emitter.instruction("mov rbp, rsp");                                        // establish a stable base for all scanner state slots
    emitter.instruction("sub rsp, 208");                                        // reserve scanner state and callee-saved register spill slots
    emitter.instruction("mov QWORD PTR [rbp - 192], r8");                       // preserve the requested result mode
    emitter.instruction("mov QWORD PTR [rbp - 152], rbx");                      // preserve the saved caller rbx across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 160], r12");                      // preserve the saved caller r12 across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 168], r13");                      // preserve the saved caller r13 across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 176], r14");                      // preserve the saved caller r14 across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 184], r15");                      // preserve the saved caller r15 across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the subject pointer across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the subject byte length across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the allow-list pointer across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // preserve the original allow-list length across subsequent scan operations

    // -- reserve output, and allow/tag/norm scratch when an allow-list is present --
    emitter.comment("-- reserve output, and allow/tag/norm scratch when an allow-list is present --");
    emitter.instruction("mov rax, rsi");                                        // request at least the subject length for retained output
    emitter.instruction("test rcx, rcx");                                       // test the original allow-list length before the guarded scan transition
    emitter.instruction("jz __rt_strip_tags_reserve_x86");                      // reserve output without unnecessary allow-list scratch
    emitter.instruction("mov rdx, rsi");                                        // start sizing the subject-sized tag workspaces
    emitter.instruction("shl rdx, 1");                                          // include output and the first subject-sized tag workspace
    emitter.instruction("jo __rt_strip_tags_overflow_x86");                     // reject a wrapped reservation size before allocating
    emitter.instruction("add rdx, rsi");                                        // include the second subject-sized tag workspace
    emitter.instruction("jo __rt_strip_tags_overflow_x86");                     // reject a wrapped reservation size before allocating
    emitter.instruction("add rdx, rcx");                                        // include the complete allow-list case-folding workspace
    emitter.instruction("jo __rt_strip_tags_overflow_x86");                     // reject a wrapped reservation size before allocating
    emitter.instruction("add rdx, 4");                                          // reserve both normalized-tag delimiters without size wrap
    emitter.instruction("jo __rt_strip_tags_overflow_x86");                     // reject a wrapped reservation size before allocating
    emitter.instruction("mov rax, rdx");                                        // pass the checked total workspace size to the reservation helper
    emitter.label("__rt_strip_tags_reserve_x86");
    emitter.instruction("call __rt_concat_reserve");                            // reserve bounded output and allow-list workspace
    super::emit_mark_reserved_result_temporary(emitter);
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the output reservation start across subsequent scan operations
    emitter.instruction("mov rbx, rax");                                        // keep the output write cursor in a callee-saved register
    emitter.instruction("xor eax, eax");                                        // initialize the per-call workspace and length slots to zero
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the collected-tag buffer across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // preserve the collected-tag length across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // preserve the lowercased allow-list buffer across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // preserve the effective allow-list length across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // preserve the normalized-tag buffer across subsequent scan operations
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // check the original allow-list length before accepting the next scan transition
    emitter.instruction("je __rt_strip_tags_init_x86");                         // begin input scanning after normalizing the allow-list

    // -- partition the reservation: out | allow_lc | tag | norm --
    emitter.comment("-- partition the reservation: out | allow_lc | tag | norm --");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the output reservation start for this scan step
    emitter.instruction("add rax, QWORD PTR [rbp - 16]");                       // place allow-list workspace after the possible retained output
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // preserve the lowercased allow-list buffer across subsequent scan operations
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the original allow-list length for this scan step
    emitter.instruction("mov QWORD PTR [rbp - 80], rdx");                       // preserve the effective allow-list length across subsequent scan operations
    emitter.instruction("add rax, rdx");                                        // advance the workspace cursor past the preceding buffer
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the collected-tag buffer across subsequent scan operations
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the subject byte length for this scan step
    emitter.instruction("add rdx, 2");                                          // include both delimiters in the subject-sized tag workspace
    emitter.instruction("add rax, rdx");                                        // advance the workspace cursor past the preceding buffer
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // preserve the normalized-tag buffer across subsequent scan operations
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the allow-list pointer for this scan step
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // reload the lowercased allow-list buffer for this scan step
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the original allow-list length for this scan step
    emitter.instruction("xor r8d, r8d");                                        // start allow-list normalization at its first byte
    emitter.label("__rt_strip_tags_lc_allow_x86");
    emitter.instruction("cmp r8, rcx");                                         // stop allow-list normalization at its declared byte length
    emitter.instruction("jae __rt_strip_tags_init_x86");                        // begin input scanning after normalizing the allow-list
    emitter.instruction("movzx eax, BYTE PTR [rsi + r8]");                      // read the next bounded allow-list byte
    // PHP's strstr-based allow lookup ends at the first NUL, not the PHP string length.
    emitter.instruction("test al, al");                                         // test the output reservation start before the guarded scan transition
    emitter.instruction("jz __rt_strip_tags_allow_end_x86");                    // stop allow-list matching at PHP's first embedded NUL
    emitter.instruction("cmp al, 65");                                          // recognize the first uppercase ASCII letter
    emitter.instruction("jb __rt_strip_tags_lc_store_x86");                     // preserve allow bytes that need no ASCII case folding
    emitter.instruction("cmp al, 90");                                          // recognize the last uppercase ASCII letter
    emitter.instruction("ja __rt_strip_tags_lc_store_x86");                     // preserve allow bytes that need no ASCII case folding
    emitter.instruction("or al, 32");                                           // fold an ASCII uppercase allow-list letter to lowercase
    emitter.label("__rt_strip_tags_lc_store_x86");
    emitter.instruction("mov BYTE PTR [rdi + r8], al");                         // store one ASCII-normalized allow-list byte
    emitter.instruction("add r8, 1");                                           // advance the allow-list normalization cursor
    emitter.instruction("jmp __rt_strip_tags_lc_allow_x86");                    // normalize the next allow-list byte

    emitter.label("__rt_strip_tags_allow_end_x86");
    emitter.instruction("mov QWORD PTR [rbp - 80], r8");                        // preserve the effective allow-list length across subsequent scan operations
    emitter.label("__rt_strip_tags_init_x86");
    emitter.instruction("xor eax, eax");                                        // initialize the per-call workspace and length slots to zero
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // preserve the subject scan cursor across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // preserve the scanner state across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 112], rax");                      // preserve the active quote delimiter across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 120], rax");                      // preserve the nested angle-bracket depth across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 128], rax");                      // preserve the PHP parenthesis depth across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 136], rax");                      // preserve the last significant PHP byte across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 144], rax");                      // remember that XML-specific closing rules are active
    emitter.instruction("mov r12, QWORD PTR [rbp - 8]");                        // reload the subject pointer for this scan step
    emitter.instruction("mov r13, QWORD PTR [rbp - 16]");                       // reload the subject byte length for this scan step
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // is an allow-list active?
    emitter.instruction("jne __rt_strip_tags_loop_x86");                        // the full machine handles allowed tags
    emitter.instruction("jmp __rt_strip_tags_fast_entry_x86");                  // no allow-list uses the simple tag skipper

    emitter.label("__rt_strip_tags_loop_x86");
    emitter.instruction("mov r14, QWORD PTR [rbp - 96]");                       // reload the subject scan cursor for this scan step
    emitter.instruction("cmp r14, r13");                                        // stop scanning before the subject cursor reaches EOF
    emitter.instruction("jae __rt_strip_tags_done_x86");                        // publish the bytes retained after the subject is exhausted
    emitter.instruction("movzx eax, BYTE PTR [r12 + r14]");                     // load the current subject byte for state dispatch
    emitter.instruction("mov r15d, eax");                                       // carry the current subject byte into the next scan operation
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // reload the scanner state for this scan step
    emitter.instruction("cmp rax, 0");                                          // select 0 handling for the current byte
    emitter.instruction("je __rt_strip_tags_s0_x86");                           // handle ordinary output bytes
    emitter.instruction("cmp rax, 1");                                          // select HTML-tag handling for the current byte
    emitter.instruction("je __rt_strip_tags_s1_x86");                           // handle an HTML or XML tag
    emitter.instruction("cmp rax, 2");                                          // select PHP-tag handling for the current byte
    emitter.instruction("je __rt_strip_tags_s2_x86");                           // handle stripped PHP-tag contents
    emitter.instruction("cmp rax, 3");                                          // select declaration handling for the current byte
    emitter.instruction("je __rt_strip_tags_s3_x86");                           // handle markup declarations and comment openers
    emitter.instruction("jmp __rt_strip_tags_s4_x86");                          // handle stripped HTML-comment contents

    emit_state0_x86(emitter);
    emit_state1_x86(emitter);
    emit_state2_x86(emitter);
    emit_state3_x86(emitter);
    emit_state4_x86(emitter);
    emit_helpers_x86(emitter);
    emit_fast_path_x86(emitter);

    // -- publish the retained output and restore the caller --
    emitter.label("__rt_strip_tags_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the output reservation start for this scan step
    emitter.instruction("mov rdx, rbx");                                        // take the live output write cursor
    emitter.instruction("sub rdx, rax");                                        // derive retained output length from the final write cursor
    emitter.instruction("cmp QWORD PTR [rbp - 192], 0");                        // was a full string result requested?
    emitter.instruction("je __rt_strip_tags_publish_x86");                      // full string results publish and persist the reservation
    emitter.instruction("mov QWORD PTR [rbp - 192], rdx");                      // preserve the counted length across an optional heap release
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("mov r11, rax");                                        // classify the reservation by its offset from concat scratch
    emitter.instruction("sub r11, r10");                                        // scratch reservations stay inside the shared window
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "r10", emitter.concat_capacity as i64);
    emitter.instruction("cmp r11, r10");                                        // did this call fall back to owned heap storage?
    emitter.instruction("jb __rt_strip_tags_count_return_x86");                 // scratch reservations need no deallocation
    emitter.instruction("call __rt_heap_free_safe");                            // discard output bytes the length-only caller never observes
    emitter.label("__rt_strip_tags_count_return_x86");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 192]");                      // reload the counted output length
    emitter.instruction("mov rax, rdx");                                        // return the length in the integer result register
    emitter.instruction("jmp __rt_strip_tags_restore_x86");                     // skip string publication
    emitter.label("__rt_strip_tags_publish_x86");
    emitter.instruction("call __rt_concat_publish");                            // publish only the retained output bytes
    emitter.instruction("call __rt_str_persist");                               // return independent storage while adopting a sole heap reservation in place
    emitter.label("__rt_strip_tags_restore_x86");
    emitter.instruction("mov rbx, QWORD PTR [rbp - 152]");                      // reload the saved caller rbx for this scan step
    emitter.instruction("mov r12, QWORD PTR [rbp - 160]");                      // reload the saved caller r12 for this scan step
    emitter.instruction("mov r13, QWORD PTR [rbp - 168]");                      // reload the saved caller r13 for this scan step
    emitter.instruction("mov r14, QWORD PTR [rbp - 176]");                      // reload the saved caller r14 for this scan step
    emitter.instruction("mov r15, QWORD PTR [rbp - 184]");                      // reload the saved caller r15 for this scan step
    emitter.instruction("add rsp, 208");                                        // release all scanner state and register spill slots
    emitter.instruction("pop rbp");                                             // restore the caller's frame base before returning
    emitter.instruction("ret");                                                 // return the stripped byte string with caller state restored

    // -- refuse impossible output or workspace sizes --
    emitter.label("__rt_strip_tags_overflow_x86");
    emitter.instruction("jmp __rt_alloc_overflow");                             // abort before a wrapped reservation can under-size the output
}

/// Emits the no-allow-list fast path for x86_64. Text blocks are copied with
/// SSE2; ordinary tags are skipped with a direct quote-aware scan. Comments, PHP
/// tags, declarations and nested angle brackets restart the full state machine.
fn emit_fast_path_x86(emitter: &mut Emitter) {
    emitter.label("__rt_strip_tags_fast_entry_x86");
    emitter.instruction("xor r14d, r14d");                                      // start at the first subject byte
    emitter.label("__rt_strip_tags_fast_x86");
    emitter.instruction("cmp r14, r13");                                        // has the subject ended?
    emitter.instruction("jae __rt_strip_tags_fast_done_x86");                   // publish the retained bytes
    emitter.instruction("movzx eax, BYTE PTR [r12 + r14]");                     // load the next subject byte
    emitter.instruction("mov r15d, eax");                                       // carry the byte into the tag checks
    emitter.instruction("cmp al, 60");                                          // is this an opening delimiter?
    emitter.instruction("jne __rt_strip_tags_fast_text_block_x86");             // copy ordinary text
    emitter.instruction("lea r9, [r14 + 1]");                                   // address the byte after '<'
    emitter.instruction("cmp r9, r13");                                         // is the opener the final byte?
    emitter.instruction("jae __rt_strip_tags_fast_done_x86");                   // an unterminated tag is discarded
    emitter.instruction("movzx ecx, BYTE PTR [r12 + r9]");                      // load the byte after '<'
    emitter.instruction("cmp cl, 32");                                          // is the opener followed by a space?
    emitter.instruction("je __rt_strip_tags_fast_copy_lt_x86");                 // whitespace keeps '<' as text
    emitter.instruction("cmp cl, 9");                                           // is the lookahead below the whitespace range?
    emitter.instruction("jb __rt_strip_tags_fast_tag_prefix_x86");              // a control byte is not whitespace
    emitter.instruction("cmp cl, 13");                                          // is the lookahead inside the whitespace range?
    emitter.instruction("jbe __rt_strip_tags_fast_copy_lt_x86");                // tab/newline/form-feed keeps '<' as text
    emitter.label("__rt_strip_tags_fast_tag_prefix_x86");
    emitter.instruction("cmp cl, 33");                                          // is this a declaration or comment?
    emitter.instruction("je __rt_strip_tags_fast_fallback_x86");                // let the full machine handle it
    emitter.instruction("cmp cl, 37");                                          // is this an ASP tag?
    emitter.instruction("je __rt_strip_tags_fast_fallback_x86");                // let the full machine handle it
    emitter.instruction("cmp cl, 63");                                          // is this a PHP tag?
    emitter.instruction("je __rt_strip_tags_fast_fallback_x86");                // let the full machine handle it
    emitter.instruction("mov r9, r14");                                         // remember the tag opener for a fallback
    emitter.instruction("add r14, 1");                                          // consume the opening delimiter
    emitter.label("__rt_strip_tags_fast_tag_loop_x86");
    emitter.instruction("cmp r14, r13");                                        // has the tag reached EOF?
    emitter.instruction("jae __rt_strip_tags_fast_done_x86");                   // an unterminated tag is discarded
    emitter.instruction("movzx eax, BYTE PTR [r12 + r14]");                     // load the next tag byte
    emitter.instruction("test al, al");                                         // are embedded NUL bytes present?
    emitter.instruction("jz __rt_strip_tags_fast_tag_next_x86");                // embedded NULs are skipped
    emitter.instruction("cmp al, 60");                                          // is this a nested angle bracket?
    emitter.instruction("je __rt_strip_tags_fast_restart_tag_x86");             // the full machine tracks nesting
    emitter.instruction("cmp al, 34");                                          // is this a double quote?
    emitter.instruction("je __rt_strip_tags_fast_quote_x86");                   // skip the quoted attribute
    emitter.instruction("cmp al, 39");                                          // is this a single quote?
    emitter.instruction("je __rt_strip_tags_fast_quote_x86");                   // skip the quoted attribute
    emitter.instruction("cmp al, 62");                                          // is this the tag closer?
    emitter.instruction("je __rt_strip_tags_fast_tag_close_x86");               // consume the complete tag
    emitter.label("__rt_strip_tags_fast_tag_next_x86");
    emitter.instruction("add r14, 1");                                          // consume one tag byte
    emitter.instruction("jmp __rt_strip_tags_fast_tag_loop_x86");               // scan the next tag byte
    emitter.label("__rt_strip_tags_fast_tag_close_x86");
    emitter.instruction("add r14, 1");                                          // consume the closing '>'
    emitter.instruction("jmp __rt_strip_tags_fast_x86");                        // scan the next text run
    emitter.label("__rt_strip_tags_fast_quote_x86");
    emitter.instruction("mov r10b, al");                                        // remember the active quote byte
    emitter.instruction("add r14, 1");                                          // consume the opening quote
    emitter.label("__rt_strip_tags_fast_quote_loop_x86");
    emitter.instruction("cmp r14, r13");                                        // did the quote reach EOF?
    emitter.instruction("jae __rt_strip_tags_fast_done_x86");                   // the unterminated tag is discarded
    emitter.instruction("movzx eax, BYTE PTR [r12 + r14]");                     // load the next quoted byte
    emitter.instruction("test al, al");                                         // are embedded NUL bytes present?
    emitter.instruction("jz __rt_strip_tags_fast_quote_next_x86");              // embedded NULs are skipped
    emitter.instruction("cmp al, r10b");                                        // is this the matching quote?
    emitter.instruction("je __rt_strip_tags_fast_quote_end_x86");               // resume ordinary tag scanning
    emitter.label("__rt_strip_tags_fast_quote_next_x86");
    emitter.instruction("add r14, 1");                                          // consume one quoted byte
    emitter.instruction("jmp __rt_strip_tags_fast_quote_loop_x86");             // scan the next quoted byte
    emitter.label("__rt_strip_tags_fast_quote_end_x86");
    emitter.instruction("add r14, 1");                                          // consume the closing quote
    emitter.instruction("jmp __rt_strip_tags_fast_tag_loop_x86");               // resume ordinary tag scanning
    emitter.label("__rt_strip_tags_fast_restart_tag_x86");
    emitter.instruction("mov r14, r9");                                         // restart at the tag opener
    emitter.instruction("jmp __rt_strip_tags_fast_fallback_x86");               // let the full machine handle nesting
    emitter.label("__rt_strip_tags_fast_copy_lt_x86");
    emitter.instruction("mov BYTE PTR [rbx], 60");                              // retain the literal '<'
    emitter.instruction("add rbx, 1");                                          // advance the output write cursor
    emitter.instruction("add r14, 1");                                          // consume the literal '<'
    emitter.instruction("jmp __rt_strip_tags_fast_x86");                        // scan the next text run
    emitter.label("__rt_strip_tags_fast_text_block_x86");
    emitter.instruction("lea r9, [r14 + 16]");                                  // tentatively take a full sixteen-byte block
    emitter.instruction("cmp r9, r13");                                         // does the block stay inside the subject?
    emitter.instruction("ja __rt_strip_tags_fast_text_scalar_x86");             // the tail is handled byte by byte
    emitter.instruction("movdqu xmm0, XMMWORD PTR [r12 + r14]");                // load sixteen text bytes
    emitter.instruction("pxor xmm1, xmm1");                                     // clear the NUL mask
    emitter.instruction("pcmpeqb xmm1, xmm0");                                  // mark embedded NUL bytes
    emitter.instruction("mov eax, 0x3c3c3c3c");                                 // broadcast the '<' delimiter
    emitter.instruction("movd xmm2, eax");                                      // move the delimiter into an XMM register
    emitter.instruction("pshufd xmm2, xmm2, 0");                                // replicate it across all sixteen bytes
    emitter.instruction("pcmpeqb xmm2, xmm0");                                  // mark '<' bytes
    emitter.instruction("por xmm1, xmm2");                                      // a special byte starts the scalar path
    emitter.instruction("pmovmskb eax, xmm1");                                  // move the block verdict into an integer register
    emitter.instruction("test eax, eax");                                       // did any byte need scalar handling?
    emitter.instruction("jnz __rt_strip_tags_fast_text_scalar_x86");            // fall back to the scalar loop
    emitter.instruction("movdqu XMMWORD PTR [rbx], xmm0");                      // copy the whole text block
    emitter.instruction("add rbx, 16");                                         // advance the output write cursor
    emitter.instruction("add r14, 16");                                         // consume the copied block
    emitter.instruction("jmp __rt_strip_tags_fast_text_block_x86");             // scan the next block
    emitter.label("__rt_strip_tags_fast_text_scalar_x86");
    emitter.instruction("cmp r14, r13");                                        // has the subject ended?
    emitter.instruction("jae __rt_strip_tags_fast_done_x86");                   // publish the retained bytes
    emitter.instruction("movzx eax, BYTE PTR [r12 + r14]");                     // load the next text byte
    emitter.instruction("test al, al");                                         // is this an embedded NUL?
    emitter.instruction("jz __rt_strip_tags_fast_skip_nul_x86");                // embedded NULs are not emitted
    emitter.instruction("cmp al, 60");                                          // is this an opening delimiter?
    emitter.instruction("je __rt_strip_tags_fast_x86");                         // dispatch the tag opener
    emitter.instruction("mov BYTE PTR [rbx], al");                              // copy one ordinary text byte
    emitter.instruction("add rbx, 1");                                          // advance the output write cursor
    emitter.label("__rt_strip_tags_fast_skip_nul_x86");
    emitter.instruction("add r14, 1");                                          // consume the text byte
    emitter.instruction("jmp __rt_strip_tags_fast_text_scalar_x86");            // continue with the tail
    emitter.label("__rt_strip_tags_fast_fallback_x86");
    emitter.instruction("mov QWORD PTR [rbp - 96], r14");                       // preserve the scan cursor for the full machine
    emitter.instruction("jmp __rt_strip_tags_loop_x86");                        // resume the state machine
    emitter.label("__rt_strip_tags_fast_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 96], r14");                       // preserve the scan cursor at EOF
    emitter.instruction("jmp __rt_strip_tags_done_x86");                        // publish or count the retained bytes
}

/// Emits output-state handling (state 0).
fn emit_state0_x86(emitter: &mut Emitter) {
    // -- state 0: copy bytes until a real tag opener --
    emitter.comment("-- state 0: copy bytes until a real tag opener --");
    emitter.label("__rt_strip_tags_s0_x86");
    emitter.instruction("test r15b, r15b");                                     // discard embedded NULs instead of emitting or collecting them
    emitter.instruction("jz __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("cmp r15b, 60");                                        // recognize an opening tag delimiter
    emitter.instruction("jne __rt_strip_tags_s0_gt_x86");                       // check a non-opening byte for a closing angle bracket
    emitter.instruction("cmp QWORD PTR [rbp - 112], 0");                        // check the active quote delimiter before accepting the next scan transition
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("call __rt_strip_tags_next_x86");                       // peek ahead without reading beyond the subject
    emitter.instruction("call __rt_strip_tags_isspace_x86");                    // classify lookahead using PHP's C-locale whitespace rules
    emitter.instruction("test eax, eax");                                       // test the C-locale whitespace result before the guarded scan transition
    emitter.instruction("jz __rt_strip_tags_s0_open_x86");                      // recognize markup only when the opener is not followed by whitespace
    emitter.instruction("mov BYTE PTR [rbx], 60");                              // append the literal opening delimiter to the output
    emitter.instruction("add rbx, 1");                                          // advance the output write cursor
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s0_open_x86");
    emitter.instruction("mov QWORD PTR [rbp - 136], 60");                       // preserve the last significant PHP byte across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 104], 1");                        // select HTML-tag handling for subsequent bytes
    emitter.instruction("cmp QWORD PTR [rbp - 80], 0");                         // check the effective allow-list length before accepting the next scan transition
    emitter.instruction("je __rt_strip_tags_s0_open_adv_x86");                  // skip collection when no allow-list is active
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // clear the collected-tag length
    emitter.instruction("mov eax, 60");                                         // supply the opening tag delimiter to tag collection
    emitter.instruction("call __rt_strip_tags_collect_x86");                    // retain this raw tag byte for possible allow-list matching
    emitter.label("__rt_strip_tags_s0_open_adv_x86");
    emitter.instruction("add QWORD PTR [rbp - 96], 1");                         // advance the subject scan cursor after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_loop_x86");                        // resume state dispatch at the saved subject cursor
    emitter.label("__rt_strip_tags_s0_gt_x86");
    emitter.instruction("cmp r15b, 62");                                        // recognize a closing tag delimiter
    emitter.instruction("jne __rt_strip_tags_s0_copy_x86");                     // retain the ordinary text byte in the output
    emitter.instruction("cmp QWORD PTR [rbp - 120], 0");                        // check the nested angle-bracket depth before accepting the next scan transition
    emitter.instruction("jle __rt_strip_tags_s0_gt_q_x86");                     // check quoting after nested angle brackets are exhausted
    emitter.instruction("sub QWORD PTR [rbp - 120], 1");                        // reduce the nested angle-bracket depth after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s0_gt_q_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 112], 0");                        // check the active quote delimiter before accepting the next scan transition
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s0_copy_x86");
    emitter.instruction("mov BYTE PTR [rbx], r15b");                            // append one retained byte to the output
    emitter.instruction("add rbx, 1");                                          // advance the output write cursor
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
}

/// Emits HTML-tag-state handling (state 1).
fn emit_state1_x86(emitter: &mut Emitter) {
    // -- state 1: inside a normal HTML/XML tag --
    emitter.comment("-- state 1: inside a normal HTML/XML tag --");
    emitter.label("__rt_strip_tags_s1_x86");
    emitter.instruction("test r15b, r15b");                                     // discard embedded NULs instead of emitting or collecting them
    emitter.instruction("jz __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("cmp r15b, 60");                                        // recognize an opening tag delimiter
    emitter.instruction("jne __rt_strip_tags_s1_gt_x86");                       // check whether this tag byte is a closer
    emitter.instruction("cmp QWORD PTR [rbp - 112], 0");                        // check the active quote delimiter before accepting the next scan transition
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("call __rt_strip_tags_next_x86");                       // peek ahead without reading beyond the subject
    emitter.instruction("call __rt_strip_tags_isspace_x86");                    // classify lookahead using PHP's C-locale whitespace rules
    emitter.instruction("test eax, eax");                                       // test the C-locale whitespace result before the guarded scan transition
    emitter.instruction("jz __rt_strip_tags_s1_depth_x86");                     // track an unquoted nested angle bracket
    emitter.instruction("mov eax, r15d");                                       // forward the current subject byte to output or tag collection
    emitter.instruction("call __rt_strip_tags_collect_x86");                    // retain this raw tag byte for possible allow-list matching
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s1_depth_x86");
    emitter.instruction("add QWORD PTR [rbp - 120], 1");                        // advance the nested angle-bracket depth after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s1_gt_x86");
    emitter.instruction("cmp r15b, 62");                                        // recognize a closing tag delimiter
    emitter.instruction("jne __rt_strip_tags_s1_quote_x86");                    // recognize quotes and special tag prefixes
    emitter.instruction("cmp QWORD PTR [rbp - 120], 0");                        // check the nested angle-bracket depth before accepting the next scan transition
    emitter.instruction("jle __rt_strip_tags_s1_gt_q_x86");                     // check quote and XML rules before accepting the tag closer
    emitter.instruction("sub QWORD PTR [rbp - 120], 1");                        // reduce the nested angle-bracket depth after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s1_gt_q_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 112], 0");                        // check the active quote delimiter before accepting the next scan transition
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("cmp QWORD PTR [rbp - 144], 0");                        // check the XML-prologue flag before accepting the next scan transition
    emitter.instruction("je __rt_strip_tags_s1_close_x86");                     // finish a tag whose closing delimiter is permitted
    emitter.instruction("test r14, r14");                                       // require a preceding subject byte before looking back
    emitter.instruction("je __rt_strip_tags_s1_close_x86");                     // finish a tag whose closing delimiter is permitted
    emitter.instruction("cmp BYTE PTR [r12 + r14 - 1], 45");                    // require a comment delimiter dash in the adjacent markup byte
    emitter.instruction("je __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s1_close_x86");
    emitter.instruction("mov QWORD PTR [rbp - 136], 62");                       // preserve the last significant PHP byte across subsequent scan operations
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                        // clear the active quote delimiter
    emitter.instruction("mov QWORD PTR [rbp - 104], 0");                        // clear the scanner state
    emitter.instruction("mov QWORD PTR [rbp - 144], 0");                        // clear the XML-prologue flag
    emitter.instruction("cmp QWORD PTR [rbp - 80], 0");                         // check the effective allow-list length before accepting the next scan transition
    emitter.instruction("je __rt_strip_tags_s1_close_adv_x86");                 // continue scanning when allow-list matching is unnecessary
    emitter.instruction("mov eax, 62");                                         // supply the closing tag delimiter to tag collection
    emitter.instruction("call __rt_strip_tags_collect_x86");                    // retain this raw tag byte for possible allow-list matching
    emitter.instruction("call __rt_strip_tags_tag_find_x86");                   // decide whether the complete normalized tag token is allowed
    emitter.instruction("test eax, eax");                                       // test the complete allow-list token result before the guarded scan transition
    emitter.instruction("jz __rt_strip_tags_s1_clear_tag_x86");                 // discard a buffered tag whose token is not allowed
    emitter.instruction("call __rt_strip_tags_emit_tag_x86");                   // copy the allowed tag, including its original attributes
    emitter.label("__rt_strip_tags_s1_clear_tag_x86");
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // clear the collected-tag length
    emitter.label("__rt_strip_tags_s1_close_adv_x86");
    emitter.instruction("add QWORD PTR [rbp - 96], 1");                         // advance the subject scan cursor after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_loop_x86");                        // resume state dispatch at the saved subject cursor
    emitter.label("__rt_strip_tags_s1_quote_x86");
    emitter.instruction("cmp r15b, 34");                                        // recognize a double-quote boundary
    emitter.instruction("je __rt_strip_tags_s1_toggle_q_x86");                  // update the attribute quote delimiter
    emitter.instruction("cmp r15b, 39");                                        // recognize a single-quote boundary
    emitter.instruction("je __rt_strip_tags_s1_toggle_q_x86");                  // update the attribute quote delimiter
    emitter.instruction("cmp r15b, 33");                                        // recognize a declaration marker after the opener
    emitter.instruction("je __rt_strip_tags_s1_bang_x86");                      // recognize a markup declaration after the tag opener
    emitter.instruction("cmp r15b, 63");                                        // recognize a PHP-tag question mark
    emitter.instruction("je __rt_strip_tags_s1_quest_x86");                     // recognize PHP markup after the tag opener
    emitter.instruction("mov eax, r15d");                                       // forward the current subject byte to output or tag collection
    emitter.instruction("call __rt_strip_tags_collect_x86");                    // retain this raw tag byte for possible allow-list matching
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s1_toggle_q_x86");
    emitter.instruction("test r14, r14");                                       // require a preceding subject byte before looking back
    emitter.instruction("je __rt_strip_tags_s1_q_collect_x86");                 // preserve literal quotes in the collected tag
    emitter.instruction("mov rax, QWORD PTR [rbp - 112]");                      // reload the active quote delimiter for this scan step
    emitter.instruction("test rax, rax");                                       // test the active quote delimiter before the guarded scan transition
    emitter.instruction("jz __rt_strip_tags_s1_q_set_x86");                     // remember the quote that opens this attribute
    emitter.instruction("cmp rax, r15");                                        // close a quote only when it matches the saved delimiter
    emitter.instruction("jne __rt_strip_tags_s1_q_collect_x86");                // preserve literal quotes in the collected tag
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                        // clear the active quote delimiter
    emitter.instruction("jmp __rt_strip_tags_s1_q_collect_x86");                // preserve literal quotes in the collected tag
    emitter.label("__rt_strip_tags_s1_q_set_x86");
    emitter.instruction("mov QWORD PTR [rbp - 112], r15");                      // preserve the active quote delimiter across subsequent scan operations
    emitter.label("__rt_strip_tags_s1_q_collect_x86");
    emitter.instruction("mov eax, r15d");                                       // forward the current subject byte to output or tag collection
    emitter.instruction("call __rt_strip_tags_collect_x86");                    // retain this raw tag byte for possible allow-list matching
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s1_bang_x86");
    emitter.instruction("test r14, r14");                                       // require a preceding subject byte before looking back
    emitter.instruction("je __rt_strip_tags_s1_bang_col_x86");                  // collect an exclamation mark that does not open a declaration
    emitter.instruction("cmp BYTE PTR [r12 + r14 - 1], 60");                    // require an opening tag delimiter in the adjacent markup byte
    emitter.instruction("jne __rt_strip_tags_s1_bang_col_x86");                 // collect an exclamation mark that does not open a declaration
    emitter.instruction("mov QWORD PTR [rbp - 104], 3");                        // select declaration handling for subsequent bytes
    emitter.instruction("mov QWORD PTR [rbp - 136], r15");                      // preserve the last significant PHP byte across subsequent scan operations
    emitter.instruction("add QWORD PTR [rbp - 96], 1");                         // advance the subject scan cursor after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_loop_x86");                        // resume state dispatch at the saved subject cursor
    emitter.label("__rt_strip_tags_s1_bang_col_x86");
    emitter.instruction("mov eax, r15d");                                       // forward the current subject byte to output or tag collection
    emitter.instruction("call __rt_strip_tags_collect_x86");                    // retain this raw tag byte for possible allow-list matching
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s1_quest_x86");
    emitter.instruction("test r14, r14");                                       // require a preceding subject byte before looking back
    emitter.instruction("je __rt_strip_tags_s1_quest_col_x86");                 // collect a question mark that does not open PHP markup
    emitter.instruction("cmp BYTE PTR [r12 + r14 - 1], 60");                    // require an opening tag delimiter in the adjacent markup byte
    emitter.instruction("jne __rt_strip_tags_s1_quest_col_x86");                // collect a question mark that does not open PHP markup
    emitter.instruction("mov QWORD PTR [rbp - 128], 0");                        // clear the PHP parenthesis depth
    emitter.instruction("mov QWORD PTR [rbp - 104], 2");                        // select PHP-tag handling for subsequent bytes
    emitter.instruction("add QWORD PTR [rbp - 96], 1");                         // advance the subject scan cursor after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_loop_x86");                        // resume state dispatch at the saved subject cursor
    emitter.label("__rt_strip_tags_s1_quest_col_x86");
    emitter.instruction("mov eax, r15d");                                       // forward the current subject byte to output or tag collection
    emitter.instruction("call __rt_strip_tags_collect_x86");                    // retain this raw tag byte for possible allow-list matching
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
}

/// Emits PHP-tag-state handling (state 2).
fn emit_state2_x86(emitter: &mut Emitter) {
    // -- state 2: inside a PHP tag, unless `<?xml` returns to HTML --
    emitter.comment("-- state 2: inside a PHP tag, unless `<?xml` returns to HTML --");
    emitter.label("__rt_strip_tags_s2_x86");
    emitter.instruction("cmp r15b, 40");                                        // recognize an opening PHP parenthesis
    emitter.instruction("jne __rt_strip_tags_s2_rparen_x86");                   // check whether this PHP byte closes a parenthesis
    emitter.instruction("cmp QWORD PTR [rbp - 136], 34");                       // recognize a double-quote boundary in the last significant PHP byte
    emitter.instruction("je __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("cmp QWORD PTR [rbp - 136], 39");                       // recognize a single-quote boundary in the last significant PHP byte
    emitter.instruction("je __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("mov QWORD PTR [rbp - 136], 40");                       // preserve the last significant PHP byte across subsequent scan operations
    emitter.instruction("add QWORD PTR [rbp - 128], 1");                        // advance the PHP parenthesis depth after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s2_rparen_x86");
    emitter.instruction("cmp r15b, 41");                                        // recognize a closing PHP parenthesis
    emitter.instruction("jne __rt_strip_tags_s2_gt_x86");                       // check whether this PHP byte can terminate markup
    emitter.instruction("cmp QWORD PTR [rbp - 136], 34");                       // recognize a double-quote boundary in the last significant PHP byte
    emitter.instruction("je __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("cmp QWORD PTR [rbp - 136], 39");                       // recognize a single-quote boundary in the last significant PHP byte
    emitter.instruction("je __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("mov QWORD PTR [rbp - 136], 41");                       // preserve the last significant PHP byte across subsequent scan operations
    emitter.instruction("sub QWORD PTR [rbp - 128], 1");                        // reduce the PHP parenthesis depth after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s2_gt_x86");
    emitter.instruction("cmp r15b, 62");                                        // recognize a closing tag delimiter
    emitter.instruction("jne __rt_strip_tags_s2_quote_x86");                    // track PHP quotes and recognize the XML exception
    emitter.instruction("cmp QWORD PTR [rbp - 120], 0");                        // check the nested angle-bracket depth before accepting the next scan transition
    emitter.instruction("jle __rt_strip_tags_s2_gt_q_x86");                     // check PHP quotes after nested angle brackets are exhausted
    emitter.instruction("sub QWORD PTR [rbp - 120], 1");                        // reduce the nested angle-bracket depth after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s2_gt_q_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 112], 0");                        // check the active quote delimiter before accepting the next scan transition
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("cmp QWORD PTR [rbp - 128], 0");                        // check the PHP parenthesis depth before accepting the next scan transition
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("cmp QWORD PTR [rbp - 136], 34");                       // recognize a double-quote boundary in the last significant PHP byte
    emitter.instruction("je __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("test r14, r14");                                       // require a preceding subject byte before looking back
    emitter.instruction("je __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("cmp BYTE PTR [r12 + r14 - 1], 63");                    // require a PHP-tag question mark in the adjacent markup byte
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                        // clear the active quote delimiter
    emitter.instruction("mov QWORD PTR [rbp - 104], 0");                        // clear the scanner state
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // clear the collected-tag length
    emitter.instruction("add QWORD PTR [rbp - 96], 1");                         // advance the subject scan cursor after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_loop_x86");                        // resume state dispatch at the saved subject cursor
    emitter.label("__rt_strip_tags_s2_quote_x86");
    emitter.instruction("cmp r15b, 34");                                        // recognize a double-quote boundary
    emitter.instruction("je __rt_strip_tags_s2_q_x86");                         // process a PHP quote only when it is not escaped
    emitter.instruction("cmp r15b, 39");                                        // recognize a single-quote boundary
    emitter.instruction("je __rt_strip_tags_s2_q_x86");                         // process a PHP quote only when it is not escaped
    emitter.instruction("cmp r15b, 108");                                       // recognize a lowercase XML suffix
    emitter.instruction("je __rt_strip_tags_s2_xml_x86");                       // validate the complete preceding XML opener
    emitter.instruction("cmp r15b, 76");                                        // recognize an uppercase XML suffix
    emitter.instruction("je __rt_strip_tags_s2_xml_x86");                       // validate the complete preceding XML opener
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s2_q_x86");
    emitter.instruction("test r14, r14");                                       // require a preceding subject byte before looking back
    emitter.instruction("je __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("cmp BYTE PTR [r12 + r14 - 1], 92");                    // require the matching tag neighbor in the adjacent markup byte
    emitter.instruction("je __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("mov rax, QWORD PTR [rbp - 136]");                      // reload the last significant PHP byte for this scan step
    emitter.instruction("cmp rax, r15");                                        // close a quote only when it matches the saved delimiter
    emitter.instruction("jne __rt_strip_tags_s2_q_set_x86");                    // remember the new significant PHP quote byte
    emitter.instruction("mov QWORD PTR [rbp - 136], 0");                        // clear the last significant PHP byte
    emitter.instruction("jmp __rt_strip_tags_s2_q_in_x86");                     // update the active PHP quote delimiter
    emitter.label("__rt_strip_tags_s2_q_set_x86");
    emitter.instruction("cmp rax, 92");                                         // recognize a backslash that escapes quoting
    emitter.instruction("je __rt_strip_tags_s2_q_in_x86");                      // update the active PHP quote delimiter
    emitter.instruction("mov QWORD PTR [rbp - 136], r15");                      // preserve the last significant PHP byte across subsequent scan operations
    emitter.label("__rt_strip_tags_s2_q_in_x86");
    emitter.instruction("test r14, r14");                                       // require a preceding subject byte before looking back
    emitter.instruction("je __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("mov rax, QWORD PTR [rbp - 112]");                      // reload the active quote delimiter for this scan step
    emitter.instruction("test rax, rax");                                       // test the active quote delimiter before the guarded scan transition
    emitter.instruction("jz __rt_strip_tags_s2_q_open_x86");                    // remember the opening PHP quote
    emitter.instruction("cmp rax, r15");                                        // close a quote only when it matches the saved delimiter
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                        // clear the active quote delimiter
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s2_q_open_x86");
    emitter.instruction("mov QWORD PTR [rbp - 112], r15");                      // preserve the active quote delimiter across subsequent scan operations
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s2_xml_x86");
    emitter.instruction("cmp r14, 4");                                          // keep a leading XML opener in PHP state using PHP's strict offset bound
    emitter.instruction("jbe __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("movzx eax, BYTE PTR [r12 + r14 - 1]");                 // read subject byte 1 before the cursor for prefix validation
    emitter.instruction("or al, 32");                                           // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp al, 109");                                         // recognize the XML prefix letter m
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("movzx eax, BYTE PTR [r12 + r14 - 2]");                 // read subject byte 2 before the cursor for prefix validation
    emitter.instruction("or al, 32");                                           // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp al, 120");                                         // recognize the XML prefix letter x
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("cmp BYTE PTR [r12 + r14 - 3], 63");                    // require a PHP-tag question mark in the adjacent markup byte
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("cmp BYTE PTR [r12 + r14 - 4], 60");                    // require an opening tag delimiter in the adjacent markup byte
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("mov QWORD PTR [rbp - 104], 1");                        // select HTML-tag handling for subsequent bytes
    emitter.instruction("mov QWORD PTR [rbp - 144], 1");                        // remember that XML-specific closing rules are active
    emitter.instruction("add QWORD PTR [rbp - 96], 1");                         // advance the subject scan cursor after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_loop_x86");                        // resume state dispatch at the saved subject cursor
}

/// Emits `<!...` handling (state 3), including comments and `<!DOCTYPE`.
fn emit_state3_x86(emitter: &mut Emitter) {
    // -- state 3: `<!` markup, comments, and DOCTYPE --
    emitter.comment("-- state 3: `<!` markup, comments, and DOCTYPE --");
    emitter.label("__rt_strip_tags_s3_x86");
    emitter.instruction("cmp r15b, 62");                                        // recognize a closing tag delimiter
    emitter.instruction("jne __rt_strip_tags_s3_quote_x86");                    // recognize declaration quotes, comments and DOCTYPE
    emitter.instruction("cmp QWORD PTR [rbp - 120], 0");                        // check the nested angle-bracket depth before accepting the next scan transition
    emitter.instruction("jle __rt_strip_tags_s3_gt_q_x86");                     // close a declaration only outside quoted contents
    emitter.instruction("sub QWORD PTR [rbp - 120], 1");                        // reduce the nested angle-bracket depth after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s3_gt_q_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 112], 0");                        // check the active quote delimiter before accepting the next scan transition
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                        // clear the active quote delimiter
    emitter.instruction("mov QWORD PTR [rbp - 104], 0");                        // clear the scanner state
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // clear the collected-tag length
    emitter.instruction("add QWORD PTR [rbp - 96], 1");                         // advance the subject scan cursor after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_loop_x86");                        // resume state dispatch at the saved subject cursor
    emitter.label("__rt_strip_tags_s3_quote_x86");
    emitter.instruction("cmp r15b, 34");                                        // recognize a double-quote boundary
    emitter.instruction("je __rt_strip_tags_s3_q_x86");                         // process an unescaped declaration quote
    emitter.instruction("cmp r15b, 39");                                        // recognize a single-quote boundary
    emitter.instruction("je __rt_strip_tags_s3_q_x86");                         // process an unescaped declaration quote
    emitter.instruction("cmp r15b, 45");                                        // recognize a comment delimiter dash
    emitter.instruction("je __rt_strip_tags_s3_dash_x86");                      // recognize the second dash of a comment opener
    emitter.instruction("cmp r15b, 69");                                        // recognize an uppercase DOCTYPE suffix
    emitter.instruction("je __rt_strip_tags_s3_doctype_x86");                   // recognize a complete case-insensitive DOCTYPE prefix
    emitter.instruction("cmp r15b, 101");                                       // recognize a lowercase DOCTYPE suffix
    emitter.instruction("je __rt_strip_tags_s3_doctype_x86");                   // recognize a complete case-insensitive DOCTYPE prefix
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s3_q_x86");
    emitter.instruction("test r14, r14");                                       // require a preceding subject byte before looking back
    emitter.instruction("je __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("cmp BYTE PTR [r12 + r14 - 1], 92");                    // require the matching tag neighbor in the adjacent markup byte
    emitter.instruction("je __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("mov rax, QWORD PTR [rbp - 112]");                      // reload the active quote delimiter for this scan step
    emitter.instruction("test rax, rax");                                       // test the active quote delimiter before the guarded scan transition
    emitter.instruction("jz __rt_strip_tags_s3_q_open_x86");                    // remember the opening declaration quote
    emitter.instruction("cmp rax, r15");                                        // close a quote only when it matches the saved delimiter
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                        // clear the active quote delimiter
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s3_q_open_x86");
    emitter.instruction("mov QWORD PTR [rbp - 112], r15");                      // preserve the active quote delimiter across subsequent scan operations
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s3_dash_x86");
    emitter.instruction("cmp r14, 2");                                          // require two preceding subject bytes before recognizing a comment delimiter
    emitter.instruction("jb __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("cmp BYTE PTR [r12 + r14 - 1], 45");                    // require a comment delimiter dash in the adjacent markup byte
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("cmp BYTE PTR [r12 + r14 - 2], 33");                    // require the matching tag neighbor in the adjacent markup byte
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("mov QWORD PTR [rbp - 104], 4");                        // select comment handling for subsequent bytes
    emitter.instruction("add QWORD PTR [rbp - 96], 1");                         // advance the subject scan cursor after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_loop_x86");                        // resume state dispatch at the saved subject cursor
    emitter.label("__rt_strip_tags_s3_doctype_x86");
    emitter.instruction("cmp r14, 7");                                          // require a complete preceding DOCTYPE marker
    emitter.instruction("jbe __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("movzx eax, BYTE PTR [r12 + r14 - 1]");                 // read subject byte 1 before the cursor for prefix validation
    emitter.instruction("or al, 32");                                           // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp al, 112");                                         // recognize the DOCTYPE prefix letter p
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("movzx eax, BYTE PTR [r12 + r14 - 2]");                 // read subject byte 2 before the cursor for prefix validation
    emitter.instruction("or al, 32");                                           // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp al, 121");                                         // recognize the DOCTYPE prefix letter y
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("movzx eax, BYTE PTR [r12 + r14 - 3]");                 // read subject byte 3 before the cursor for prefix validation
    emitter.instruction("or al, 32");                                           // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp al, 116");                                         // recognize the DOCTYPE prefix letter t
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("movzx eax, BYTE PTR [r12 + r14 - 4]");                 // read subject byte 4 before the cursor for prefix validation
    emitter.instruction("or al, 32");                                           // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp al, 99");                                          // recognize the DOCTYPE prefix letter c
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("movzx eax, BYTE PTR [r12 + r14 - 5]");                 // read subject byte 5 before the cursor for prefix validation
    emitter.instruction("or al, 32");                                           // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp al, 111");                                         // recognize the DOCTYPE prefix letter o
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("movzx eax, BYTE PTR [r12 + r14 - 6]");                 // read subject byte 6 before the cursor for prefix validation
    emitter.instruction("or al, 32");                                           // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp al, 100");                                         // recognize the DOCTYPE prefix letter d
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("mov QWORD PTR [rbp - 104], 1");                        // select HTML-tag handling for subsequent bytes
    emitter.instruction("add QWORD PTR [rbp - 96], 1");                         // advance the subject scan cursor after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_loop_x86");                        // resume state dispatch at the saved subject cursor
}

/// Emits HTML-comment handling (state 4).
fn emit_state4_x86(emitter: &mut Emitter) {
    // -- state 4: HTML comment, closed only by `-->` --
    emitter.comment("-- state 4: HTML comment, closed only by `-->` --");
    emitter.label("__rt_strip_tags_s4_x86");
    emitter.instruction("cmp r15b, 62");                                        // recognize a closing tag delimiter
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("cmp QWORD PTR [rbp - 112], 0");                        // check the active quote delimiter before accepting the next scan transition
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("cmp r14, 2");                                          // require two preceding subject bytes before recognizing a comment delimiter
    emitter.instruction("jb __rt_strip_tags_advance_x86");                      // consume this byte and continue scanning
    emitter.instruction("cmp BYTE PTR [r12 + r14 - 1], 45");                    // require a comment delimiter dash in the adjacent markup byte
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("cmp BYTE PTR [r12 + r14 - 2], 45");                    // require a comment delimiter dash in the adjacent markup byte
    emitter.instruction("jne __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                        // clear the active quote delimiter
    emitter.instruction("mov QWORD PTR [rbp - 104], 0");                        // clear the scanner state
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // clear the collected-tag length
    emitter.instruction("jmp __rt_strip_tags_advance_x86");                     // consume this byte and continue scanning
}

/// Emits local helpers shared by the x86_64 state machine.
fn emit_helpers_x86(emitter: &mut Emitter) {
    // -- consume the current subject byte before redispatch --
    emitter.label("__rt_strip_tags_advance_x86");
    emitter.instruction("add QWORD PTR [rbp - 96], 1");                         // advance the subject scan cursor after consuming its delimiter
    emitter.instruction("jmp __rt_strip_tags_loop_x86");                        // resume state dispatch at the saved subject cursor

    // -- append one collected tag byte when an allow-list is active --
    emitter.comment("-- append one collected tag byte when an allow-list is active --");
    emitter.label("__rt_strip_tags_collect_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 80], 0");                         // check the effective allow-list length before accepting the next scan transition
    emitter.instruction("je __rt_strip_tags_collect_ret_x86");                  // return after optional tag collection
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // reload the collected-tag buffer for this scan step
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // reload the collected-tag length for this scan step
    emitter.instruction("mov BYTE PTR [rdx + rcx], al");                        // append the current byte to the collected raw tag
    emitter.instruction("add rcx, 1");                                          // advance the collected-tag length after appending a raw byte
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // preserve the collected-tag length across subsequent scan operations
    emitter.label("__rt_strip_tags_collect_ret_x86");
    emitter.instruction("ret");                                                 // return after optional raw-tag collection

    // -- copy a kept allowed tag into the output --
    emitter.comment("-- copy a kept allowed tag into the output --");
    emitter.label("__rt_strip_tags_emit_tag_x86");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the collected-tag buffer for this scan step
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // reload the collected-tag length for this scan step
    emitter.instruction("mov rdi, rbx");                                        // take the live output write cursor
    emitter.instruction("xor r8d, r8d");                                        // start copying the allowed raw tag at its first byte
    emitter.label("__rt_strip_tags_emit_tag_loop_x86");
    emitter.instruction("cmp r8, rcx");                                         // stop after copying every byte of the complete raw tag
    emitter.instruction("jae __rt_strip_tags_emit_tag_done_x86");               // commit the copied tag to the output cursor
    emitter.instruction("movzx eax, BYTE PTR [rsi + r8]");                      // read the next allowed raw-tag byte
    emitter.instruction("mov BYTE PTR [rdi + r8], al");                         // copy an allowed raw tag without losing its original attributes
    emitter.instruction("add r8, 1");                                           // advance the raw-tag traversal cursor
    emitter.instruction("jmp __rt_strip_tags_emit_tag_loop_x86");               // copy the next byte of an allowed tag
    emitter.label("__rt_strip_tags_emit_tag_done_x86");
    emitter.instruction("add rbx, rcx");                                        // advance the output cursor past the complete retained tag
    emitter.instruction("ret");                                                 // return after committing the retained tag bytes

    // -- load the next subject byte, or 0 at EOF, into al --
    emitter.comment("-- load the next subject byte, or 0 at EOF, into al --");
    emitter.label("__rt_strip_tags_next_x86");
    emitter.instruction("mov rdx, r14");                                        // start from the subject cursor for bounded lookahead
    emitter.instruction("add rdx, 1");                                          // address the next subject byte for bounded lookahead
    emitter.instruction("cmp rdx, r13");                                        // keep the lookahead read inside the subject byte range
    emitter.instruction("jae __rt_strip_tags_next_eof_x86");                    // return an EOF byte without reading beyond the subject
    emitter.instruction("movzx eax, BYTE PTR [r12 + rdx]");                     // read the bounded lookahead byte
    emitter.instruction("ret");                                                 // return the bounded lookahead byte
    emitter.label("__rt_strip_tags_next_eof_x86");
    emitter.instruction("xor eax, eax");                                        // return a zero lookahead at EOF
    emitter.instruction("ret");                                                 // return the bounded lookahead byte

    // -- C-locale isspace: 0x09-0x0d or 0x20; result in eax --
    emitter.comment("-- C-locale isspace: 0x09-0x0d or 0x20; result in eax --");
    emitter.label("__rt_strip_tags_isspace_x86");
    emitter.instruction("cmp al, 9");                                           // recognize the start of PHP's C-locale whitespace range
    emitter.instruction("jb __rt_strip_tags_isspace_no_x86");                   // reject a byte outside PHP's C-locale whitespace set
    emitter.instruction("cmp al, 13");                                          // recognize the end of PHP's C-locale whitespace range
    emitter.instruction("jbe __rt_strip_tags_isspace_yes_x86");                 // accept a byte in PHP's C-locale whitespace set
    emitter.instruction("cmp al, 32");                                          // recognize a literal space in the tag or lookahead
    emitter.instruction("je __rt_strip_tags_isspace_yes_x86");                  // accept a byte in PHP's C-locale whitespace set
    emitter.label("__rt_strip_tags_isspace_no_x86");
    emitter.instruction("xor eax, eax");                                        // report a byte outside PHP's C-locale whitespace set
    emitter.instruction("ret");                                                 // return the whitespace classification to the scanner
    emitter.label("__rt_strip_tags_isspace_yes_x86");
    emitter.instruction("mov eax, 1");                                          // report a C-locale whitespace byte
    emitter.instruction("ret");                                                 // return the whitespace classification to the scanner

    // -- php_tag_find: normalize the collected tag and strstr the allow set --
    emitter.comment("-- php_tag_find: normalize the collected tag and strstr the allow set --");
    emitter.label("__rt_strip_tags_tag_find_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // reload the collected-tag length for this scan step
    emitter.instruction("test rcx, rcx");                                       // test the collected-tag length before the guarded scan transition
    emitter.instruction("jz __rt_strip_tags_tag_find_no_x86");                  // reject a tag without a complete allow-list token match
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the collected-tag buffer for this scan step
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // reload the normalized-tag buffer for this scan step
    emitter.instruction("xor r8d, r8d");                                        // clear the collected-tag scan cursor
    emitter.instruction("xor r9d, r9d");                                        // clear the normalized-token length
    emitter.instruction("xor r10d, r10d");                                      // clear the tag-name-started flag
    emitter.label("__rt_strip_tags_tag_find_norm_x86");
    emitter.instruction("cmp r8, rcx");                                         // stop normalization once every collected-tag byte is consumed
    emitter.instruction("jae __rt_strip_tags_tag_find_end_x86");                // terminate the normalized token before searching the allow-list
    emitter.instruction("movzx eax, BYTE PTR [rsi + r8]");                      // read the next collected-tag byte for normalization
    emitter.instruction("cmp al, 65");                                          // recognize the first uppercase ASCII letter
    emitter.instruction("jb __rt_strip_tags_tag_find_lc_done_x86");             // preserve a tag byte that needs no ASCII case folding
    emitter.instruction("cmp al, 90");                                          // recognize the last uppercase ASCII letter
    emitter.instruction("ja __rt_strip_tags_tag_find_lc_done_x86");             // preserve a tag byte that needs no ASCII case folding
    emitter.instruction("or al, 32");                                           // fold an ASCII uppercase tag-name letter to lowercase
    emitter.label("__rt_strip_tags_tag_find_lc_done_x86");
    emitter.instruction("cmp al, 60");                                          // recognize an opening tag delimiter
    emitter.instruction("jne __rt_strip_tags_tag_find_gt_x86");                 // recognize the end of the collected tag name
    emitter.instruction("mov BYTE PTR [rdi + r9], al");                         // append a delimiter or name byte to the normalized token
    emitter.instruction("add r9, 1");                                           // advance the normalized-token length after retaining one byte
    emitter.instruction("jmp __rt_strip_tags_tag_find_next_x86");               // advance the source-tag normalization cursor
    emitter.label("__rt_strip_tags_tag_find_gt_x86");
    emitter.instruction("cmp al, 62");                                          // recognize a closing tag delimiter
    emitter.instruction("je __rt_strip_tags_tag_find_end_x86");                 // terminate the normalized token before searching the allow-list
    emitter.instruction("cmp al, 9");                                           // recognize the start of PHP's C-locale whitespace range
    emitter.instruction("jb __rt_strip_tags_tag_find_char_x86");                // process a non-whitespace tag-name byte
    emitter.instruction("cmp al, 13");                                          // recognize the end of PHP's C-locale whitespace range
    emitter.instruction("jbe __rt_strip_tags_tag_find_space_x86");              // ignore leading spaces and stop after the tag name
    emitter.instruction("cmp al, 32");                                          // recognize a literal space in the tag or lookahead
    emitter.instruction("jne __rt_strip_tags_tag_find_char_x86");               // process a non-whitespace tag-name byte
    emitter.label("__rt_strip_tags_tag_find_space_x86");
    emitter.instruction("cmp r10, 1");                                          // stop a name at whitespace only after its first name byte
    emitter.instruction("je __rt_strip_tags_tag_find_end_x86");                 // terminate the normalized token before searching the allow-list
    emitter.instruction("jmp __rt_strip_tags_tag_find_next_x86");               // advance the source-tag normalization cursor
    emitter.label("__rt_strip_tags_tag_find_char_x86");
    emitter.instruction("test r10, r10");                                       // check whether the normalized tag name has started
    emitter.instruction("jnz __rt_strip_tags_tag_find_slash_x86");              // distinguish delimiter slashes from literal name bytes
    emitter.instruction("mov r10, 1");                                          // remember that the tag-name body has begun
    emitter.label("__rt_strip_tags_tag_find_slash_x86");
    emitter.instruction("cmp al, 47");                                          // recognize a closing or self-closing tag slash
    emitter.instruction("jne __rt_strip_tags_tag_find_keep_x86");               // retain a slash that is not a tag delimiter
    emitter.instruction("test r8, r8");                                         // keep a first tag slash literal until a preceding opener exists
    emitter.instruction("je __rt_strip_tags_tag_find_keep_x86");                // retain a slash that is not a tag delimiter
    emitter.instruction("cmp BYTE PTR [rsi + r8 - 1], 60");                     // require an opening tag delimiter in the adjacent markup byte
    emitter.instruction("je __rt_strip_tags_tag_find_next_x86");                // advance the source-tag normalization cursor
    emitter.instruction("mov rdx, r8");                                         // address the current tag byte before checking a following slash neighbor
    emitter.instruction("add rdx, 1");                                          // address the next tag byte for self-closing slash validation
    emitter.instruction("cmp rdx, rcx");                                        // keep the following slash-neighbor read within the collected tag
    emitter.instruction("jae __rt_strip_tags_tag_find_keep_x86");               // retain a slash that is not a tag delimiter
    emitter.instruction("cmp BYTE PTR [rsi + rdx], 62");                        // require a closing tag delimiter in the adjacent markup byte
    emitter.instruction("je __rt_strip_tags_tag_find_next_x86");                // advance the source-tag normalization cursor
    emitter.label("__rt_strip_tags_tag_find_keep_x86");
    emitter.instruction("mov BYTE PTR [rdi + r9], al");                         // append a delimiter or name byte to the normalized token
    emitter.instruction("add r9, 1");                                           // advance the normalized-token length after retaining one byte
    emitter.label("__rt_strip_tags_tag_find_next_x86");
    emitter.instruction("add r8, 1");                                           // advance the raw-tag traversal cursor
    emitter.instruction("jmp __rt_strip_tags_tag_find_norm_x86");               // normalize the next collected-tag byte
    emitter.label("__rt_strip_tags_tag_find_end_x86");
    emitter.instruction("mov BYTE PTR [rdi + r9], 62");                         // terminate the normalized token with a closing angle bracket
    emitter.instruction("add r9, 1");                                           // advance the normalized-token length after retaining one byte
    emitter.instruction("mov r8, QWORD PTR [rbp - 72]");                        // reload the lowercased allow-list buffer for this scan step
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the effective allow-list length for this scan step
    emitter.instruction("xor r11d, r11d");                                      // begin allow-list matching at its first candidate
    emitter.label("__rt_strip_tags_tag_find_scan_x86");
    emitter.instruction("mov rax, r10");                                        // start computing allow bytes remaining after this candidate
    emitter.instruction("sub rax, r11");                                        // exclude allow bytes preceding the current candidate offset
    emitter.instruction("cmp rax, r9");                                         // reject candidates too short for the entire normalized token
    emitter.instruction("jb __rt_strip_tags_tag_find_no_x86");                  // reject a tag without a complete allow-list token match
    emitter.instruction("xor ecx, ecx");                                        // start each candidate with zero matched token bytes
    emitter.label("__rt_strip_tags_tag_find_cmp_x86");
    emitter.instruction("cmp rcx, r9");                                         // recognize a match only after every normalized byte was compared
    emitter.instruction("jae __rt_strip_tags_tag_find_yes_x86");                // accept a tag only after its whole normalized token matches
    emitter.instruction("movzx eax, BYTE PTR [r8 + r11]");                      // read the current allow-list candidate byte
    emitter.instruction("movzx edx, BYTE PTR [rdi + rcx]");                     // read the corresponding normalized tag-token byte
    emitter.instruction("cmp al, dl");                                          // require exact byte equality for the complete normalized token
    emitter.instruction("jne __rt_strip_tags_tag_find_scan_next_x86");          // restart at the next candidate after a mismatch
    emitter.instruction("add rcx, 1");                                          // advance the matched token prefix by one byte
    emitter.instruction("add r11, 1");                                          // advance the allow-list candidate cursor
    emitter.instruction("jmp __rt_strip_tags_tag_find_cmp_x86");                // compare the complete normalized token at this candidate
    emitter.label("__rt_strip_tags_tag_find_scan_next_x86");
    emitter.instruction("sub r11, rcx");                                        // rewind to the candidate start after a partial mismatch
    emitter.instruction("add r11, 1");                                          // advance the allow-list candidate cursor
    emitter.instruction("jmp __rt_strip_tags_tag_find_scan_x86");               // try the next bounded allow-list match
    emitter.label("__rt_strip_tags_tag_find_yes_x86");
    emitter.instruction("mov eax, 1");                                          // report a complete allow-list token match
    emitter.instruction("ret");                                                 // return whether the complete tag token matched
    emitter.label("__rt_strip_tags_tag_find_no_x86");
    emitter.instruction("xor eax, eax");                                        // report that no complete allow-list token matched
    emitter.instruction("ret");                                                 // return whether the complete tag token matched
}
