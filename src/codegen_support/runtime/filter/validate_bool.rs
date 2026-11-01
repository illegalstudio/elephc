//! Purpose:
//! Emits the `__rt_filter_validate_bool_str` runtime helper: matches a PHP
//! string against `filter_var()`'s `FILTER_VALIDATE_BOOL(EAN)` token set
//! after trimming PHP-filter whitespace.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::filter`.
//! - `crate::codegen::lower_inst::builtins::filter` (the `filter_var()` EIR lowering).
//!
//! Key details:
//! - Token set (php-verified with `php -n -r 'var_dump(filter_var(..., FILTER_VALIDATE_BOOL));'`,
//!   PHP 8.5.6 local), matched case-insensitively after trimming: `"1"`,
//!   `"true"`, `"on"`, `"yes"` -> true; `"0"`, `"false"`, `"off"`, `"no"`, `""`
//!   (including whitespace-only input, which trims to `""`) -> valid false;
//!   anything else -> NOT MATCHED (the caller applies `FILTER_NULL_ON_FAILURE`).
//!   `""` is a VALID false result, not a failure — `filter_var(" ", ...,
//!   FILTER_NULL_ON_FAILURE)` still returns `false`, not `null`.
//! - Returns a THREE-way result (0=false, 1=true, 2=no match) so the caller can
//!   distinguish "matched false" from "failed to match" per the flag.
//! - Reuses the existing `__rt_strcasecmp` case-insensitive comparator (an
//!   existing string primitive, not a filter-specific parser) for the multi-byte
//!   tokens; a length mismatch alone already makes it return nonzero, so every
//!   candidate can be probed unconditionally without a length pre-check.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_filter_validate_bool_str` for the host target.
///
/// AArch64: input x1=ptr, x2=len. Output: x0 = 0 (matched false) / 1 (matched
/// true) / 2 (no token matched).
/// x86_64: input rax=ptr, rdx=len. Output: rax = 0 / 1 / 2, same meaning.
pub fn emit_filter_validate_bool_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_filter_validate_bool_str_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: filter_validate_bool_str ---");
    emitter.label_global("__rt_filter_validate_bool_str");

    emitter.instruction("sub sp, sp, #16");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across nested calls
    emitter.instruction("add x29, sp, #0");                                     // establish a stable helper frame pointer
    abi::emit_call_label(emitter, "__rt_filter_trim_ws");                       // trim PHP-filter whitespace: x1=ptr, x2=len (trimmed)

    emitter.instruction("cbnz x2, __rt_filter_validate_bool_str_nonempty");     // "" (all-whitespace input trims to this) is a VALID false result
    emitter.instruction("mov x0, #0");                                          // report matched false
    emitter.instruction("b __rt_filter_validate_bool_str_done");                // done

    emitter.label("__rt_filter_validate_bool_str_nonempty");
    emitter.instruction("cmp x2, #1");                                          // is the trimmed string exactly one byte?
    emitter.instruction("b.ne __rt_filter_validate_bool_str_multi");            // longer strings need the token comparisons below
    emitter.instruction("ldrb w4, [x1]");                                       // load the sole byte
    emitter.instruction("cmp w4, #0x31");                                       // is it '1'?
    emitter.instruction("b.eq __rt_filter_validate_bool_str_true");             // "1" matches true
    emitter.instruction("cmp w4, #0x30");                                       // is it '0'?
    emitter.instruction("b.eq __rt_filter_validate_bool_str_matched_false");    // "0" matches false
    emitter.instruction("b __rt_filter_validate_bool_str_none");                // any other single byte matches nothing

    emitter.label("__rt_filter_validate_bool_str_multi");
    // -- true tokens: "true", "on", "yes" (x1/x2 are read-only inputs to __rt_strcasecmp, so they survive every call below) --
    abi::emit_symbol_address(emitter, "x3", "_filter_bool_true");               // x3 = "true" literal address
    emitter.instruction("mov x4, #4");                                          // x4 = "true" length
    abi::emit_call_label(emitter, "__rt_strcasecmp");
    emitter.instruction("cbz x0, __rt_filter_validate_bool_str_true");          // exact case-insensitive match -> true

    abi::emit_symbol_address(emitter, "x3", "_filter_bool_on");                 // x3 = "on" literal address
    emitter.instruction("mov x4, #2");                                          // x4 = "on" length
    abi::emit_call_label(emitter, "__rt_strcasecmp");
    emitter.instruction("cbz x0, __rt_filter_validate_bool_str_true");          // exact case-insensitive match -> true

    abi::emit_symbol_address(emitter, "x3", "_filter_bool_yes");                // x3 = "yes" literal address
    emitter.instruction("mov x4, #3");                                          // x4 = "yes" length
    abi::emit_call_label(emitter, "__rt_strcasecmp");
    emitter.instruction("cbz x0, __rt_filter_validate_bool_str_true");          // exact case-insensitive match -> true

    // -- false tokens: "false", "off", "no" --
    abi::emit_symbol_address(emitter, "x3", "_filter_bool_false");              // x3 = "false" literal address
    emitter.instruction("mov x4, #5");                                          // x4 = "false" length
    abi::emit_call_label(emitter, "__rt_strcasecmp");
    emitter.instruction("cbz x0, __rt_filter_validate_bool_str_matched_false"); // exact case-insensitive match -> false

    abi::emit_symbol_address(emitter, "x3", "_filter_bool_off");                // x3 = "off" literal address
    emitter.instruction("mov x4, #3");                                          // x4 = "off" length
    abi::emit_call_label(emitter, "__rt_strcasecmp");
    emitter.instruction("cbz x0, __rt_filter_validate_bool_str_matched_false"); // exact case-insensitive match -> false

    abi::emit_symbol_address(emitter, "x3", "_filter_bool_no");                 // x3 = "no" literal address
    emitter.instruction("mov x4, #2");                                          // x4 = "no" length
    abi::emit_call_label(emitter, "__rt_strcasecmp");
    emitter.instruction("cbz x0, __rt_filter_validate_bool_str_matched_false"); // exact case-insensitive match -> false

    emitter.instruction("b __rt_filter_validate_bool_str_none");                // no candidate token matched

    emitter.label("__rt_filter_validate_bool_str_true");
    emitter.instruction("mov x0, #1");                                          // report matched true
    emitter.instruction("b __rt_filter_validate_bool_str_done");                // done

    emitter.label("__rt_filter_validate_bool_str_matched_false");
    emitter.instruction("mov x0, #0");                                          // report matched false
    emitter.instruction("b __rt_filter_validate_bool_str_done");                // done

    emitter.label("__rt_filter_validate_bool_str_none");
    emitter.instruction("mov x0, #2");                                          // report no token matched

    emitter.label("__rt_filter_validate_bool_str_done");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the 0/1/2 match result in x0
}

/// Emits the x86_64 System V variant of `__rt_filter_validate_bool_str`.
fn emit_filter_validate_bool_str_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: filter_validate_bool_str ---");
    emitter.label_global("__rt_filter_validate_bool_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // 16-byte-align the frame for the nested calls below
    abi::emit_call_label(emitter, "__rt_filter_trim_ws");                       // trim PHP-filter whitespace: rax=ptr, rdx=len (trimmed)

    emitter.instruction("test rdx, rdx");                                       // "" (all-whitespace input trims to this) is a VALID false result
    emitter.instruction("jnz __rt_filter_validate_bool_str_nonempty_x86_64");
    emitter.instruction("xor eax, eax");                                        // report matched false
    emitter.instruction("jmp __rt_filter_validate_bool_str_done_x86_64");       // done

    emitter.label("__rt_filter_validate_bool_str_nonempty_x86_64");
    emitter.instruction("cmp rdx, 1");                                          // is the trimmed string exactly one byte?
    emitter.instruction("jne __rt_filter_validate_bool_str_multi_x86_64");      // longer strings need the token comparisons below
    emitter.instruction("movzx r8d, BYTE PTR [rax]");                           // load the sole byte
    emitter.instruction("cmp r8d, 0x31");                                       // is it '1'?
    emitter.instruction("je __rt_filter_validate_bool_str_true_x86_64");        // "1" matches true
    emitter.instruction("cmp r8d, 0x30");                                       // is it '0'?
    emitter.instruction("je __rt_filter_validate_bool_str_matched_false_x86_64"); // "0" matches false
    emitter.instruction("jmp __rt_filter_validate_bool_str_none_x86_64");       // any other single byte matches nothing

    emitter.label("__rt_filter_validate_bool_str_multi_x86_64");
    // __rt_strcasecmp reads rdi/rsi (its ptr_a/len_a) without modifying them, so
    // they are set ONCE here and survive every call below; only rdx/rcx (ptr_b/len_b)
    // change per candidate token.
    emitter.instruction("mov rdi, rax");                                        // rdi = trimmed string pointer
    emitter.instruction("mov rsi, rdx");                                        // rsi = trimmed string length
    // -- true tokens: "true", "on", "yes" --
    abi::emit_symbol_address(emitter, "rdx", "_filter_bool_true");              // rdx = "true" literal address
    emitter.instruction("mov ecx, 4");                                          // rcx = "true" length
    abi::emit_call_label(emitter, "__rt_strcasecmp");
    emitter.instruction("test rax, rax");
    emitter.instruction("je __rt_filter_validate_bool_str_true_x86_64");        // exact case-insensitive match -> true

    abi::emit_symbol_address(emitter, "rdx", "_filter_bool_on");                // rdx = "on" literal address
    emitter.instruction("mov ecx, 2");                                          // rcx = "on" length
    abi::emit_call_label(emitter, "__rt_strcasecmp");
    emitter.instruction("test rax, rax");
    emitter.instruction("je __rt_filter_validate_bool_str_true_x86_64");        // exact case-insensitive match -> true

    abi::emit_symbol_address(emitter, "rdx", "_filter_bool_yes");               // rdx = "yes" literal address
    emitter.instruction("mov ecx, 3");                                          // rcx = "yes" length
    abi::emit_call_label(emitter, "__rt_strcasecmp");
    emitter.instruction("test rax, rax");
    emitter.instruction("je __rt_filter_validate_bool_str_true_x86_64");        // exact case-insensitive match -> true

    // -- false tokens: "false", "off", "no" --
    abi::emit_symbol_address(emitter, "rdx", "_filter_bool_false");             // rdx = "false" literal address
    emitter.instruction("mov ecx, 5");                                          // rcx = "false" length
    abi::emit_call_label(emitter, "__rt_strcasecmp");
    emitter.instruction("test rax, rax");
    emitter.instruction("je __rt_filter_validate_bool_str_matched_false_x86_64"); // exact case-insensitive match -> false

    abi::emit_symbol_address(emitter, "rdx", "_filter_bool_off");               // rdx = "off" literal address
    emitter.instruction("mov ecx, 3");                                          // rcx = "off" length
    abi::emit_call_label(emitter, "__rt_strcasecmp");
    emitter.instruction("test rax, rax");
    emitter.instruction("je __rt_filter_validate_bool_str_matched_false_x86_64"); // exact case-insensitive match -> false

    abi::emit_symbol_address(emitter, "rdx", "_filter_bool_no");                // rdx = "no" literal address
    emitter.instruction("mov ecx, 2");                                          // rcx = "no" length
    abi::emit_call_label(emitter, "__rt_strcasecmp");
    emitter.instruction("test rax, rax");
    emitter.instruction("je __rt_filter_validate_bool_str_matched_false_x86_64"); // exact case-insensitive match -> false

    emitter.instruction("jmp __rt_filter_validate_bool_str_none_x86_64");       // no candidate token matched

    emitter.label("__rt_filter_validate_bool_str_true_x86_64");
    emitter.instruction("mov eax, 1");                                          // report matched true
    emitter.instruction("jmp __rt_filter_validate_bool_str_done_x86_64");       // done

    emitter.label("__rt_filter_validate_bool_str_matched_false_x86_64");
    emitter.instruction("xor eax, eax");                                        // report matched false
    emitter.instruction("jmp __rt_filter_validate_bool_str_done_x86_64");       // done

    emitter.label("__rt_filter_validate_bool_str_none_x86_64");
    emitter.instruction("mov eax, 2");                                          // report no token matched

    emitter.label("__rt_filter_validate_bool_str_done_x86_64");
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the 0/1/2 match result in rax
}
