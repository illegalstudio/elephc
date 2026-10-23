//! Purpose:
//! Emits the `__rt_substr_compare` runtime helper assembly for PHP's `substr_compare`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Mirrors php-src: an explicit `$length` of 0 answers 0 before anything else, a negative
//!   one is an error; a negative `$offset` counts from the end and clamps at zero, and an
//!   offset past the end is an error. The helper reports both errors through an error code
//!   so the generated call site raises PHP's catchable `ValueError`.
//! - The comparison is `zend_binary_strncmp` / `zend_binary_strncasecmp_l`: the first
//!   differing byte difference (ASCII-lowercased when case-insensitive), otherwise the
//!   three-way comparison of the compared lengths.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Error code: the comparison succeeded.
pub(crate) const SUBSTR_COMPARE_OK: i64 = 0;

/// Error code: an explicit `$length` was negative.
pub(crate) const SUBSTR_COMPARE_NEGATIVE_LENGTH: i64 = 1;

/// Error code: the resolved `$offset` lies past the end of the haystack.
pub(crate) const SUBSTR_COMPARE_OFFSET_OUT_OF_RANGE: i64 = 2;

/// Emits the `__rt_substr_compare` runtime helper.
///
/// ABI (AArch64):
///   Input:  `x0` = case-insensitive flag, `x1`/`x2` = haystack, `x3`/`x4` = needle,
///           `x5` = offset, `x6` = length, `x7` = 1 when `$length` was given (non-null).
///   Output: `x0` = comparison result, `x1` = error code.
///
/// ABI (x86_64, internal):
///   Input:  `r11` = case-insensitive flag, `rdi`/`rsi` = haystack, `rdx`/`rcx` = needle,
///           `r8` = offset, `r9` = length, `r10` = 1 when `$length` was given (non-null).
///   Output: `rax` = comparison result, `rdx` = error code.
///
/// The helper is a leaf and touches no stack.
pub fn emit_substr_compare(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_substr_compare_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: substr_compare ---");
    emitter.label_global("__rt_substr_compare");

    // -- an explicit length is validated first: 0 answers 0, negative is an error --
    emitter.instruction("mov x15, x0");                                         // keep the case-insensitive flag out of the result register
    emitter.instruction("cbz x7, __rt_substr_compare_length_ok");               // a null length is resolved after the offset
    emitter.instruction("cmp x6, #0");                                          // classify the explicit length
    emitter.instruction("b.lt __rt_substr_compare_negative_length");            // a negative length is a ValueError
    emitter.instruction("b.ne __rt_substr_compare_length_ok");                  // a positive length is used as the compare bound
    emitter.instruction("mov x0, #0");                                          // a zero length compares nothing and is equal
    emitter.instruction(&format!("mov x1, #{SUBSTR_COMPARE_OK}"));              // report success
    emitter.instruction("ret");                                                 // return the equal result

    // -- resolve the offset --
    emitter.label("__rt_substr_compare_length_ok");
    emitter.instruction("cmp x5, #0");                                          // is the offset counted from the end?
    emitter.instruction("b.ge __rt_substr_compare_offset_ready");               // non-negative offsets are checked against the length
    emitter.instruction("add x5, x5, x2");                                      // convert the negative offset into a byte index
    emitter.instruction("cmp x5, #0");                                          // did the offset reach before the haystack start?
    emitter.instruction("csel x5, xzr, x5, lt");                                // clamp it to the haystack start, as php-src does

    emitter.label("__rt_substr_compare_offset_ready");
    emitter.instruction("cmp x5, x2");                                          // is the offset past the end of the haystack?
    emitter.instruction("b.gt __rt_substr_compare_offset_out_of_range");        // php-src raises a ValueError for it
    emitter.instruction("add x1, x1, x5");                                      // point at the compared part of the haystack
    emitter.instruction("sub x2, x2, x5");                                      // bytes of the haystack from the offset on
    emitter.instruction("cbnz x7, __rt_substr_compare_bound_ready");            // an explicit length is the compare bound
    emitter.instruction("cmp x4, x2");                                          // a null length compares up to the longer operand
    emitter.instruction("csel x6, x4, x2, gt");                                 // bound = max(needle length, haystack tail length)

    // -- compare up to min(bound, haystack tail, needle) bytes --
    emitter.label("__rt_substr_compare_bound_ready");
    emitter.instruction("cmp x2, x4");                                          // pick the shorter operand
    emitter.instruction("csel x9, x2, x4, lt");                                 // min(haystack tail length, needle length)
    emitter.instruction("cmp x6, x9");                                          // the bound may be shorter still
    emitter.instruction("csel x9, x6, x9, lt");                                 // bytes compared one by one
    emitter.instruction("mov x10, #0");                                         // byte index

    emitter.label("__rt_substr_compare_loop");
    emitter.instruction("cmp x10, x9");                                         // have all comparable bytes matched?
    emitter.instruction("b.ge __rt_substr_compare_lengths");                    // fall back to the length comparison
    emitter.instruction("ldrb w11, [x1, x10]");                                 // load the haystack byte
    emitter.instruction("ldrb w12, [x3, x10]");                                 // load the needle byte
    emitter.instruction("cbz x15, __rt_substr_compare_bytes");                  // a case-sensitive compare uses the raw bytes
    emitter.instruction("sub w13, w11, #65");                                   // is the haystack byte an ASCII uppercase letter?
    emitter.instruction("cmp w13, #26");                                        // A..Z map to 0..25
    emitter.instruction("b.hs __rt_substr_compare_needle_case");                // other bytes keep their value
    emitter.instruction("add w11, w11, #32");                                   // lowercase the haystack byte

    emitter.label("__rt_substr_compare_needle_case");
    emitter.instruction("sub w13, w12, #65");                                   // is the needle byte an ASCII uppercase letter?
    emitter.instruction("cmp w13, #26");                                        // A..Z map to 0..25
    emitter.instruction("b.hs __rt_substr_compare_bytes");                      // other bytes keep their value
    emitter.instruction("add w12, w12, #32");                                   // lowercase the needle byte

    emitter.label("__rt_substr_compare_bytes");
    emitter.instruction("subs w13, w11, w12");                                  // difference of the two bytes
    emitter.instruction("b.ne __rt_substr_compare_differ");                     // the first difference decides the result
    emitter.instruction("add x10, x10, #1");                                    // advance to the next byte
    emitter.instruction("b __rt_substr_compare_loop");                          // keep comparing

    emitter.label("__rt_substr_compare_differ");
    emitter.instruction("sxtw x0, w13");                                        // return the signed byte difference
    emitter.instruction(&format!("mov x1, #{SUBSTR_COMPARE_OK}"));              // report success
    emitter.instruction("ret");                                                 // return the comparison result

    // -- equal prefix: three-way comparison of the bounded lengths --
    emitter.label("__rt_substr_compare_lengths");
    emitter.instruction("cmp x6, x2");                                          // bound the haystack tail length
    emitter.instruction("csel x9, x6, x2, lt");                                 // min(bound, haystack tail length)
    emitter.instruction("cmp x6, x4");                                          // bound the needle length
    emitter.instruction("csel x10, x6, x4, lt");                                // min(bound, needle length)
    emitter.instruction("cmp x9, x10");                                         // compare the two bounded lengths
    emitter.instruction("cset x0, gt");                                         // 1 when the haystack side is longer
    emitter.instruction("csinv x0, x0, xzr, ge");                               // -1 when it is shorter, 0 when equal
    emitter.instruction(&format!("mov x1, #{SUBSTR_COMPARE_OK}"));              // report success
    emitter.instruction("ret");                                                 // return the comparison result

    emitter.label("__rt_substr_compare_negative_length");
    emitter.instruction("mov x0, #0");                                          // no comparison result
    emitter.instruction(&format!("mov x1, #{SUBSTR_COMPARE_NEGATIVE_LENGTH}")); // report the negative-length error
    emitter.instruction("ret");                                                 // the call site raises the ValueError

    emitter.label("__rt_substr_compare_offset_out_of_range");
    emitter.instruction("mov x0, #0");                                          // no comparison result
    emitter.instruction(&format!("mov x1, #{SUBSTR_COMPARE_OFFSET_OUT_OF_RANGE}")); // report the out-of-range offset error
    emitter.instruction("ret");                                                 // the call site raises the ValueError
}

/// Emits `__rt_substr_compare` for x86_64 Linux with the internal register ABI documented above.
fn emit_substr_compare_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: substr_compare ---");
    emitter.label_global("__rt_substr_compare");

    // -- an explicit length is validated first: 0 answers 0, negative is an error --
    emitter.instruction("test r10, r10");                                       // was an explicit length given?
    emitter.instruction("jz __rt_substr_compare_length_ok_linux_x86_64");       // a null length is resolved after the offset
    emitter.instruction("test r9, r9");                                         // classify the explicit length
    emitter.instruction("js __rt_substr_compare_negative_length_linux_x86_64"); // a negative length is a ValueError
    emitter.instruction("jnz __rt_substr_compare_length_ok_linux_x86_64");      // a positive length is used as the compare bound
    emitter.instruction("xor eax, eax");                                        // a zero length compares nothing and is equal
    emitter.instruction("xor edx, edx");                                        // report success
    emitter.instruction("ret");                                                 // return the equal result

    // -- resolve the offset --
    emitter.label("__rt_substr_compare_length_ok_linux_x86_64");
    emitter.instruction("test r8, r8");                                         // is the offset counted from the end?
    emitter.instruction("jns __rt_substr_compare_offset_ready_linux_x86_64");   // non-negative offsets are checked against the length
    emitter.instruction("add r8, rsi");                                         // convert the negative offset into a byte index
    emitter.instruction("jns __rt_substr_compare_offset_ready_linux_x86_64");   // a start inside the haystack is resolved
    emitter.instruction("xor r8d, r8d");                                        // clamp an offset before the haystack start to zero

    emitter.label("__rt_substr_compare_offset_ready_linux_x86_64");
    emitter.instruction("cmp r8, rsi");                                         // is the offset past the end of the haystack?
    emitter.instruction("jg __rt_substr_compare_offset_out_of_range_linux_x86_64"); // php-src raises a ValueError for it
    emitter.instruction("add rdi, r8");                                         // point at the compared part of the haystack
    emitter.instruction("sub rsi, r8");                                         // bytes of the haystack from the offset on
    emitter.instruction("test r10, r10");                                       // was an explicit length given?
    emitter.instruction("jnz __rt_substr_compare_bound_ready_linux_x86_64");    // an explicit length is the compare bound
    emitter.instruction("mov r9, rcx");                                         // a null length compares up to the longer operand
    emitter.instruction("cmp rsi, rcx");                                        // pick the longer operand
    emitter.instruction("cmovg r9, rsi");                                       // bound = max(needle length, haystack tail length)

    // -- compare up to min(bound, haystack tail, needle) bytes --
    emitter.label("__rt_substr_compare_bound_ready_linux_x86_64");
    emitter.instruction("mov r8, rsi");                                         // start from the haystack tail length
    emitter.instruction("cmp rcx, r8");                                         // pick the shorter operand
    emitter.instruction("cmovl r8, rcx");                                       // min(haystack tail length, needle length)
    emitter.instruction("cmp r9, r8");                                          // the bound may be shorter still
    emitter.instruction("cmovl r8, r9");                                        // bytes compared one by one
    emitter.instruction("xor r10d, r10d");                                      // byte index
    emitter.instruction("test r11, r11");                                       // case-insensitive comparison?
    emitter.instruction("jnz __rt_substr_compare_ci_loop_linux_x86_64");        // fold ASCII case in a separate loop

    emitter.label("__rt_substr_compare_loop_linux_x86_64");
    emitter.instruction("cmp r10, r8");                                         // have all comparable bytes matched?
    emitter.instruction("jge __rt_substr_compare_lengths_linux_x86_64");        // fall back to the length comparison
    emitter.instruction("movzx eax, BYTE PTR [rdi + r10]");                     // load the haystack byte
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r10]");                    // load the needle byte
    emitter.instruction("sub eax, r11d");                                       // difference of the two bytes
    emitter.instruction("jnz __rt_substr_compare_differ_linux_x86_64");         // the first difference decides the result
    emitter.instruction("add r10, 1");                                          // advance to the next byte
    emitter.instruction("jmp __rt_substr_compare_loop_linux_x86_64");           // keep comparing

    emitter.label("__rt_substr_compare_ci_loop_linux_x86_64");
    emitter.instruction("cmp r10, r8");                                         // have all comparable bytes matched?
    emitter.instruction("jge __rt_substr_compare_lengths_linux_x86_64");        // fall back to the length comparison
    emitter.instruction("movzx eax, BYTE PTR [rdi + r10]");                     // load the haystack byte
    emitter.instruction("cmp eax, 65");                                         // below 'A'?
    emitter.instruction("jb __rt_substr_compare_ci_needle_linux_x86_64");       // other bytes keep their value
    emitter.instruction("cmp eax, 90");                                         // above 'Z'?
    emitter.instruction("ja __rt_substr_compare_ci_needle_linux_x86_64");       // other bytes keep their value
    emitter.instruction("add eax, 32");                                         // lowercase the haystack byte

    emitter.label("__rt_substr_compare_ci_needle_linux_x86_64");
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r10]");                    // load the needle byte
    emitter.instruction("cmp r11d, 65");                                        // below 'A'?
    emitter.instruction("jb __rt_substr_compare_ci_bytes_linux_x86_64");        // other bytes keep their value
    emitter.instruction("cmp r11d, 90");                                        // above 'Z'?
    emitter.instruction("ja __rt_substr_compare_ci_bytes_linux_x86_64");        // other bytes keep their value
    emitter.instruction("add r11d, 32");                                        // lowercase the needle byte

    emitter.label("__rt_substr_compare_ci_bytes_linux_x86_64");
    emitter.instruction("sub eax, r11d");                                       // difference of the two folded bytes
    emitter.instruction("jnz __rt_substr_compare_differ_linux_x86_64");         // the first difference decides the result
    emitter.instruction("add r10, 1");                                          // advance to the next byte
    emitter.instruction("jmp __rt_substr_compare_ci_loop_linux_x86_64");        // keep comparing

    emitter.label("__rt_substr_compare_differ_linux_x86_64");
    emitter.instruction("movsxd rax, eax");                                     // return the signed byte difference
    emitter.instruction("xor edx, edx");                                        // report success
    emitter.instruction("ret");                                                 // return the comparison result

    // -- equal prefix: three-way comparison of the bounded lengths --
    emitter.label("__rt_substr_compare_lengths_linux_x86_64");
    emitter.instruction("mov rax, rsi");                                        // start from the haystack tail length
    emitter.instruction("cmp r9, rax");                                         // bound the haystack tail length
    emitter.instruction("cmovl rax, r9");                                       // min(bound, haystack tail length)
    emitter.instruction("mov r10, rcx");                                        // start from the needle length
    emitter.instruction("cmp r9, r10");                                         // bound the needle length
    emitter.instruction("cmovl r10, r9");                                       // min(bound, needle length)
    emitter.instruction("cmp rax, r10");                                        // compare the two bounded lengths
    emitter.instruction("mov eax, 0");                                          // equal lengths compare as 0 (mov keeps the flags)
    emitter.instruction("setg al");                                             // 1 when the haystack side is longer
    emitter.instruction("mov r11, -1");                                         // -1 for a shorter haystack side (mov keeps the flags)
    emitter.instruction("cmovl rax, r11");                                      // select -1 when the haystack side is shorter
    emitter.instruction("xor edx, edx");                                        // report success
    emitter.instruction("ret");                                                 // return the comparison result

    emitter.label("__rt_substr_compare_negative_length_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // no comparison result
    emitter.instruction(&format!("mov edx, {SUBSTR_COMPARE_NEGATIVE_LENGTH}")); // report the negative-length error
    emitter.instruction("ret");                                                 // the call site raises the ValueError

    emitter.label("__rt_substr_compare_offset_out_of_range_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // no comparison result
    emitter.instruction(&format!("mov edx, {SUBSTR_COMPARE_OFFSET_OUT_OF_RANGE}")); // report the out-of-range offset error
    emitter.instruction("ret");                                                 // the call site raises the ValueError
}
