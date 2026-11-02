//! Purpose:
//! Emits the `__rt_preg_strip`, `__rt_preg_strip_done` runtime helper assembly for preg strip.
//! Keeps PHP regex delimiter handling and PCRE2 POSIX-wrapper flag extraction in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - Regex helpers preserve PHP PCRE-flavored pattern bytes and pass PCRE2 POSIX-wrapper flags downstream.
//! - A pattern php rejects before compiling -- empty, an alphanumeric/backslash/NUL delimiter, no
//!   closing delimiter, an unknown modifier -- is recorded in the PCRE2 shim
//!   (`elephc_pcre2_v1_record_pattern_error`). Every caller compiles right after stripping, so that
//!   compile fails with php's reason and the caller's ordinary failure path, warning included,
//!   takes over. These patterns used to be accepted silently: an undelimited `abc` matched as the
//!   regex `abc`, and `/abc/Q` ignored the `Q`.
//! - The removed `/e` modifier is an unknown modifier like any other (PHP 8 rejects it with
//!   "Unknown modifier 'e'"), so it takes the recorded-error path and never reaches `pcre2_regcomp`;
//!   the former private bit 30 marker, which callers may still test, is therefore never set here.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Pattern error kinds shared with `elephc_pcre2_v1_record_pattern_error` in the PCRE2 shim.
const PATTERN_EMPTY: i64 = 1;
const PATTERN_DELIMITER: i64 = 2;
const PATTERN_NO_END: i64 = 3;
const PATTERN_MODIFIER: i64 = 4;

/// php modifiers and the shim compile flag each one sets (0: accepted, no flag).
const MODIFIERS: &[(u8, i64)] = &[
    (b'i', 1),       // REG_ICASE
    (b'm', 2),       // REG_NEWLINE: multiline anchors
    (b's', 16),      // REG_DOTALL
    (b'u', 1088),    // REG_UTF | REG_UCP
    (b'U', 512),     // REG_UNGREEDY
    (b'A', 8192),    // anchored at the offset (shim flag)
    (b'x', 16384),   // PCRE2_EXTENDED (shim flag)
    (b'D', 32768),   // PCRE2_DOLLAR_ENDONLY (shim flag)
    (b'J', 65536),   // PCRE2_DUPNAMES (shim flag)
    (b'n', 131072),  // PCRE2_NO_AUTO_CAPTURE (shim flag)
    (b'S', 0),       // php's "study": no effect under PCRE2
    (b'X', 0),       // php's "extra": always on under PCRE2
    (b'r', 0),       // php 8.4 caseless-restrict: accepted
    (b' ', 0),       // php skips blanks and line breaks among modifiers
    (b'\n', 0),
    (b'\r', 0),
];

/// Emits the `__rt_preg_strip` runtime helper for stripping PHP regex delimiters.
///
/// Transforms PHP PCRE patterns by removing the delimiters and extracting PCRE2 POSIX-wrapper
/// and Elephc shim flags. For example, `"/pattern/i"` becomes `("pattern", REG_ICASE)`, while `A`
/// requests matching at the supplied offset. The closing delimiter is found the way php finds
/// it: scanning FORWARD past backslash escapes, and counting nesting for bracket delimiters.
///
/// Input:  x1=pattern ptr, x2=pattern len
/// Output: x1=stripped pattern ptr, x2=stripped len, x3=PCRE2/shim cflags
pub(crate) fn emit_preg_strip(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_strip_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: preg_strip_delimiters ---");
    emitter.label_global("__rt_preg_strip");

    // -- set up stack frame: pattern pair at 0/8, x4-x8 saved around the error record at 16..56 --
    emitter.instruction("sub sp, sp, #80");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set new frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save pattern ptr
    emitter.instruction("str x2, [sp, #8]");                                    // save pattern len
    emitter.instruction("mov x3, #0");                                          // flags = 0

    // -- php-src skips leading whitespace before the delimiter, so elephc must too --
    // `php_pcre_get_compiled_regex_cache` advances past isspace() bytes before reading the
    // delimiter. Symfony's `HeaderUtils::split()` writes its pattern as a heredoc-style literal
    // that OPENS with a newline.
    emitter.label("__rt_preg_strip_lead_ws");
    emitter.instruction("cbz x2, __rt_preg_strip_err_empty");                   // nothing but whitespace: php's "Empty regular expression"
    emitter.instruction("ldrb w9, [x1]");                                       // inspect the next leading byte
    emitter.instruction("cmp w9, #32");                                         // space
    emitter.instruction("b.eq __rt_preg_strip_lead_ws_skip");                   // consume it
    emitter.instruction("sub w12, w9, #9");                                     // shift HT..CR (9..13) to the bottom of the range
    emitter.instruction("cmp w12, #5");                                         // tab, newline, vertical tab, form feed, carriage return
    emitter.instruction("b.hs __rt_preg_strip_lead_ws_done");                   // anything else opens the pattern
    emitter.label("__rt_preg_strip_lead_ws_skip");
    emitter.instruction("add x1, x1, #1");                                      // advance past the whitespace byte
    emitter.instruction("sub x2, x2, #1");                                      // and shorten the pattern to match
    emitter.instruction("b __rt_preg_strip_lead_ws");                           // keep skipping
    emitter.label("__rt_preg_strip_lead_ws_done");
    emitter.instruction("str x1, [sp, #0]");                                    // re-save the advanced pattern pointer
    emitter.instruction("str x2, [sp, #8]");                                    // re-save the shortened pattern length

    // -- the delimiter is any byte but an alphanumeric, a backslash or NUL --
    // Symfony's Yaml component writes every pattern as `#...#`, so it is not always '/'.
    emitter.instruction("ldrb w9, [x1]");                                       // the opening delimiter
    emitter.instruction("cbz w9, __rt_preg_strip_err_delimiter");               // NUL cannot delimit
    emitter.instruction("cmp w9, #92");                                         // nor can a backslash
    emitter.instruction("b.eq __rt_preg_strip_err_delimiter");
    emitter.instruction("sub w12, w9, #48");                                    // shift '0'..'9' to the bottom of the range
    emitter.instruction("cmp w12, #10");                                        // a digit?
    emitter.instruction("b.lo __rt_preg_strip_err_delimiter");
    emitter.instruction("sub w12, w9, #65");                                    // shift 'A'..'Z' to the bottom of the range
    emitter.instruction("cmp w12, #26");                                        // an uppercase letter?
    emitter.instruction("b.lo __rt_preg_strip_err_delimiter");
    emitter.instruction("sub w12, w9, #97");                                    // shift 'a'..'z' to the bottom of the range
    emitter.instruction("cmp w12, #26");                                        // a lowercase letter?
    emitter.instruction("b.lo __rt_preg_strip_err_delimiter");

    // -- bracket delimiters close with their mate; every other one closes with itself --
    emitter.instruction("mov w11, w9");                                         // default: the closing delimiter equals the opening one
    for (open, close) in [(40, 41), (123, 125), (91, 93), (60, 62)] {
        let next = format!("__rt_preg_strip_pair_{open}");
        emitter.instruction(&format!("cmp w9, #{open}"));                       // a bracket that pairs with its mate?
        emitter.instruction(&format!("b.ne {next}"));
        emitter.instruction(&format!("mov w11, #{close}"));                     // the mate closes the pattern
        emitter.label(&next);
    }

    // -- scan forward for the closing delimiter, as php does --
    emitter.instruction("mov x10, #1");                                         // x10 = scan index, just past the opening delimiter
    emitter.instruction("mov x13, #1");                                         // x13 = bracket nesting depth
    emitter.label("__rt_preg_strip_scan");
    emitter.instruction("cmp x10, x2");                                         // ran off the end?
    emitter.instruction("b.hs __rt_preg_strip_err_no_end");                     // php's "No ending delimiter"
    emitter.instruction("ldrb w12, [x1, x10]");                                 // the byte at the scan index
    emitter.instruction("cmp w12, #92");                                        // a backslash escapes the next byte
    emitter.instruction("b.ne __rt_preg_strip_scan_plain");
    emitter.instruction("add x10, x10, #2");                                    // skip the backslash and the byte it escapes
    emitter.instruction("b __rt_preg_strip_scan");
    emitter.label("__rt_preg_strip_scan_plain");
    emitter.instruction("cmp w9, w11");                                         // same opening and closing delimiter?
    emitter.instruction("b.eq __rt_preg_strip_scan_same");
    emitter.instruction("cmp w12, w9");                                         // a nested opening bracket
    emitter.instruction("b.ne __rt_preg_strip_scan_close");
    emitter.instruction("add x13, x13, #1");                                    // one level deeper
    emitter.instruction("b __rt_preg_strip_scan_next");
    emitter.label("__rt_preg_strip_scan_close");
    emitter.instruction("cmp w12, w11");                                        // a closing bracket
    emitter.instruction("b.ne __rt_preg_strip_scan_next");
    emitter.instruction("sub x13, x13, #1");                                    // one level out
    emitter.instruction("cbz x13, __rt_preg_strip_found");                      // back at the outermost level: the pattern ends here
    emitter.instruction("b __rt_preg_strip_scan_next");
    emitter.label("__rt_preg_strip_scan_same");
    emitter.instruction("cmp w12, w11");                                        // the closing delimiter?
    emitter.instruction("b.eq __rt_preg_strip_found");
    emitter.label("__rt_preg_strip_scan_next");
    emitter.instruction("add x10, x10, #1");                                    // next byte
    emitter.instruction("b __rt_preg_strip_scan");

    // -- every byte after the closing delimiter is a modifier --
    emitter.label("__rt_preg_strip_found");
    emitter.instruction("add x14, x10, #1");                                    // x14 = first modifier index
    emitter.label("__rt_preg_strip_mods");
    emitter.instruction("cmp x14, x2");                                         // every modifier read?
    emitter.instruction("b.hs __rt_preg_strip_mods_done");
    emitter.instruction("ldrb w9, [x1, x14]");                                  // the next modifier
    for (index, (modifier, flag)) in MODIFIERS.iter().enumerate() {
        let next = format!("__rt_preg_strip_mod_{index}");
        emitter.instruction(&format!("cmp w9, #{modifier}"));                   // this modifier?
        emitter.instruction(&format!("b.ne {next}"));
        if *flag != 0 {
            emitter.instruction(&format!("mov x12, #{flag}"));                  // its compile flag
            emitter.instruction("orr x3, x3, x12");
        }
        emitter.instruction("b __rt_preg_strip_mod_next");
        emitter.label(&next);
    }
    emitter.instruction("b __rt_preg_strip_err_modifier");                      // anything else is php's "Unknown modifier"
    emitter.label("__rt_preg_strip_mod_next");
    emitter.instruction("add x14, x14, #1");
    emitter.instruction("b __rt_preg_strip_mods");
    emitter.label("__rt_preg_strip_mods_done");
    emitter.instruction("add x1, x1, #1");                                      // skip the opening delimiter
    emitter.instruction("sub x2, x10, #1");                                     // length = closing index - 1
    emitter.instruction("b __rt_preg_strip_done");

    // -- a rejected pattern: record php's reason for the compile that follows --
    emitter.label("__rt_preg_strip_err_empty");
    emitter.instruction(&format!("mov x0, #{PATTERN_EMPTY}"));
    emitter.instruction("mov x1, #0");
    emitter.instruction("b __rt_preg_strip_err_record");
    emitter.label("__rt_preg_strip_err_delimiter");
    emitter.instruction(&format!("mov x0, #{PATTERN_DELIMITER}"));
    emitter.instruction("mov x1, #0");
    emitter.instruction("b __rt_preg_strip_err_record");
    emitter.label("__rt_preg_strip_err_no_end");
    emitter.instruction(&format!("mov x0, #{PATTERN_NO_END}"));
    emitter.instruction("mov w1, w11");                                         // the delimiter php says it did not find
    emitter.instruction("b __rt_preg_strip_err_record");
    emitter.label("__rt_preg_strip_err_modifier");
    emitter.instruction(&format!("mov x0, #{PATTERN_MODIFIER}"));
    emitter.instruction("mov w1, w9");                                          // the modifier php does not know
    emitter.label("__rt_preg_strip_err_record");
    emitter.instruction("stp x4, x5, [sp, #16]");                               // the C call may clobber what callers keep in x4-x8
    emitter.instruction("stp x6, x7, [sp, #32]");
    emitter.instruction("str x8, [sp, #48]");
    emitter.bl_c("elephc_pcre2_v1_record_pattern_error");
    emitter.instruction("ldp x4, x5, [sp, #16]");
    emitter.instruction("ldp x6, x7, [sp, #32]");
    emitter.instruction("ldr x8, [sp, #48]");
    emitter.instruction("ldr x1, [sp, #0]");                                    // hand the pattern on unchanged; its compile will fail
    emitter.instruction("ldr x2, [sp, #8]");
    emitter.instruction("mov x3, #0");

    emitter.label("__rt_preg_strip_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return
}

/// x86_64-specific emitter for `__rt_preg_strip`.
///
/// Uses System V AMD64 ABI: pattern pointer in `rax`, length in `rdx`; returns stripped
/// pattern pointer in `rax`, stripped length in `rdx`, and PCRE2 POSIX cflags in `rcx`.
/// Clobbers `r8`-`r11` as temporaries; `rbx` is saved and restored.
fn emit_preg_strip_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_strip_delimiters ---");
    emitter.label_global("__rt_preg_strip");

    emitter.instruction("push rbx");                                            // one more scratch register for the scan
    emitter.instruction("xor ecx, ecx");                                        // flags = 0
    // -- php-src skips leading whitespace before the delimiter; see the AArch64 path --
    emitter.label("__rt_preg_strip_lead_ws_linux_x86_64");
    emitter.instruction("test rdx, rdx");                                       // nothing but whitespace?
    emitter.instruction("jz __rt_preg_strip_err_empty_linux_x86_64");           // php's "Empty regular expression"
    emitter.instruction("movzx r8d, BYTE PTR [rax]");                           // inspect the next leading byte
    emitter.instruction("cmp r8d, 32");                                         // space
    emitter.instruction("je __rt_preg_strip_lead_ws_skip_linux_x86_64");        // consume it
    emitter.instruction("mov r10d, r8d");                                       // shift HT..CR (9..13) to the bottom of the range
    emitter.instruction("sub r10d, 9");
    emitter.instruction("cmp r10d, 5");                                         // tab, newline, vertical tab, form feed, carriage return
    emitter.instruction("jae __rt_preg_strip_lead_ws_done_linux_x86_64");       // anything else opens the pattern
    emitter.label("__rt_preg_strip_lead_ws_skip_linux_x86_64");
    emitter.instruction("add rax, 1");                                          // advance past the whitespace byte
    emitter.instruction("sub rdx, 1");                                          // and shorten the pattern to match
    emitter.instruction("jmp __rt_preg_strip_lead_ws_linux_x86_64");            // keep skipping
    emitter.label("__rt_preg_strip_lead_ws_done_linux_x86_64");

    // -- the delimiter is any byte but an alphanumeric, a backslash or NUL --
    emitter.instruction("movzx r8d, BYTE PTR [rax]");                           // the opening delimiter
    emitter.instruction("test r8d, r8d");                                       // NUL cannot delimit
    emitter.instruction("jz __rt_preg_strip_err_delimiter_linux_x86_64");
    emitter.instruction("cmp r8d, 92");                                         // nor can a backslash
    emitter.instruction("je __rt_preg_strip_err_delimiter_linux_x86_64");
    for (base, span) in [(48, 10), (65, 26), (97, 26)] {
        emitter.instruction("mov r10d, r8d");                                   // shift the range to the bottom
        emitter.instruction(&format!("sub r10d, {base}"));
        emitter.instruction(&format!("cmp r10d, {span}"));                      // a digit or a letter?
        emitter.instruction("jb __rt_preg_strip_err_delimiter_linux_x86_64");
    }

    // -- bracket delimiters close with their mate; every other one closes with itself --
    emitter.instruction("mov r11d, r8d");                                       // default: the closing delimiter equals the opening one
    for (open, close) in [(40, 41), (123, 125), (91, 93), (60, 62)] {
        let next = format!("__rt_preg_strip_pair_{open}_linux_x86_64");
        emitter.instruction(&format!("cmp r8d, {open}"));                       // a bracket that pairs with its mate?
        emitter.instruction(&format!("jne {next}"));
        emitter.instruction(&format!("mov r11d, {close}"));                     // the mate closes the pattern
        emitter.label(&next);
    }

    // -- scan forward for the closing delimiter, as php does --
    emitter.instruction("mov r9, 1");                                           // r9 = scan index, just past the opening delimiter
    emitter.instruction("mov r10, 1");                                          // r10 = bracket nesting depth
    emitter.label("__rt_preg_strip_scan_linux_x86_64");
    emitter.instruction("cmp r9, rdx");                                         // ran off the end?
    emitter.instruction("jae __rt_preg_strip_err_no_end_linux_x86_64");         // php's "No ending delimiter"
    emitter.instruction("movzx ebx, BYTE PTR [rax + r9]");                      // the byte at the scan index
    emitter.instruction("cmp ebx, 92");                                         // a backslash escapes the next byte
    emitter.instruction("jne __rt_preg_strip_scan_plain_linux_x86_64");
    emitter.instruction("add r9, 2");                                           // skip the backslash and the byte it escapes
    emitter.instruction("jmp __rt_preg_strip_scan_linux_x86_64");
    emitter.label("__rt_preg_strip_scan_plain_linux_x86_64");
    emitter.instruction("cmp r8d, r11d");                                       // same opening and closing delimiter?
    emitter.instruction("je __rt_preg_strip_scan_same_linux_x86_64");
    emitter.instruction("cmp ebx, r8d");                                        // a nested opening bracket
    emitter.instruction("jne __rt_preg_strip_scan_close_linux_x86_64");
    emitter.instruction("add r10, 1");                                          // one level deeper
    emitter.instruction("jmp __rt_preg_strip_scan_next_linux_x86_64");
    emitter.label("__rt_preg_strip_scan_close_linux_x86_64");
    emitter.instruction("cmp ebx, r11d");                                       // a closing bracket
    emitter.instruction("jne __rt_preg_strip_scan_next_linux_x86_64");
    emitter.instruction("sub r10, 1");                                          // one level out
    emitter.instruction("jz __rt_preg_strip_found_linux_x86_64");               // back at the outermost level: the pattern ends here
    emitter.instruction("jmp __rt_preg_strip_scan_next_linux_x86_64");
    emitter.label("__rt_preg_strip_scan_same_linux_x86_64");
    emitter.instruction("cmp ebx, r11d");                                       // the closing delimiter?
    emitter.instruction("je __rt_preg_strip_found_linux_x86_64");
    emitter.label("__rt_preg_strip_scan_next_linux_x86_64");
    emitter.instruction("add r9, 1");                                           // next byte
    emitter.instruction("jmp __rt_preg_strip_scan_linux_x86_64");

    // -- every byte after the closing delimiter is a modifier --
    emitter.label("__rt_preg_strip_found_linux_x86_64");
    emitter.instruction("lea r10, [r9 + 1]");                                   // r10 = first modifier index
    emitter.label("__rt_preg_strip_mods_linux_x86_64");
    emitter.instruction("cmp r10, rdx");                                        // every modifier read?
    emitter.instruction("jae __rt_preg_strip_mods_done_linux_x86_64");
    emitter.instruction("movzx ebx, BYTE PTR [rax + r10]");                     // the next modifier
    for (index, (modifier, flag)) in MODIFIERS.iter().enumerate() {
        let next = format!("__rt_preg_strip_mod_{index}_linux_x86_64");
        emitter.instruction(&format!("cmp ebx, {modifier}"));                   // this modifier?
        emitter.instruction(&format!("jne {next}"));
        if *flag != 0 {
            emitter.instruction(&format!("or rcx, {flag}"));                    // its compile flag
        }
        emitter.instruction("jmp __rt_preg_strip_mod_next_linux_x86_64");
        emitter.label(&next);
    }
    emitter.instruction("jmp __rt_preg_strip_err_modifier_linux_x86_64");       // anything else is php's "Unknown modifier"
    emitter.label("__rt_preg_strip_mod_next_linux_x86_64");
    emitter.instruction("add r10, 1");
    emitter.instruction("jmp __rt_preg_strip_mods_linux_x86_64");
    emitter.label("__rt_preg_strip_mods_done_linux_x86_64");
    emitter.instruction("add rax, 1");                                          // skip the opening delimiter
    emitter.instruction("lea rdx, [r9 - 1]");                                   // length = closing index - 1
    emitter.instruction("pop rbx");
    emitter.instruction("ret");                                                 // stripped pattern in rax/rdx, flags in rcx

    // -- a rejected pattern: record php's reason for the compile that follows --
    emitter.label("__rt_preg_strip_err_empty_linux_x86_64");
    emitter.instruction(&format!("mov r10d, {PATTERN_EMPTY}"));
    emitter.instruction("xor r11d, r11d");
    emitter.instruction("jmp __rt_preg_strip_err_record_linux_x86_64");
    emitter.label("__rt_preg_strip_err_delimiter_linux_x86_64");
    emitter.instruction(&format!("mov r10d, {PATTERN_DELIMITER}"));
    emitter.instruction("xor r11d, r11d");
    emitter.instruction("jmp __rt_preg_strip_err_record_linux_x86_64");
    emitter.label("__rt_preg_strip_err_no_end_linux_x86_64");
    emitter.instruction(&format!("mov r10d, {PATTERN_NO_END}"));                // r11 already holds the missing delimiter
    emitter.instruction("jmp __rt_preg_strip_err_record_linux_x86_64");
    emitter.label("__rt_preg_strip_err_modifier_linux_x86_64");
    emitter.instruction(&format!("mov r10d, {PATTERN_MODIFIER}"));
    emitter.instruction("mov r11d, ebx");                                       // the modifier php does not know
    emitter.label("__rt_preg_strip_err_record_linux_x86_64");
    emitter.instruction("push rax");                                            // the pattern goes on unchanged; its compile will fail
    emitter.instruction("push rdx");
    emitter.instruction("push rdi");                                            // the C call may clobber what callers keep in rdi/rsi
    emitter.instruction("push rsi");                                            // four pushes after rbx keep the call 16-byte aligned
    emitter.instruction("mov edi, r10d");                                       // error kind
    emitter.instruction("mov esi, r11d");                                       // the byte the message names
    emitter.bl_c("elephc_pcre2_v1_record_pattern_error");
    emitter.instruction("pop rsi");
    emitter.instruction("pop rdi");
    emitter.instruction("pop rdx");
    emitter.instruction("pop rax");
    emitter.instruction("xor ecx, ecx");                                        // no flags for a rejected pattern
    emitter.instruction("pop rbx");
    emitter.instruction("ret");
}
