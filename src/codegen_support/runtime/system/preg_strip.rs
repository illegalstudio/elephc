//! Purpose:
//! Emits the `__rt_preg_strip` runtime helper assembly: PHP regex delimiter parsing and
//! modifier-to-compile-flag mapping, shared by every compiled `preg_*` helper.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - Follows php-src's `pcre_get_compiled_regex_cache_ex()`: leading whitespace is skipped,
//!   the delimiter is any byte other than a letter, a digit, a backslash or NUL, `(`, `[`,
//!   `{` and `<` close with their bracket pair (nesting counted), and the closing delimiter
//!   is the first one no backslash escapes.
//! - Every pattern PHP rejects before compilation (an empty or all-whitespace pattern, an
//!   invalid delimiter, a missing ending delimiter, an unknown modifier, the removed `/e`)
//!   sets [`PREG_INVALID_PATTERN_FLAG`]. That bit is not a PCRE2 flag; every regex helper
//!   tests it right after the strip and returns its failure value without compiling.
//! - `x`, `A`, `D`, `n`, `J` and `r` map to private compile bits the PCRE2 shim
//!   (`src/native_deps/recipes/pcre2_shim.c`) turns into PCRE2 compile options. `S`, `X`,
//!   space, LF and CR are accepted and ignored, as in PHP 8.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Set in the returned flags when PHP would reject the pattern before compiling it.
pub(crate) const PREG_INVALID_PATTERN_FLAG: u64 = 1 << 30;

/// PHP modifiers and the compile flags they add. The values are pcre2posix `REG_*` bits
/// and the shim's private `ELEPHC_PCRE2_*` bits; a zero flag is a modifier PHP accepts
/// without effect. Any byte missing from this table is an unknown modifier.
const PREG_MODIFIERS: &[(u8, u64)] = &[
    (b'i', 0x1),     // REG_ICASE → PCRE2_CASELESS
    (b'm', 0x2),     // REG_NEWLINE → PCRE2_MULTILINE
    (b's', 0x10),    // REG_DOTALL → PCRE2_DOTALL
    (b'u', 0x440),   // REG_UTF | REG_UCP → PCRE2_UTF | PCRE2_UCP
    (b'U', 0x200),   // REG_UNGREEDY → PCRE2_UNGREEDY
    (b'x', 0x2000),  // ELEPHC_PCRE2_EXTENDED → PCRE2_EXTENDED
    (b'A', 0x4000),  // ELEPHC_PCRE2_ANCHORED → PCRE2_ANCHORED
    (b'D', 0x8000),  // ELEPHC_PCRE2_DOLLAR_ENDONLY → PCRE2_DOLLAR_ENDONLY
    (b'n', 0x20000), // ELEPHC_PCRE2_NO_AUTO_CAPTURE → PCRE2_NO_AUTO_CAPTURE
    (b'J', 0x40000), // ELEPHC_PCRE2_DUPNAMES → PCRE2_DUPNAMES
    (b'r', 0x80000), // ELEPHC_PCRE2_CASELESS_RESTRICT → PCRE2_EXTRA_CASELESS_RESTRICT
    (b'S', 0),       // no-op since PHP 7.3
    (b'X', 0),       // no-op since PHP 7.3
    (b' ', 0),       // ignored in the modifier suffix
    (b'\n', 0),      // ignored in the modifier suffix
    (b'\r', 0),      // ignored in the modifier suffix
];

/// Emits the `__rt_preg_strip` runtime helper for the current target.
///
/// Turns a PHP pattern such as `"/pattern/i"`, `" #pattern#"` or `"{pat{2}ern}"` into the
/// bytes between its delimiters plus compile flags. A pattern PHP rejects comes back with
/// [`PREG_INVALID_PATTERN_FLAG`] set; its pointer and length are then meaningless.
///
/// ARM64 input: x1=pattern ptr, x2=pattern len. Output: x1=stripped ptr, x2=stripped len,
/// x3=compile flags. Only x9, x10 and x12 are clobbered besides the outputs.
pub(crate) fn emit_preg_strip(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_strip_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: preg_strip_delimiters ---");
    emitter.label_global("__rt_preg_strip");

    // -- preserve the scratch registers the scan needs beyond x9, x10 and x12 --
    emitter.instruction("sub sp, sp, #32");                                     // reserve spill slots for x11, x13, x14 and x15
    emitter.instruction("stp x11, x13, [sp]");                                  // preserve the caller's x11 and x13
    emitter.instruction("stp x14, x15, [sp, #16]");                             // preserve the caller's x14 and x15
    emitter.instruction("mov x3, #0");                                          // no compile flags yet
    emitter.instruction("mov x10, #0");                                         // scan from the first pattern byte

    // -- skip leading whitespace, as PHP does with isspace() --
    emitter.label("__rt_preg_strip_ws");
    emitter.instruction("cmp x10, x2");                                         // did the pattern end before a delimiter?
    emitter.instruction("b.hs __rt_preg_strip_invalid");                        // an empty or all-whitespace pattern is rejected
    emitter.instruction("ldrb w9, [x1, x10]");                                  // load the next leading byte
    emitter.instruction("cmp w9, #32");                                         // is it a space?
    emitter.instruction("b.eq __rt_preg_strip_ws_next");                        // skip it
    emitter.instruction("sub w12, w9, #9");                                     // offset the byte from '\t' to test for \t, \n, \v, \f and \r
    emitter.instruction("cmp w12, #4");                                         // is it one of those whitespace controls?
    emitter.instruction("b.hi __rt_preg_strip_delimiter");                      // the first non-whitespace byte is the delimiter
    emitter.label("__rt_preg_strip_ws_next");
    emitter.instruction("add x10, x10, #1");                                    // move past the whitespace byte
    emitter.instruction("b __rt_preg_strip_ws");                                // keep skipping leading whitespace

    // -- validate the opening delimiter and choose the closing one --
    emitter.label("__rt_preg_strip_delimiter");
    emitter.instruction("cbz w9, __rt_preg_strip_invalid");                     // NUL cannot delimit a pattern
    emitter.instruction("cmp w9, #92");                                         // is it a backslash?
    emitter.instruction("b.eq __rt_preg_strip_invalid");                        // a backslash cannot delimit a pattern
    emitter.instruction("sub w12, w9, #48");                                    // offset the byte from '0' to test for a digit
    emitter.instruction("cmp w12, #9");                                         // is it a digit?
    emitter.instruction("b.ls __rt_preg_strip_invalid");                        // a digit cannot delimit a pattern
    emitter.instruction("orr w12, w9, #32");                                    // fold ASCII letters to lower case
    emitter.instruction("sub w12, w12, #97");                                   // offset the folded byte from 'a'
    emitter.instruction("cmp w12, #25");                                        // is it a letter?
    emitter.instruction("b.ls __rt_preg_strip_invalid");                        // a letter cannot delimit a pattern
    emitter.instruction("mov w13, w9");                                         // remember the opening delimiter for bracket nesting
    emitter.instruction("mov w11, w9");                                         // the closing delimiter defaults to the opening one
    for (open, close) in [(b'(', b')'), (b'[', b']'), (b'{', b'}'), (b'<', b'>')] {
        let next = format!("__rt_preg_strip_bracket_{}", open);
        emitter.instruction(&format!("cmp w9, #{}", open));                     // does the pattern open with this bracket?
        emitter.instruction(&format!("b.ne {}", next));                         // try the next bracket pair
        emitter.instruction(&format!("mov w11, #{}", close));                   // the bracket closes with its pair
        emitter.label(&next);
    }

    // -- find the first unescaped closing delimiter, counting bracket nesting --
    emitter.instruction("add x10, x10, #1");                                    // move past the opening delimiter
    emitter.instruction("mov x15, x10");                                        // the regex body starts here
    emitter.instruction("mov x14, #1");                                         // one open delimiter is pending
    emitter.label("__rt_preg_strip_scan");
    emitter.instruction("cmp x10, x2");                                         // did the pattern end before the closing delimiter?
    emitter.instruction("b.hs __rt_preg_strip_invalid");                        // PHP rejects a missing ending delimiter
    emitter.instruction("ldrb w9, [x1, x10]");                                  // load the next body byte
    emitter.instruction("cmp w9, #92");                                         // is it a backslash?
    emitter.instruction("b.ne __rt_preg_strip_scan_close");                     // other bytes may close or nest the delimiter
    emitter.instruction("add x12, x10, #1");                                    // index of the escaped byte
    emitter.instruction("cmp x12, x2");                                         // does an escaped byte follow?
    emitter.instruction("b.hs __rt_preg_strip_scan_next");                      // a trailing backslash escapes nothing
    emitter.instruction("mov x10, x12");                                        // skip the escaped byte
    emitter.instruction("b __rt_preg_strip_scan_next");                         // an escaped delimiter does not close the pattern
    emitter.label("__rt_preg_strip_scan_close");
    emitter.instruction("cmp w9, w11");                                         // is it the closing delimiter?
    emitter.instruction("b.ne __rt_preg_strip_scan_open");                      // otherwise it may open a nested bracket
    emitter.instruction("subs x14, x14, #1");                                   // close one pending delimiter
    emitter.instruction("b.eq __rt_preg_strip_found");                          // the outermost delimiter closes the body
    emitter.instruction("b __rt_preg_strip_scan_next");                         // a nested bracket closed
    emitter.label("__rt_preg_strip_scan_open");
    emitter.instruction("cmp w9, w13");                                         // does it open a nested bracket?
    emitter.instruction("b.ne __rt_preg_strip_scan_next");                      // ordinary body byte
    emitter.instruction("add x14, x14, #1");                                    // one more bracket must close first
    emitter.label("__rt_preg_strip_scan_next");
    emitter.instruction("add x10, x10, #1");                                    // advance to the next body byte
    emitter.instruction("b __rt_preg_strip_scan");                              // keep looking for the closing delimiter

    // -- map every byte after the closing delimiter to a compile flag --
    emitter.label("__rt_preg_strip_found");
    emitter.instruction("mov x14, x10");                                        // remember where the body ends
    emitter.label("__rt_preg_strip_modifier_next");
    emitter.instruction("add x10, x10, #1");                                    // move to the next modifier byte
    emitter.label("__rt_preg_strip_modifier");
    emitter.instruction("cmp x10, x2");                                         // are all modifiers mapped?
    emitter.instruction("b.hs __rt_preg_strip_ok");                             // return the body and its flags
    emitter.instruction("ldrb w9, [x1, x10]");                                  // load the next modifier byte
    for (index, (byte, flag)) in PREG_MODIFIERS.iter().enumerate() {
        let next = format!("__rt_preg_strip_modifier_{}", index);
        emitter.instruction(&format!("cmp w9, #{}", byte));                     // is it this PHP modifier?
        emitter.instruction(&format!("b.ne {}", next));                         // try the next known modifier
        if *flag != 0 {
            emitter.instruction(&format!("mov x12, #0x{:x}", flag));            // materialize the modifier's compile flag
            emitter.instruction("orr x3, x3, x12");                             // add it to the compile flags
        }
        emitter.instruction("b __rt_preg_strip_modifier_next");                 // continue with the next modifier byte
        emitter.label(&next);
    }
    emitter.instruction("b __rt_preg_strip_invalid");                           // PHP rejects an unknown modifier (and the removed /e)

    emitter.label("__rt_preg_strip_ok");
    emitter.instruction("sub x2, x14, x15");                                    // body length = closing index - body start
    emitter.instruction("add x1, x1, x15");                                     // body pointer = pattern + body start
    emitter.instruction("b __rt_preg_strip_ret");                               // restore the scratch registers

    emitter.label("__rt_preg_strip_invalid");
    emitter.instruction(&format!("mov x3, #0x{:x}", PREG_INVALID_PATTERN_FLAG)); // mark the pattern invalid for every caller

    emitter.label("__rt_preg_strip_ret");
    emitter.instruction("ldp x14, x15, [sp, #16]");                             // restore the caller's x14 and x15
    emitter.instruction("ldp x11, x13, [sp]");                                  // restore the caller's x11 and x13
    emitter.instruction("add sp, sp, #32");                                     // release the spill slots
    emitter.instruction("ret");                                                 // return the body in x1/x2 and the flags in x3
}

/// x86_64 emitter for `__rt_preg_strip`.
///
/// Input: pattern pointer in `rax`, length in `rdx`. Output: stripped pointer in `rax`,
/// stripped length in `rdx`, compile flags in `rcx`. Clobbers `r8` and `r9`; `r10` to
/// `r14` are preserved.
fn emit_preg_strip_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_strip_delimiters ---");
    emitter.label_global("__rt_preg_strip");

    // -- preserve the scratch registers the scan needs beyond r8 and r9 --
    for reg in ["r10", "r11", "r12", "r13", "r14"] {
        emitter.instruction(&format!("push {}", reg));                          // preserve a caller register the scan uses
    }
    emitter.instruction("xor ecx, ecx");                                        // no compile flags yet
    emitter.instruction("xor r9d, r9d");                                        // scan from the first pattern byte

    // -- skip leading whitespace, as PHP does with isspace() --
    emitter.label("__rt_preg_strip_ws_linux_x86_64");
    emitter.instruction("cmp r9, rdx");                                         // did the pattern end before a delimiter?
    emitter.instruction("jae __rt_preg_strip_invalid_linux_x86_64");            // an empty or all-whitespace pattern is rejected
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next leading byte
    emitter.instruction("cmp r8d, 32");                                         // is it a space?
    emitter.instruction("je __rt_preg_strip_ws_next_linux_x86_64");             // skip it
    emitter.instruction("lea r14d, [r8 - 9]");                                  // offset the byte from '\t' to test for \t, \n, \v, \f and \r
    emitter.instruction("cmp r14d, 4");                                         // is it one of those whitespace controls?
    emitter.instruction("ja __rt_preg_strip_delimiter_linux_x86_64");           // the first non-whitespace byte is the delimiter
    emitter.label("__rt_preg_strip_ws_next_linux_x86_64");
    emitter.instruction("add r9, 1");                                           // move past the whitespace byte
    emitter.instruction("jmp __rt_preg_strip_ws_linux_x86_64");                 // keep skipping leading whitespace

    // -- validate the opening delimiter and choose the closing one --
    emitter.label("__rt_preg_strip_delimiter_linux_x86_64");
    emitter.instruction("test r8d, r8d");                                       // is it NUL?
    emitter.instruction("jz __rt_preg_strip_invalid_linux_x86_64");             // NUL cannot delimit a pattern
    emitter.instruction("cmp r8d, 92");                                         // is it a backslash?
    emitter.instruction("je __rt_preg_strip_invalid_linux_x86_64");             // a backslash cannot delimit a pattern
    emitter.instruction("lea r14d, [r8 - 48]");                                 // offset the byte from '0' to test for a digit
    emitter.instruction("cmp r14d, 9");                                         // is it a digit?
    emitter.instruction("jbe __rt_preg_strip_invalid_linux_x86_64");            // a digit cannot delimit a pattern
    emitter.instruction("mov r14d, r8d");                                       // copy the byte to test for a letter
    emitter.instruction("or r14d, 32");                                         // fold ASCII letters to lower case
    emitter.instruction("sub r14d, 97");                                        // offset the folded byte from 'a'
    emitter.instruction("cmp r14d, 25");                                        // is it a letter?
    emitter.instruction("jbe __rt_preg_strip_invalid_linux_x86_64");            // a letter cannot delimit a pattern
    emitter.instruction("mov r11d, r8d");                                       // remember the opening delimiter for bracket nesting
    emitter.instruction("mov r10d, r8d");                                       // the closing delimiter defaults to the opening one
    for (open, close) in [(b'(', b')'), (b'[', b']'), (b'{', b'}'), (b'<', b'>')] {
        let next = format!("__rt_preg_strip_bracket_{}_linux_x86_64", open);
        emitter.instruction(&format!("cmp r8d, {}", open));                     // does the pattern open with this bracket?
        emitter.instruction(&format!("jne {}", next));                          // try the next bracket pair
        emitter.instruction(&format!("mov r10d, {}", close));                   // the bracket closes with its pair
        emitter.label(&next);
    }

    // -- find the first unescaped closing delimiter, counting bracket nesting --
    emitter.instruction("add r9, 1");                                           // move past the opening delimiter
    emitter.instruction("mov r13, r9");                                         // the regex body starts here
    emitter.instruction("mov r12d, 1");                                         // one open delimiter is pending
    emitter.label("__rt_preg_strip_scan_linux_x86_64");
    emitter.instruction("cmp r9, rdx");                                         // did the pattern end before the closing delimiter?
    emitter.instruction("jae __rt_preg_strip_invalid_linux_x86_64");            // PHP rejects a missing ending delimiter
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next body byte
    emitter.instruction("cmp r8d, 92");                                         // is it a backslash?
    emitter.instruction("jne __rt_preg_strip_scan_close_linux_x86_64");         // other bytes may close or nest the delimiter
    emitter.instruction("lea r14, [r9 + 1]");                                   // index of the escaped byte
    emitter.instruction("cmp r14, rdx");                                        // does an escaped byte follow?
    emitter.instruction("jae __rt_preg_strip_scan_next_linux_x86_64");          // a trailing backslash escapes nothing
    emitter.instruction("mov r9, r14");                                         // skip the escaped byte
    emitter.instruction("jmp __rt_preg_strip_scan_next_linux_x86_64");          // an escaped delimiter does not close the pattern
    emitter.label("__rt_preg_strip_scan_close_linux_x86_64");
    emitter.instruction("cmp r8d, r10d");                                       // is it the closing delimiter?
    emitter.instruction("jne __rt_preg_strip_scan_open_linux_x86_64");          // otherwise it may open a nested bracket
    emitter.instruction("sub r12, 1");                                          // close one pending delimiter
    emitter.instruction("jz __rt_preg_strip_found_linux_x86_64");               // the outermost delimiter closes the body
    emitter.instruction("jmp __rt_preg_strip_scan_next_linux_x86_64");          // a nested bracket closed
    emitter.label("__rt_preg_strip_scan_open_linux_x86_64");
    emitter.instruction("cmp r8d, r11d");                                       // does it open a nested bracket?
    emitter.instruction("jne __rt_preg_strip_scan_next_linux_x86_64");          // ordinary body byte
    emitter.instruction("add r12, 1");                                          // one more bracket must close first
    emitter.label("__rt_preg_strip_scan_next_linux_x86_64");
    emitter.instruction("add r9, 1");                                           // advance to the next body byte
    emitter.instruction("jmp __rt_preg_strip_scan_linux_x86_64");               // keep looking for the closing delimiter

    // -- map every byte after the closing delimiter to a compile flag --
    emitter.label("__rt_preg_strip_found_linux_x86_64");
    emitter.instruction("mov r12, r9");                                         // remember where the body ends
    emitter.label("__rt_preg_strip_modifier_next_linux_x86_64");
    emitter.instruction("add r9, 1");                                           // move to the next modifier byte
    emitter.instruction("cmp r9, rdx");                                         // are all modifiers mapped?
    emitter.instruction("jae __rt_preg_strip_ok_linux_x86_64");                 // return the body and its flags
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next modifier byte
    for (index, (byte, flag)) in PREG_MODIFIERS.iter().enumerate() {
        let next = format!("__rt_preg_strip_modifier_{}_linux_x86_64", index);
        emitter.instruction(&format!("cmp r8d, {}", byte));                     // is it this PHP modifier?
        emitter.instruction(&format!("jne {}", next));                          // try the next known modifier
        if *flag != 0 {
            emitter.instruction(&format!("or rcx, 0x{:x}", flag));              // add the modifier's compile flag
        }
        emitter.instruction("jmp __rt_preg_strip_modifier_next_linux_x86_64");  // continue with the next modifier byte
        emitter.label(&next);
    }
    emitter.instruction("jmp __rt_preg_strip_invalid_linux_x86_64");            // PHP rejects an unknown modifier (and the removed /e)

    emitter.label("__rt_preg_strip_ok_linux_x86_64");
    emitter.instruction("mov rdx, r12");                                        // body length starts from the closing index
    emitter.instruction("sub rdx, r13");                                        // body length = closing index - body start
    emitter.instruction("add rax, r13");                                        // body pointer = pattern + body start
    emitter.instruction("jmp __rt_preg_strip_ret_linux_x86_64");                // restore the scratch registers

    emitter.label("__rt_preg_strip_invalid_linux_x86_64");
    emitter.instruction(&format!("mov ecx, 0x{:x}", PREG_INVALID_PATTERN_FLAG)); // mark the pattern invalid for every caller

    emitter.label("__rt_preg_strip_ret_linux_x86_64");
    for reg in ["r14", "r13", "r12", "r11", "r10"] {
        emitter.instruction(&format!("pop {}", reg));                           // restore a caller register the scan used
    }
    emitter.instruction("ret");                                                 // return the body in rax/rdx and the flags in rcx
}
