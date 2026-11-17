//! Purpose:
//! Emits `__rt_strip_tags` for PHP 8.5 `strip_tags()`, including the optional
//! allow-list, for every supported target.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Ports php-src `php_strip_tags_ex(..., allow_tag_spaces=0)` and `php_tag_find`.
//! - Input is a subject pointer/length plus an allow-list pointer/length. A zero
//!   allow length means "strip every tag". Output is a fresh string in the
//!   target string-result registers, reserved through `__rt_concat_reserve`.
//! - HTML comments and PHP tags are always stripped and cannot be allow-listed.
//! - Oversized output reservations stay transient until the helper adopts them into
//!   independently owned string storage before returning.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::CONCAT_TEMP_HEAP_KIND;

mod aarch64;
mod x86_64;

/// Emits `__rt_strip_tags` for the active target.
pub fn emit_strip_tags(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_strip_tags_linux_x86_64(emitter);
        return;
    }
    aarch64::emit_strip_tags_aarch64(emitter);
}

/// Emits `__rt_strip_tags_count`, the allocation-free no-allow-list length scanner.
///
/// The helper counts retained text bytes and skips ordinary tags without reserving an
/// output buffer. Comments, PHP/ASP tags, declarations and nested angle brackets tail-call
/// the full scanner in count mode so their exact semantics remain in one implementation.
pub fn emit_strip_tags_count(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strip_tags_count_x86_64(emitter);
        return;
    }
    emit_strip_tags_count_aarch64(emitter);
}

/// Emits the AArch64 no-allow-list count scanner.
fn emit_strip_tags_count_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strip_tags_count ---");
    emitter.label_global("__rt_strip_tags_count");
    emitter.instruction("mov x3, xzr");                                         // start at the first subject byte
    emitter.instruction("mov x4, xzr");                                         // retained byte count starts at zero
    emitter.label("__rt_strip_tags_count_loop");
    emitter.instruction("cmp x3, x2");                                          // has the subject ended?
    emitter.instruction("b.hs __rt_strip_tags_count_done");                     // return the retained byte count
    emitter.instruction("ldrb w5, [x1, x3]");                                   // load the next subject byte
    emitter.instruction("cmp w5, #60");                                         // is this an opening delimiter?
    emitter.instruction("b.ne __rt_strip_tags_count_text");                     // count ordinary text
    emitter.instruction("add x6, x3, #1");                                      // address the byte after '<'
    emitter.instruction("cmp x6, x2");                                          // is the opener the final byte?
    emitter.instruction("b.hs __rt_strip_tags_count_done");                     // an unterminated tag is discarded
    emitter.instruction("ldrb w8, [x1, x6]");                                   // load the byte after '<'
    emitter.instruction("cmp w8, #32");                                         // is the opener followed by a space?
    emitter.instruction("b.eq __rt_strip_tags_count_literal");                  // whitespace keeps '<' as text
    emitter.instruction("cmp w8, #9");                                          // is the lookahead below the whitespace range?
    emitter.instruction("b.lo __rt_strip_tags_count_tag_prefix");               // a control byte is not whitespace
    emitter.instruction("cmp w8, #13");                                         // is the lookahead inside the whitespace range?
    emitter.instruction("b.ls __rt_strip_tags_count_literal");                  // tab/newline/form-feed keeps '<' as text
    emitter.label("__rt_strip_tags_count_tag_prefix");
    emitter.instruction("cmp w8, #33");                                         // is this a declaration or comment?
    emitter.instruction("b.eq __rt_strip_tags_count_fallback");                 // let the full scanner handle it
    emitter.instruction("cmp w8, #37");                                         // is this an ASP tag?
    emitter.instruction("b.eq __rt_strip_tags_count_fallback");                 // let the full scanner handle it
    emitter.instruction("cmp w8, #63");                                         // is this a PHP tag?
    emitter.instruction("b.eq __rt_strip_tags_count_fallback");                 // let the full scanner handle it
    emitter.instruction("mov x7, xzr");                                         // no active attribute quote yet
    emitter.label("__rt_strip_tags_count_tag_loop");
    emitter.instruction("cmp x6, x2");                                          // has the tag reached EOF?
    emitter.instruction("b.hs __rt_strip_tags_count_done");                     // an unterminated tag is discarded
    emitter.instruction("ldrb w8, [x1, x6]");                                   // load the next tag byte
    emitter.instruction("cbz w8, __rt_strip_tags_count_tag_next");              // embedded NULs are skipped
    emitter.instruction("cbnz x7, __rt_strip_tags_count_quote");                // handle an active quoted attribute
    emitter.instruction("cmp w8, #34");                                         // is this a double quote?
    emitter.instruction("b.eq __rt_strip_tags_count_open_quote");               // skip the quoted attribute
    emitter.instruction("cmp w8, #39");                                         // is this a single quote?
    emitter.instruction("b.eq __rt_strip_tags_count_open_quote");               // skip the quoted attribute
    emitter.instruction("cmp w8, #60");                                         // is this a nested angle bracket?
    emitter.instruction("b.eq __rt_strip_tags_count_fallback");                 // the full machine tracks nesting
    emitter.instruction("cmp w8, #62");                                         // is this the tag closer?
    emitter.instruction("b.eq __rt_strip_tags_count_tag_close");                // consume the complete tag
    emitter.label("__rt_strip_tags_count_tag_next");
    emitter.instruction("add x6, x6, #1");                                      // consume one tag byte
    emitter.instruction("b __rt_strip_tags_count_tag_loop");                    // scan the next tag byte
    emitter.label("__rt_strip_tags_count_quote");
    emitter.instruction("cmp w8, w7");                                          // is this the matching quote?
    emitter.instruction("b.eq __rt_strip_tags_count_close_quote");              // resume ordinary tag scanning
    emitter.instruction("b __rt_strip_tags_count_tag_next");                    // consume one quoted byte
    emitter.label("__rt_strip_tags_count_open_quote");
    emitter.instruction("mov w7, w8");                                          // remember the active quote byte
    emitter.instruction("b __rt_strip_tags_count_tag_next");                    // consume the opening quote
    emitter.label("__rt_strip_tags_count_close_quote");
    emitter.instruction("mov x7, xzr");                                         // leave the quoted attribute
    emitter.instruction("b __rt_strip_tags_count_tag_next");                    // resume ordinary tag scanning
    emitter.label("__rt_strip_tags_count_tag_close");
    emitter.instruction("add x3, x6, #1");                                      // continue after the tag closer
    emitter.instruction("b __rt_strip_tags_count_loop");                        // scan the next text run
    emitter.label("__rt_strip_tags_count_literal");
    emitter.instruction("add x4, x4, #1");                                      // an escaped '<' is retained as text
    emitter.instruction("add x3, x3, #1");                                      // consume the literal '<'
    emitter.instruction("b __rt_strip_tags_count_loop");                        // scan the next text run
    emitter.label("__rt_strip_tags_count_text");
    emitter.label("__rt_strip_tags_count_text_block");
    emitter.instruction("add x9, x3, #16");                                     // tentatively take a full sixteen-byte block
    emitter.instruction("cmp x9, x2");                                          // does the block stay inside the subject?
    emitter.instruction("b.hi __rt_strip_tags_count_text_scalar");              // the tail is handled byte by byte
    emitter.instruction("ldr q0, [x1, x3]");                                    // load sixteen text bytes
    emitter.instruction("movi v1.16b, #60");                                    // '<' delimiter
    emitter.instruction("cmeq v1.16b, v0.16b, v1.16b");                         // mark '<' bytes
    emitter.instruction("cmeq v2.16b, v0.16b, #0");                             // mark embedded NUL bytes
    emitter.instruction("orr v1.16b, v1.16b, v2.16b");                          // a special byte starts the scalar path
    emitter.instruction("umaxv b1, v1.16b");                                    // did any byte need scalar handling?
    emitter.instruction("umov w9, v1.b[0]");                                    // move the block verdict into an integer register
    emitter.instruction("cbnz w9, __rt_strip_tags_count_text_scalar");          // fall back to the scalar loop
    emitter.instruction("add x4, x4, #16");                                     // count the whole text block
    emitter.instruction("add x3, x3, #16");                                     // consume the copied block
    emitter.instruction("b __rt_strip_tags_count_text_block");                  // scan the next block
    emitter.label("__rt_strip_tags_count_text_scalar");
    emitter.instruction("cmp x3, x2");                                          // has the subject ended?
    emitter.instruction("b.hs __rt_strip_tags_count_done");                     // return the retained byte count
    emitter.instruction("ldrb w5, [x1, x3]");                                   // load the next text byte
    emitter.instruction("cbz w5, __rt_strip_tags_count_text_skip");             // embedded NULs are not counted
    emitter.instruction("cmp w5, #60");                                         // is this an opening delimiter?
    emitter.instruction("b.eq __rt_strip_tags_count_loop");                     // dispatch the tag opener
    emitter.instruction("add x4, x4, #1");                                      // count one retained text byte
    emitter.label("__rt_strip_tags_count_text_skip");
    emitter.instruction("add x3, x3, #1");                                      // consume the text byte
    emitter.instruction("b __rt_strip_tags_count_text_scalar");                 // continue with the tail
    emitter.label("__rt_strip_tags_count_fallback");
    emitter.instruction("mov x3, xzr");                                         // empty allow-list pointer
    emitter.instruction("mov x4, xzr");                                         // empty allow-list length
    emitter.instruction("mov x5, #1");                                          // request the count-only result
    emitter.instruction("b __rt_strip_tags");                                   // reuse the full scanner for complex markup
    emitter.label("__rt_strip_tags_count_done");
    emitter.instruction("mov x0, x4");                                          // return the retained byte count
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 no-allow-list count scanner.
fn emit_strip_tags_count_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strip_tags_count ---");
    emitter.label_global("__rt_strip_tags_count");
    emitter.instruction("mov rsi, rax");                                        // rsi = subject pointer
    emitter.instruction("mov rcx, rdx");                                        // rcx = subject length
    emitter.instruction("xor r8d, r8d");                                        // start at the first subject byte
    emitter.instruction("xor r9d, r9d");                                        // retained byte count starts at zero
    emitter.label("__rt_strip_tags_count_loop_x86");
    emitter.instruction("cmp r8, rcx");                                         // has the subject ended?
    emitter.instruction("jae __rt_strip_tags_count_done_x86");                  // return the retained byte count
    emitter.instruction("movzx r10d, BYTE PTR [rsi + r8]");                     // load the next subject byte
    emitter.instruction("cmp r10d, 60");                                        // is this an opening delimiter?
    emitter.instruction("jne __rt_strip_tags_count_text_x86");                  // count ordinary text
    emitter.instruction("lea r11, [r8 + 1]");                                   // address the byte after '<'
    emitter.instruction("cmp r11, rcx");                                        // is the opener the final byte?
    emitter.instruction("jae __rt_strip_tags_count_done_x86");                  // an unterminated tag is discarded
    emitter.instruction("movzx eax, BYTE PTR [rsi + r11]");                     // load the byte after '<'
    emitter.instruction("cmp eax, 32");                                         // is the opener followed by a space?
    emitter.instruction("je __rt_strip_tags_count_literal_x86");                // whitespace keeps '<' as text
    emitter.instruction("cmp eax, 9");                                          // is the lookahead below the whitespace range?
    emitter.instruction("jb __rt_strip_tags_count_tag_prefix_x86");             // a control byte is not whitespace
    emitter.instruction("cmp eax, 13");                                         // is the lookahead inside the whitespace range?
    emitter.instruction("jbe __rt_strip_tags_count_literal_x86");               // tab/newline/form-feed keeps '<' as text
    emitter.label("__rt_strip_tags_count_tag_prefix_x86");
    emitter.instruction("cmp eax, 33");                                         // is this a declaration or comment?
    emitter.instruction("je __rt_strip_tags_count_fallback_x86");               // let the full scanner handle it
    emitter.instruction("cmp eax, 37");                                         // is this an ASP tag?
    emitter.instruction("je __rt_strip_tags_count_fallback_x86");               // let the full scanner handle it
    emitter.instruction("cmp eax, 63");                                         // is this a PHP tag?
    emitter.instruction("je __rt_strip_tags_count_fallback_x86");               // let the full scanner handle it
    emitter.instruction("xor edi, edi");                                        // no active attribute quote yet
    emitter.label("__rt_strip_tags_count_tag_loop_x86");
    emitter.instruction("cmp r11, rcx");                                        // has the tag reached EOF?
    emitter.instruction("jae __rt_strip_tags_count_done_x86");                  // an unterminated tag is discarded
    emitter.instruction("movzx eax, BYTE PTR [rsi + r11]");                     // load the next tag byte
    emitter.instruction("test eax, eax");                                       // is this an embedded NUL?
    emitter.instruction("jz __rt_strip_tags_count_tag_next_x86");               // embedded NULs are skipped
    emitter.instruction("test edi, edi");                                       // is an attribute quote active?
    emitter.instruction("jnz __rt_strip_tags_count_quote_x86");                 // handle the quoted byte
    emitter.instruction("cmp eax, 34");                                         // is this a double quote?
    emitter.instruction("je __rt_strip_tags_count_open_quote_x86");             // skip the quoted attribute
    emitter.instruction("cmp eax, 39");                                         // is this a single quote?
    emitter.instruction("je __rt_strip_tags_count_open_quote_x86");             // skip the quoted attribute
    emitter.instruction("cmp eax, 60");                                         // is this a nested angle bracket?
    emitter.instruction("je __rt_strip_tags_count_fallback_x86");               // the full machine tracks nesting
    emitter.instruction("cmp eax, 62");                                         // is this the tag closer?
    emitter.instruction("je __rt_strip_tags_count_tag_close_x86");              // consume the complete tag
    emitter.label("__rt_strip_tags_count_tag_next_x86");
    emitter.instruction("inc r11");                                             // consume one tag byte
    emitter.instruction("jmp __rt_strip_tags_count_tag_loop_x86");              // scan the next tag byte
    emitter.label("__rt_strip_tags_count_quote_x86");
    emitter.instruction("cmp eax, edi");                                        // is this the matching quote?
    emitter.instruction("je __rt_strip_tags_count_close_quote_x86");            // resume ordinary tag scanning
    emitter.instruction("jmp __rt_strip_tags_count_tag_next_x86");              // consume one quoted byte
    emitter.label("__rt_strip_tags_count_open_quote_x86");
    emitter.instruction("mov edi, eax");                                        // remember the active quote byte
    emitter.instruction("jmp __rt_strip_tags_count_tag_next_x86");              // consume the opening quote
    emitter.label("__rt_strip_tags_count_close_quote_x86");
    emitter.instruction("xor edi, edi");                                        // leave the quoted attribute
    emitter.instruction("jmp __rt_strip_tags_count_tag_next_x86");              // resume ordinary tag scanning
    emitter.label("__rt_strip_tags_count_tag_close_x86");
    emitter.instruction("lea r8, [r11 + 1]");                                   // continue after the tag closer
    emitter.instruction("jmp __rt_strip_tags_count_loop_x86");                  // scan the next text run
    emitter.label("__rt_strip_tags_count_literal_x86");
    emitter.instruction("inc r9");                                              // an escaped '<' is retained as text
    emitter.instruction("inc r8");                                              // consume the literal '<'
    emitter.instruction("jmp __rt_strip_tags_count_loop_x86");                  // scan the next text run
    emitter.label("__rt_strip_tags_count_text_x86");
    emitter.label("__rt_strip_tags_count_text_block_x86");
    emitter.instruction("lea r10, [r8 + 16]");                                  // tentatively take a full sixteen-byte block
    emitter.instruction("cmp r10, rcx");                                        // does the block stay inside the subject?
    emitter.instruction("ja __rt_strip_tags_count_text_scalar_x86");            // the tail is handled byte by byte
    emitter.instruction("movdqu xmm0, XMMWORD PTR [rsi + r8]");                 // load sixteen text bytes
    emitter.instruction("pxor xmm1, xmm1");                                     // clear the NUL mask
    emitter.instruction("pcmpeqb xmm1, xmm0");                                  // mark embedded NUL bytes
    emitter.instruction("mov eax, 0x3c3c3c3c");                                 // broadcast the '<' delimiter
    emitter.instruction("movd xmm2, eax");                                      // move the delimiter into an XMM register
    emitter.instruction("pshufd xmm2, xmm2, 0");                                // replicate it across all sixteen bytes
    emitter.instruction("pcmpeqb xmm2, xmm0");                                  // mark '<' bytes
    emitter.instruction("por xmm1, xmm2");                                      // a special byte starts the scalar path
    emitter.instruction("pmovmskb eax, xmm1");                                  // move the block verdict into an integer register
    emitter.instruction("test eax, eax");                                       // did any byte need scalar handling?
    emitter.instruction("jnz __rt_strip_tags_count_text_scalar_x86");           // fall back to the scalar loop
    emitter.instruction("add r9, 16");                                          // count the whole text block
    emitter.instruction("add r8, 16");                                          // consume the copied block
    emitter.instruction("jmp __rt_strip_tags_count_text_block_x86");            // scan the next block
    emitter.label("__rt_strip_tags_count_text_scalar_x86");
    emitter.instruction("cmp r8, rcx");                                         // has the subject ended?
    emitter.instruction("jae __rt_strip_tags_count_done_x86");                  // return the retained byte count
    emitter.instruction("movzx r10d, BYTE PTR [rsi + r8]");                     // load the next text byte
    emitter.instruction("test r10d, r10d");                                     // is this an embedded NUL?
    emitter.instruction("jz __rt_strip_tags_count_text_skip_x86");              // embedded NULs are not counted
    emitter.instruction("cmp r10d, 60");                                        // is this an opening delimiter?
    emitter.instruction("je __rt_strip_tags_count_loop_x86");                   // dispatch the tag opener
    emitter.instruction("inc r9");                                              // count one retained text byte
    emitter.label("__rt_strip_tags_count_text_skip_x86");
    emitter.instruction("inc r8");                                              // consume the text byte
    emitter.instruction("jmp __rt_strip_tags_count_text_scalar_x86");           // continue with the tail
    emitter.label("__rt_strip_tags_count_fallback_x86");
    emitter.instruction("mov rdi, rsi");                                        // subject pointer for the full scanner
    emitter.instruction("mov rsi, rcx");                                        // subject length for the full scanner
    emitter.instruction("xor edx, edx");                                        // empty allow-list pointer
    emitter.instruction("xor ecx, ecx");                                        // empty allow-list length
    emitter.instruction("mov r8d, 1");                                          // request the count-only result
    emitter.instruction("jmp __rt_strip_tags");                                 // reuse the full scanner for complex markup
    emitter.label("__rt_strip_tags_count_done_x86");
    emitter.instruction("mov rax, r9");                                         // return the retained byte count
    emitter.instruction("ret");                                                 // return to the caller
}

/// Marks only heap-backed reservations as temporary so final persistence can adopt them.
pub(super) fn emit_mark_reserved_result_temporary(emitter: &mut Emitter) {
    let ready = "__rt_strip_tags_reservation_ready";
    // -- retain scratch semantics, but transfer oversized reservations to their consumer --
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_concat_buf");
            emitter.instruction("sub x10, x0, x9");                             // classify the reservation by its unsigned offset from concat scratch
            abi::emit_load_int_immediate(emitter, "x11", emitter.concat_capacity as i64);
            emitter.instruction("cmp x10, x11");                                // scratch reservations do not own a heap header
            emitter.instruction(&format!("b.lo {}", ready));                    // leave scratch storage untagged for ordinary persistence
            abi::emit_load_int_immediate(emitter, "x9", CONCAT_TEMP_HEAP_KIND as i64);
            emitter.instruction("str x9, [x0, #-8]");                           // let string persistence adopt this sole heap reservation instead of copying it
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", "_concat_buf");
            emitter.instruction("mov r11, rax");                                // preserve the output pointer while classifying its reservation
            emitter.instruction("sub r11, r10");                                // an unsigned out-of-window offset identifies owned heap fallback storage
            crate::codegen_support::abi::emit_load_int_immediate(emitter, "r9", emitter.concat_capacity as i64); // load the shared scratch capacity
            emitter.instruction("cmp r11, r9");                                 // scratch reservations do not own a heap header
            emitter.instruction(&format!("jb {}", ready));                      // leave scratch storage untagged for ordinary persistence
            abi::emit_load_int_immediate(emitter, "r10",
                crate::codegen_support::sentinels::x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND) as i64);
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // let string persistence adopt the sole heap reservation with its required kind marker
        }
    }
    emitter.label(ready);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Emits the strip-tags scanner for one target and returns the assembly text.
    fn strip_tags_asm(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_strip_tags(&mut emitter);
        emitter.output()
    }

    /// The no-allow-list fast path copies text with a sixteen-byte block and skips
    /// ordinary tags without entering the full state machine.
    #[test]
    fn strip_tags_no_allow_list_uses_the_fast_path() {
        let arm = strip_tags_asm(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("__rt_strip_tags_fast_entry:"), "{arm}");
        assert!(arm.contains("str q0, [x23], #16"), "{arm}");

        let x64 = strip_tags_asm(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x64.contains("__rt_strip_tags_fast_entry_x86:"), "{x64}");
        assert!(x64.contains("movdqu XMMWORD PTR [rbx], xmm0"), "{x64}");
    }

    /// The hot output cursor stays in a callee-saved register instead of being
    /// reloaded and stored around every retained byte.
    #[test]
    fn strip_tags_output_cursor_stays_in_a_register() {
        let arm = strip_tags_asm(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("strb w22, [x23], #1"), "{arm}");
        assert!(arm.contains("add x23, x23, x2"), "{arm}");

        let x64 = strip_tags_asm(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x64.contains("mov BYTE PTR [rbx], r15b"), "{x64}");
        assert!(x64.contains("add rbx, rcx"), "{x64}");
    }

    /// Every target adopts the reservation into independently owned storage before returning.
    #[test]
    fn strip_tags_output_is_independent_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_strip_tags(&mut emitter);
            let assembly = emitter.output();
            let (reserve, mark, save, publish, persist) = if target.arch == Arch::AArch64 {
                (
                    "bl __rt_concat_reserve",
                    "str x9, [x0, #-8]",
                    "str x0, [x29, #-40]",
                    "bl __rt_concat_publish",
                    "bl __rt_str_persist",
                )
            } else {
                (
                    "call __rt_concat_reserve",
                    "mov QWORD PTR [rax - 8], r10",
                    "mov QWORD PTR [rbp - 40], rax",
                    "call __rt_concat_publish",
                    "call __rt_str_persist",
                )
            };
            assert!(assembly.find(reserve).unwrap() < assembly.find(mark).unwrap(), "{name}");
            assert!(assembly.find(mark).unwrap() < assembly.find(save).unwrap(), "{name}");
            assert!(assembly.find(publish).unwrap() < assembly.find(persist).unwrap(), "{name}");
            assert!(assembly.contains("__rt_strip_tags_reservation_ready"), "{name}: guard scratch pointers");
        }
    }
}
