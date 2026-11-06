//! Purpose:
//! Emits the `__rt_str_to_int_base` runtime helper: PHP `intval()`'s two-argument string
//! parser, which is C `strtol()` plus php-src's extra `0b` binary prefix.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - php-src's `PHP_FUNCTION(intval)` special-cases a `0b`/`0B` prefix for bases `0` and `2`
//!   and then hands everything else to `ZEND_STRTOL`, so this helper reproduces `strtol()`:
//!   leading whitespace is skipped, one optional sign is consumed, a `0x`/`0X` prefix is
//!   accepted for bases `0` and `16`, base `0` falls back to octal on a leading `0` and to
//!   decimal otherwise, and the scan stops at the first byte that is not a digit of the
//!   resolved base. That "stops at the first bad byte" rule is what separates this helper
//!   from `__rt_base_to_number`, which instead *ignores* such bytes for `hexdec()` and
//!   friends (`intval("a0z", 16) === 160` there, `2575` here would be wrong).
//! - A base that is neither `0` nor in `2..=36` makes `strtol()` fail with `EINVAL` and
//!   return `0`; reference PHP 8.4 surfaces that as `intval("42", 1) === 0` with no
//!   diagnostic, so the helper simply returns `0` instead of raising anything.
//! - Overflow saturates at `PHP_INT_MAX`/`PHP_INT_MIN` exactly like `ZEND_STRTOL`'s
//!   `ERANGE` clamp (`intval("ffffffffffffffffff", 16) === PHP_INT_MAX`). The accumulator is
//!   therefore unsigned and every comparison against it uses unsigned conditions, because a
//!   negative parse legitimately reaches `2**63`.
//! - The helper allocates nothing and calls nothing, so it needs no frame.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_str_to_int_base` runtime helper.
///
/// ABI (AArch64):
///   Input:  `x1` = string pointer, `x2` = string length, `x3` = requested base.
///   Output: `x0` = the parsed PHP integer.
///
/// ABI (x86_64 System V):
///   Input:  `rdi` = string pointer, `rsi` = string length, `rdx` = requested base.
///   Output: `rax` = the parsed PHP integer.
///
/// An empty string, a string whose first non-blank byte is not a digit of the resolved base,
/// and an out-of-range base all yield `0` — the same as reference PHP.
pub fn emit_str_to_int_base(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_to_int_base_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_to_int_base ---");
    emitter.label_global("__rt_str_to_int_base");

    // -- base 10 uses PHP's numeric-string rules --
    // `intval("1e3", 10)` is 1000 and `intval("9.9e18", 10)` caps at PHP_INT_MAX, while every
    // other base parses digits only (`intval("1e3", 16)` is 0x1e3). The general string-to-int
    // helper already implements the base-10 float form, so tail-call it without touching the
    // string registers.
    emitter.instruction("cmp x3, #10");                                         // is decimal the requested base?
    emitter.instruction("b.eq __rt_str_to_int");                                // base 10 follows the numeric-string parser

    // -- reject the bases strtol() answers with EINVAL --
    emitter.instruction("cbz x3, __rt_str_to_int_base_scan");                   // base 0 asks for prefix auto-detection
    emitter.instruction("cmp x3, #2");                                          // is the requested base below strtol()'s minimum?
    emitter.instruction("b.lt __rt_str_to_int_base_zero");                      // an unusable base parses nothing
    emitter.instruction("cmp x3, #36");                                         // is the requested base above strtol()'s maximum?
    emitter.instruction("b.gt __rt_str_to_int_base_zero");                      // an unusable base parses nothing

    // -- skip the leading whitespace strtol() ignores --
    emitter.label("__rt_str_to_int_base_scan");
    emitter.instruction("mov x4, #0");                                          // start with a positive sign
    emitter.label("__rt_str_to_int_base_space");
    emitter.instruction("cbz x2, __rt_str_to_int_base_zero");                   // a blank-only string parses nothing
    emitter.instruction("ldrb w7, [x1]");                                       // load the next candidate byte without consuming it
    emitter.instruction("cmp w7, #32");                                         // is the byte a plain space?
    emitter.instruction("b.eq __rt_str_to_int_base_space_next");                // spaces are skipped
    emitter.instruction("sub w9, w7, #9");                                      // rebase the byte onto the tab..carriage-return block
    emitter.instruction("cmp w9, #4");                                          // is the byte one of \t \n \v \f \r?
    emitter.instruction("b.hi __rt_str_to_int_base_sign");                      // the first non-blank byte starts the number
    emitter.label("__rt_str_to_int_base_space_next");
    emitter.instruction("add x1, x1, #1");                                      // consume the blank byte
    emitter.instruction("sub x2, x2, #1");                                      // record that one input byte has been consumed
    emitter.instruction("b __rt_str_to_int_base_space");                        // keep skipping blanks

    // -- consume the single optional sign --
    emitter.label("__rt_str_to_int_base_sign");
    emitter.instruction("cmp w7, #45");                                         // is the byte a minus sign?
    emitter.instruction("b.ne __rt_str_to_int_base_plus");                      // try the plus sign instead
    emitter.instruction("mov x4, #1");                                          // remember that the result is negative
    emitter.instruction("b __rt_str_to_int_base_sign_taken");                   // consume the sign byte
    emitter.label("__rt_str_to_int_base_plus");
    emitter.instruction("cmp w7, #43");                                         // is the byte a plus sign?
    emitter.instruction("b.ne __rt_str_to_int_base_prefix");                    // no sign means the digits start here
    emitter.label("__rt_str_to_int_base_sign_taken");
    emitter.instruction("add x1, x1, #1");                                      // consume the sign byte
    emitter.instruction("sub x2, x2, #1");                                      // record that one input byte has been consumed

    // -- accept the 0x/0b prefixes the resolved base allows --
    emitter.label("__rt_str_to_int_base_prefix");
    emitter.instruction("cbz x2, __rt_str_to_int_base_zero");                   // a sign with no digits parses nothing
    emitter.instruction("cmp x2, #2");                                          // is the remainder long enough to carry a prefix?
    emitter.instruction("b.lt __rt_str_to_int_base_auto");                      // a single byte can only pick the automatic base
    emitter.instruction("ldrb w7, [x1]");                                       // load the candidate prefix's leading zero
    emitter.instruction("cmp w7, #48");                                         // does the remainder start with '0'?
    emitter.instruction("b.ne __rt_str_to_int_base_auto");                      // without a leading zero there is no prefix
    emitter.instruction("ldrb w9, [x1, #1]");                                   // load the candidate prefix letter
    emitter.instruction("orr w9, w9, #32");                                     // fold the prefix letter to lowercase
    emitter.instruction("cmp w9, #120");                                        // is the prefix letter 'x'?
    emitter.instruction("b.ne __rt_str_to_int_base_binary_prefix");             // try the binary prefix instead
    emitter.instruction("cbz x3, __rt_str_to_int_base_take_hex");               // base 0 resolves '0x' to hexadecimal
    emitter.instruction("cmp x3, #16");                                         // was hexadecimal requested explicitly?
    emitter.instruction("b.ne __rt_str_to_int_base_auto");                      // any other base treats 'x' as a terminator
    emitter.label("__rt_str_to_int_base_take_hex");
    emitter.instruction("mov x3, #16");                                         // resolve the scan to base 16
    emitter.instruction("b __rt_str_to_int_base_skip_prefix");                  // consume the two prefix bytes
    emitter.label("__rt_str_to_int_base_binary_prefix");
    emitter.instruction("cmp w9, #98");                                         // is the prefix letter 'b'?
    emitter.instruction("b.ne __rt_str_to_int_base_auto");                      // any other letter is not a prefix
    emitter.instruction("cbz x3, __rt_str_to_int_base_take_binary");            // base 0 resolves '0b' to binary
    emitter.instruction("cmp x3, #2");                                          // was binary requested explicitly?
    emitter.instruction("b.ne __rt_str_to_int_base_auto");                      // any other base treats 'b' as a digit or terminator
    emitter.label("__rt_str_to_int_base_take_binary");
    emitter.instruction("mov x3, #2");                                          // resolve the scan to base 2
    emitter.label("__rt_str_to_int_base_skip_prefix");
    emitter.instruction("add x1, x1, #2");                                      // consume the two prefix bytes
    emitter.instruction("sub x2, x2, #2");                                      // record that the prefix has been consumed
    emitter.instruction("b __rt_str_to_int_base_ready");                        // the prefix already resolved the base

    // -- resolve base 0 the way strtol() does when no 0x/0b prefix applied --
    emitter.label("__rt_str_to_int_base_auto");
    emitter.instruction("cbnz x3, __rt_str_to_int_base_ready");                 // an explicit base needs no auto-detection
    emitter.instruction("ldrb w7, [x1]");                                       // inspect the first digit byte
    emitter.instruction("mov x3, #10");                                         // default the automatic base to decimal
    emitter.instruction("cmp w7, #48");                                         // does the number start with '0'?
    emitter.instruction("b.ne __rt_str_to_int_base_ready");                     // a non-zero lead digit keeps the decimal base
    emitter.instruction("mov x3, #8");                                          // a leading zero selects octal, and stays a valid digit

    // -- derive the saturation limit this sign allows --
    emitter.label("__rt_str_to_int_base_ready");
    emitter.instruction("cbz x2, __rt_str_to_int_base_zero");                   // a prefix with no digits parses nothing
    abi::emit_load_int_immediate(emitter, "x6", i64::MAX);
    emitter.instruction("cbz x4, __rt_str_to_int_base_limit_done");             // a positive parse saturates at PHP_INT_MAX
    abi::emit_load_int_immediate(emitter, "x6", i64::MIN);
    emitter.label("__rt_str_to_int_base_limit_done");
    emitter.instruction("udiv x10, x6, x3");                                    // x10 = limit / base, the last accumulator that can still take a digit
    emitter.instruction("mov x5, #0");                                          // start the unsigned accumulator at zero

    // -- accumulate digits until the first byte that is not one --
    emitter.label("__rt_str_to_int_base_loop");
    emitter.instruction("cbz x2, __rt_str_to_int_base_done");                   // stop once every input byte has been consumed
    emitter.instruction("ldrb w7, [x1], #1");                                   // load the next input byte and advance the cursor
    emitter.instruction("sub x2, x2, #1");                                      // record that one input byte has been consumed
    emitter.instruction("sub w8, w7, #48");                                     // try the ASCII numerals first
    emitter.instruction("cmp w8, #9");                                          // is the byte in '0'..'9' (unsigned, so lower bytes wrap high)?
    emitter.instruction("b.ls __rt_str_to_int_base_digit");                     // an ASCII numeral decodes directly
    emitter.instruction("cmp w7, #65");                                         // is the byte below 'A'?
    emitter.instruction("b.lo __rt_str_to_int_base_done");                      // a non-digit byte terminates the scan
    emitter.instruction("cmp w7, #90");                                         // is the byte at or below 'Z'?
    emitter.instruction("b.hi __rt_str_to_int_base_lower");                     // try the lowercase letters instead
    emitter.instruction("sub w8, w7, #55");                                     // map 'A'..'Z' to digit values 10..35
    emitter.instruction("b __rt_str_to_int_base_digit");                        // the uppercase letter decoded to a digit
    emitter.label("__rt_str_to_int_base_lower");
    emitter.instruction("cmp w7, #97");                                         // is the byte below 'a'?
    emitter.instruction("b.lo __rt_str_to_int_base_done");                      // a non-digit byte terminates the scan
    emitter.instruction("cmp w7, #122");                                        // is the byte above 'z'?
    emitter.instruction("b.hi __rt_str_to_int_base_done");                      // a non-digit byte terminates the scan
    emitter.instruction("sub w8, w7, #87");                                     // map 'a'..'z' to digit values 10..35

    emitter.label("__rt_str_to_int_base_digit");
    emitter.instruction("cmp x8, x3");                                          // is the decoded digit valid in the resolved base?
    emitter.instruction("b.hs __rt_str_to_int_base_done");                      // a digit outside the base terminates the scan
    emitter.instruction("cmp x5, x10");                                         // would shifting the accumulator already pass the limit?
    emitter.instruction("b.hi __rt_str_to_int_base_saturate");                  // an accumulator past the threshold can only overflow
    emitter.instruction("mul x5, x5, x3");                                      // shift the accumulator up by one digit position
    emitter.instruction("sub x9, x6, x8");                                      // compute the largest accumulator this digit still fits in
    emitter.instruction("cmp x5, x9");                                          // would adding this digit pass the limit?
    emitter.instruction("b.hi __rt_str_to_int_base_saturate");                  // clamp instead of wrapping past PHP_INT_MAX/PHP_INT_MIN
    emitter.instruction("add x5, x5, x8");                                      // accumulate the digit into the unsigned result
    emitter.instruction("b __rt_str_to_int_base_loop");                         // continue scanning the remaining input bytes

    emitter.label("__rt_str_to_int_base_saturate");
    emitter.instruction("mov x5, x6");                                          // clamp to the limit this sign allows

    emitter.label("__rt_str_to_int_base_done");
    emitter.instruction("mov x0, x5");                                          // return the accumulated magnitude
    emitter.instruction("cbz x4, __rt_str_to_int_base_ret");                    // a positive parse is already the result
    emitter.instruction("neg x0, x0");                                          // apply the consumed minus sign
    emitter.label("__rt_str_to_int_base_ret");
    emitter.instruction("ret");                                                 // hand the parsed integer back to the caller

    emitter.label("__rt_str_to_int_base_zero");
    emitter.instruction("mov x0, #0");                                          // an unusable base or digit-free string parses to zero
    emitter.instruction("ret");                                                 // hand the zero result back to the caller
}

/// Emits `__rt_str_to_int_base` for x86_64 Linux using the System V ABI.
///
/// The base is moved into `rcx` up front so `rdx` stays free for the decoded digit, and the
/// saturation threshold is derived with an unsigned division whose `rdx:rax` pair would
/// otherwise collide with the argument registers.
fn emit_str_to_int_base_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_to_int_base ---");
    emitter.label_global("__rt_str_to_int_base");

    // -- base 10 uses PHP's numeric-string rules --
    // Forward to the general string-to-int helper, whose System V string arguments are rdi/rsi.
    emitter.instruction("cmp rdx, 10");                                         // is decimal the requested base?
    emitter.instruction("jne __rt_str_to_int_base_not_ten_linux_x86_64");       // other bases keep digit-only parsing
    emitter.instruction("mov rax, rdi");                                        // move the string pointer into the general helper's first string register
    emitter.instruction("mov rdx, rsi");                                        // move the string length into the general helper's second string register
    emitter.instruction("jmp __rt_str_to_int");                                 // base 10 follows the numeric-string parser
    emitter.label("__rt_str_to_int_base_not_ten_linux_x86_64");

    // -- reject the bases strtol() answers with EINVAL --
    emitter.instruction("mov rcx, rdx");                                        // keep the requested base in a stable register
    emitter.instruction("test rcx, rcx");                                       // was base 0 requested?
    emitter.instruction("jz __rt_str_to_int_base_scan_linux_x86_64");           // base 0 asks for prefix auto-detection
    emitter.instruction("cmp rcx, 2");                                          // is the requested base below strtol()'s minimum?
    emitter.instruction("jl __rt_str_to_int_base_zero_linux_x86_64");           // an unusable base parses nothing
    emitter.instruction("cmp rcx, 36");                                         // is the requested base above strtol()'s maximum?
    emitter.instruction("jg __rt_str_to_int_base_zero_linux_x86_64");           // an unusable base parses nothing

    // -- skip the leading whitespace strtol() ignores --
    emitter.label("__rt_str_to_int_base_scan_linux_x86_64");
    emitter.instruction("xor r8d, r8d");                                        // start with a positive sign
    emitter.label("__rt_str_to_int_base_space_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // is any input byte left?
    emitter.instruction("jz __rt_str_to_int_base_zero_linux_x86_64");           // a blank-only string parses nothing
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load the next candidate byte without consuming it
    emitter.instruction("cmp eax, 32");                                         // is the byte a plain space?
    emitter.instruction("je __rt_str_to_int_base_space_next_linux_x86_64");     // spaces are skipped
    emitter.instruction("mov edx, eax");                                        // copy the byte before rebasing it
    emitter.instruction("sub edx, 9");                                          // rebase the byte onto the tab..carriage-return block
    emitter.instruction("cmp edx, 4");                                          // is the byte one of \t \n \v \f \r?
    emitter.instruction("ja __rt_str_to_int_base_sign_linux_x86_64");           // the first non-blank byte starts the number
    emitter.label("__rt_str_to_int_base_space_next_linux_x86_64");
    emitter.instruction("add rdi, 1");                                          // consume the blank byte
    emitter.instruction("sub rsi, 1");                                          // record that one input byte has been consumed
    emitter.instruction("jmp __rt_str_to_int_base_space_linux_x86_64");         // keep skipping blanks

    // -- consume the single optional sign --
    emitter.label("__rt_str_to_int_base_sign_linux_x86_64");
    emitter.instruction("cmp eax, 45");                                         // is the byte a minus sign?
    emitter.instruction("jne __rt_str_to_int_base_plus_linux_x86_64");          // try the plus sign instead
    emitter.instruction("mov r8, 1");                                           // remember that the result is negative
    emitter.instruction("jmp __rt_str_to_int_base_sign_taken_linux_x86_64");    // consume the sign byte
    emitter.label("__rt_str_to_int_base_plus_linux_x86_64");
    emitter.instruction("cmp eax, 43");                                         // is the byte a plus sign?
    emitter.instruction("jne __rt_str_to_int_base_prefix_linux_x86_64");        // no sign means the digits start here
    emitter.label("__rt_str_to_int_base_sign_taken_linux_x86_64");
    emitter.instruction("add rdi, 1");                                          // consume the sign byte
    emitter.instruction("sub rsi, 1");                                          // record that one input byte has been consumed

    // -- accept the 0x/0b prefixes the resolved base allows --
    emitter.label("__rt_str_to_int_base_prefix_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // is any input byte left?
    emitter.instruction("jz __rt_str_to_int_base_zero_linux_x86_64");           // a sign with no digits parses nothing
    emitter.instruction("cmp rsi, 2");                                          // is the remainder long enough to carry a prefix?
    emitter.instruction("jl __rt_str_to_int_base_auto_linux_x86_64");           // a single byte can only pick the automatic base
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load the candidate prefix's leading zero
    emitter.instruction("cmp eax, 48");                                         // does the remainder start with '0'?
    emitter.instruction("jne __rt_str_to_int_base_auto_linux_x86_64");          // without a leading zero there is no prefix
    emitter.instruction("movzx edx, BYTE PTR [rdi + 1]");                       // load the candidate prefix letter
    emitter.instruction("or edx, 32");                                          // fold the prefix letter to lowercase
    emitter.instruction("cmp edx, 120");                                        // is the prefix letter 'x'?
    emitter.instruction("jne __rt_str_to_int_base_binary_prefix_linux_x86_64"); // try the binary prefix instead
    emitter.instruction("test rcx, rcx");                                       // was base 0 requested?
    emitter.instruction("jz __rt_str_to_int_base_take_hex_linux_x86_64");       // base 0 resolves '0x' to hexadecimal
    emitter.instruction("cmp rcx, 16");                                         // was hexadecimal requested explicitly?
    emitter.instruction("jne __rt_str_to_int_base_auto_linux_x86_64");          // any other base treats 'x' as a terminator
    emitter.label("__rt_str_to_int_base_take_hex_linux_x86_64");
    emitter.instruction("mov rcx, 16");                                         // resolve the scan to base 16
    emitter.instruction("jmp __rt_str_to_int_base_skip_prefix_linux_x86_64");   // consume the two prefix bytes
    emitter.label("__rt_str_to_int_base_binary_prefix_linux_x86_64");
    emitter.instruction("cmp edx, 98");                                         // is the prefix letter 'b'?
    emitter.instruction("jne __rt_str_to_int_base_auto_linux_x86_64");          // any other letter is not a prefix
    emitter.instruction("test rcx, rcx");                                       // was base 0 requested?
    emitter.instruction("jz __rt_str_to_int_base_take_binary_linux_x86_64");    // base 0 resolves '0b' to binary
    emitter.instruction("cmp rcx, 2");                                          // was binary requested explicitly?
    emitter.instruction("jne __rt_str_to_int_base_auto_linux_x86_64");          // any other base treats 'b' as a digit or terminator
    emitter.label("__rt_str_to_int_base_take_binary_linux_x86_64");
    emitter.instruction("mov rcx, 2");                                          // resolve the scan to base 2
    emitter.label("__rt_str_to_int_base_skip_prefix_linux_x86_64");
    emitter.instruction("add rdi, 2");                                          // consume the two prefix bytes
    emitter.instruction("sub rsi, 2");                                          // record that the prefix has been consumed
    emitter.instruction("jmp __rt_str_to_int_base_ready_linux_x86_64");         // the prefix already resolved the base

    // -- resolve base 0 the way strtol() does when no 0x/0b prefix applied --
    emitter.label("__rt_str_to_int_base_auto_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // was an explicit base requested?
    emitter.instruction("jnz __rt_str_to_int_base_ready_linux_x86_64");         // an explicit base needs no auto-detection
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // inspect the first digit byte
    emitter.instruction("mov rcx, 10");                                         // default the automatic base to decimal
    emitter.instruction("cmp eax, 48");                                         // does the number start with '0'?
    emitter.instruction("jne __rt_str_to_int_base_ready_linux_x86_64");         // a non-zero lead digit keeps the decimal base
    emitter.instruction("mov rcx, 8");                                          // a leading zero selects octal, and stays a valid digit

    // -- derive the saturation limit this sign allows --
    emitter.label("__rt_str_to_int_base_ready_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // is any input byte left?
    emitter.instruction("jz __rt_str_to_int_base_zero_linux_x86_64");           // a prefix with no digits parses nothing
    abi::emit_load_int_immediate(emitter, "r10", i64::MAX);
    emitter.instruction("test r8, r8");                                         // did the scan consume a minus sign?
    emitter.instruction("jz __rt_str_to_int_base_limit_done_linux_x86_64");     // a positive parse saturates at PHP_INT_MAX
    abi::emit_load_int_immediate(emitter, "r10", i64::MIN);
    emitter.label("__rt_str_to_int_base_limit_done_linux_x86_64");
    emitter.instruction("mov rax, r10");                                        // stage the limit as the unsigned dividend
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before the unsigned division
    emitter.instruction("div rcx");                                             // rax = limit / base, the last accumulator that can still take a digit
    emitter.instruction("mov r11, rax");                                        // keep the accumulator threshold for the overflow test
    emitter.instruction("xor r9d, r9d");                                        // start the unsigned accumulator at zero

    // -- accumulate digits until the first byte that is not one --
    emitter.label("__rt_str_to_int_base_loop_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // is any input byte left?
    emitter.instruction("jz __rt_str_to_int_base_done_linux_x86_64");           // stop once every input byte has been consumed
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load the next input byte
    emitter.instruction("add rdi, 1");                                          // advance the input cursor
    emitter.instruction("sub rsi, 1");                                          // record that one input byte has been consumed
    emitter.instruction("mov rdx, rax");                                        // copy the byte before deriving its numeral value
    emitter.instruction("sub rdx, 48");                                         // try the ASCII numerals first
    emitter.instruction("cmp rdx, 9");                                          // is the byte in '0'..'9' (unsigned, so lower bytes wrap high)?
    emitter.instruction("jbe __rt_str_to_int_base_digit_linux_x86_64");         // an ASCII numeral decodes directly
    emitter.instruction("cmp rax, 65");                                         // is the byte below 'A'?
    emitter.instruction("jb __rt_str_to_int_base_done_linux_x86_64");           // a non-digit byte terminates the scan
    emitter.instruction("cmp rax, 90");                                         // is the byte above 'Z'?
    emitter.instruction("ja __rt_str_to_int_base_lower_linux_x86_64");          // try the lowercase letters instead
    emitter.instruction("mov rdx, rax");                                        // copy the byte before deriving its letter value
    emitter.instruction("sub rdx, 55");                                         // map 'A'..'Z' to digit values 10..35
    emitter.instruction("jmp __rt_str_to_int_base_digit_linux_x86_64");         // the uppercase letter decoded to a digit
    emitter.label("__rt_str_to_int_base_lower_linux_x86_64");
    emitter.instruction("cmp rax, 97");                                         // is the byte below 'a'?
    emitter.instruction("jb __rt_str_to_int_base_done_linux_x86_64");           // a non-digit byte terminates the scan
    emitter.instruction("cmp rax, 122");                                        // is the byte above 'z'?
    emitter.instruction("ja __rt_str_to_int_base_done_linux_x86_64");           // a non-digit byte terminates the scan
    emitter.instruction("mov rdx, rax");                                        // copy the byte before deriving its letter value
    emitter.instruction("sub rdx, 87");                                         // map 'a'..'z' to digit values 10..35

    emitter.label("__rt_str_to_int_base_digit_linux_x86_64");
    emitter.instruction("cmp rdx, rcx");                                        // is the decoded digit valid in the resolved base?
    emitter.instruction("jae __rt_str_to_int_base_done_linux_x86_64");          // a digit outside the base terminates the scan
    emitter.instruction("cmp r9, r11");                                         // would shifting the accumulator already pass the limit?
    emitter.instruction("ja __rt_str_to_int_base_saturate_linux_x86_64");       // an accumulator past the threshold can only overflow
    emitter.instruction("mov rax, r9");                                         // stage the accumulator for the digit shift
    emitter.instruction("imul rax, rcx");                                       // shift the accumulator up by one digit position
    emitter.instruction("mov r9, r10");                                         // copy the limit before deriving the per-digit headroom
    emitter.instruction("sub r9, rdx");                                         // compute the largest accumulator this digit still fits in
    emitter.instruction("cmp rax, r9");                                         // would adding this digit pass the limit?
    emitter.instruction("ja __rt_str_to_int_base_saturate_linux_x86_64");       // clamp instead of wrapping past PHP_INT_MAX/PHP_INT_MIN
    emitter.instruction("add rax, rdx");                                        // accumulate the digit into the unsigned result
    emitter.instruction("mov r9, rax");                                         // keep the updated accumulator
    emitter.instruction("jmp __rt_str_to_int_base_loop_linux_x86_64");          // continue scanning the remaining input bytes

    emitter.label("__rt_str_to_int_base_saturate_linux_x86_64");
    emitter.instruction("mov r9, r10");                                         // clamp to the limit this sign allows

    emitter.label("__rt_str_to_int_base_done_linux_x86_64");
    emitter.instruction("mov rax, r9");                                         // return the accumulated magnitude
    emitter.instruction("test r8, r8");                                         // did the scan consume a minus sign?
    emitter.instruction("jz __rt_str_to_int_base_ret_linux_x86_64");            // a positive parse is already the result
    emitter.instruction("neg rax");                                             // apply the consumed minus sign
    emitter.label("__rt_str_to_int_base_ret_linux_x86_64");
    emitter.instruction("ret");                                                 // hand the parsed integer back to the caller

    emitter.label("__rt_str_to_int_base_zero_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // an unusable base or digit-free string parses to zero
    emitter.instruction("ret");                                                 // hand the zero result back to the caller
}
