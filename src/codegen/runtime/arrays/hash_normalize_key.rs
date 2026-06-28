//! Purpose:
//! Emits the `__rt_hash_normalize_key`, `__rt_hash_normalize_key_string` runtime helper assembly for hash normalize key.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Hash helpers must normalize PHP keys and preserve bucket layout, ownership, and iteration conventions.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_hash_normalize_key` runtime helper.
///
/// Dispatches to the target-specific implementation after a blank line and global label.
/// For ARM64: input x1=string_ptr, x2=string_len; output x1=key_lo, x2=key_hi where
/// key_hi=-1 indicates an integer key and key_hi=0 signals a string key (x1/x2 unchanged).
/// For x86_64 Linux: input rax=string_ptr, rdx=string_len; output rax=key_lo, rdx=key_hi
/// with the same conventions. String keys return with rax/rdx (x1/x2) untouched.
pub fn emit_hash_normalize_key(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_normalize_key_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_normalize_key ---");
    emitter.label_global("__rt_hash_normalize_key");

    emitter.instruction("cbz x2, __rt_hash_normalize_key_string");              // empty strings remain string keys
    emitter.instruction("mov x9, x1");                                          // copy the scan pointer so the original key payload remains returnable
    emitter.instruction("mov x10, x2");                                         // copy the remaining byte length for numeric-string validation
    emitter.instruction("mov x11, #0");                                         // sign flag = 0 for positive numeric strings
    emitter.instruction("ldrb w12, [x9]");                                      // load the first byte to classify sign, zero, or digit cases
    emitter.instruction("cmp w12, #45");                                        // is the first byte '-'?
    emitter.instruction("b.ne __rt_hash_normalize_key_unsigned");               // unsigned numeric strings start directly with a digit
    emitter.instruction("cmp x10, #1");                                         // a bare '-' is not a numeric array key
    emitter.instruction("b.eq __rt_hash_normalize_key_string");                 // keep bare '-' as a string key
    emitter.instruction("mov x11, #1");                                         // sign flag = 1 for negative numeric strings
    emitter.instruction("add x9, x9, #1");                                      // skip the leading '-' before digit validation
    emitter.instruction("sub x10, x10, #1");                                    // shrink the digit count after consuming the sign byte
    emitter.instruction("ldrb w12, [x9]");                                      // load the first digit after the '-' sign
    emitter.instruction("cmp w12, #48");                                        // negative zero or negative values with leading zero stay string keys
    emitter.instruction("b.eq __rt_hash_normalize_key_string");                 // PHP does not cast '-0' or '-01' array keys to integers
    emitter.instruction("b __rt_hash_normalize_key_digit_start");               // validate the non-zero negative digit sequence

    emitter.label("__rt_hash_normalize_key_unsigned");
    emitter.instruction("cmp w12, #48");                                        // does the unsigned key start with '0'?
    emitter.instruction("b.ne __rt_hash_normalize_key_digit_start");            // non-zero unsigned numeric strings parse normally
    emitter.instruction("cmp x10, #1");                                         // only the exact string '0' casts to integer zero
    emitter.instruction("b.ne __rt_hash_normalize_key_string");                 // leading-zero strings like '01' remain string keys
    emitter.instruction("mov x1, #0");                                          // normalized integer key payload = 0
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks the key as an integer
    emitter.instruction("ret");                                                 // return the normalized integer zero key

    emitter.label("__rt_hash_normalize_key_digit_start");
    emitter.instruction("cmp w12, #49");                                        // reject bytes below '1' for non-zero numeric keys
    emitter.instruction("b.lt __rt_hash_normalize_key_string");                 // non-digit or leading-zero input remains a string key
    emitter.instruction("cmp w12, #57");                                        // reject bytes above '9' for non-zero numeric keys
    emitter.instruction("b.gt __rt_hash_normalize_key_string");                 // non-digit input remains a string key
    emitter.instruction("mov x13, #0");                                         // accumulator = 0 before parsing the decimal digits

    emitter.label("__rt_hash_normalize_key_parse");
    emitter.instruction("cbz x10, __rt_hash_normalize_key_parsed");             // finish once every digit byte has been consumed
    emitter.instruction("ldrb w12, [x9], #1");                                  // load the next digit byte and advance the scan pointer
    emitter.instruction("cmp w12, #48");                                        // reject bytes below '0' while parsing
    emitter.instruction("b.lt __rt_hash_normalize_key_string");                 // non-digit input remains a string key
    emitter.instruction("cmp w12, #57");                                        // reject bytes above '9' while parsing
    emitter.instruction("b.gt __rt_hash_normalize_key_string");                 // non-digit input remains a string key
    emitter.instruction("sub x12, x12, #48");                                   // convert the ASCII digit into its numeric value
    emitter.instruction("movz x15, #0xcccc");                                   // materialize max_int_div_10 bits 15:0 for overflow checking
    emitter.instruction("movk x15, #0xcccc, lsl #16");                          // materialize max_int_div_10 bits 31:16
    emitter.instruction("movk x15, #0xcccc, lsl #32");                          // materialize max_int_div_10 bits 47:32
    emitter.instruction("movk x15, #0x0ccc, lsl #48");                          // materialize max_int_div_10 bits 63:48
    emitter.instruction("cmp x13, x15");                                        // compare accumulator against the largest safe value before multiplying by ten
    emitter.instruction("b.hi __rt_hash_normalize_key_string");                 // overflowing numeric strings remain string keys
    emitter.instruction("b.lo __rt_hash_normalize_key_no_overflow");            // smaller accumulators are safe for another digit
    emitter.instruction("cbz x11, __rt_hash_normalize_key_pos_limit");          // positive max-int allows only a final digit up to 7
    emitter.instruction("cmp x12, #8");                                         // negative min-int allows one extra absolute-value unit
    emitter.instruction("b.gt __rt_hash_normalize_key_string");                 // overflowing negative numeric strings remain string keys
    emitter.instruction("b __rt_hash_normalize_key_no_overflow");               // the final negative-boundary digit is safe
    emitter.label("__rt_hash_normalize_key_pos_limit");
    emitter.instruction("cmp x12, #7");                                         // positive max-int allows only a final digit up to 7
    emitter.instruction("b.gt __rt_hash_normalize_key_string");                 // overflowing positive numeric strings remain string keys
    emitter.label("__rt_hash_normalize_key_no_overflow");
    emitter.instruction("mov x14, #10");                                        // decimal parsing multiplies the accumulator by ten
    emitter.instruction("mul x13, x13, x14");                                   // shift the existing accumulator one decimal place
    emitter.instruction("add x13, x13, x12");                                   // add the new digit to the accumulator
    emitter.instruction("sub x10, x10, #1");                                    // consume one remaining digit byte
    emitter.instruction("b __rt_hash_normalize_key_parse");                     // continue validating and parsing the digit sequence

    emitter.label("__rt_hash_normalize_key_parsed");
    emitter.instruction("cbz x11, __rt_hash_normalize_key_positive");           // positive numeric strings use the accumulator as-is
    emitter.instruction("neg x13, x13");                                        // negative numeric strings store the negated accumulator
    emitter.label("__rt_hash_normalize_key_positive");
    emitter.instruction("mov x1, x13");                                         // publish the normalized integer key payload
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks the key as an integer
    emitter.instruction("ret");                                                 // return the normalized integer key

    emitter.label("__rt_hash_normalize_key_string");
    emitter.instruction("ret");                                                 // leave x1/x2 unchanged for string keys
}

/// Emits the x86_64 Linux variant of `__rt_hash_normalize_key`.
///
/// Input registers: rax=string_ptr, rdx=string_len (rdi/rsi/rcx/r8/r9/r10 are used as scratch).
/// Output: rax=key_lo, rdx=key_hi where key_hi=-1 marks an integer key and key_hi=0
/// means the original string pointer/length are returned unchanged (string key).
/// The function validates numeric strings against PHP array-key semantics:
/// - Empty strings remain string keys.
/// - Exact `"0"` (unsigned) or `"0"` (negative) are integer zero.
/// - Leading zeros beyond `"0"` keep the key as a string.
/// - Overflow beyond i64 bounds keeps the key as a string.
/// - A bare `"-"` is a string key, not an integer.
fn emit_hash_normalize_key_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_normalize_key ---");
    emitter.label_global("__rt_hash_normalize_key");

    emitter.instruction("test rdx, rdx");                                       // empty strings remain string keys
    emitter.instruction("jz __rt_hash_normalize_key_string");                   // return the original empty string payload unchanged
    emitter.instruction("mov rdi, rax");                                        // copy the scan pointer so the original key payload remains returnable
    emitter.instruction("mov rsi, rdx");                                        // copy the remaining byte length for numeric-string validation
    emitter.instruction("xor ecx, ecx");                                        // sign flag = 0 for positive numeric strings
    emitter.instruction("movzx r8d, BYTE PTR [rdi]");                           // load the first byte to classify sign, zero, or digit cases
    emitter.instruction("cmp r8b, 45");                                         // is the first byte '-'?
    emitter.instruction("jne __rt_hash_normalize_key_unsigned");                // unsigned numeric strings start directly with a digit
    emitter.instruction("cmp rsi, 1");                                          // a bare '-' is not a numeric array key
    emitter.instruction("je __rt_hash_normalize_key_string");                   // keep bare '-' as a string key
    emitter.instruction("mov rcx, 1");                                          // sign flag = 1 for negative numeric strings
    emitter.instruction("add rdi, 1");                                          // skip the leading '-' before digit validation
    emitter.instruction("sub rsi, 1");                                          // shrink the digit count after consuming the sign byte
    emitter.instruction("movzx r8d, BYTE PTR [rdi]");                           // load the first digit after the '-' sign
    emitter.instruction("cmp r8b, 48");                                         // negative zero or negative values with leading zero stay string keys
    emitter.instruction("je __rt_hash_normalize_key_string");                   // PHP does not cast '-0' or '-01' array keys to integers
    emitter.instruction("jmp __rt_hash_normalize_key_digit_start");             // validate the non-zero negative digit sequence

    emitter.label("__rt_hash_normalize_key_unsigned");
    emitter.instruction("cmp r8b, 48");                                         // does the unsigned key start with '0'?
    emitter.instruction("jne __rt_hash_normalize_key_digit_start");             // non-zero unsigned numeric strings parse normally
    emitter.instruction("cmp rsi, 1");                                          // only the exact string '0' casts to integer zero
    emitter.instruction("jne __rt_hash_normalize_key_string");                  // leading-zero strings like '01' remain string keys
    emitter.instruction("xor eax, eax");                                        // normalized integer key payload = 0
    emitter.instruction("mov rdx, -1");                                         // key_hi sentinel marks the key as an integer
    emitter.instruction("ret");                                                 // return the normalized integer zero key

    emitter.label("__rt_hash_normalize_key_digit_start");
    emitter.instruction("cmp r8b, 49");                                         // reject bytes below '1' for non-zero numeric keys
    emitter.instruction("jl __rt_hash_normalize_key_string");                   // non-digit or leading-zero input remains a string key
    emitter.instruction("cmp r8b, 57");                                         // reject bytes above '9' for non-zero numeric keys
    emitter.instruction("jg __rt_hash_normalize_key_string");                   // non-digit input remains a string key
    emitter.instruction("xor r9d, r9d");                                        // accumulator = 0 before parsing the decimal digits

    emitter.label("__rt_hash_normalize_key_parse");
    emitter.instruction("test rsi, rsi");                                       // finish once every digit byte has been consumed
    emitter.instruction("jz __rt_hash_normalize_key_parsed");                   // leave the parse loop with a complete integer accumulator
    emitter.instruction("movzx r8d, BYTE PTR [rdi]");                           // load the next digit byte for validation
    emitter.instruction("add rdi, 1");                                          // advance the scan pointer after reading the digit
    emitter.instruction("cmp r8b, 48");                                         // reject bytes below '0' while parsing
    emitter.instruction("jl __rt_hash_normalize_key_string");                   // non-digit input remains a string key
    emitter.instruction("cmp r8b, 57");                                         // reject bytes above '9' while parsing
    emitter.instruction("jg __rt_hash_normalize_key_string");                   // non-digit input remains a string key
    emitter.instruction("sub r8, 48");                                          // convert the ASCII digit into its numeric value
    emitter.instruction("mov r10, 922337203685477580");                         // materialize max_int_div_10 for overflow checking
    emitter.instruction("cmp r9, r10");                                         // compare accumulator against the largest safe value before multiplying by ten
    emitter.instruction("ja __rt_hash_normalize_key_string");                   // overflowing numeric strings remain string keys
    emitter.instruction("jb __rt_hash_normalize_key_no_overflow");              // smaller accumulators are safe for another digit
    emitter.instruction("test rcx, rcx");                                       // choose the final-digit limit based on the sign flag
    emitter.instruction("jz __rt_hash_normalize_key_pos_limit");                // positive max-int allows only a final digit up to 7
    emitter.instruction("cmp r8, 8");                                           // negative min-int allows one extra absolute-value unit
    emitter.instruction("jg __rt_hash_normalize_key_string");                   // overflowing negative numeric strings remain string keys
    emitter.instruction("jmp __rt_hash_normalize_key_no_overflow");             // the final negative-boundary digit is safe
    emitter.label("__rt_hash_normalize_key_pos_limit");
    emitter.instruction("cmp r8, 7");                                           // positive max-int allows only a final digit up to 7
    emitter.instruction("jg __rt_hash_normalize_key_string");                   // overflowing positive numeric strings remain string keys
    emitter.label("__rt_hash_normalize_key_no_overflow");
    emitter.instruction("imul r9, r9, 10");                                     // shift the existing accumulator one decimal place
    emitter.instruction("add r9, r8");                                          // add the new digit to the accumulator
    emitter.instruction("sub rsi, 1");                                          // consume one remaining digit byte
    emitter.instruction("jmp __rt_hash_normalize_key_parse");                   // continue validating and parsing the digit sequence

    emitter.label("__rt_hash_normalize_key_parsed");
    emitter.instruction("test rcx, rcx");                                       // check whether the original numeric string had a '-' prefix
    emitter.instruction("jz __rt_hash_normalize_key_positive");                 // positive numeric strings use the accumulator as-is
    emitter.instruction("neg r9");                                              // negative numeric strings store the negated accumulator
    emitter.label("__rt_hash_normalize_key_positive");
    emitter.instruction("mov rax, r9");                                         // publish the normalized integer key payload
    emitter.instruction("mov rdx, -1");                                         // key_hi sentinel marks the key as an integer
    emitter.instruction("ret");                                                 // return the normalized integer key

    emitter.label("__rt_hash_normalize_key_string");
    emitter.instruction("ret");                                                 // leave rax/rdx unchanged for string keys
}
