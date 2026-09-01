//! Purpose:
//! Emits `__rt_str_inc_dec` and `__rt_mixed_inc_dec`, the runtime implementation of PHP's
//! `++` / `--` on a string value. `__rt_str_inc_dec` takes a raw PHP byte string, applies
//! PHP's numeric-string / perl-style-alphanumeric rules, and returns the new value already
//! boxed into a Mixed cell (the operator can change the value's type, so the result is
//! always dynamically tagged). `__rt_mixed_inc_dec` is the boxed entry point: it routes a
//! string payload here and everything else to the existing numeric helper.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//! - `crate::codegen::lower_inst::strings::lower_str_inc_dec()` for `Op::StrIncDec`.
//!
//! Key details:
//! - PHP's rules, verified against PHP 8.4.20: a numeric string increments NUMERICALLY and
//!   changes type (`"9"++` is `int(10)`, `"1.5"++` is `float(2.5)`, and an int result that
//!   overflows promotes to float); the empty string increments to `string "1"` and
//!   decrements to `int(-1)`; any other string increments with perl-style alphanumeric
//!   carry (`"az"++` is `"ba"`, `"Zz"++` is `"AAa"`, `"a9"++` is `"b0"`, `"zz"++` is
//!   `"aaa"`) and DECREMENTS TO ITSELF (PHP leaves a non-numeric string unchanged).
//! - The carry stops at the first non-alphanumeric byte, so `"a-"++` is `"a-"` while
//!   `"-a"++` is `"-b"`. Bytes are compared as raw ASCII: this is byte-oriented exactly
//!   like php-src, so multi-byte characters are never carried into.
//! - The carried result is built in the shared `_concat_buf` scratch (one spare byte in
//!   front for the `z`→`aa` growth) and immediately handed to `__rt_mixed_from_value`,
//!   which persists it into owned heap storage; the scratch offset is deliberately NOT
//!   advanced because the bytes do not outlive that call.
//! - The numeric classification is `__rt_php_num_scan`'s fully-numeric flag, so it matches
//!   `is_numeric()` byte for byte (`" 5"` and `"5 "` are numeric, `"0x1A"` and `"1_0"` are
//!   not, and therefore carry alphanumerically like PHP).
//! - PHP additionally raises `E_DEPRECATED` for `++` on a non-alphanumeric string and for
//!   `--` on a non-numeric string. elephc has no runtime deprecation channel, so only the
//!   resulting VALUE is reproduced here.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};

/// Emits `__rt_str_inc_dec` for the active target.
///
/// # ABI
/// - **AArch64**: `x1` = string pointer, `x2` = string length, `x3` = delta (`+1` or `-1`)
///   → `x0` = owned boxed Mixed cell holding the new PHP value.
/// - **x86_64**: `rax` = string pointer, `rdx` = string length, `rcx` = delta
///   → `rax` = owned boxed Mixed cell.
///
/// The operand string is only read; the caller keeps its ownership.
pub fn emit_str_inc_dec(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_inc_dec_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_inc_dec (PHP ++/-- on a string) ---");
    emitter.label_global("__rt_str_inc_dec");

    // -- frame: [sp+0] ptr, [sp+8] len, [sp+16] delta, [sp+24] numeric run, [sp+32] carry prefix --
    emitter.instruction("sub sp, sp, #64");                                     // reserve the helper frame for the operand, the clipped numeric run, and saved linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame pointer and return address across nested runtime calls
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer above the saved loop state
    emitter.instruction("str x1, [sp, #0]");                                    // save the operand string pointer for the carry and unchanged paths
    emitter.instruction("str x2, [sp, #8]");                                    // save the operand string length for the carry and unchanged paths
    emitter.instruction("str x3, [sp, #16]");                                   // save the +1/-1 delta for every result path
    emitter.instruction("cbz x2, __rt_sid_empty");                              // the empty string has its own PHP rules and never reaches the scanner

    // -- classify the operand under PHP's numeric-string grammar --
    emitter.instruction("bl __rt_cstr");                                        // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("bl __rt_php_num_scan");                                // clip the scratch to PHP's leading numeric run
    emitter.instruction("str x0, [sp, #24]");                                   // save the clipped numeric run for the integer parser and strtod
    emitter.instruction("cbnz x1, __rt_sid_numeric");                           // fully numeric strings always use numeric increment/decrement
    emitter.instruction("ldr x3, [sp, #16]");                                   // inspect the caller's delta for unary-plus conversion mode
    emitter.instruction("cbnz x3, __rt_sid_alpha");                             // ordinary ++/-- keeps the alphanumeric path for nonnumeric strings
    emitter.instruction("ldrb w10, [x0]");                                      // unary plus accepts a leading numeric run with a warning
    emitter.instruction("cbz w10, __rt_sid_alpha");                             // no numeric prefix remains a TypeError at the unary-plus caller
    emitter.label("__rt_sid_numeric");

    // -- a '.' or an exponent marker in the run forces PHP's float result --
    emitter.instruction("mov x9, x0");                                          // x9 = cursor over the clipped numeric run
    emitter.label("__rt_sid_scan");
    emitter.instruction("ldrb w10, [x9]");                                      // load the next byte of the clipped numeric run
    emitter.instruction("cbz w10, __rt_sid_int");                               // an integer-shaped run ends without any float marker
    emitter.instruction("cmp w10, #46");                                        // ASCII '.' makes the numeric string a float in PHP
    emitter.instruction("b.eq __rt_sid_float");                                 // a decimal point selects the float result path
    emitter.instruction("orr w11, w10, #32");                                   // fold the byte to lowercase so 'E' and 'e' compare alike
    emitter.instruction("cmp w11, #101");                                       // ASCII 'e' introduces PHP's exponent form, which is always a float
    emitter.instruction("b.eq __rt_sid_float");                                 // an exponent marker selects the float result path
    emitter.instruction("add x9, x9, #1");                                      // advance to the next byte of the numeric run
    emitter.instruction("b __rt_sid_scan");                                     // keep scanning until the run ends or a float marker appears

    // -- integer-shaped numeric string: parse the magnitude exactly, detecting overflow --
    emitter.label("__rt_sid_int");
    emitter.instruction("ldr x9, [sp, #24]");                                   // x9 = cursor back at the start of the clipped numeric run
    emitter.instruction("mov x12, #0");                                         // x12 = 1 once a leading '-' marks the run negative
    emitter.instruction("ldrb w10, [x9]");                                      // load the optional sign byte of the numeric run
    emitter.instruction("cmp w10, #45");                                        // ASCII '-' introduces a negative numeric string
    emitter.instruction("b.ne __rt_sid_int_plus");                              // no minus sign: check for an explicit plus instead
    emitter.instruction("mov x12, #1");                                         // remember that the parsed magnitude must be negated
    emitter.instruction("add x9, x9, #1");                                      // consume the minus sign before the digits
    emitter.instruction("b __rt_sid_int_digits");                               // continue with the digit run
    emitter.label("__rt_sid_int_plus");
    emitter.instruction("cmp w10, #43");                                        // ASCII '+' is also allowed in front of a PHP numeric string
    emitter.instruction("b.ne __rt_sid_int_digits");                            // no sign at all: the digits start here
    emitter.instruction("add x9, x9, #1");                                      // consume the plus sign before the digits

    emitter.label("__rt_sid_int_digits");
    emitter.instruction("mov x14, #0");                                         // x14 = the accumulated unsigned magnitude
    emitter.instruction("mov x16, #10");                                        // x16 = the decimal radix used by the accumulation
    emitter.label("__rt_sid_int_loop");
    emitter.instruction("ldrb w10, [x9]");                                      // load the next candidate digit of the numeric run
    emitter.instruction("sub w15, w10, #48");                                   // normalize the byte into a 0..9 digit value
    emitter.instruction("cmp w15, #9");                                         // is this byte still a decimal digit?
    emitter.instruction("b.hi __rt_sid_int_parsed");                            // the digit run has ended, so the magnitude is complete
    emitter.instruction("umulh x17, x14, x16");                                 // compute the high half of magnitude * 10 to detect 64-bit overflow
    emitter.instruction("cbnz x17, __rt_sid_float");                            // a magnitude past 64 bits is PHP's float result
    emitter.instruction("mul x14, x14, x16");                                   // shift the accumulated magnitude one decimal place
    emitter.instruction("adds x14, x14, x15");                                  // add the new digit and record any carry out of 64 bits
    emitter.instruction("b.cs __rt_sid_float");                                 // an unsigned carry means the magnitude no longer fits, so PHP uses a float
    emitter.instruction("add x9, x9, #1");                                      // advance past the consumed digit
    emitter.instruction("b __rt_sid_int_loop");                                 // keep accumulating until the digit run ends

    emitter.label("__rt_sid_int_parsed");
    emitter.instruction("cbnz x12, __rt_sid_int_negative");                     // a negative run has a different magnitude bound than a positive one
    emitter.instruction("mov x16, #-1");                                        // start from an all-ones word to materialize the signed maximum
    emitter.instruction("lsr x16, x16, #1");                                    // x16 = 0x7fffffffffffffff, the largest PHP integer
    emitter.instruction("cmp x14, x16");                                        // does the parsed magnitude still fit in a PHP integer?
    emitter.instruction("b.hi __rt_sid_float");                                 // a magnitude above PHP_INT_MAX is a float numeric string
    emitter.instruction("mov x15, x14");                                        // x15 = the signed value of a positive numeric string
    emitter.instruction("b __rt_sid_int_value");                                // continue with the shared integer increment
    emitter.label("__rt_sid_int_negative");
    emitter.instruction("mov x16, #1");                                         // start from one to materialize the magnitude of PHP_INT_MIN
    emitter.instruction("lsl x16, x16, #63");                                   // x16 = 0x8000000000000000, the magnitude of the smallest PHP integer
    emitter.instruction("cmp x14, x16");                                        // does the negative magnitude still fit in a PHP integer?
    emitter.instruction("b.hi __rt_sid_float");                                 // a magnitude below PHP_INT_MIN is a float numeric string
    emitter.instruction("neg x15, x14");                                        // x15 = the signed value of a negative numeric string

    emitter.label("__rt_sid_int_value");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the +1/-1 delta for the integer increment
    emitter.instruction("adds x15, x15, x3");                                   // apply the increment and record signed overflow
    emitter.instruction("b.vs __rt_sid_float");                                 // PHP promotes an overflowing integer increment to a float
    emitter.instruction("mov x1, x15");                                         // pass the new integer as the boxing helper's low payload word
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a second word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("bl __rt_mixed_from_value");                            // box the incremented integer for the caller
    emitter.instruction("b __rt_sid_return");                                   // share the epilogue with every other result path

    // -- float-shaped (or out-of-range) numeric string: reparse and add the delta as a double --
    emitter.label("__rt_sid_float");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the clipped numeric run for the libc parser
    emitter.instruction("mov x1, #0");                                          // strtod endptr = NULL: the run is already clipped
    emitter.bl_c("strtod");                                                     // parse the clipped numeric run into d0
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the +1/-1 delta for the float increment
    emitter.instruction("scvtf d1, x3");                                        // convert the delta into a double so the addition is exact
    emitter.instruction("fadd d0, d0, d1");                                     // apply PHP's float increment to the parsed value
    emitter.instruction("fmov x1, d0");                                         // move the resulting double bits into the boxing helper payload register
    emitter.instruction("mov x2, xzr");                                         // float payloads only use the low word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = float
    emitter.instruction("bl __rt_mixed_from_value");                            // box the incremented double for the caller
    emitter.instruction("b __rt_sid_return");                                   // share the epilogue with every other result path

    // -- the empty string: PHP yields string "1" for ++ and int(-1) for -- --
    emitter.label("__rt_sid_empty");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the delta to tell the two empty-string rules apart
    emitter.instruction("cmp x3, #0");                                          // is this a decrement of the empty string?
    emitter.instruction("b.lt __rt_sid_empty_dec");                             // decrementing the empty string yields PHP's int(-1)
    abi::emit_symbol_address(emitter, "x9", "_concat_buf");
    emitter.instruction("mov w10, #49");                                        // ASCII '1' is the whole result of incrementing the empty string
    emitter.instruction("strb w10, [x9]");                                      // materialize the one-byte result in the shared scratch buffer
    emitter.instruction("mov x1, x9");                                          // pass the scratch pointer as the boxing helper's string payload
    emitter.instruction("mov x2, #1");                                          // the incremented empty string is exactly one byte long
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // persist the one-byte result and box it for the caller
    emitter.instruction("b __rt_sid_return");                                   // share the epilogue with every other result path
    emitter.label("__rt_sid_empty_dec");
    emitter.instruction("mov x1, #-1");                                         // decrementing the empty string yields PHP's int(-1)
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a second word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("bl __rt_mixed_from_value");                            // box the int(-1) result for the caller
    emitter.instruction("b __rt_sid_return");                                   // share the epilogue with every other result path

    // -- non-numeric string: '--' is a no-op, '++' carries alphanumerically --
    emitter.label("__rt_sid_alpha");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the delta to separate the increment from the decrement rule
    emitter.instruction("cmp x3, #0");                                          // is this a decrement of a non-numeric string?
    emitter.instruction("b.gt __rt_sid_carry");                                 // only the increment applies PHP's perl-style carry
    emitter.instruction("ldr x1, [sp, #0]");                                    // PHP leaves a decremented non-numeric string unchanged
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the unchanged string length
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // persist the unchanged string and box it for the caller
    emitter.instruction("b __rt_sid_return");                                   // share the epilogue with every other result path

    // -- copy the operand into scratch, leaving one spare byte for a 'z' -> 'aa' growth --
    emitter.label("__rt_sid_carry");
    abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load the current shared scratch write offset
    abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute the scratch cursor for this result
    emitter.instruction("add x9, x9, #16");                                     // keep a header-sized gap so the heap-kind probe never reads before the buffer
    emitter.instruction("ldr x10, [sp, #0]");                                   // x10 = source cursor over the operand string
    emitter.instruction("ldr x11, [sp, #8]");                                   // x11 = the operand string length
    emitter.instruction("add x12, x9, #1");                                     // x12 = destination cursor, one byte past the growth slot
    emitter.instruction("mov x13, x11");                                        // x13 = remaining bytes to copy
    emitter.label("__rt_sid_copy");
    emitter.instruction("cbz x13, __rt_sid_copied");                            // stop once the whole operand has been copied into scratch
    emitter.instruction("ldrb w14, [x10], #1");                                 // load one operand byte and advance the source cursor
    emitter.instruction("strb w14, [x12], #1");                                 // store the byte into scratch and advance the destination cursor
    emitter.instruction("sub x13, x13, #1");                                    // account for the copied byte
    emitter.instruction("b __rt_sid_copy");                                     // continue until the operand is fully copied

    // -- carry from the last byte towards the front, exactly like php-src --
    emitter.label("__rt_sid_copied");
    emitter.instruction("add x12, x9, #1");                                     // x12 = base of the mutable copy inside scratch
    emitter.instruction("mov x13, x11");                                        // x13 = one past the byte position still to be processed
    emitter.label("__rt_sid_carry_loop");
    emitter.instruction("cbz x13, __rt_sid_carry_escaped");                     // a carry out of position zero grows the string by one byte
    emitter.instruction("sub x14, x13, #1");                                    // x14 = the byte position currently being incremented
    emitter.instruction("ldrb w15, [x12, x14]");                                // load the byte the carry has reached
    emitter.instruction("cmp w15, #97");                                        // is the byte below lowercase 'a'?
    emitter.instruction("b.lo __rt_sid_not_lower");                             // check the uppercase and digit ranges instead
    emitter.instruction("cmp w15, #122");                                       // is the byte above lowercase 'z'?
    emitter.instruction("b.hi __rt_sid_stop");                                  // a byte past 'z' is not alphanumeric and stops the carry
    emitter.instruction("cmp w15, #122");                                       // is the byte exactly lowercase 'z'?
    emitter.instruction("b.eq __rt_sid_wrap_lower");                            // 'z' wraps to 'a' and carries into the previous byte
    emitter.instruction("add w15, w15, #1");                                    // any other lowercase letter simply advances by one
    emitter.instruction("strb w15, [x12, x14]");                                // store the advanced letter back into the scratch copy
    emitter.instruction("b __rt_sid_stop");                                     // the carry is absorbed, so the result is complete
    emitter.label("__rt_sid_wrap_lower");
    emitter.instruction("mov w15, #97");                                        // 'z' wraps around to lowercase 'a'
    emitter.instruction("strb w15, [x12, x14]");                                // store the wrapped letter back into the scratch copy
    emitter.instruction("mov w15, #97");                                        // a lowercase carry out of the string prepends another 'a'
    emitter.instruction("str x15, [sp, #32]");                                  // remember the prefix byte in case the carry escapes the string
    emitter.instruction("b __rt_sid_carry_next");                               // continue the carry into the previous byte
    emitter.label("__rt_sid_not_lower");
    emitter.instruction("cmp w15, #65");                                        // is the byte below uppercase 'A'?
    emitter.instruction("b.lo __rt_sid_not_upper");                             // check the digit range instead
    emitter.instruction("cmp w15, #90");                                        // is the byte above uppercase 'Z'?
    emitter.instruction("b.hi __rt_sid_stop");                                  // a byte between 'Z' and 'a' is not alphanumeric and stops the carry
    emitter.instruction("cmp w15, #90");                                        // is the byte exactly uppercase 'Z'?
    emitter.instruction("b.eq __rt_sid_wrap_upper");                            // 'Z' wraps to 'A' and carries into the previous byte
    emitter.instruction("add w15, w15, #1");                                    // any other uppercase letter simply advances by one
    emitter.instruction("strb w15, [x12, x14]");                                // store the advanced letter back into the scratch copy
    emitter.instruction("b __rt_sid_stop");                                     // the carry is absorbed, so the result is complete
    emitter.label("__rt_sid_wrap_upper");
    emitter.instruction("mov w15, #65");                                        // 'Z' wraps around to uppercase 'A'
    emitter.instruction("strb w15, [x12, x14]");                                // store the wrapped letter back into the scratch copy
    emitter.instruction("mov w15, #65");                                        // an uppercase carry out of the string prepends another 'A'
    emitter.instruction("str x15, [sp, #32]");                                  // remember the prefix byte in case the carry escapes the string
    emitter.instruction("b __rt_sid_carry_next");                               // continue the carry into the previous byte
    emitter.label("__rt_sid_not_upper");
    emitter.instruction("cmp w15, #48");                                        // is the byte below digit '0'?
    emitter.instruction("b.lo __rt_sid_stop");                                  // a byte below '0' is not alphanumeric and stops the carry
    emitter.instruction("cmp w15, #57");                                        // is the byte above digit '9'?
    emitter.instruction("b.hi __rt_sid_stop");                                  // a byte between '9' and 'A' is not alphanumeric and stops the carry
    emitter.instruction("cmp w15, #57");                                        // is the byte exactly digit '9'?
    emitter.instruction("b.eq __rt_sid_wrap_digit");                            // '9' wraps to '0' and carries into the previous byte
    emitter.instruction("add w15, w15, #1");                                    // any other digit simply advances by one
    emitter.instruction("strb w15, [x12, x14]");                                // store the advanced digit back into the scratch copy
    emitter.instruction("b __rt_sid_stop");                                     // the carry is absorbed, so the result is complete
    emitter.label("__rt_sid_wrap_digit");
    emitter.instruction("mov w15, #48");                                        // '9' wraps around to digit '0'
    emitter.instruction("strb w15, [x12, x14]");                                // store the wrapped digit back into the scratch copy
    emitter.instruction("mov w15, #49");                                        // a digit carry out of the string prepends a '1'
    emitter.instruction("str x15, [sp, #32]");                                  // remember the prefix byte in case the carry escapes the string
    emitter.label("__rt_sid_carry_next");
    emitter.instruction("sub x13, x13, #1");                                    // move the carry one byte towards the front of the string
    emitter.instruction("b __rt_sid_carry_loop");                               // keep carrying until it is absorbed or escapes

    emitter.label("__rt_sid_stop");
    emitter.instruction("add x1, x9, #1");                                      // the result starts at the copy, leaving the growth slot unused
    emitter.instruction("mov x2, x11");                                         // an absorbed carry keeps the original length
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // persist the carried result and box it for the caller
    emitter.instruction("b __rt_sid_return");                                   // share the epilogue with every other result path

    emitter.label("__rt_sid_carry_escaped");
    emitter.instruction("ldr x15, [sp, #32]");                                  // reload the prefix byte chosen by the last wrap
    emitter.instruction("strb w15, [x9]");                                      // write the prefix into the reserved growth slot
    emitter.instruction("mov x1, x9");                                          // the grown result starts one byte earlier
    emitter.instruction("add x2, x11, #1");                                     // an escaped carry makes the result one byte longer
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // persist the grown result and box it for the caller

    emitter.label("__rt_sid_return");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed Mixed result in x0
}

/// Emits the Linux x86_64 implementation of `__rt_str_inc_dec`.
///
/// Same contract and same PHP rules as the AArch64 helper: `rax` = string pointer,
/// `rdx` = string length, `rcx` = delta → `rax` = owned boxed Mixed cell.
fn emit_str_inc_dec_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_inc_dec (PHP ++/-- on a string) ---");
    emitter.label_global("__rt_str_inc_dec");

    // -- frame: [rbp-8] ptr, [rbp-16] len, [rbp-24] delta, [rbp-32] numeric run, [rbp-40] carry prefix --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before nested runtime and libc calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the helper locals
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for the operand, the clipped run, and the carry prefix
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the operand string pointer for the carry and unchanged paths
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the operand string length for the carry and unchanged paths
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the +1/-1 delta for every result path
    emitter.instruction("test rdx, rdx");                                       // does the operand hold any byte at all?
    emitter.instruction("jz __rt_sid_empty_x86");                               // the empty string has its own PHP rules and never reaches the scanner

    // -- classify the operand under PHP's numeric-string grammar --
    emitter.instruction("call __rt_cstr");                                      // copy the bounded PHP string into the C-string scratch buffer
    emitter.instruction("mov rdi, rax");                                        // pass the C-string pointer to the numeric-grammar scanner
    emitter.instruction("call __rt_php_num_scan");                              // clip the scratch to PHP's leading numeric run
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the clipped numeric run for the integer parser and strtod
    emitter.instruction("test rdx, rdx");                                       // did the scanner report the whole string as numeric?
    emitter.instruction("jnz __rt_sid_numeric_x86");                            // fully numeric strings always use numeric increment/decrement
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // inspect the caller's delta for unary-plus conversion mode
    emitter.instruction("jne __rt_sid_alpha_x86");                              // ordinary ++/-- keeps the alphanumeric path for nonnumeric strings
    emitter.instruction("cmp BYTE PTR [rax], 0");                               // unary plus accepts a leading numeric run with a warning
    emitter.instruction("je __rt_sid_alpha_x86");                               // no numeric prefix remains a TypeError at the unary-plus caller
    emitter.label("__rt_sid_numeric_x86");

    // -- a '.' or an exponent marker in the run forces PHP's float result --
    emitter.instruction("mov r8, rax");                                         // r8 = cursor over the clipped numeric run
    emitter.label("__rt_sid_scan_x86");
    emitter.instruction("movzx r9d, BYTE PTR [r8]");                            // load the next byte of the clipped numeric run
    emitter.instruction("test r9b, r9b");                                       // has the clipped run reached its terminator?
    emitter.instruction("jz __rt_sid_int_x86");                                 // an integer-shaped run ends without any float marker
    emitter.instruction("cmp r9b, 46");                                         // ASCII '.' makes the numeric string a float in PHP
    emitter.instruction("je __rt_sid_float_x86");                               // a decimal point selects the float result path
    emitter.instruction("or r9b, 32");                                          // fold the byte to lowercase so 'E' and 'e' compare alike
    emitter.instruction("cmp r9b, 101");                                        // ASCII 'e' introduces PHP's exponent form, which is always a float
    emitter.instruction("je __rt_sid_float_x86");                               // an exponent marker selects the float result path
    emitter.instruction("add r8, 1");                                           // advance to the next byte of the numeric run
    emitter.instruction("jmp __rt_sid_scan_x86");                               // keep scanning until the run ends or a float marker appears

    // -- integer-shaped numeric string: parse the magnitude exactly, detecting overflow --
    emitter.label("__rt_sid_int_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // r8 = cursor back at the start of the clipped numeric run
    emitter.instruction("xor r11d, r11d");                                      // r11 = 1 once a leading '-' marks the run negative
    emitter.instruction("movzx r9d, BYTE PTR [r8]");                            // load the optional sign byte of the numeric run
    emitter.instruction("cmp r9b, 45");                                         // ASCII '-' introduces a negative numeric string
    emitter.instruction("jne __rt_sid_int_plus_x86");                           // no minus sign: check for an explicit plus instead
    emitter.instruction("mov r11d, 1");                                         // remember that the parsed magnitude must be negated
    emitter.instruction("add r8, 1");                                           // consume the minus sign before the digits
    emitter.instruction("jmp __rt_sid_int_digits_x86");                         // continue with the digit run
    emitter.label("__rt_sid_int_plus_x86");
    emitter.instruction("cmp r9b, 43");                                         // ASCII '+' is also allowed in front of a PHP numeric string
    emitter.instruction("jne __rt_sid_int_digits_x86");                         // no sign at all: the digits start here
    emitter.instruction("add r8, 1");                                           // consume the plus sign before the digits

    emitter.label("__rt_sid_int_digits_x86");
    emitter.instruction("xor r10d, r10d");                                      // r10 = the accumulated unsigned magnitude
    emitter.label("__rt_sid_int_loop_x86");
    emitter.instruction("movzx r9d, BYTE PTR [r8]");                            // load the next candidate digit of the numeric run
    emitter.instruction("sub r9d, 48");                                         // normalize the byte into a 0..9 digit value
    emitter.instruction("cmp r9d, 9");                                          // is this byte still a decimal digit?
    emitter.instruction("ja __rt_sid_int_parsed_x86");                          // the digit run has ended, so the magnitude is complete
    emitter.instruction("mov rax, r10");                                        // stage the accumulated magnitude in the multiplier's implicit operand
    emitter.instruction("mov rcx, 10");                                         // the decimal radix used by the accumulation
    emitter.instruction("mul rcx");                                             // multiply the magnitude by ten, leaving any overflow in rdx
    emitter.instruction("test rdx, rdx");                                       // did the decimal shift leave the 64-bit range?
    emitter.instruction("jnz __rt_sid_float_x86");                              // a magnitude past 64 bits is PHP's float result
    emitter.instruction("movsxd rcx, r9d");                                     // widen the parsed digit before adding it to the magnitude
    emitter.instruction("add rax, rcx");                                        // add the new digit and record any carry out of 64 bits
    emitter.instruction("jc __rt_sid_float_x86");                               // an unsigned carry means the magnitude no longer fits, so PHP uses a float
    emitter.instruction("mov r10, rax");                                        // keep the updated magnitude for the next digit
    emitter.instruction("add r8, 1");                                           // advance past the consumed digit
    emitter.instruction("jmp __rt_sid_int_loop_x86");                           // keep accumulating until the digit run ends

    emitter.label("__rt_sid_int_parsed_x86");
    emitter.instruction("test r11, r11");                                       // is the parsed numeric string negative?
    emitter.instruction("jnz __rt_sid_int_negative_x86");                       // a negative run has a different magnitude bound than a positive one
    emitter.instruction("mov rcx, 0x7fffffffffffffff");                         // the largest PHP integer bounds a positive numeric string
    emitter.instruction("cmp r10, rcx");                                        // does the parsed magnitude still fit in a PHP integer?
    emitter.instruction("ja __rt_sid_float_x86");                               // a magnitude above PHP_INT_MAX is a float numeric string
    emitter.instruction("mov rax, r10");                                        // rax = the signed value of a positive numeric string
    emitter.instruction("jmp __rt_sid_int_value_x86");                          // continue with the shared integer increment
    emitter.label("__rt_sid_int_negative_x86");
    emitter.instruction("mov rcx, 0x8000000000000000");                         // the magnitude of PHP_INT_MIN bounds a negative numeric string
    emitter.instruction("cmp r10, rcx");                                        // does the negative magnitude still fit in a PHP integer?
    emitter.instruction("ja __rt_sid_float_x86");                               // a magnitude below PHP_INT_MIN is a float numeric string
    emitter.instruction("mov rax, r10");                                        // stage the magnitude before turning it into a negative value
    emitter.instruction("neg rax");                                             // rax = the signed value of a negative numeric string

    emitter.label("__rt_sid_int_value_x86");
    emitter.instruction("add rax, QWORD PTR [rbp - 24]");                       // apply the +1/-1 delta and record signed overflow
    emitter.instruction("jo __rt_sid_float_x86");                               // PHP promotes an overflowing integer increment to a float
    emitter.instruction("mov rdi, rax");                                        // pass the new integer as the boxing helper's low payload word
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a second word
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    emitter.instruction("call __rt_mixed_from_value");                          // box the incremented integer for the caller
    emitter.instruction("jmp __rt_sid_return_x86");                             // share the epilogue with every other result path

    // -- float-shaped (or out-of-range) numeric string: reparse and add the delta as a double --
    emitter.label("__rt_sid_float_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the clipped numeric run for the libc parser
    emitter.instruction("xor esi, esi");                                        // strtod endptr = NULL: the run is already clipped
    emitter.instruction("call strtod");                                         // parse the clipped numeric run into xmm0
    emitter.instruction("cvtsi2sd xmm1, QWORD PTR [rbp - 24]");                 // convert the +1/-1 delta into a double so the addition is exact
    emitter.instruction("addsd xmm0, xmm1");                                    // apply PHP's float increment to the parsed value
    emitter.instruction("movq rdi, xmm0");                                      // move the resulting double bits into the boxing helper payload register
    emitter.instruction("xor esi, esi");                                        // float payloads only use the low word
    emitter.instruction("mov rax, 2");                                          // runtime tag 2 = float
    emitter.instruction("call __rt_mixed_from_value");                          // box the incremented double for the caller
    emitter.instruction("jmp __rt_sid_return_x86");                             // share the epilogue with every other result path

    // -- the empty string: PHP yields string "1" for ++ and int(-1) for -- --
    emitter.label("__rt_sid_empty_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // is this a decrement of the empty string?
    emitter.instruction("jl __rt_sid_empty_dec_x86");                           // decrementing the empty string yields PHP's int(-1)
    abi::emit_symbol_address(emitter, "r8", "_concat_buf");
    emitter.instruction("mov BYTE PTR [r8], 49");                               // ASCII '1' is the whole result of incrementing the empty string
    emitter.instruction("mov rdi, r8");                                         // pass the scratch pointer as the boxing helper's string payload
    emitter.instruction("mov rsi, 1");                                          // the incremented empty string is exactly one byte long
    emitter.instruction("mov rax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // persist the one-byte result and box it for the caller
    emitter.instruction("jmp __rt_sid_return_x86");                             // share the epilogue with every other result path
    emitter.label("__rt_sid_empty_dec_x86");
    emitter.instruction("mov rdi, -1");                                         // decrementing the empty string yields PHP's int(-1)
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a second word
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    emitter.instruction("call __rt_mixed_from_value");                          // box the int(-1) result for the caller
    emitter.instruction("jmp __rt_sid_return_x86");                             // share the epilogue with every other result path

    // -- non-numeric string: '--' is a no-op, '++' carries alphanumerically --
    emitter.label("__rt_sid_alpha_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // is this a decrement of a non-numeric string?
    emitter.instruction("jg __rt_sid_carry_x86");                               // only the increment applies PHP's perl-style carry
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // PHP leaves a decremented non-numeric string unchanged
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the unchanged string length
    emitter.instruction("mov rax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // persist the unchanged string and box it for the caller
    emitter.instruction("jmp __rt_sid_return_x86");                             // share the epilogue with every other result path

    // -- copy the operand into scratch, leaving one spare byte for a 'z' -> 'aa' growth --
    emitter.label("__rt_sid_carry_x86");
    abi::emit_symbol_address(emitter, "rcx", "_concat_off");
    emitter.instruction("mov rcx, QWORD PTR [rcx]");                            // load the current shared scratch write offset
    abi::emit_symbol_address(emitter, "r8", "_concat_buf");
    emitter.instruction("add r8, rcx");                                         // compute the scratch cursor for this result
    emitter.instruction("add r8, 16");                                          // keep a header-sized gap so the heap-kind probe never reads before the buffer
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // r9 = source cursor over the operand string
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // r10 = the operand string length
    emitter.instruction("lea r11, [r8 + 1]");                                   // r11 = destination cursor, one byte past the growth slot
    emitter.instruction("mov rcx, r10");                                        // rcx = remaining bytes to copy
    emitter.label("__rt_sid_copy_x86");
    emitter.instruction("test rcx, rcx");                                       // stop once the whole operand has been copied into scratch
    emitter.instruction("jz __rt_sid_copied_x86");                              // the scratch copy is complete
    emitter.instruction("mov al, BYTE PTR [r9]");                               // load one operand byte from the source cursor
    emitter.instruction("mov BYTE PTR [r11], al");                              // store the byte into the scratch copy
    emitter.instruction("add r9, 1");                                           // advance the source cursor
    emitter.instruction("add r11, 1");                                          // advance the destination cursor
    emitter.instruction("sub rcx, 1");                                          // account for the copied byte
    emitter.instruction("jmp __rt_sid_copy_x86");                               // continue until the operand is fully copied

    // -- carry from the last byte towards the front, exactly like php-src --
    emitter.label("__rt_sid_copied_x86");
    emitter.instruction("lea r11, [r8 + 1]");                                   // r11 = base of the mutable copy inside scratch
    emitter.instruction("mov rcx, r10");                                        // rcx = one past the byte position still to be processed
    emitter.label("__rt_sid_carry_loop_x86");
    emitter.instruction("test rcx, rcx");                                       // has the carry moved past the front of the string?
    emitter.instruction("jz __rt_sid_carry_escaped_x86");                       // a carry out of position zero grows the string by one byte
    emitter.instruction("movzx eax, BYTE PTR [r11 + rcx - 1]");                 // load the byte the carry has reached
    emitter.instruction("cmp al, 97");                                          // is the byte below lowercase 'a'?
    emitter.instruction("jb __rt_sid_not_lower_x86");                           // check the uppercase and digit ranges instead
    emitter.instruction("cmp al, 122");                                         // is the byte above lowercase 'z'?
    emitter.instruction("ja __rt_sid_stop_x86");                                // a byte past 'z' is not alphanumeric and stops the carry
    emitter.instruction("cmp al, 122");                                         // is the byte exactly lowercase 'z'?
    emitter.instruction("je __rt_sid_wrap_lower_x86");                          // 'z' wraps to 'a' and carries into the previous byte
    emitter.instruction("add al, 1");                                           // any other lowercase letter simply advances by one
    emitter.instruction("mov BYTE PTR [r11 + rcx - 1], al");                    // store the advanced letter back into the scratch copy
    emitter.instruction("jmp __rt_sid_stop_x86");                               // the carry is absorbed, so the result is complete
    emitter.label("__rt_sid_wrap_lower_x86");
    emitter.instruction("mov BYTE PTR [r11 + rcx - 1], 97");                    // 'z' wraps around to lowercase 'a'
    emitter.instruction("mov QWORD PTR [rbp - 40], 97");                        // a lowercase carry out of the string prepends another 'a'
    emitter.instruction("jmp __rt_sid_carry_next_x86");                         // continue the carry into the previous byte
    emitter.label("__rt_sid_not_lower_x86");
    emitter.instruction("cmp al, 65");                                          // is the byte below uppercase 'A'?
    emitter.instruction("jb __rt_sid_not_upper_x86");                           // check the digit range instead
    emitter.instruction("cmp al, 90");                                          // is the byte above uppercase 'Z'?
    emitter.instruction("ja __rt_sid_stop_x86");                                // a byte between 'Z' and 'a' is not alphanumeric and stops the carry
    emitter.instruction("cmp al, 90");                                          // is the byte exactly uppercase 'Z'?
    emitter.instruction("je __rt_sid_wrap_upper_x86");                          // 'Z' wraps to 'A' and carries into the previous byte
    emitter.instruction("add al, 1");                                           // any other uppercase letter simply advances by one
    emitter.instruction("mov BYTE PTR [r11 + rcx - 1], al");                    // store the advanced letter back into the scratch copy
    emitter.instruction("jmp __rt_sid_stop_x86");                               // the carry is absorbed, so the result is complete
    emitter.label("__rt_sid_wrap_upper_x86");
    emitter.instruction("mov BYTE PTR [r11 + rcx - 1], 65");                    // 'Z' wraps around to uppercase 'A'
    emitter.instruction("mov QWORD PTR [rbp - 40], 65");                        // an uppercase carry out of the string prepends another 'A'
    emitter.instruction("jmp __rt_sid_carry_next_x86");                         // continue the carry into the previous byte
    emitter.label("__rt_sid_not_upper_x86");
    emitter.instruction("cmp al, 48");                                          // is the byte below digit '0'?
    emitter.instruction("jb __rt_sid_stop_x86");                                // a byte below '0' is not alphanumeric and stops the carry
    emitter.instruction("cmp al, 57");                                          // is the byte above digit '9'?
    emitter.instruction("ja __rt_sid_stop_x86");                                // a byte between '9' and 'A' is not alphanumeric and stops the carry
    emitter.instruction("cmp al, 57");                                          // is the byte exactly digit '9'?
    emitter.instruction("je __rt_sid_wrap_digit_x86");                          // '9' wraps to '0' and carries into the previous byte
    emitter.instruction("add al, 1");                                           // any other digit simply advances by one
    emitter.instruction("mov BYTE PTR [r11 + rcx - 1], al");                    // store the advanced digit back into the scratch copy
    emitter.instruction("jmp __rt_sid_stop_x86");                               // the carry is absorbed, so the result is complete
    emitter.label("__rt_sid_wrap_digit_x86");
    emitter.instruction("mov BYTE PTR [r11 + rcx - 1], 48");                    // '9' wraps around to digit '0'
    emitter.instruction("mov QWORD PTR [rbp - 40], 49");                        // a digit carry out of the string prepends a '1'
    emitter.label("__rt_sid_carry_next_x86");
    emitter.instruction("sub rcx, 1");                                          // move the carry one byte towards the front of the string
    emitter.instruction("jmp __rt_sid_carry_loop_x86");                         // keep carrying until it is absorbed or escapes

    emitter.label("__rt_sid_stop_x86");
    emitter.instruction("lea rdi, [r8 + 1]");                                   // the result starts at the copy, leaving the growth slot unused
    emitter.instruction("mov rsi, r10");                                        // an absorbed carry keeps the original length
    emitter.instruction("mov rax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // persist the carried result and box it for the caller
    emitter.instruction("jmp __rt_sid_return_x86");                             // share the epilogue with every other result path

    emitter.label("__rt_sid_carry_escaped_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the prefix byte chosen by the last wrap
    emitter.instruction("mov BYTE PTR [r8], al");                               // write the prefix into the reserved growth slot
    emitter.instruction("mov rdi, r8");                                         // the grown result starts one byte earlier
    emitter.instruction("lea rsi, [r10 + 1]");                                  // an escaped carry makes the result one byte longer
    emitter.instruction("mov rax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // persist the grown result and box it for the caller

    emitter.label("__rt_sid_return_x86");
    emitter.instruction("mov rsp, rbp");                                        // release every helper-local slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed result in rax
}

/// Emits `__rt_mixed_inc_dec`, the boxed entry point for PHP's `++` / `--`.
///
/// A string payload is routed to [`emit_str_inc_dec`]'s helper so PHP's string rules apply;
/// every other payload keeps the pre-existing numeric behavior by boxing the delta and
/// reusing `__rt_mixed_numeric_add` (adding `-1` is `- 1` for both the integer and the
/// float paths, so one helper covers `++` and `--`).
///
/// # ABI
/// - **AArch64**: `x0` = borrowed boxed Mixed cell, `x1` = delta → `x0` = owned Mixed cell.
/// - **x86_64**: `rax` = borrowed boxed Mixed cell, `rdi` = delta → `rax` = owned Mixed cell.
pub fn emit_mixed_inc_dec(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_inc_dec_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_inc_dec (PHP ++/-- on a boxed value) ---");
    emitter.label_global("__rt_mixed_inc_dec");

    emitter.instruction("sub sp, sp, #48");                                     // reserve the helper frame for the operand, the delta, and the temporaries
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame pointer and return address across nested calls
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer above the saved state
    emitter.instruction("str x0, [sp, #0]");                                    // save the borrowed operand cell for the numeric path
    emitter.instruction("str x1, [sp, #8]");                                    // save the +1/-1 delta for both result paths
    emitter.instruction("bl __rt_mixed_unbox");                                 // read the operand's runtime tag and payload words
    emitter.instruction("cmp x0, #1");                                          // does the boxed operand hold a string payload?
    emitter.instruction("b.ne __rt_mid_numeric");                               // every other payload keeps the existing numeric behavior
    emitter.instruction("ldr x3, [sp, #8]");                                    // reload the delta as the string helper's third argument
    emitter.instruction("bl __rt_str_inc_dec");                                 // apply PHP's string increment/decrement rules
    emitter.instruction("b __rt_mid_return");                                   // share the epilogue with the numeric path

    emitter.label("__rt_mid_numeric");
    emitter.instruction("ldr x1, [sp, #8]");                                    // the delta becomes the right-hand operand of the numeric addition
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a second word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("bl __rt_mixed_from_value");                            // box the delta so the shared numeric helper can consume it
    emitter.instruction("str x0, [sp, #16]");                                   // keep the boxed delta so it can be released afterwards
    emitter.instruction("mov x1, x0");                                          // pass the boxed delta as the right-hand operand
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the borrowed operand cell as the left-hand operand
    emitter.instruction("bl __rt_mixed_numeric_add");                           // reuse PHP's boxed numeric addition, including overflow promotion
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the boxed result while the delta temporary is released
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the boxed delta temporary
    emitter.instruction("bl __rt_decref_mixed");                                // release the temporary so the increment does not leak one cell per use
    emitter.instruction("ldr x0, [sp, #24]");                                   // restore the boxed result for the caller

    emitter.label("__rt_mid_return");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed Mixed result in x0
}

/// Emits the Linux x86_64 implementation of `__rt_mixed_inc_dec`.
///
/// Same contract as the AArch64 helper: `rax` = borrowed boxed Mixed cell, `rdi` = delta
/// → `rax` = owned boxed Mixed cell.
fn emit_mixed_inc_dec_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_inc_dec (PHP ++/-- on a boxed value) ---");
    emitter.label_global("__rt_mixed_inc_dec");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before nested runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the helper locals
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for the operand, the delta, and the temporaries
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the borrowed operand cell for the numeric path
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the +1/-1 delta for both result paths
    emitter.instruction("call __rt_mixed_unbox");                               // read the operand's runtime tag and payload words
    emitter.instruction("cmp rax, 1");                                          // does the boxed operand hold a string payload?
    emitter.instruction("jne __rt_mid_numeric_x86");                            // every other payload keeps the existing numeric behavior
    emitter.instruction("mov rax, rdi");                                        // the unboxed payload low word is the string pointer the helper expects in rax
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the delta as the string helper's third argument (the length is already in rdx)
    emitter.instruction("call __rt_str_inc_dec");                               // apply PHP's string increment/decrement rules
    emitter.instruction("jmp __rt_mid_return_x86");                             // share the epilogue with the numeric path

    emitter.label("__rt_mid_numeric_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the delta becomes the right-hand operand of the numeric addition
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a second word
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = int
    emitter.instruction("call __rt_mixed_from_value");                          // box the delta so the shared numeric helper can consume it
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // keep the boxed delta so it can be released afterwards
    emitter.instruction("mov rdi, rax");                                        // pass the boxed delta as the right-hand operand
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the borrowed operand cell as the left-hand operand
    emitter.instruction("call __rt_mixed_numeric_add");                         // reuse PHP's boxed numeric addition, including overflow promotion
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the boxed result while the delta temporary is released
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the boxed delta temporary
    emitter.instruction("call __rt_decref_mixed");                              // release the temporary so the increment does not leak one cell per use
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // restore the boxed result for the caller

    emitter.label("__rt_mid_return_x86");
    emitter.instruction("mov rsp, rbp");                                        // release every helper-local slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed result in rax
}
