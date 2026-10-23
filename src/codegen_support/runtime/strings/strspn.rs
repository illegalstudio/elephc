//! Purpose:
//! Emits the `__rt_strspn` runtime helper assembly shared by PHP's `strspn` and `strcspn`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Mirrors php-src's `php_spn_common_handler` window rules: a negative `$offset` counts
//!   from the end and clamps at zero, an `$offset` past the end answers 0, a negative
//!   `$length` stops that many bytes before the end of the remaining tail, and an oversized
//!   `$length` (including the `i64::MAX` the lowering passes for `null`) is clamped.
//! - One helper serves both builtins: mode 0 counts leading bytes found in the mask
//!   (`strspn`), mode 1 counts leading bytes NOT found in it (`strcspn`). An empty mask makes
//!   `strcspn` count the whole window, as in php-src.
//! - The membership test is a linear scan of the mask, like php-src's `php_strspn`.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Mode selecting `strspn()` semantics: count bytes that ARE in the mask.
pub(crate) const STRSPN_MODE_SPAN: i64 = 0;

/// Mode selecting `strcspn()` semantics: count bytes that are NOT in the mask.
pub(crate) const STRSPN_MODE_COMPLEMENT: i64 = 1;

/// Emits the `__rt_strspn` runtime helper.
///
/// ABI (AArch64):
///   Input:  `x0` = mode, `x1`/`x2` = subject, `x3`/`x4` = mask, `x5` = offset,
///           `x6` = length (`i64::MAX` for null).
///   Output: `x0` = span length.
///
/// ABI (x86_64, internal):
///   Input:  `r10` = mode, `rdi`/`rsi` = subject, `rdx`/`rcx` = mask, `r8` = offset,
///           `r9` = length (`i64::MAX` for null).
///   Output: `rax` = span length.
///
/// The helper is a leaf and touches no stack.
pub fn emit_strspn(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strspn_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strspn / strcspn ---");
    emitter.label_global("__rt_strspn");

    // -- resolve the window start --
    emitter.instruction("cmp x5, #0");                                          // is the offset counted from the end?
    emitter.instruction("b.ge __rt_strspn_offset_positive");                    // non-negative offsets are checked against the length
    emitter.instruction("add x5, x5, x2");                                      // convert the negative offset into a byte index
    emitter.instruction("cmp x5, #0");                                          // did the offset reach before the subject start?
    emitter.instruction("csel x5, xzr, x5, lt");                                // clamp it to the subject start, as php-src does
    emitter.instruction("b __rt_strspn_offset_ready");                          // the start index is resolved

    emitter.label("__rt_strspn_offset_positive");
    emitter.instruction("cmp x5, x2");                                          // is the offset past the end of the subject?
    emitter.instruction("b.gt __rt_strspn_empty");                              // php-src answers 0 for an offset past the end

    // -- resolve the window length --
    emitter.label("__rt_strspn_offset_ready");
    emitter.instruction("sub x9, x2, x5");                                      // bytes remaining after the start index
    emitter.instruction("cmp x6, #0");                                          // is the length counted from the end?
    emitter.instruction("b.ge __rt_strspn_length_positive");                    // non-negative lengths only need clamping
    emitter.instruction("add x6, x9, x6");                                      // stop that many bytes before the end of the tail
    emitter.instruction("cmp x6, #0");                                          // were more bytes omitted than remain?
    emitter.instruction("csel x6, xzr, x6, lt");                                // an over-long omission selects an empty window
    emitter.instruction("b __rt_strspn_length_ready");                          // the window length is resolved

    emitter.label("__rt_strspn_length_positive");
    emitter.instruction("cmp x6, x9");                                          // does the length run past the end of the tail?
    emitter.instruction("csel x6, x9, x6, gt");                                 // clamp it to the remaining bytes

    // -- count leading bytes whose mask membership matches the mode --
    emitter.label("__rt_strspn_length_ready");
    emitter.instruction("add x1, x1, x5");                                      // point at the first byte of the window
    emitter.instruction("mov x10, #0");                                         // span length counted so far

    emitter.label("__rt_strspn_loop");
    emitter.instruction("cmp x10, x6");                                         // has the whole window been consumed?
    emitter.instruction("b.ge __rt_strspn_done");                               // the span covers the whole window
    emitter.instruction("ldrb w11, [x1, x10]");                                 // load the next window byte
    emitter.instruction("mov x12, #0");                                         // mask scan index
    emitter.instruction("mov x13, #0");                                         // assume the byte is not in the mask

    emitter.label("__rt_strspn_mask_loop");
    emitter.instruction("cmp x12, x4");                                         // has the whole mask been scanned?
    emitter.instruction("b.ge __rt_strspn_mask_done");                          // the byte is not in the mask
    emitter.instruction("ldrb w14, [x3, x12]");                                 // load the next mask byte
    emitter.instruction("add x12, x12, #1");                                    // advance the mask scan index
    emitter.instruction("cmp w14, w11");                                        // does the mask byte match the window byte?
    emitter.instruction("b.ne __rt_strspn_mask_loop");                          // keep scanning the mask on a mismatch
    emitter.instruction("mov x13, #1");                                         // the byte is in the mask

    emitter.label("__rt_strspn_mask_done");
    emitter.instruction("cmp x13, x0");                                         // strspn stops on a miss (0), strcspn on a hit (1)
    emitter.instruction("b.eq __rt_strspn_done");                               // the span ends at this byte
    emitter.instruction("add x10, x10, #1");                                    // the byte extends the span
    emitter.instruction("b __rt_strspn_loop");                                  // continue with the next window byte

    emitter.label("__rt_strspn_done");
    emitter.instruction("mov x0, x10");                                         // return the span length
    emitter.instruction("ret");                                                 // return to the generated call site

    emitter.label("__rt_strspn_empty");
    emitter.instruction("mov x0, #0");                                          // an offset past the end spans nothing
    emitter.instruction("ret");                                                 // return to the generated call site
}

/// Emits `__rt_strspn` for x86_64 Linux with the internal register ABI documented above.
fn emit_strspn_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strspn / strcspn ---");
    emitter.label_global("__rt_strspn");

    // -- resolve the window start --
    emitter.instruction("test r8, r8");                                         // is the offset counted from the end?
    emitter.instruction("jns __rt_strspn_offset_positive_linux_x86_64");        // non-negative offsets are checked against the length
    emitter.instruction("add r8, rsi");                                         // convert the negative offset into a byte index
    emitter.instruction("jns __rt_strspn_offset_ready_linux_x86_64");           // a start inside the subject is resolved
    emitter.instruction("xor r8d, r8d");                                        // clamp an offset before the subject start to zero
    emitter.instruction("jmp __rt_strspn_offset_ready_linux_x86_64");           // the start index is resolved

    emitter.label("__rt_strspn_offset_positive_linux_x86_64");
    emitter.instruction("cmp r8, rsi");                                         // is the offset past the end of the subject?
    emitter.instruction("jg __rt_strspn_empty_linux_x86_64");                   // php-src answers 0 for an offset past the end

    // -- resolve the window length --
    emitter.label("__rt_strspn_offset_ready_linux_x86_64");
    emitter.instruction("mov rax, rsi");                                        // copy the subject length
    emitter.instruction("sub rax, r8");                                         // bytes remaining after the start index
    emitter.instruction("test r9, r9");                                         // is the length counted from the end?
    emitter.instruction("jns __rt_strspn_length_positive_linux_x86_64");        // non-negative lengths only need clamping
    emitter.instruction("add r9, rax");                                         // stop that many bytes before the end of the tail
    emitter.instruction("jns __rt_strspn_length_ready_linux_x86_64");           // a non-negative window length is resolved
    emitter.instruction("xor r9d, r9d");                                        // an over-long omission selects an empty window
    emitter.instruction("jmp __rt_strspn_length_ready_linux_x86_64");           // the window length is resolved

    emitter.label("__rt_strspn_length_positive_linux_x86_64");
    emitter.instruction("cmp r9, rax");                                         // does the length run past the end of the tail?
    emitter.instruction("cmovg r9, rax");                                       // clamp it to the remaining bytes

    // -- count leading bytes whose mask membership matches the mode --
    emitter.label("__rt_strspn_length_ready_linux_x86_64");
    emitter.instruction("add rdi, r8");                                         // point at the first byte of the window
    emitter.instruction("xor eax, eax");                                        // span length counted so far

    emitter.label("__rt_strspn_loop_linux_x86_64");
    emitter.instruction("cmp rax, r9");                                         // has the whole window been consumed?
    emitter.instruction("jge __rt_strspn_done_linux_x86_64");                   // the span covers the whole window
    emitter.instruction("movzx r8d, BYTE PTR [rdi + rax]");                     // load the next window byte
    emitter.instruction("xor r11d, r11d");                                      // mask scan index
    emitter.instruction("xor esi, esi");                                        // assume the byte is not in the mask

    emitter.label("__rt_strspn_mask_loop_linux_x86_64");
    emitter.instruction("cmp r11, rcx");                                        // has the whole mask been scanned?
    emitter.instruction("jge __rt_strspn_mask_done_linux_x86_64");              // the byte is not in the mask
    emitter.instruction("cmp r8b, BYTE PTR [rdx + r11]");                       // does the mask byte match the window byte?
    emitter.instruction("lea r11, [r11 + 1]");                                  // advance the mask scan index without touching the flags
    emitter.instruction("jne __rt_strspn_mask_loop_linux_x86_64");              // keep scanning the mask on a mismatch
    emitter.instruction("mov esi, 1");                                          // the byte is in the mask

    emitter.label("__rt_strspn_mask_done_linux_x86_64");
    emitter.instruction("cmp rsi, r10");                                        // strspn stops on a miss (0), strcspn on a hit (1)
    emitter.instruction("je __rt_strspn_done_linux_x86_64");                    // the span ends at this byte
    emitter.instruction("add rax, 1");                                          // the byte extends the span
    emitter.instruction("jmp __rt_strspn_loop_linux_x86_64");                   // continue with the next window byte

    emitter.label("__rt_strspn_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return the span length in rax

    emitter.label("__rt_strspn_empty_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // an offset past the end spans nothing
    emitter.instruction("ret");                                                 // return to the generated call site
}
