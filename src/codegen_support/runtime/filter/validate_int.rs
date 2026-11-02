//! Purpose:
//! Emits the `__rt_filter_validate_int` runtime helper: a DEDICATED decimal-only
//! integer parser backing `filter_var($v, FILTER_VALIDATE_INT)` on string input.
//! Never reuses `strtoll`/`intval`-style parsing, which accepts octal/hex
//! prefixes PHP's filter rejects.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::filter`.
//! - `crate::codegen::lower_inst::builtins::filter` (the `filter_var()` EIR lowering).
//!
//! Key details:
//! - Grammar (php-verified with `php -n -r 'var_dump(filter_var(..., FILTER_VALIDATE_INT));'`,
//!   PHP 8.5.6 local): optional PHP-filter whitespace (see `__rt_filter_trim_ws`),
//!   optional `+`/`-` sign, then one or more DECIMAL digits only — no `0x`/octal
//!   forms. `"012"` and `"00"` are REJECTED (a leading `0` is only valid when it is
//!   the sole digit, e.g. `"0"`/`"-0"`/`"+0"` all parse to `0`); `"0x1A"` is rejected
//!   at the first non-digit byte (`x`); `""`/whitespace-only/sign-only are rejected.
//! - Overflow uses a saturating-comparison accumulation against
//!   `PHP_INT_MAX/10 == PHP_INT_MIN_MAGNITUDE/10 == 922337203685477580` (same
//!   for both signs) with a sign-dependent last-digit bound (7 for positive,
//!   8 for negative, since `|PHP_INT_MIN| == PHP_INT_MAX + 1`) — this matches
//!   PHP exactly: `"9223372036854775807"` succeeds, `"9223372036854775808"` and
//!   `"-9223372036854775809"` fail, `"-9223372036854775808"` succeeds as
//!   `PHP_INT_MIN`.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// `PHP_INT_MAX / 10` == the unsigned magnitude limit `/ 10` for `PHP_INT_MIN`
/// (both signs share this constant: `9223372036854775807 / 10 ==
/// 9223372036854775808 / 10 == 922337203685477580`).
const LIMIT_DIV10: i64 = 922_337_203_685_477_580;

/// Emits `__rt_filter_validate_int` for the host target.
///
/// AArch64: input x1=ptr, x2=len. Output: x0=success (0/1), x1=parsed `i64`
/// (defined only when x0=1).
/// x86_64: input rax=ptr, rdx=len. Output: rax=success (0/1), rdx=parsed `i64`
/// (defined only when rax=1).
pub fn emit_filter_validate_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_filter_validate_int_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: filter_validate_int ---");
    emitter.label_global("__rt_filter_validate_int");

    // -- set up the helper frame (only needed to preserve x30 across the nested trim call) --
    emitter.instruction("sub sp, sp, #16");                                     // allocate the frame slot for the saved link register
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across the trim call
    emitter.instruction("add x29, sp, #0");                                     // establish a stable helper frame pointer
    emitter.instruction("bl __rt_filter_trim_ws");                              // trim PHP-filter whitespace: x1=ptr, x2=len (trimmed)

    emitter.instruction("cbz x2, __rt_filter_validate_int_fail");               // an empty (post-trim) string is never a valid integer

    // -- optional sign --
    emitter.instruction("mov x3, #0");                                          // scan index
    emitter.instruction("mov x6, #0");                                          // sign flag: 0 = positive, 1 = negative
    emitter.instruction("ldrb w4, [x1]");                                       // load the first byte to check for a sign
    emitter.instruction("cmp w4, #0x2D");                                       // is it '-'?
    emitter.instruction("b.eq __rt_filter_validate_int_minus");                 // negative sign found
    emitter.instruction("cmp w4, #0x2B");                                       // is it '+'?
    emitter.instruction("b.eq __rt_filter_validate_int_plus");                  // positive sign found
    emitter.instruction("b __rt_filter_validate_int_nosign");                   // no sign present
    emitter.label("__rt_filter_validate_int_minus");
    emitter.instruction("mov x6, #1");                                          // remember the negative sign
    emitter.instruction("add x3, x3, #1");                                      // consume the sign byte
    emitter.instruction("b __rt_filter_validate_int_nosign");                   // continue to digit scanning
    emitter.label("__rt_filter_validate_int_plus");
    emitter.instruction("add x3, x3, #1");                                      // consume the sign byte
    emitter.label("__rt_filter_validate_int_nosign");
    emitter.instruction("cmp x3, x2");                                          // did the sign consume the whole string?
    emitter.instruction("b.ge __rt_filter_validate_int_fail");                  // sign-only input is never valid

    // -- digit scan with saturating overflow detection --
    emitter.instruction("mov x5, #0");                                          // digit count
    emitter.instruction("mov x7, #0");                                          // unsigned magnitude accumulator
    emitter.instruction("mov x10, #0");                                         // first digit's value (captured below, used for the leading-zero rule)
    abi::emit_load_int_immediate(emitter, "x9", LIMIT_DIV10);                 // x9 = PHP_INT_MAX/10 overflow-detection limit

    emitter.label("__rt_filter_validate_int_loop");
    emitter.instruction("cmp x3, x2");                                          // reached the end of the (trimmed) string?
    emitter.instruction("b.ge __rt_filter_validate_int_digits_done");           // finish scanning once exhausted
    emitter.instruction("ldrb w4, [x1, x3]");                                   // load the current byte
    emitter.instruction("sub w8, w4, #0x30");                                   // normalize to a candidate decimal digit
    emitter.instruction("cmp w8, #9");                                          // is it a decimal digit (0-9)?
    emitter.instruction("b.hi __rt_filter_validate_int_fail");                  // any non-digit byte (including 'x' from a hex prefix) fails

    emitter.instruction("cbnz x5, __rt_filter_validate_int_not_first");         // only the FIRST digit needs capturing
    emitter.instruction("mov x10, x8");                                         // remember the first digit's value for the leading-zero rule
    emitter.label("__rt_filter_validate_int_not_first");

    emitter.instruction("cmp x7, x9");                                          // compare the accumulator against the overflow limit
    emitter.instruction("b.hi __rt_filter_validate_int_fail");                  // already past the limit: this digit always overflows
    emitter.instruction("b.ne __rt_filter_validate_int_no_mod_check");          // strictly below the limit: this digit can never overflow
    emitter.instruction("cbnz x6, __rt_filter_validate_int_check_mod_neg");     // at the limit exactly: bound depends on the sign
    emitter.instruction("cmp x8, #7");                                          // positive bound: PHP_INT_MAX's last digit is 7
    emitter.instruction("b.hi __rt_filter_validate_int_fail");                  // digit exceeds the positive last-digit bound
    emitter.instruction("b __rt_filter_validate_int_no_mod_check");             // within bounds, proceed to accumulate
    emitter.label("__rt_filter_validate_int_check_mod_neg");
    emitter.instruction("cmp x8, #8");                                          // negative bound: |PHP_INT_MIN|'s last digit is 8
    emitter.instruction("b.hi __rt_filter_validate_int_fail");                  // digit exceeds the negative last-digit bound
    emitter.label("__rt_filter_validate_int_no_mod_check");

    emitter.instruction("mov x11, #10");                                        // decimal shift factor
    emitter.instruction("mul x7, x7, x11");                                     // shift the accumulator up one decimal place
    emitter.instruction("add x7, x7, x8");                                      // fold in this digit

    emitter.instruction("add x3, x3, #1");                                      // advance to the next byte
    emitter.instruction("add x5, x5, #1");                                      // record one more consumed digit
    emitter.instruction("b __rt_filter_validate_int_loop");                     // continue scanning digits

    emitter.label("__rt_filter_validate_int_digits_done");
    emitter.instruction("cbz x5, __rt_filter_validate_int_fail");               // a sign with no digits at all is never valid
    emitter.instruction("cbnz x10, __rt_filter_validate_int_success");          // first digit non-zero: no leading-zero issue
    emitter.instruction("cmp x5, #1");                                          // first digit is '0': only a single digit is allowed
    emitter.instruction("b.gt __rt_filter_validate_int_fail");                  // "00"/"012"-style leading zero: reject

    emitter.label("__rt_filter_validate_int_success");
    emitter.instruction("mov x0, #1");                                          // report success
    emitter.instruction("cbz x6, __rt_filter_validate_int_positive");           // branch on the recorded sign
    emitter.instruction("neg x1, x7");                                          // negate the magnitude for a negative result
    emitter.instruction("b __rt_filter_validate_int_done");                     // done
    emitter.label("__rt_filter_validate_int_positive");
    emitter.instruction("mov x1, x7");                                          // the positive magnitude is the final value
    emitter.instruction("b __rt_filter_validate_int_done");                     // done

    emitter.label("__rt_filter_validate_int_fail");
    emitter.instruction("mov x0, #0");                                          // report failure
    emitter.instruction("mov x1, #0");                                          // the value is undefined on failure

    emitter.label("__rt_filter_validate_int_done");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return (x0=success, x1=value)
}

/// Emits the x86_64 System V variant of `__rt_filter_validate_int`.
fn emit_filter_validate_int_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: filter_validate_int ---");
    emitter.label_global("__rt_filter_validate_int");

    // System V wants rsp 16-byte aligned at the `call`, and this helper is entered with the
    // caller's return address already pushed, so eight bytes of padding is the whole frame it
    // needs — `__rt_filter_trim_ws` takes its arguments in registers.
    emitter.instruction("sub rsp, 8");                                          // realign the stack for the System V call below
    emitter.instruction("call __rt_filter_trim_ws");                            // trim PHP-filter whitespace: rax=ptr, rdx=len (trimmed)
    emitter.instruction("add rsp, 8");                                          // release the alignment padding

    emitter.instruction("test rdx, rdx");                                       // an empty (post-trim) string is never a valid integer
    emitter.instruction("je __rt_filter_validate_int_fail_x86_64");             // reject empty input

    // -- optional sign --
    emitter.instruction("xor r8, r8");                                          // scan index
    emitter.instruction("xor ecx, ecx");                                        // sign flag: 0 = positive, 1 = negative
    emitter.instruction("movzx r9d, BYTE PTR [rax]");                           // load the first byte to check for a sign
    emitter.instruction("cmp r9d, 0x2D");                                       // is it '-'?
    emitter.instruction("je __rt_filter_validate_int_minus_x86_64");            // negative sign found
    emitter.instruction("cmp r9d, 0x2B");                                       // is it '+'?
    emitter.instruction("je __rt_filter_validate_int_plus_x86_64");             // positive sign found
    emitter.instruction("jmp __rt_filter_validate_int_nosign_x86_64");          // no sign present
    emitter.label("__rt_filter_validate_int_minus_x86_64");
    emitter.instruction("mov ecx, 1");                                          // remember the negative sign
    emitter.instruction("inc r8");                                              // consume the sign byte
    emitter.instruction("jmp __rt_filter_validate_int_nosign_x86_64");          // continue to digit scanning
    emitter.label("__rt_filter_validate_int_plus_x86_64");
    emitter.instruction("inc r8");                                              // consume the sign byte
    emitter.label("__rt_filter_validate_int_nosign_x86_64");
    emitter.instruction("cmp r8, rdx");                                         // did the sign consume the whole string?
    emitter.instruction("jge __rt_filter_validate_int_fail_x86_64");            // sign-only input is never valid

    // -- digit scan with saturating overflow detection --
    emitter.instruction("xor r10, r10");                                        // digit count
    emitter.instruction("xor r11, r11");                                        // unsigned magnitude accumulator
    emitter.instruction("xor rdi, rdi");                                        // first digit's value (captured below)
    abi::emit_load_int_immediate(emitter, "rsi", LIMIT_DIV10);                 // rsi = PHP_INT_MAX/10 overflow-detection limit

    emitter.label("__rt_filter_validate_int_loop_x86_64");
    emitter.instruction("cmp r8, rdx");                                         // reached the end of the (trimmed) string?
    emitter.instruction("jge __rt_filter_validate_int_digits_done_x86_64");     // finish scanning once exhausted
    emitter.instruction("movzx r9d, BYTE PTR [rax + r8]");                      // load the current byte
    emitter.instruction("sub r9d, 0x30");                                       // normalize to a candidate decimal digit
    emitter.instruction("cmp r9d, 9");                                          // is it a decimal digit (0-9)?
    emitter.instruction("ja __rt_filter_validate_int_fail_x86_64");             // any non-digit byte (including 'x' from a hex prefix) fails

    emitter.instruction("test r10, r10");                                       // only the FIRST digit needs capturing
    emitter.instruction("jnz __rt_filter_validate_int_not_first_x86_64");       // skip capture past the first digit
    emitter.instruction("mov rdi, r9");                                         // remember the first digit's value for the leading-zero rule
    emitter.label("__rt_filter_validate_int_not_first_x86_64");

    emitter.instruction("cmp r11, rsi");                                        // compare the accumulator against the overflow limit
    emitter.instruction("ja __rt_filter_validate_int_fail_x86_64");             // already past the limit: this digit always overflows
    emitter.instruction("jne __rt_filter_validate_int_no_mod_check_x86_64");    // strictly below the limit: this digit can never overflow
    emitter.instruction("test ecx, ecx");                                       // at the limit exactly: bound depends on the sign
    emitter.instruction("jnz __rt_filter_validate_int_mod_neg_x86_64");         // negative-sign bound path
    emitter.instruction("cmp r9, 7");                                           // positive bound: PHP_INT_MAX's last digit is 7
    emitter.instruction("ja __rt_filter_validate_int_fail_x86_64");             // digit exceeds the positive last-digit bound
    emitter.instruction("jmp __rt_filter_validate_int_no_mod_check_x86_64");    // within bounds, proceed to accumulate
    emitter.label("__rt_filter_validate_int_mod_neg_x86_64");
    emitter.instruction("cmp r9, 8");                                           // negative bound: |PHP_INT_MIN|'s last digit is 8
    emitter.instruction("ja __rt_filter_validate_int_fail_x86_64");             // digit exceeds the negative last-digit bound
    emitter.label("__rt_filter_validate_int_no_mod_check_x86_64");

    emitter.instruction("imul r11, r11, 10");                                   // shift the accumulator up one decimal place (rax/rdx-safe three-operand form)
    emitter.instruction("add r11, r9");                                         // fold in this digit

    emitter.instruction("inc r8");                                              // advance to the next byte
    emitter.instruction("inc r10");                                             // record one more consumed digit
    emitter.instruction("jmp __rt_filter_validate_int_loop_x86_64");            // continue scanning digits

    emitter.label("__rt_filter_validate_int_digits_done_x86_64");
    emitter.instruction("test r10, r10");                                       // a sign with no digits at all is never valid
    emitter.instruction("jz __rt_filter_validate_int_fail_x86_64");             // reject sign-only/empty digit runs
    emitter.instruction("test rdi, rdi");                                       // first digit non-zero: no leading-zero issue
    emitter.instruction("jnz __rt_filter_validate_int_success_x86_64");         // skip the leading-zero check
    emitter.instruction("cmp r10, 1");                                          // first digit is '0': only a single digit is allowed
    emitter.instruction("jg __rt_filter_validate_int_fail_x86_64");             // "00"/"012"-style leading zero: reject

    emitter.label("__rt_filter_validate_int_success_x86_64");
    emitter.instruction("mov eax, 1");                                          // report success
    emitter.instruction("test ecx, ecx");                                       // branch on the recorded sign
    emitter.instruction("jz __rt_filter_validate_int_positive_x86_64");         // positive path
    emitter.instruction("mov rdx, r11");                                        // move the magnitude into the value result register
    emitter.instruction("neg rdx");                                             // negate the magnitude for a negative result
    emitter.instruction("jmp __rt_filter_validate_int_done_x86_64");            // done
    emitter.label("__rt_filter_validate_int_positive_x86_64");
    emitter.instruction("mov rdx, r11");                                        // the positive magnitude is the final value
    emitter.instruction("jmp __rt_filter_validate_int_done_x86_64");            // done

    emitter.label("__rt_filter_validate_int_fail_x86_64");
    emitter.instruction("xor eax, eax");                                        // report failure
    emitter.instruction("xor edx, edx");                                        // the value is undefined on failure

    emitter.label("__rt_filter_validate_int_done_x86_64");
    emitter.instruction("ret");                                                 // return (rax=success, rdx=value)
}
