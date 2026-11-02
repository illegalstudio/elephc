//! Purpose:
//! AArch64 `__rt_strip_tags` state machine for PHP 8.5 `strip_tags()`.
//!
//! Called from:
//! - `super::emit_strip_tags()` on macos-aarch64, ios-arm64, ios-sim-arm64,
//!   and linux-aarch64.
//!
//! Key details:
//! - ABI: `x1`/`x2` = subject pointer/length, `x3`/`x4` = allow pointer/length.
//! - Result: `x1`/`x2` = stripped pointer/length, published through
//!   `__rt_concat_publish`.
//! - Zero allow length is treated as no allow-list.

use crate::codegen_support::emit::Emitter;


/// Emits the AArch64 `__rt_strip_tags` helper.
pub(super) fn emit_strip_tags_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    // --- runtime: strip_tags ---
    emitter.comment("--- runtime: strip_tags ---");
    emitter.label_global("__rt_strip_tags");

    // -- preserve the caller and initialize per-call scanner state --
    emitter.instruction("sub sp, sp, #224");                                    // reserve spill space for the scanner state and caller registers
    emitter.instruction("stp x29, x30, [sp, #208]");                            // preserve the caller frame and return linkage
    emitter.instruction("add x29, sp, #208");                                   // establish a stable base for scanner state slots
    emitter.instruction("stp x19, x20, [x29, #-200]");                          // preserve the caller's callee-saved subject registers
    emitter.instruction("stp x21, x22, [x29, #-184]");                          // preserve the caller's callee-saved cursor registers
    emitter.instruction("str x23, [x29, #-168]");                               // preserve the remaining callee-saved scanner register
    emitter.instruction("str x1, [x29, #-8]");                                  // preserve the subject pointer
    emitter.instruction("str x2, [x29, #-16]");                                 // preserve the subject byte length
    emitter.instruction("str x3, [x29, #-24]");                                 // preserve the allow-list pointer
    emitter.instruction("str x4, [x29, #-32]");                                 // preserve the original allow-list length

    // -- reserve output, and allow/tag/norm scratch when an allow-list is present --
    emitter.comment("-- reserve output, and allow/tag/norm scratch when an allow-list is present --");
    emitter.instruction("mov x0, x2");                                          // request at least the subject length for retained output
    emitter.instruction("cbz x4, __rt_strip_tags_reserve");                     // reserve output without unnecessary allow-list scratch
    emitter.instruction("adds x9, x2, x2");                                     // include output and the first subject-sized tag workspace
    emitter.instruction("b.cs __rt_strip_tags_overflow");                       // reject a wrapped reservation size before allocating
    emitter.instruction("adds x9, x9, x2");                                     // include the second subject-sized tag workspace
    emitter.instruction("b.cs __rt_strip_tags_overflow");                       // reject a wrapped reservation size before allocating
    emitter.instruction("adds x9, x9, x4");                                     // include the complete allow-list case-folding workspace
    emitter.instruction("b.cs __rt_strip_tags_overflow");                       // reject a wrapped reservation size before allocating
    emitter.instruction("adds x9, x9, #4");                                     // reserve both tag terminators without allowing size wrap
    emitter.instruction("b.cs __rt_strip_tags_overflow");                       // reject a wrapped reservation size before allocating
    emitter.instruction("mov x0, x9");                                          // pass the checked total workspace size to the reservation helper
    emitter.label("__rt_strip_tags_reserve");
    emitter.instruction("bl __rt_concat_reserve");                              // reserve bounded output and allow-list workspace
    super::emit_mark_reserved_result_temporary(emitter);
    emitter.instruction("str x0, [x29, #-40]");                                 // preserve the output reservation start
    emitter.instruction("str x0, [x29, #-48]");                                 // preserve the output write cursor
    emitter.instruction("str xzr, [x29, #-56]");                                // clear the collected-tag buffer
    emitter.instruction("str xzr, [x29, #-64]");                                // clear the collected-tag length
    emitter.instruction("str xzr, [x29, #-72]");                                // clear the lowercased allow-list buffer
    emitter.instruction("str xzr, [x29, #-80]");                                // clear the effective allow-list length
    emitter.instruction("str xzr, [x29, #-88]");                                // clear the normalized-tag buffer
    emitter.instruction("ldr x9, [x29, #-32]");                                 // reload the original allow-list length for this scan step
    emitter.instruction("cbz x9, __rt_strip_tags_init");                        // begin input scanning after normalizing the allow-list

    // -- partition the reservation: out | allow_lc | tag | norm --
    emitter.comment("-- partition the reservation: out | allow_lc | tag | norm --");
    emitter.instruction("ldr x10, [x29, #-40]");                                // reload the output reservation start for this scan step
    emitter.instruction("ldr x11, [x29, #-16]");                                // reload the subject byte length for this scan step
    emitter.instruction("add x10, x10, x11");                                   // place allow-list workspace after the possible retained output
    emitter.instruction("str x10, [x29, #-72]");                                // preserve the lowercased allow-list buffer
    emitter.instruction("str x9, [x29, #-80]");                                 // preserve the effective allow-list length
    emitter.instruction("add x10, x10, x9");                                    // place collected-tag workspace after the normalized allow-list
    emitter.instruction("str x10, [x29, #-56]");                                // preserve the collected-tag buffer
    emitter.instruction("add x12, x11, #2");                                    // reserve the subject-sized tag plus its delimiters
    emitter.instruction("add x10, x10, x12");                                   // place normalized-tag workspace after the collected tag
    emitter.instruction("str x10, [x29, #-88]");                                // preserve the normalized-tag buffer
    emitter.instruction("ldr x1, [x29, #-24]");                                 // reload the allow-list pointer for this scan step
    emitter.instruction("ldr x2, [x29, #-72]");                                 // reload the lowercased allow-list buffer for this scan step
    emitter.instruction("mov x3, xzr");                                         // begin allow-list normalization at its first byte
    emitter.label("__rt_strip_tags_lc_allow");
    emitter.instruction("cmp x3, x9");                                          // stop allow-list normalization at its declared byte length
    emitter.instruction("b.hs __rt_strip_tags_init");                           // begin input scanning after normalizing the allow-list
    emitter.instruction("ldrb w4, [x1, x3]");                                   // read the next allow-list byte without leaving its length bound
    // PHP's strstr-based allow lookup ends at the first NUL, not the PHP string length.
    emitter.instruction("cbz w4, __rt_strip_tags_allow_end");                   // stop allow-list matching at PHP's first embedded NUL
    emitter.instruction("cmp w4, #65");                                         // recognize the first uppercase ASCII letter
    emitter.instruction("b.lo __rt_strip_tags_lc_store");                       // preserve allow bytes that need no ASCII case folding
    emitter.instruction("cmp w4, #90");                                         // recognize the last uppercase ASCII letter
    emitter.instruction("b.hi __rt_strip_tags_lc_store");                       // preserve allow bytes that need no ASCII case folding
    emitter.instruction("orr w4, w4, #32");                                     // fold an ASCII uppercase allow-list letter to lowercase
    emitter.label("__rt_strip_tags_lc_store");
    emitter.instruction("strb w4, [x2, x3]");                                   // store one case-folded allow-list byte
    emitter.instruction("add x3, x3, #1");                                      // advance the allow-list normalization cursor
    emitter.instruction("b __rt_strip_tags_lc_allow");                          // normalize the next allow-list byte

    emitter.label("__rt_strip_tags_allow_end");
    emitter.instruction("str x3, [x29, #-80]");                                 // preserve the effective allow-list length
    emitter.label("__rt_strip_tags_init");
    emitter.instruction("str xzr, [x29, #-96]");                                // clear the subject scan cursor
    emitter.instruction("str xzr, [x29, #-104]");                               // clear the scanner state
    emitter.instruction("str xzr, [x29, #-112]");                               // clear the active quote delimiter
    emitter.instruction("str xzr, [x29, #-120]");                               // clear the nested angle-bracket depth
    emitter.instruction("str xzr, [x29, #-128]");                               // clear the PHP parenthesis depth
    emitter.instruction("str xzr, [x29, #-136]");                               // clear the last significant PHP byte
    emitter.instruction("str xzr, [x29, #-144]");                               // clear the XML-prologue flag
    emitter.instruction("ldr x19, [x29, #-8]");                                 // reload the subject pointer for this scan step
    emitter.instruction("ldr x20, [x29, #-16]");                                // reload the subject byte length for this scan step

    emitter.label("__rt_strip_tags_loop");
    emitter.instruction("ldr x21, [x29, #-96]");                                // reload the subject scan cursor for this scan step
    emitter.instruction("cmp x21, x20");                                        // stop scanning before the subject cursor reaches EOF
    emitter.instruction("b.hs __rt_strip_tags_done");                           // publish the bytes retained after the subject is exhausted
    emitter.instruction("ldrb w22, [x19, x21]");                                // load the current subject byte for state dispatch
    emitter.instruction("ldr x9, [x29, #-104]");                                // reload the scanner state for this scan step
    emitter.instruction("cbz x9, __rt_strip_tags_s0");                          // handle ordinary output bytes
    emitter.instruction("cmp x9, #1");                                          // select HTML-tag handling for the current byte
    emitter.instruction("b.eq __rt_strip_tags_s1");                             // handle an HTML or XML tag
    emitter.instruction("cmp x9, #2");                                          // select PHP-tag handling for the current byte
    emitter.instruction("b.eq __rt_strip_tags_s2");                             // handle stripped PHP-tag contents
    emitter.instruction("cmp x9, #3");                                          // select declaration handling for the current byte
    emitter.instruction("b.eq __rt_strip_tags_s3");                             // handle markup declarations and comment openers
    emitter.instruction("b __rt_strip_tags_s4");                                // handle stripped HTML-comment contents

    emit_state0_aarch64(emitter);
    emit_state1_aarch64(emitter);
    emit_state2_aarch64(emitter);
    emit_state3_aarch64(emitter);
    emit_state4_aarch64(emitter);
    emit_helpers_aarch64(emitter);

    // -- publish the retained output and restore the caller --
    emitter.label("__rt_strip_tags_done");
    emitter.instruction("ldr x1, [x29, #-40]");                                 // reload the output reservation start for this scan step
    emitter.instruction("ldr x2, [x29, #-48]");                                 // reload the output write cursor for this scan step
    emitter.instruction("sub x2, x2, x1");                                      // derive retained output length from the final write cursor
    emitter.instruction("bl __rt_concat_publish");                              // publish only the retained output bytes
    emitter.instruction("bl __rt_str_persist");                                 // return independent storage while adopting a sole heap reservation in place
    emitter.instruction("ldp x19, x20, [x29, #-200]");                          // restore the caller's preserved subject registers
    emitter.instruction("ldp x21, x22, [x29, #-184]");                          // restore the caller's preserved cursor registers
    emitter.instruction("ldr x23, [x29, #-168]");                               // restore the remaining callee-saved scanner register
    emitter.instruction("ldp x29, x30, [sp, #208]");                            // restore the caller frame and return linkage
    emitter.instruction("add sp, sp, #224");                                    // release all scanner state and register spill slots
    emitter.instruction("ret");                                                 // return the stripped byte string with caller state restored

    // -- refuse impossible output or workspace sizes --
    emitter.label("__rt_strip_tags_overflow");
    emitter.instruction("b __rt_alloc_overflow");                               // abort before a wrapped reservation can under-size the output
}

/// Emits output-state handling (state 0).
fn emit_state0_aarch64(emitter: &mut Emitter) {
    // -- state 0: copy bytes until a real tag opener --
    emitter.comment("-- state 0: copy bytes until a real tag opener --");
    emitter.label("__rt_strip_tags_s0");
    emitter.instruction("cbz w22, __rt_strip_tags_advance");                    // consume this byte and continue scanning
    emitter.instruction("cmp w22, #60");                                        // recognize an opening tag delimiter
    emitter.instruction("b.ne __rt_strip_tags_s0_gt");                          // check a non-opening byte for a closing angle bracket
    emitter.instruction("ldr x9, [x29, #-112]");                                // reload the active quote delimiter for this scan step
    emitter.instruction("cbnz x9, __rt_strip_tags_advance");                    // consume this byte and continue scanning
    emitter.instruction("bl __rt_strip_tags_next");                             // peek ahead without reading beyond the subject
    emitter.instruction("bl __rt_strip_tags_isspace");                          // classify the lookahead using PHP's C-locale whitespace rules
    emitter.instruction("cbz w0, __rt_strip_tags_s0_open");                     // recognize markup only when the opener is not followed by whitespace
    emitter.instruction("mov w0, #60");                                         // prepare the opening tag delimiter for the scanner
    emitter.instruction("bl __rt_strip_tags_putc");                             // append one retained byte to the output
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s0_open");
    emitter.instruction("mov x9, #60");                                         // prepare the opening tag delimiter for the scanner
    emitter.instruction("str x9, [x29, #-136]");                                // preserve the last significant PHP byte
    emitter.instruction("mov x9, #1");                                          // select HTML-tag state for subsequent subject bytes
    emitter.instruction("str x9, [x29, #-104]");                                // preserve the scanner state
    emitter.instruction("ldr x9, [x29, #-80]");                                 // reload the effective allow-list length for this scan step
    emitter.instruction("cbz x9, __rt_strip_tags_s0_open_adv");                 // skip collection when no allow-list is active
    emitter.instruction("str xzr, [x29, #-64]");                                // clear the collected-tag length
    emitter.instruction("mov w0, #60");                                         // prepare the opening tag delimiter for the scanner
    emitter.instruction("bl __rt_strip_tags_collect");                          // retain this raw tag byte for possible allow-list matching
    emitter.label("__rt_strip_tags_s0_open_adv");
    emitter.instruction("add x21, x21, #1");                                    // consume the current subject byte exactly once
    emitter.instruction("str x21, [x29, #-96]");                                // preserve the subject scan cursor
    emitter.instruction("b __rt_strip_tags_loop");                              // resume state dispatch at the saved subject cursor
    emitter.label("__rt_strip_tags_s0_gt");
    emitter.instruction("cmp w22, #62");                                        // recognize a closing tag delimiter
    emitter.instruction("b.ne __rt_strip_tags_s0_copy");                        // retain the ordinary text byte in the output
    emitter.instruction("ldr x9, [x29, #-120]");                                // reload the nested angle-bracket depth for this scan step
    emitter.instruction("cmp x9, #0");                                          // check whether the nested angle-bracket depth still blocks the closer
    emitter.instruction("b.le __rt_strip_tags_s0_gt_q");                        // check quoting after nested angle brackets are exhausted
    emitter.instruction("sub x9, x9, #1");                                      // reduce the nested angle-bracket depth after processing one delimiter
    emitter.instruction("str x9, [x29, #-120]");                                // preserve the nested angle-bracket depth
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s0_gt_q");
    emitter.instruction("ldr x9, [x29, #-112]");                                // reload the active quote delimiter for this scan step
    emitter.instruction("cbnz x9, __rt_strip_tags_advance");                    // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s0_copy");
    emitter.instruction("mov w0, w22");                                         // forward the current subject byte to output or tag collection
    emitter.instruction("bl __rt_strip_tags_putc");                             // append one retained byte to the output
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
}

/// Emits HTML-tag-state handling (state 1).
fn emit_state1_aarch64(emitter: &mut Emitter) {
    // -- state 1: inside a normal HTML/XML tag --
    emitter.comment("-- state 1: inside a normal HTML/XML tag --");
    emitter.label("__rt_strip_tags_s1");
    emitter.instruction("cbz w22, __rt_strip_tags_advance");                    // consume this byte and continue scanning
    emitter.instruction("cmp w22, #60");                                        // recognize an opening tag delimiter
    emitter.instruction("b.ne __rt_strip_tags_s1_gt");                          // check whether this tag byte is a closer
    emitter.instruction("ldr x9, [x29, #-112]");                                // reload the active quote delimiter for this scan step
    emitter.instruction("cbnz x9, __rt_strip_tags_advance");                    // consume this byte and continue scanning
    emitter.instruction("bl __rt_strip_tags_next");                             // peek ahead without reading beyond the subject
    emitter.instruction("bl __rt_strip_tags_isspace");                          // classify the lookahead using PHP's C-locale whitespace rules
    emitter.instruction("cbz w0, __rt_strip_tags_s1_depth");                    // track an unquoted nested angle bracket
    emitter.instruction("mov w0, w22");                                         // forward the current subject byte to output or tag collection
    emitter.instruction("bl __rt_strip_tags_collect");                          // retain this raw tag byte for possible allow-list matching
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s1_depth");
    emitter.instruction("ldr x9, [x29, #-120]");                                // reload the nested angle-bracket depth for this scan step
    emitter.instruction("add x9, x9, #1");                                      // advance the matched-prefix length within this candidate
    emitter.instruction("str x9, [x29, #-120]");                                // preserve the nested angle-bracket depth
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s1_gt");
    emitter.instruction("cmp w22, #62");                                        // recognize a closing tag delimiter
    emitter.instruction("b.ne __rt_strip_tags_s1_quote");                       // recognize quotes and special tag prefixes
    emitter.instruction("ldr x9, [x29, #-120]");                                // reload the nested angle-bracket depth for this scan step
    emitter.instruction("cmp x9, #0");                                          // check whether the nested angle-bracket depth still blocks the closer
    emitter.instruction("b.le __rt_strip_tags_s1_gt_q");                        // check quote and XML rules before accepting the tag closer
    emitter.instruction("sub x9, x9, #1");                                      // reduce the nested angle-bracket depth after processing one delimiter
    emitter.instruction("str x9, [x29, #-120]");                                // preserve the nested angle-bracket depth
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s1_gt_q");
    emitter.instruction("ldr x9, [x29, #-112]");                                // reload the active quote delimiter for this scan step
    emitter.instruction("cbnz x9, __rt_strip_tags_advance");                    // consume this byte and continue scanning
    emitter.instruction("ldr x9, [x29, #-144]");                                // reload the XML-prologue flag for this scan step
    emitter.instruction("cbz x9, __rt_strip_tags_s1_close");                    // finish a tag whose closing delimiter is permitted
    emitter.instruction("cbz x21, __rt_strip_tags_s1_close");                   // finish a tag whose closing delimiter is permitted
    emitter.instruction("sub x10, x21, #1");                                    // address subject byte 1 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("cmp w9, #45");                                         // recognize a comment delimiter dash
    emitter.instruction("b.eq __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s1_close");
    emitter.instruction("mov x9, #62");                                         // prepare the closing tag delimiter for the scanner
    emitter.instruction("str x9, [x29, #-136]");                                // preserve the last significant PHP byte
    emitter.instruction("str xzr, [x29, #-112]");                               // clear the active quote delimiter
    emitter.instruction("str xzr, [x29, #-104]");                               // clear the scanner state
    emitter.instruction("str xzr, [x29, #-144]");                               // clear the XML-prologue flag
    emitter.instruction("ldr x9, [x29, #-80]");                                 // reload the effective allow-list length for this scan step
    emitter.instruction("cbz x9, __rt_strip_tags_s1_close_adv");                // continue scanning when allow-list matching is unnecessary
    emitter.instruction("mov w0, #62");                                         // prepare the closing tag delimiter for the scanner
    emitter.instruction("bl __rt_strip_tags_collect");                          // retain this raw tag byte for possible allow-list matching
    emitter.instruction("bl __rt_strip_tags_tag_find");                         // decide whether the complete normalized tag token is allowed
    emitter.instruction("cbz w0, __rt_strip_tags_s1_clear_tag");                // discard a buffered tag whose token is not allowed
    emitter.instruction("bl __rt_strip_tags_emit_tag");                         // copy the allowed tag, including its original attributes
    emitter.label("__rt_strip_tags_s1_clear_tag");
    emitter.instruction("str xzr, [x29, #-64]");                                // clear the collected-tag length
    emitter.label("__rt_strip_tags_s1_close_adv");
    emitter.instruction("add x21, x21, #1");                                    // consume the current subject byte exactly once
    emitter.instruction("str x21, [x29, #-96]");                                // preserve the subject scan cursor
    emitter.instruction("b __rt_strip_tags_loop");                              // resume state dispatch at the saved subject cursor
    emitter.label("__rt_strip_tags_s1_quote");
    emitter.instruction("cmp w22, #34");                                        // recognize a double-quote boundary
    emitter.instruction("b.eq __rt_strip_tags_s1_toggle_q");                    // update the attribute quote delimiter
    emitter.instruction("cmp w22, #39");                                        // recognize a single-quote boundary
    emitter.instruction("b.eq __rt_strip_tags_s1_toggle_q");                    // update the attribute quote delimiter
    emitter.instruction("cmp w22, #33");                                        // recognize a declaration marker after the opener
    emitter.instruction("b.eq __rt_strip_tags_s1_bang");                        // recognize a markup declaration after the tag opener
    emitter.instruction("cmp w22, #63");                                        // recognize a PHP-tag question mark
    emitter.instruction("b.eq __rt_strip_tags_s1_quest");                       // recognize PHP markup after the tag opener
    emitter.instruction("mov w0, w22");                                         // forward the current subject byte to output or tag collection
    emitter.instruction("bl __rt_strip_tags_collect");                          // retain this raw tag byte for possible allow-list matching
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s1_toggle_q");
    emitter.instruction("cbz x21, __rt_strip_tags_s1_q_collect");               // preserve literal quotes in the collected tag
    emitter.instruction("ldr x9, [x29, #-112]");                                // reload the active quote delimiter for this scan step
    emitter.instruction("cbz x9, __rt_strip_tags_s1_q_set");                    // remember the quote that opens this attribute
    emitter.instruction("cmp x9, x22");                                         // close the quote only when it matches the saved delimiter
    emitter.instruction("b.ne __rt_strip_tags_s1_q_collect");                   // preserve literal quotes in the collected tag
    emitter.instruction("str xzr, [x29, #-112]");                               // clear the active quote delimiter
    emitter.instruction("b __rt_strip_tags_s1_q_collect");                      // preserve literal quotes in the collected tag
    emitter.label("__rt_strip_tags_s1_q_set");
    emitter.instruction("str x22, [x29, #-112]");                               // preserve the active quote delimiter
    emitter.label("__rt_strip_tags_s1_q_collect");
    emitter.instruction("mov w0, w22");                                         // forward the current subject byte to output or tag collection
    emitter.instruction("bl __rt_strip_tags_collect");                          // retain this raw tag byte for possible allow-list matching
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s1_bang");
    emitter.instruction("cbz x21, __rt_strip_tags_s1_bang_col");                // collect an exclamation mark that does not open a declaration
    emitter.instruction("sub x10, x21, #1");                                    // address subject byte 1 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("cmp w9, #60");                                         // recognize an opening tag delimiter
    emitter.instruction("b.ne __rt_strip_tags_s1_bang_col");                    // collect an exclamation mark that does not open a declaration
    emitter.instruction("mov x9, #3");                                          // select declaration state for subsequent subject bytes
    emitter.instruction("str x9, [x29, #-104]");                                // preserve the scanner state
    emitter.instruction("str x22, [x29, #-136]");                               // preserve the last significant PHP byte
    emitter.instruction("add x21, x21, #1");                                    // consume the current subject byte exactly once
    emitter.instruction("str x21, [x29, #-96]");                                // preserve the subject scan cursor
    emitter.instruction("b __rt_strip_tags_loop");                              // resume state dispatch at the saved subject cursor
    emitter.label("__rt_strip_tags_s1_bang_col");
    emitter.instruction("mov w0, w22");                                         // forward the current subject byte to output or tag collection
    emitter.instruction("bl __rt_strip_tags_collect");                          // retain this raw tag byte for possible allow-list matching
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s1_quest");
    emitter.instruction("cbz x21, __rt_strip_tags_s1_quest_col");               // collect a question mark that does not open PHP markup
    emitter.instruction("sub x10, x21, #1");                                    // address subject byte 1 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("cmp w9, #60");                                         // recognize an opening tag delimiter
    emitter.instruction("b.ne __rt_strip_tags_s1_quest_col");                   // collect a question mark that does not open PHP markup
    emitter.instruction("str xzr, [x29, #-128]");                               // clear the PHP parenthesis depth
    emitter.instruction("mov x9, #2");                                          // select PHP-tag state for subsequent subject bytes
    emitter.instruction("str x9, [x29, #-104]");                                // preserve the scanner state
    emitter.instruction("add x21, x21, #1");                                    // consume the current subject byte exactly once
    emitter.instruction("str x21, [x29, #-96]");                                // preserve the subject scan cursor
    emitter.instruction("b __rt_strip_tags_loop");                              // resume state dispatch at the saved subject cursor
    emitter.label("__rt_strip_tags_s1_quest_col");
    emitter.instruction("mov w0, w22");                                         // forward the current subject byte to output or tag collection
    emitter.instruction("bl __rt_strip_tags_collect");                          // retain this raw tag byte for possible allow-list matching
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
}

/// Emits PHP-tag-state handling (state 2).
fn emit_state2_aarch64(emitter: &mut Emitter) {
    // -- state 2: inside a PHP tag, unless `<?xml` returns to HTML --
    emitter.comment("-- state 2: inside a PHP tag, unless `<?xml` returns to HTML --");
    emitter.label("__rt_strip_tags_s2");
    emitter.instruction("cmp w22, #40");                                        // recognize an opening PHP parenthesis
    emitter.instruction("b.ne __rt_strip_tags_s2_rparen");                      // check whether this PHP byte closes a parenthesis
    emitter.instruction("ldr x9, [x29, #-136]");                                // reload the last significant PHP byte for this scan step
    emitter.instruction("cmp x9, #34");                                         // recognize a double-quote boundary
    emitter.instruction("b.eq __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("cmp x9, #39");                                         // recognize a single-quote boundary
    emitter.instruction("b.eq __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("mov x9, #40");                                         // prepare the opening PHP parenthesis for the scanner
    emitter.instruction("str x9, [x29, #-136]");                                // preserve the last significant PHP byte
    emitter.instruction("ldr x9, [x29, #-128]");                                // reload the PHP parenthesis depth for this scan step
    emitter.instruction("add x9, x9, #1");                                      // advance the matched-prefix length within this candidate
    emitter.instruction("str x9, [x29, #-128]");                                // preserve the PHP parenthesis depth
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s2_rparen");
    emitter.instruction("cmp w22, #41");                                        // recognize a closing PHP parenthesis
    emitter.instruction("b.ne __rt_strip_tags_s2_gt");                          // check whether this PHP byte can terminate markup
    emitter.instruction("ldr x9, [x29, #-136]");                                // reload the last significant PHP byte for this scan step
    emitter.instruction("cmp x9, #34");                                         // recognize a double-quote boundary
    emitter.instruction("b.eq __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("cmp x9, #39");                                         // recognize a single-quote boundary
    emitter.instruction("b.eq __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("mov x9, #41");                                         // prepare the closing PHP parenthesis for the scanner
    emitter.instruction("str x9, [x29, #-136]");                                // preserve the last significant PHP byte
    emitter.instruction("ldr x9, [x29, #-128]");                                // reload the PHP parenthesis depth for this scan step
    emitter.instruction("sub x9, x9, #1");                                      // reduce the PHP parenthesis depth after processing one delimiter
    emitter.instruction("str x9, [x29, #-128]");                                // preserve the PHP parenthesis depth
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s2_gt");
    emitter.instruction("cmp w22, #62");                                        // recognize a closing tag delimiter
    emitter.instruction("b.ne __rt_strip_tags_s2_quote");                       // track PHP quotes and recognize the XML exception
    emitter.instruction("ldr x9, [x29, #-120]");                                // reload the nested angle-bracket depth for this scan step
    emitter.instruction("cmp x9, #0");                                          // check whether the nested angle-bracket depth still blocks the closer
    emitter.instruction("b.le __rt_strip_tags_s2_gt_q");                        // check PHP quotes after nested angle brackets are exhausted
    emitter.instruction("sub x9, x9, #1");                                      // reduce the nested angle-bracket depth after processing one delimiter
    emitter.instruction("str x9, [x29, #-120]");                                // preserve the nested angle-bracket depth
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s2_gt_q");
    emitter.instruction("ldr x9, [x29, #-112]");                                // reload the active quote delimiter for this scan step
    emitter.instruction("cbnz x9, __rt_strip_tags_advance");                    // consume this byte and continue scanning
    emitter.instruction("ldr x9, [x29, #-128]");                                // reload the PHP parenthesis depth for this scan step
    emitter.instruction("cbnz x9, __rt_strip_tags_advance");                    // consume this byte and continue scanning
    emitter.instruction("ldr x9, [x29, #-136]");                                // reload the last significant PHP byte for this scan step
    emitter.instruction("cmp x9, #34");                                         // recognize a double-quote boundary
    emitter.instruction("b.eq __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("cbz x21, __rt_strip_tags_advance");                    // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #1");                                    // address subject byte 1 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("cmp w9, #63");                                         // recognize a PHP-tag question mark
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("str xzr, [x29, #-112]");                               // clear the active quote delimiter
    emitter.instruction("str xzr, [x29, #-104]");                               // clear the scanner state
    emitter.instruction("str xzr, [x29, #-64]");                                // clear the collected-tag length
    emitter.instruction("add x21, x21, #1");                                    // consume the current subject byte exactly once
    emitter.instruction("str x21, [x29, #-96]");                                // preserve the subject scan cursor
    emitter.instruction("b __rt_strip_tags_loop");                              // resume state dispatch at the saved subject cursor
    emitter.label("__rt_strip_tags_s2_quote");
    emitter.instruction("cmp w22, #34");                                        // recognize a double-quote boundary
    emitter.instruction("b.eq __rt_strip_tags_s2_q");                           // process a PHP quote only when it is not escaped
    emitter.instruction("cmp w22, #39");                                        // recognize a single-quote boundary
    emitter.instruction("b.eq __rt_strip_tags_s2_q");                           // process a PHP quote only when it is not escaped
    emitter.instruction("cmp w22, #108");                                       // recognize a lowercase XML suffix
    emitter.instruction("b.eq __rt_strip_tags_s2_xml");                         // validate the complete preceding XML opener
    emitter.instruction("cmp w22, #76");                                        // recognize an uppercase XML suffix
    emitter.instruction("b.eq __rt_strip_tags_s2_xml");                         // validate the complete preceding XML opener
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s2_q");
    emitter.instruction("cbz x21, __rt_strip_tags_advance");                    // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #1");                                    // address subject byte 1 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("cmp w9, #92");                                         // recognize a backslash that escapes quoting
    emitter.instruction("b.eq __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("ldr x9, [x29, #-136]");                                // reload the last significant PHP byte for this scan step
    emitter.instruction("cmp x9, x22");                                         // close the quote only when it matches the saved delimiter
    emitter.instruction("b.ne __rt_strip_tags_s2_q_set");                       // remember the new significant PHP quote byte
    emitter.instruction("str xzr, [x29, #-136]");                               // clear the last significant PHP byte
    emitter.instruction("b __rt_strip_tags_s2_q_in");                           // update the active PHP quote delimiter
    emitter.label("__rt_strip_tags_s2_q_set");
    emitter.instruction("cmp x9, #92");                                         // recognize a backslash that escapes quoting
    emitter.instruction("b.eq __rt_strip_tags_s2_q_in");                        // update the active PHP quote delimiter
    emitter.instruction("str x22, [x29, #-136]");                               // preserve the last significant PHP byte
    emitter.label("__rt_strip_tags_s2_q_in");
    emitter.instruction("cbz x21, __rt_strip_tags_advance");                    // consume this byte and continue scanning
    emitter.instruction("ldr x9, [x29, #-112]");                                // reload the active quote delimiter for this scan step
    emitter.instruction("cbz x9, __rt_strip_tags_s2_q_open");                   // remember the opening PHP quote
    emitter.instruction("cmp x9, x22");                                         // close the quote only when it matches the saved delimiter
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("str xzr, [x29, #-112]");                               // clear the active quote delimiter
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s2_q_open");
    emitter.instruction("str x22, [x29, #-112]");                               // preserve the active quote delimiter
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s2_xml");
    emitter.instruction("cmp x21, #4");                                         // keep a leading XML opener in PHP state using PHP's strict offset bound
    emitter.instruction("b.ls __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #1");                                    // address subject byte 1 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("orr w9, w9, #32");                                     // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp w9, #109");                                        // recognize the expected XML prefix letter m
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #2");                                    // address subject byte 2 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("orr w9, w9, #32");                                     // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp w9, #120");                                        // recognize the expected XML prefix letter x
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #3");                                    // address subject byte 3 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("cmp w9, #63");                                         // recognize a PHP-tag question mark
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #4");                                    // address subject byte 4 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("cmp w9, #60");                                         // recognize an opening tag delimiter
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("mov x9, #1");                                          // select HTML-tag state for subsequent subject bytes
    emitter.instruction("str x9, [x29, #-104]");                                // preserve the scanner state
    emitter.instruction("str x9, [x29, #-144]");                                // preserve the XML-prologue flag
    emitter.instruction("add x21, x21, #1");                                    // consume the current subject byte exactly once
    emitter.instruction("str x21, [x29, #-96]");                                // preserve the subject scan cursor
    emitter.instruction("b __rt_strip_tags_loop");                              // resume state dispatch at the saved subject cursor
}

/// Emits `<!...` handling (state 3), including comments and `<!DOCTYPE`.
fn emit_state3_aarch64(emitter: &mut Emitter) {
    // -- state 3: `<!` markup, comments, and DOCTYPE --
    emitter.comment("-- state 3: `<!` markup, comments, and DOCTYPE --");
    emitter.label("__rt_strip_tags_s3");
    emitter.instruction("cmp w22, #62");                                        // recognize a closing tag delimiter
    emitter.instruction("b.ne __rt_strip_tags_s3_quote");                       // recognize declaration quotes, comments and DOCTYPE
    emitter.instruction("ldr x9, [x29, #-120]");                                // reload the nested angle-bracket depth for this scan step
    emitter.instruction("cmp x9, #0");                                          // check whether the nested angle-bracket depth still blocks the closer
    emitter.instruction("b.le __rt_strip_tags_s3_gt_q");                        // close a declaration only outside quoted contents
    emitter.instruction("sub x9, x9, #1");                                      // reduce the nested angle-bracket depth after processing one delimiter
    emitter.instruction("str x9, [x29, #-120]");                                // preserve the nested angle-bracket depth
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s3_gt_q");
    emitter.instruction("ldr x9, [x29, #-112]");                                // reload the active quote delimiter for this scan step
    emitter.instruction("cbnz x9, __rt_strip_tags_advance");                    // consume this byte and continue scanning
    emitter.instruction("str xzr, [x29, #-112]");                               // clear the active quote delimiter
    emitter.instruction("str xzr, [x29, #-104]");                               // clear the scanner state
    emitter.instruction("str xzr, [x29, #-64]");                                // clear the collected-tag length
    emitter.instruction("add x21, x21, #1");                                    // consume the current subject byte exactly once
    emitter.instruction("str x21, [x29, #-96]");                                // preserve the subject scan cursor
    emitter.instruction("b __rt_strip_tags_loop");                              // resume state dispatch at the saved subject cursor
    emitter.label("__rt_strip_tags_s3_quote");
    emitter.instruction("cmp w22, #34");                                        // recognize a double-quote boundary
    emitter.instruction("b.eq __rt_strip_tags_s3_q");                           // process an unescaped declaration quote
    emitter.instruction("cmp w22, #39");                                        // recognize a single-quote boundary
    emitter.instruction("b.eq __rt_strip_tags_s3_q");                           // process an unescaped declaration quote
    emitter.instruction("cmp w22, #45");                                        // recognize a comment delimiter dash
    emitter.instruction("b.eq __rt_strip_tags_s3_dash");                        // recognize the second dash of a comment opener
    emitter.instruction("cmp w22, #69");                                        // recognize an uppercase DOCTYPE suffix
    emitter.instruction("b.eq __rt_strip_tags_s3_doctype");                     // recognize a complete case-insensitive DOCTYPE prefix
    emitter.instruction("cmp w22, #101");                                       // recognize a lowercase DOCTYPE suffix
    emitter.instruction("b.eq __rt_strip_tags_s3_doctype");                     // recognize a complete case-insensitive DOCTYPE prefix
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s3_q");
    emitter.instruction("cbz x21, __rt_strip_tags_advance");                    // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #1");                                    // address subject byte 1 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("cmp w9, #92");                                         // recognize a backslash that escapes quoting
    emitter.instruction("b.eq __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("ldr x9, [x29, #-112]");                                // reload the active quote delimiter for this scan step
    emitter.instruction("cbz x9, __rt_strip_tags_s3_q_open");                   // remember the opening declaration quote
    emitter.instruction("cmp x9, x22");                                         // close the quote only when it matches the saved delimiter
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("str xzr, [x29, #-112]");                               // clear the active quote delimiter
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s3_q_open");
    emitter.instruction("str x22, [x29, #-112]");                               // preserve the active quote delimiter
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
    emitter.label("__rt_strip_tags_s3_dash");
    emitter.instruction("cmp x21, #2");                                         // require two preceding subject bytes before recognizing a comment delimiter
    emitter.instruction("b.lo __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #1");                                    // address subject byte 1 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("cmp w9, #45");                                         // recognize a comment delimiter dash
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #2");                                    // address subject byte 2 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("cmp w9, #33");                                         // recognize a declaration marker after the opener
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("mov x9, #4");                                          // select comment state for subsequent subject bytes
    emitter.instruction("str x9, [x29, #-104]");                                // preserve the scanner state
    emitter.instruction("add x21, x21, #1");                                    // consume the current subject byte exactly once
    emitter.instruction("str x21, [x29, #-96]");                                // preserve the subject scan cursor
    emitter.instruction("b __rt_strip_tags_loop");                              // resume state dispatch at the saved subject cursor
    emitter.label("__rt_strip_tags_s3_doctype");
    emitter.instruction("cmp x21, #7");                                         // require a complete preceding DOCTYPE marker
    emitter.instruction("b.ls __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #1");                                    // address subject byte 1 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("orr w9, w9, #32");                                     // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp w9, #112");                                        // recognize the DOCTYPE prefix letter p
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #2");                                    // address subject byte 2 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("orr w9, w9, #32");                                     // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp w9, #121");                                        // recognize the DOCTYPE prefix letter y
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #3");                                    // address subject byte 3 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("orr w9, w9, #32");                                     // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp w9, #116");                                        // recognize the DOCTYPE prefix letter t
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #4");                                    // address subject byte 4 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("orr w9, w9, #32");                                     // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp w9, #99");                                         // recognize the DOCTYPE prefix letter c
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #5");                                    // address subject byte 5 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("orr w9, w9, #32");                                     // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp w9, #111");                                        // recognize the DOCTYPE prefix letter o
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #6");                                    // address subject byte 6 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("orr w9, w9, #32");                                     // compare the markup prefix without ASCII case sensitivity
    emitter.instruction("cmp w9, #100");                                        // recognize the DOCTYPE prefix letter d
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("mov x9, #1");                                          // select HTML-tag state for subsequent subject bytes
    emitter.instruction("str x9, [x29, #-104]");                                // preserve the scanner state
    emitter.instruction("add x21, x21, #1");                                    // consume the current subject byte exactly once
    emitter.instruction("str x21, [x29, #-96]");                                // preserve the subject scan cursor
    emitter.instruction("b __rt_strip_tags_loop");                              // resume state dispatch at the saved subject cursor
}

/// Emits HTML-comment handling (state 4).
fn emit_state4_aarch64(emitter: &mut Emitter) {
    // -- state 4: HTML comment, closed only by `-->` --
    emitter.comment("-- state 4: HTML comment, closed only by `-->` --");
    emitter.label("__rt_strip_tags_s4");
    emitter.instruction("cmp w22, #62");                                        // recognize a closing tag delimiter
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("ldr x9, [x29, #-112]");                                // reload the active quote delimiter for this scan step
    emitter.instruction("cbnz x9, __rt_strip_tags_advance");                    // consume this byte and continue scanning
    emitter.instruction("cmp x21, #2");                                         // require two preceding subject bytes before recognizing a comment delimiter
    emitter.instruction("b.lo __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #1");                                    // address subject byte 1 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("cmp w9, #45");                                         // recognize a comment delimiter dash
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("sub x10, x21, #2");                                    // address subject byte 2 before the cursor for prefix validation
    emitter.instruction("ldrb w9, [x19, x10]");                                 // read the preceding subject byte for escape or prefix validation
    emitter.instruction("cmp w9, #45");                                         // recognize a comment delimiter dash
    emitter.instruction("b.ne __rt_strip_tags_advance");                        // consume this byte and continue scanning
    emitter.instruction("str xzr, [x29, #-112]");                               // clear the active quote delimiter
    emitter.instruction("str xzr, [x29, #-104]");                               // clear the scanner state
    emitter.instruction("str xzr, [x29, #-64]");                                // clear the collected-tag length
    emitter.instruction("b __rt_strip_tags_advance");                           // consume this byte and continue scanning
}

/// Emits local helpers shared by the AArch64 state machine.
fn emit_helpers_aarch64(emitter: &mut Emitter) {
    // -- consume the current subject byte before redispatch --
    emitter.label("__rt_strip_tags_advance");
    emitter.instruction("add x21, x21, #1");                                    // consume the current subject byte exactly once
    emitter.instruction("str x21, [x29, #-96]");                                // preserve the subject scan cursor
    emitter.instruction("b __rt_strip_tags_loop");                              // resume state dispatch at the saved subject cursor

    // -- write one kept output byte --
    emitter.comment("-- write one kept output byte --");
    emitter.label("__rt_strip_tags_putc");
    emitter.instruction("ldr x9, [x29, #-48]");                                 // reload the output write cursor for this scan step
    emitter.instruction("strb w0, [x9], #1");                                   // append one retained byte and advance the output cursor
    emitter.instruction("str x9, [x29, #-48]");                                 // preserve the output write cursor
    emitter.instruction("ret");                                                 // return after advancing the output write cursor

    // -- append one collected tag byte when an allow-list is active --
    emitter.comment("-- append one collected tag byte when an allow-list is active --");
    emitter.label("__rt_strip_tags_collect");
    emitter.instruction("ldr x9, [x29, #-80]");                                 // reload the effective allow-list length for this scan step
    emitter.instruction("cbz x9, __rt_strip_tags_collect_ret");                 // return after optional tag collection
    emitter.instruction("ldr x10, [x29, #-56]");                                // reload the collected-tag buffer for this scan step
    emitter.instruction("ldr x11, [x29, #-64]");                                // reload the collected-tag length for this scan step
    emitter.instruction("strb w0, [x10, x11]");                                 // append the current byte to the raw collected tag
    emitter.instruction("add x11, x11, #1");                                    // advance the collected-tag length after appending a byte
    emitter.instruction("str x11, [x29, #-64]");                                // preserve the collected-tag length
    emitter.label("__rt_strip_tags_collect_ret");
    emitter.instruction("ret");                                                 // return after optional raw-tag collection

    // -- copy a kept allowed tag into the output --
    emitter.comment("-- copy a kept allowed tag into the output --");
    emitter.label("__rt_strip_tags_emit_tag");
    emitter.instruction("ldr x1, [x29, #-56]");                                 // reload the collected-tag buffer for this scan step
    emitter.instruction("ldr x2, [x29, #-64]");                                 // reload the collected-tag length for this scan step
    emitter.instruction("ldr x3, [x29, #-48]");                                 // reload the output write cursor for this scan step
    emitter.instruction("mov x4, xzr");                                         // start copying the complete allowed raw tag
    emitter.label("__rt_strip_tags_emit_tag_loop");
    emitter.instruction("cmp x4, x2");                                          // stop after copying every byte of the allowed raw tag
    emitter.instruction("b.hs __rt_strip_tags_emit_tag_done");                  // commit the copied tag to the output cursor
    emitter.instruction("ldrb w9, [x1, x4]");                                   // read the next byte of the allowed raw tag
    emitter.instruction("strb w9, [x3, x4]");                                   // copy the allowed tag without losing its original attributes
    emitter.instruction("add x4, x4, #1");                                      // advance the collected-tag traversal cursor
    emitter.instruction("b __rt_strip_tags_emit_tag_loop");                     // copy the next byte of an allowed tag
    emitter.label("__rt_strip_tags_emit_tag_done");
    emitter.instruction("add x3, x3, x2");                                      // advance the output cursor past the complete retained tag
    emitter.instruction("str x3, [x29, #-48]");                                 // preserve the output write cursor
    emitter.instruction("ret");                                                 // return after committing the retained tag bytes

    // -- load the next subject byte, or 0 at EOF, into w0 --
    emitter.comment("-- load the next subject byte, or 0 at EOF, into w0 --");
    emitter.label("__rt_strip_tags_next");
    emitter.instruction("add x9, x21, #1");                                     // address the lookahead byte after the current subject cursor
    emitter.instruction("cmp x9, x20");                                         // keep the lookahead read inside the subject byte range
    emitter.instruction("b.hs __rt_strip_tags_next_eof");                       // return an EOF byte without reading beyond the subject
    emitter.instruction("ldrb w0, [x19, x9]");                                  // read the bounded lookahead byte
    emitter.instruction("ret");                                                 // return the bounded lookahead byte
    emitter.label("__rt_strip_tags_next_eof");
    emitter.instruction("mov w0, wzr");                                         // return zero rather than reading beyond the subject
    emitter.instruction("ret");                                                 // return the bounded lookahead byte

    // -- C-locale isspace: 0x09-0x0d or 0x20; result in w0 --
    emitter.comment("-- C-locale isspace: 0x09-0x0d or 0x20; result in w0 --");
    emitter.label("__rt_strip_tags_isspace");
    emitter.instruction("cmp w0, #9");                                          // recognize the start of PHP's C-locale whitespace range
    emitter.instruction("b.lo __rt_strip_tags_isspace_no");                     // reject a byte outside PHP's C-locale whitespace set
    emitter.instruction("cmp w0, #13");                                         // recognize the end of PHP's C-locale whitespace range
    emitter.instruction("b.ls __rt_strip_tags_isspace_yes");                    // accept a byte in PHP's C-locale whitespace set
    emitter.instruction("cmp w0, #32");                                         // recognize a literal space in the tag or lookahead
    emitter.instruction("b.eq __rt_strip_tags_isspace_yes");                    // accept a byte in PHP's C-locale whitespace set
    emitter.label("__rt_strip_tags_isspace_no");
    emitter.instruction("mov w0, wzr");                                         // report a byte outside PHP's C-locale whitespace set
    emitter.instruction("ret");                                                 // return the whitespace classification to the scanner
    emitter.label("__rt_strip_tags_isspace_yes");
    emitter.instruction("mov w0, #1");                                          // report a C-locale whitespace byte
    emitter.instruction("ret");                                                 // return the whitespace classification to the scanner

    // -- php_tag_find: normalize the collected tag and strstr the allow set --
    emitter.comment("-- php_tag_find: normalize the collected tag and strstr the allow set --");
    emitter.label("__rt_strip_tags_tag_find");
    emitter.instruction("ldr x2, [x29, #-64]");                                 // reload the collected-tag length for this scan step
    emitter.instruction("cbz x2, __rt_strip_tags_tag_find_no");                 // reject a tag without a complete allow-list token match
    emitter.instruction("ldr x1, [x29, #-56]");                                 // reload the collected-tag buffer for this scan step
    emitter.instruction("ldr x3, [x29, #-88]");                                 // reload the normalized-tag buffer for this scan step
    emitter.instruction("mov x4, xzr");                                         // start normalization at the first collected-tag byte
    emitter.instruction("mov x5, xzr");                                         // start the normalized tag with no retained name bytes
    emitter.instruction("mov x6, xzr");                                         // record that no tag-name byte has been seen yet
    emitter.label("__rt_strip_tags_tag_find_norm");
    emitter.instruction("cmp x4, x2");                                          // stop after consuming every collected-tag byte
    emitter.instruction("b.hs __rt_strip_tags_tag_find_end");                   // terminate the normalized token before searching the allow-list
    emitter.instruction("ldrb w0, [x1, x4]");                                   // read the next collected-tag byte for normalization
    emitter.instruction("cmp w0, #65");                                         // recognize the first uppercase ASCII letter
    emitter.instruction("b.lo __rt_strip_tags_tag_find_lc_done");               // preserve a tag byte that needs no ASCII case folding
    emitter.instruction("cmp w0, #90");                                         // recognize the last uppercase ASCII letter
    emitter.instruction("b.hi __rt_strip_tags_tag_find_lc_done");               // preserve a tag byte that needs no ASCII case folding
    emitter.instruction("orr w0, w0, #32");                                     // fold an ASCII uppercase tag-name letter to lowercase
    emitter.label("__rt_strip_tags_tag_find_lc_done");
    emitter.instruction("cmp w0, #60");                                         // recognize an opening tag delimiter
    emitter.instruction("b.ne __rt_strip_tags_tag_find_gt");                    // recognize the end of the collected tag name
    emitter.instruction("strb w0, [x3, x5]");                                   // append a retained delimiter or name byte to the normalized token
    emitter.instruction("add x5, x5, #1");                                      // advance the normalized-token length after appending a byte
    emitter.instruction("b __rt_strip_tags_tag_find_nextb");                    // advance the source-tag normalization cursor
    emitter.label("__rt_strip_tags_tag_find_gt");
    emitter.instruction("cmp w0, #62");                                         // recognize a closing tag delimiter
    emitter.instruction("b.eq __rt_strip_tags_tag_find_end");                   // terminate the normalized token before searching the allow-list
    emitter.instruction("cmp w0, #9");                                          // recognize the start of PHP's C-locale whitespace range
    emitter.instruction("b.lo __rt_strip_tags_tag_find_char");                  // process a non-whitespace tag-name byte
    emitter.instruction("cmp w0, #13");                                         // recognize the end of PHP's C-locale whitespace range
    emitter.instruction("b.ls __rt_strip_tags_tag_find_space");                 // ignore leading spaces and stop after the tag name
    emitter.instruction("cmp w0, #32");                                         // recognize a literal space in the tag or lookahead
    emitter.instruction("b.ne __rt_strip_tags_tag_find_char");                  // process a non-whitespace tag-name byte
    emitter.label("__rt_strip_tags_tag_find_space");
    emitter.instruction("cmp x6, #1");                                          // stop a name at whitespace only after its first name byte
    emitter.instruction("b.eq __rt_strip_tags_tag_find_end");                   // terminate the normalized token before searching the allow-list
    emitter.instruction("b __rt_strip_tags_tag_find_nextb");                    // advance the source-tag normalization cursor
    emitter.label("__rt_strip_tags_tag_find_char");
    emitter.instruction("cbnz x6, __rt_strip_tags_tag_find_slash");             // distinguish delimiter slashes from literal name bytes
    emitter.instruction("mov x6, #1");                                          // remember that the tag-name body has begun
    emitter.label("__rt_strip_tags_tag_find_slash");
    emitter.instruction("cmp w0, #47");                                         // recognize a closing or self-closing tag slash
    emitter.instruction("b.ne __rt_strip_tags_tag_find_keep");                  // retain a slash that is not a tag delimiter
    emitter.instruction("cbz x4, __rt_strip_tags_tag_find_keep");               // retain a slash that is not a tag delimiter
    emitter.instruction("sub x7, x4, #1");                                      // inspect the byte before a possible closing-tag slash
    emitter.instruction("ldrb w8, [x1, x7]");                                   // read a slash neighbor without changing the tag traversal cursor
    emitter.instruction("cmp w8, #60");                                         // recognize an opening tag delimiter
    emitter.instruction("b.eq __rt_strip_tags_tag_find_nextb");                 // advance the source-tag normalization cursor
    emitter.instruction("add x7, x4, #1");                                      // inspect the byte after a possible self-closing slash
    emitter.instruction("cmp x7, x2");                                          // keep the following slash-neighbor read within the collected tag
    emitter.instruction("b.hs __rt_strip_tags_tag_find_keep");                  // retain a slash that is not a tag delimiter
    emitter.instruction("ldrb w8, [x1, x7]");                                   // read a slash neighbor without changing the tag traversal cursor
    emitter.instruction("cmp w8, #62");                                         // recognize a closing tag delimiter
    emitter.instruction("b.eq __rt_strip_tags_tag_find_nextb");                 // advance the source-tag normalization cursor
    emitter.label("__rt_strip_tags_tag_find_keep");
    emitter.instruction("strb w0, [x3, x5]");                                   // append a retained delimiter or name byte to the normalized token
    emitter.instruction("add x5, x5, #1");                                      // advance the normalized-token length after appending a byte
    emitter.label("__rt_strip_tags_tag_find_nextb");
    emitter.instruction("add x4, x4, #1");                                      // advance the collected-tag traversal cursor
    emitter.instruction("b __rt_strip_tags_tag_find_norm");                     // normalize the next collected-tag byte
    emitter.label("__rt_strip_tags_tag_find_end");
    emitter.instruction("mov w0, #62");                                         // prepare the closing tag delimiter for the scanner
    emitter.instruction("strb w0, [x3, x5]");                                   // append a retained delimiter or name byte to the normalized token
    emitter.instruction("add x5, x5, #1");                                      // advance the normalized-token length after appending a byte
    emitter.instruction("ldr x4, [x29, #-72]");                                 // reload the lowercased allow-list buffer for this scan step
    emitter.instruction("ldr x6, [x29, #-80]");                                 // reload the effective allow-list length for this scan step
    emitter.instruction("mov x7, xzr");                                         // begin allow-list matching at its first byte
    emitter.label("__rt_strip_tags_tag_find_scan");
    emitter.instruction("sub x8, x6, x7");                                      // compute allow bytes remaining after the current candidate offset
    emitter.instruction("cmp x8, x5");                                          // reject candidates too short to contain the whole normalized token
    emitter.instruction("b.lo __rt_strip_tags_tag_find_no");                    // reject a tag without a complete allow-list token match
    emitter.instruction("mov x9, xzr");                                         // start each candidate with zero matched token bytes
    emitter.label("__rt_strip_tags_tag_find_cmp");
    emitter.instruction("cmp x9, x5");                                          // recognize a match only after every normalized byte was compared
    emitter.instruction("b.hs __rt_strip_tags_tag_find_yes");                   // accept a tag only after its whole normalized token matches
    emitter.instruction("ldrb w0, [x4, x7]");                                   // read the current allow-list candidate byte
    emitter.instruction("ldrb w8, [x3, x9]");                                   // read the corresponding normalized tag-token byte
    emitter.instruction("cmp w0, w8");                                          // require exact byte equality for the complete normalized token
    emitter.instruction("b.ne __rt_strip_tags_tag_find_scan_next");             // restart at the next candidate after a mismatch
    emitter.instruction("add x9, x9, #1");                                      // advance the matched-prefix length within this candidate
    emitter.instruction("add x7, x7, #1");                                      // try the next allow-list start position after a mismatch
    emitter.instruction("b __rt_strip_tags_tag_find_cmp");                      // compare the complete normalized token at this candidate
    emitter.label("__rt_strip_tags_tag_find_scan_next");
    emitter.instruction("sub x7, x7, x9");                                      // rewind to the candidate start after a partial mismatch
    emitter.instruction("add x7, x7, #1");                                      // try the next allow-list start position after a mismatch
    emitter.instruction("b __rt_strip_tags_tag_find_scan");                     // try the next bounded allow-list match
    emitter.label("__rt_strip_tags_tag_find_yes");
    emitter.instruction("mov w0, #1");                                          // report a complete allow-list token match
    emitter.instruction("ret");                                                 // return whether the complete tag token matched
    emitter.label("__rt_strip_tags_tag_find_no");
    emitter.instruction("mov w0, wzr");                                         // report that no complete allow-list token matched
    emitter.instruction("ret");                                                 // return whether the complete tag token matched
}
