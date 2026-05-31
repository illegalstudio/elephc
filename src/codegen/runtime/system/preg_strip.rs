//! Purpose:
//! Emits the `__rt_preg_strip`, `__rt_preg_strip_done` runtime helper assembly for preg strip.
//! Keeps PHP regex delimiter handling and PCRE2 POSIX-wrapper flag extraction in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - Regex helpers preserve PHP PCRE-flavored pattern bytes and pass PCRE2 POSIX-wrapper flags downstream.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits the `__rt_preg_strip` runtime helper for stripping PHP regex delimiters.
///
/// Transforms PHP PCRE patterns by removing leading/trailing '/' delimiters and extracting
/// PCRE2 POSIX-wrapper flags. For example, `"/pattern/i"` becomes
/// `("pattern", REG_ICASE)`.
///
/// Dispatches to `emit_preg_strip_linux_x86_64` on x86_64; ARM64 uses inline scalar
/// loads/stores in the main emitter. Undelimited patterns (no leading '/') are returned
/// unchanged with flags=0.
///
/// Input:  x1=pattern ptr, x2=pattern len
/// Output: x1=stripped pattern ptr, x2=stripped len, x3=PCRE2 POSIX cflags
pub(crate) fn emit_preg_strip(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_strip_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: preg_strip_delimiters ---");
    emitter.label_global("__rt_preg_strip");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set new frame pointer

    emitter.instruction("str x1, [sp, #0]");                                    // save pattern ptr
    emitter.instruction("str x2, [sp, #8]");                                    // save pattern len
    emitter.instruction("mov x3, #0");                                          // flags = 0
    emitter.instruction("str x3, [sp, #16]");                                   // save flags

    // -- check if pattern starts with '/' --
    emitter.instruction("ldrb w9, [x1]");                                       // load first byte
    emitter.instruction("cmp w9, #47");                                         // compare with '/'
    emitter.instruction("b.ne __rt_preg_strip_done");                           // not delimited, return as-is

    // -- find closing delimiter by scanning from the end --
    emitter.instruction("sub x10, x2, #1");                                     // start from last char
    emitter.label("__rt_preg_strip_scan");
    emitter.instruction("cmp x10, #1");                                         // must have at least 1 char between delimiters
    emitter.instruction("b.lt __rt_preg_strip_done");                           // no closing delimiter found
    emitter.instruction("ldrb w9, [x1, x10]");                                  // load byte at position
    emitter.instruction("cmp w9, #47");                                         // check for closing '/'
    emitter.instruction("b.eq __rt_preg_strip_found");                          // found it

    // -- check supported PCRE2 POSIX-wrapper flags --
    emitter.instruction("ldr x3, [sp, #16]");                                   // load the accumulated PCRE2 POSIX compile flags
    emitter.instruction("cmp w9, #105");                                        // check for 'i' case-insensitive modifier
    emitter.instruction("b.ne __rt_preg_strip_flag_m");                         // try the next supported regex modifier
    emitter.instruction("orr x3, x3, #1");                                      // add REG_ICASE for case-insensitive matching
    emitter.instruction("b __rt_preg_strip_save_flag");                         // save the updated flag word
    emitter.label("__rt_preg_strip_flag_m");
    emitter.instruction("cmp w9, #109");                                        // check for 'm' multiline modifier
    emitter.instruction("b.ne __rt_preg_strip_flag_s");                         // try the next supported regex modifier
    emitter.instruction("orr x3, x3, #2");                                      // add REG_NEWLINE so PCRE2 treats anchors as multiline
    emitter.instruction("b __rt_preg_strip_save_flag");                         // save the updated flag word
    emitter.label("__rt_preg_strip_flag_s");
    emitter.instruction("cmp w9, #115");                                        // check for 's' dotall modifier
    emitter.instruction("b.ne __rt_preg_strip_flag_u");                         // try the next supported regex modifier
    emitter.instruction("orr x3, x3, #16");                                     // add REG_DOTALL so '.' can match newlines
    emitter.instruction("b __rt_preg_strip_save_flag");                         // save the updated flag word
    emitter.label("__rt_preg_strip_flag_u");
    emitter.instruction("cmp w9, #117");                                        // check for 'u' UTF-8 modifier
    emitter.instruction("b.ne __rt_preg_strip_flag_U");                         // try the next supported regex modifier
    emitter.instruction("mov x12, #1088");                                      // materialize REG_UTF | REG_UCP for Unicode-aware PCRE2 matching
    emitter.instruction("orr x3, x3, x12");                                     // add UTF and Unicode-property matching flags
    emitter.instruction("b __rt_preg_strip_save_flag");                         // save the updated flag word
    emitter.label("__rt_preg_strip_flag_U");
    emitter.instruction("cmp w9, #85");                                         // check for 'U' ungreedy modifier
    emitter.instruction("b.ne __rt_preg_strip_skip_flag");                      // ignore unsupported trailing modifiers for now
    emitter.instruction("orr x3, x3, #512");                                    // add REG_UNGREEDY for inverted quantifier greediness
    emitter.label("__rt_preg_strip_save_flag");
    emitter.instruction("str x3, [sp, #16]");                                   // save accumulated PCRE2 POSIX flags
    emitter.label("__rt_preg_strip_skip_flag");
    emitter.instruction("sub x10, x10, #1");                                    // move backward
    emitter.instruction("b __rt_preg_strip_scan");                              // continue scanning

    // -- found closing delimiter at x10 --
    emitter.label("__rt_preg_strip_found");
    emitter.instruction("add x1, x1, #1");                                      // skip opening delimiter
    emitter.instruction("sub x2, x10, #1");                                     // length = closing_pos - 1

    emitter.label("__rt_preg_strip_done");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload flags

    // -- tear down and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return
}

/// x86_64-specific emitter for `__rt_preg_strip`.
///
/// Uses System V AMD64 ABI: pattern pointer in `rax`, length in `rdx`; returns stripped
/// pattern pointer in `rax`, stripped length in `rdx`, and PCRE2 POSIX cflags in `rcx`.
/// Clobbers `r8`, `r9` as temporaries during the reverse scan. Returns patterns unchanged
/// when they do not begin with '/' (undelimited raw regex payloads).
fn emit_preg_strip_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_strip_delimiters ---");
    emitter.label_global("__rt_preg_strip");

    emitter.instruction("xor ecx, ecx");                                        // clear the regex flag accumulator so undelimited patterns default to no modifiers
    emitter.instruction("test rdx, rdx");                                       // skip delimiter stripping when the pattern string is empty
    emitter.instruction("jz __rt_preg_strip_done_linux_x86_64");                // empty patterns already behave like raw undelimited regex payloads
    emitter.instruction("movzx r8d, BYTE PTR [rax]");                           // load the first pattern byte so delimiter detection can inspect the opening character
    emitter.instruction("cmp r8d, 47");                                         // test whether the pattern starts with the canonical '/' regex delimiter
    emitter.instruction("jne __rt_preg_strip_done_linux_x86_64");               // return the original pattern unchanged when it is not slash-delimited
    emitter.instruction("mov r9, rdx");                                         // seed the reverse scan cursor from the full source pattern length
    emitter.instruction("sub r9, 1");                                           // start scanning from the final byte looking for flags or the closing delimiter

    emitter.label("__rt_preg_strip_scan_linux_x86_64");
    emitter.instruction("cmp r9, 1");                                           // stop when there is no room left for a distinct closing delimiter
    emitter.instruction("jl __rt_preg_strip_done_linux_x86_64");                // malformed slash patterns fall back to the original undelimited payload
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the current reverse-scan byte from the slash-delimited pattern literal
    emitter.instruction("cmp r8d, 47");                                         // did the reverse scan find the closing '/' delimiter?
    emitter.instruction("je __rt_preg_strip_found_linux_x86_64");               // stop once the closing slash delimiter is located
    emitter.instruction("cmp r8d, 105");                                        // detect the trailing 'i' case-insensitive modifier while walking backward
    emitter.instruction("jne __rt_preg_strip_flag_m_linux_x86_64");             // try the next supported regex modifier
    emitter.instruction("or rcx, 1");                                           // record REG_ICASE for PCRE2 POSIX compilation
    emitter.instruction("jmp __rt_preg_strip_skip_flag_linux_x86_64");          // continue scanning toward the opening delimiter
    emitter.label("__rt_preg_strip_flag_m_linux_x86_64");
    emitter.instruction("cmp r8d, 109");                                        // detect the trailing 'm' multiline modifier
    emitter.instruction("jne __rt_preg_strip_flag_s_linux_x86_64");             // try the next supported regex modifier
    emitter.instruction("or rcx, 2");                                           // record REG_NEWLINE for multiline anchor behavior
    emitter.instruction("jmp __rt_preg_strip_skip_flag_linux_x86_64");          // continue scanning toward the opening delimiter
    emitter.label("__rt_preg_strip_flag_s_linux_x86_64");
    emitter.instruction("cmp r8d, 115");                                        // detect the trailing 's' dotall modifier
    emitter.instruction("jne __rt_preg_strip_flag_u_linux_x86_64");             // try the next supported regex modifier
    emitter.instruction("or rcx, 16");                                          // record REG_DOTALL so '.' can match newlines
    emitter.instruction("jmp __rt_preg_strip_skip_flag_linux_x86_64");          // continue scanning toward the opening delimiter
    emitter.label("__rt_preg_strip_flag_u_linux_x86_64");
    emitter.instruction("cmp r8d, 117");                                        // detect the trailing 'u' UTF-8 modifier
    emitter.instruction("jne __rt_preg_strip_flag_U_linux_x86_64");             // try the next supported regex modifier
    emitter.instruction("or rcx, 1088");                                        // record REG_UTF | REG_UCP for Unicode-aware PCRE2 matching
    emitter.instruction("jmp __rt_preg_strip_skip_flag_linux_x86_64");          // continue scanning toward the opening delimiter
    emitter.label("__rt_preg_strip_flag_U_linux_x86_64");
    emitter.instruction("cmp r8d, 85");                                         // detect the trailing 'U' ungreedy modifier
    emitter.instruction("jne __rt_preg_strip_skip_flag_linux_x86_64");          // ignore unsupported trailing modifiers for now
    emitter.instruction("or rcx, 512");                                         // record REG_UNGREEDY for inverted quantifier greediness

    emitter.label("__rt_preg_strip_skip_flag_linux_x86_64");
    emitter.instruction("sub r9, 1");                                           // move the reverse scan cursor one byte closer to the opening delimiter
    emitter.instruction("jmp __rt_preg_strip_scan_linux_x86_64");               // continue scanning for the closing delimiter and trailing regex modifiers

    emitter.label("__rt_preg_strip_found_linux_x86_64");
    emitter.instruction("add rax, 1");                                          // skip the opening '/' delimiter so the stripped pattern starts at the first payload byte
    emitter.instruction("lea rdx, [r9 - 1]");                                   // compute the stripped payload length as closing_delimiter_index - 1

    emitter.label("__rt_preg_strip_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return the stripped pattern in rax/rdx and the modifier flags in rcx
}
