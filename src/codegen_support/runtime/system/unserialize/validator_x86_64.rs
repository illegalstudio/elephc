//! Purpose:
//! Emits the x86_64 allocation-free grammar preflight for serialized input.
//!
//! Called from:
//! - `super::emit_unserialize()` after the public entry wrapper and before the mutating decoder.
//!
//! Key details:
//! - Every cursor, delimiter, length, and recursive child is bounded before allocation or hooks.

use crate::codegen_support::emit::Emitter;

/// Emits the x86_64 allocation-free grammar preflight used before decoding.
///
/// This mirrors the AArch64 validator and rejects malformed cursors, overflowing
/// decimal fields, and unterminated containers before the mutating parser runs.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: bounded unserialize grammar preflight ---");

    // uint(base=rdi, pos=rsi, end=rdx, delimiter=cl) -> rax=ok, rsi=value, rdx=delimiter position
    emitter.label_global("__rt_unser_validate_uint");
    emitter.instruction("lea r8, [rdi + rsi]");                                 // absolute digit cursor
    emitter.instruction("lea r9, [rdi + rdx]");                                 // absolute source end
    emitter.instruction("xor r10d, r10d");                                      // unsigned accumulator
    emitter.instruction("xor r11d, r11d");                                      // parsed digit count
    emitter.label("__rt_unser_validate_uint_loop_x");
    emitter.instruction("cmp r8, r9");                                          // is another byte available?
    emitter.instruction("jae __rt_unser_validate_uint_fail_x");                 // truncated digit run has no delimiter
    emitter.instruction("movzx r12d, BYTE PTR [r8]");                           // inspect one bounded byte
    emitter.instruction("cmp r12d, 48");                                        // below ASCII zero?
    emitter.instruction("jb __rt_unser_validate_uint_done_x");                  // require the requested delimiter below
    emitter.instruction("cmp r12d, 57");                                        // above ASCII nine?
    emitter.instruction("ja __rt_unser_validate_uint_done_x");                  // require the requested delimiter below
    emitter.instruction("sub r12d, 48");                                        // convert the byte to a digit
    emitter.instruction("mov r13, 1844674407370955161");                        // floor(u64::MAX / 10)
    emitter.instruction("cmp r10, r13");                                        // would multiplication overflow?
    emitter.instruction("ja __rt_unser_validate_uint_fail_x");                  // one more decimal shift overflows u64
    emitter.instruction("jne __rt_unser_validate_uint_mul_x");                  // strictly below: any digit is safe
    emitter.instruction("cmp r12d, 5");                                         // final digit limit when accumulator equals the threshold
    emitter.instruction("ja __rt_unser_validate_uint_fail_x");                  // appending it would pass u64::MAX
    emitter.label("__rt_unser_validate_uint_mul_x");
    emitter.instruction("imul r10, r10, 10");                                   // shift the accumulator by one decimal place
    emitter.instruction("add r10, r12");                                        // append the current digit
    emitter.instruction("add r11, 1");                                          // record one valid digit
    emitter.instruction("add r8, 1");                                           // advance within the proven source extent
    emitter.instruction("jmp __rt_unser_validate_uint_loop_x");                 // consume the next digit
    emitter.label("__rt_unser_validate_uint_done_x");
    emitter.instruction("test r11, r11");                                       // was at least one digit parsed?
    emitter.instruction("jz __rt_unser_validate_uint_fail_x");                  // a field needs at least one digit
    emitter.instruction("cmp r12b, cl");                                        // did the run end on its grammar delimiter?
    emitter.instruction("jne __rt_unser_validate_uint_fail_x");                 // wrong terminator for this field
    emitter.instruction("mov rdx, r8");                                         // absolute delimiter cursor
    emitter.instruction("sub rdx, rdi");                                        // return delimiter position as an offset
    emitter.instruction("mov rsi, r10");                                        // return parsed value
    emitter.instruction("mov eax, 1");                                          // report success
    emitter.instruction("ret");                                                 // return value and delimiter to the caller
    emitter.label("__rt_unser_validate_uint_fail_x");
    emitter.instruction("xor eax, eax");                                        // report a bounded numeric failure
    emitter.instruction("ret");                                                 // return the failure flag to the caller

    // key(base=rdi, pos=rsi, end=rdx, depth=rcx) -> rax=ok, rdx=newpos
    emitter.label_global("__rt_unser_validate_key");
    emitter.instruction("cmp rsi, rdx");                                        // require the key type byte before loading it
    emitter.instruction("jae __rt_unser_validate_key_fail_x");                  // truncated entry has no key
    emitter.instruction("movzx r8d, BYTE PTR [rdi + rsi]");                     // inspect the bounded key type
    emitter.instruction("cmp r8d, 105");                                        // integer key?
    emitter.instruction("je __rt_unser_validate_key_dispatch_x");               // join through a local conditional target
    emitter.instruction("cmp r8d, 115");                                        // string key?
    emitter.instruction("jne __rt_unser_validate_key_fail_x");                  // reject every other key marker
    emitter.label("__rt_unser_validate_key_dispatch_x");
    emitter.instruction("jmp __rt_unser_validate_at");                          // main validator owns integer/string grammar
    emitter.label("__rt_unser_validate_key_fail_x");
    emitter.instruction("xor eax, eax");                                        // only integer and string keys are valid
    emitter.instruction("ret");                                                 // return the failure flag to the container loop

    // at(base=rdi, pos=rsi, end=rdx, depth=rcx) -> rax=ok, rdx=newpos
    emitter.label_global("__rt_unser_validate_at");
    emitter.instruction("push rbp");                                            // preserve the caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish recursive validator frame
    emitter.instruction("sub rsp, 48");                                         // reserve base/pos/end/depth/count/index state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // source base
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // starting position
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // source end
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // recursion depth
    emitter.instruction("cmp rsi, rdx");                                        // require a type byte before dispatch
    emitter.instruction("jae __rt_unser_validate_at_fail_x");                   // truncated input has no value here
    emitter.instruction("cmp rcx, 512");                                        // enforce parser recursion ceiling
    emitter.instruction("jae __rt_unser_validate_at_fail_x");                   // nesting past the 512 ceiling
    emitter.instruction("movzx r8d, BYTE PTR [rdi + rsi]");                     // bounded type byte
    emitter.instruction("cmp r8d, 78");                                         // null marker (N)?
    emitter.instruction("je __rt_unser_validate_null_x");                       // validate the N; envelope
    emitter.instruction("cmp r8d, 98");                                         // bool marker (b)?
    emitter.instruction("je __rt_unser_validate_bool_x");                       // validate the b:<0|1>; envelope
    emitter.instruction("cmp r8d, 105");                                        // int marker (i)?
    emitter.instruction("je __rt_unser_validate_int_x");                        // validate the signed decimal envelope
    emitter.instruction("cmp r8d, 100");                                        // float marker (d)?
    emitter.instruction("je __rt_unser_validate_float_x");                      // validate the float envelope
    emitter.instruction("cmp r8d, 115");                                        // string marker (s)?
    emitter.instruction("je __rt_unser_validate_string_x");                     // validate the length-prefixed payload
    emitter.instruction("cmp r8d, 97");                                         // array marker (a)?
    emitter.instruction("je __rt_unser_validate_array_x");                      // recurse over the array entries
    emitter.instruction("cmp r8d, 79");                                         // object marker (O)?
    emitter.instruction("je __rt_unser_validate_object_x");                     // recurse over the object properties
    emitter.instruction("cmp r8d, 114");                                        // back-reference marker (r)?
    emitter.instruction("je __rt_unser_validate_ref_x");                        // validate the back-reference index
    emitter.instruction("cmp r8d, 82");                                         // by-reference marker (R)?
    emitter.instruction("je __rt_unser_validate_ref_x");                        // r and R share one index grammar
    emitter.instruction("jmp __rt_unser_validate_at_fail_x");                   // unknown type marker

    emitter.label("__rt_unser_validate_null_x");
    emitter.instruction("mov r8, rdx");                                         // bytes remaining from N
    emitter.instruction("sub r8, rsi");                                         // finish the remaining-byte count
    emitter.instruction("cmp r8, 2");                                           // N plus semicolon
    emitter.instruction("jb __rt_unser_validate_at_fail_x");                    // truncated null envelope
    emitter.instruction("cmp BYTE PTR [rdi + rsi + 1], 59");                    // exact semicolon
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // N must be terminated by a semicolon
    emitter.instruction("mov rdx, rsi");                                        // seed new position from start
    emitter.instruction("add rdx, 2");                                          // skip N;
    emitter.instruction("jmp __rt_unser_validate_at_ok_x");                     // null value fully bounded

    emitter.label("__rt_unser_validate_bool_x");
    emitter.instruction("mov r8, rdx");                                         // bytes remaining from b
    emitter.instruction("sub r8, rsi");                                         // finish the remaining-byte count
    emitter.instruction("cmp r8, 4");                                           // exact b:<digit>; envelope
    emitter.instruction("jb __rt_unser_validate_at_fail_x");                    // truncated bool envelope
    emitter.instruction("cmp BYTE PTR [rdi + rsi + 1], 58");                    // colon after b
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // b must be followed by a colon
    emitter.instruction("movzx r8d, BYTE PTR [rdi + rsi + 2]");                 // inspect the truth digit
    emitter.instruction("cmp r8d, 48");                                         // false (0)?
    emitter.instruction("je __rt_unser_validate_bool_delim_x");                 // 0 is a valid truth digit
    emitter.instruction("cmp r8d, 49");                                         // true (1)?
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // only 0 and 1 encode booleans
    emitter.label("__rt_unser_validate_bool_delim_x");
    emitter.instruction("cmp BYTE PTR [rdi + rsi + 3], 59");                    // terminating semicolon
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // unterminated bool envelope
    emitter.instruction("mov rdx, rsi");                                        // seed new position from start
    emitter.instruction("add rdx, 4");                                          // skip b:<digit>;
    emitter.instruction("jmp __rt_unser_validate_at_ok_x");                     // bool value fully bounded

    emitter.label("__rt_unser_validate_int_x");
    emitter.instruction("lea r8, [rsi + 1]");                                   // colon position
    emitter.instruction("cmp r8, rdx");                                         // is the colon in bounds?
    emitter.instruction("jae __rt_unser_validate_at_fail_x");                   // truncated integer envelope
    emitter.instruction("cmp BYTE PTR [rdi + r8], 58");                         // exact colon
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // i must be followed by a colon
    emitter.instruction("add r8, 1");                                           // first sign/digit position
    emitter.instruction("cmp r8, rdx");                                         // is the sign/digit in bounds?
    emitter.instruction("jae __rt_unser_validate_at_fail_x");                   // truncated integer envelope
    emitter.instruction("cmp BYTE PTR [rdi + r8], 45");                         // leading minus sign?
    emitter.instruction("jne __rt_unser_validate_int_digits_x");                // unsigned: parse digits directly
    emitter.instruction("mov QWORD PTR [rbp - 48], 1");                         // record a negative integer
    emitter.instruction("add r8, 1");                                           // skip optional minus
    emitter.instruction("jmp __rt_unser_validate_int_scan_x");                  // scan the digits after the sign
    emitter.label("__rt_unser_validate_int_digits_x");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // positive integer
    emitter.label("__rt_unser_validate_int_scan_x");
    emitter.instruction("mov rsi, r8");                                         // digit cursor for the helper
    emitter.instruction("mov ecx, 59");                                         // integer terminator
    emitter.instruction("call __rt_unser_validate_uint");                       // parse the bounded magnitude
    emitter.instruction("test rax, rax");                                       // did the digit run validate?
    emitter.instruction("jz __rt_unser_validate_at_fail_x");                    // malformed integer digits
    emitter.instruction("mov r8, 9223372036854775807");                         // i64::MAX magnitude
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // negative-sign flag
    emitter.instruction("je __rt_unser_validate_int_positive_x");               // positive values use the i64::MAX bound
    emitter.instruction("cmp rsi, r8");                                         // negative magnitude at most i64::MAX + 1
    emitter.instruction("jbe __rt_unser_validate_int_range_ok_x");              // magnitude fits a negative i64
    emitter.instruction("sub rsi, r8");                                         // only one extra magnitude value is representable
    emitter.instruction("cmp rsi, 1");                                          // exactly the i64::MIN magnitude?
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // reject magnitudes beyond i64::MIN
    emitter.instruction("jmp __rt_unser_validate_int_range_ok_x");              // i64::MIN itself is representable
    emitter.label("__rt_unser_validate_int_positive_x");
    emitter.instruction("cmp rsi, r8");                                         // positive magnitude at most i64::MAX
    emitter.instruction("ja __rt_unser_validate_at_fail_x");                    // reject magnitudes beyond i64::MAX
    emitter.label("__rt_unser_validate_int_range_ok_x");
    emitter.instruction("add rdx, 1");                                          // skip semicolon
    emitter.instruction("jmp __rt_unser_validate_at_ok_x");                     // integer value fully bounded

    emitter.label("__rt_unser_validate_float_x");
    emitter.instruction("lea r8, [rsi + 1]");                                   // colon position
    emitter.instruction("cmp r8, rdx");                                         // is the colon in bounds?
    emitter.instruction("jae __rt_unser_validate_at_fail_x");                   // truncated float envelope
    emitter.instruction("cmp BYTE PTR [rdi + r8], 58");                         // exact colon
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // d must be followed by a colon
    emitter.instruction("add r8, 1");                                           // first float byte
    emitter.instruction("mov r9, r8");                                          // remember start
    emitter.label("__rt_unser_validate_float_loop_x");
    emitter.instruction("cmp r8, rdx");                                         // is another payload byte available?
    emitter.instruction("jae __rt_unser_validate_at_fail_x");                   // unterminated float payload
    emitter.instruction("cmp BYTE PTR [rdi + r8], 59");                         // reached the terminating semicolon?
    emitter.instruction("je __rt_unser_validate_float_done_x");                 // the payload ends here
    emitter.instruction("add r8, 1");                                           // scan past one payload byte
    emitter.instruction("jmp __rt_unser_validate_float_loop_x");                // keep hunting for the semicolon
    emitter.label("__rt_unser_validate_float_done_x");
    emitter.instruction("cmp r8, r9");                                          // reject empty float payload
    emitter.instruction("je __rt_unser_validate_at_fail_x");                    // d:; carries no digits
    emitter.instruction("lea rdx, [r8 + 1]");                                   // position after semicolon
    emitter.instruction("jmp __rt_unser_validate_at_ok_x");                     // float envelope fully bounded

    emitter.label("__rt_unser_validate_string_x");
    emitter.instruction("lea r8, [rsi + 1]");                                   // colon after s
    emitter.instruction("cmp r8, rdx");                                         // is the colon in bounds?
    emitter.instruction("jae __rt_unser_validate_at_fail_x");                   // truncated string envelope
    emitter.instruction("cmp BYTE PTR [rdi + r8], 58");                         // exact colon
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // s must be followed by a colon
    emitter.instruction("lea rsi, [r8 + 1]");                                   // first length digit
    emitter.instruction("mov ecx, 58");                                         // length delimiter
    emitter.instruction("call __rt_unser_validate_uint");                       // parse the declared byte length
    emitter.instruction("test rax, rax");                                       // did the length validate?
    emitter.instruction("jz __rt_unser_validate_at_fail_x");                    // malformed length field
    emitter.instruction("mov r11, rsi");                                        // declared string length
    emitter.instruction("lea r8, [rdx + 1]");                                   // opening quote position
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // end
    emitter.instruction("cmp r8, r9");                                          // is the opening quote in bounds?
    emitter.instruction("jae __rt_unser_validate_at_fail_x");                   // truncated string envelope
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base after helper call
    emitter.instruction("cmp BYTE PTR [rdi + r8], 34");                         // exact opening quote
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // the payload must be quoted
    emitter.instruction("add r8, 1");                                           // raw payload position
    emitter.instruction("mov r10, r9");                                         // start the remaining count from end
    emitter.instruction("sub r10, r8");                                         // remaining bytes
    emitter.instruction("cmp r10, 2");                                          // closing quote plus semicolon
    emitter.instruction("jb __rt_unser_validate_at_fail_x");                    // no room for quote and semicolon
    emitter.instruction("sub r10, 2");                                          // room left for payload bytes alone
    emitter.instruction("cmp r11, r10");                                        // declared payload fits?
    emitter.instruction("ja __rt_unser_validate_at_fail_x");                    // declared length overruns the input
    emitter.instruction("add r8, r11");                                         // closing quote position
    emitter.instruction("cmp BYTE PTR [rdi + r8], 34");                         // exact closing quote
    emitter.instruction("jne __rt_unser_validate_string_close_fail_x");         // payload must end at the declared length
    emitter.instruction("add r8, 1");                                           // semicolon position
    emitter.instruction("cmp BYTE PTR [rdi + r8], 59");                         // terminating semicolon
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // unterminated string envelope
    emitter.instruction("lea rdx, [r8 + 1]");                                   // position after semicolon
    emitter.instruction("jmp __rt_unser_validate_at_ok_x");                     // string value fully bounded
    emitter.label("__rt_unser_validate_string_close_fail_x");
    emitter.instruction("mov rsi, r8");                                        // PHP reports the declared closing-quote position
    emitter.instruction("jmp __rt_unser_validate_at_fail_x");                  // return the precise malformed-string offset

    emitter.label("__rt_unser_validate_array_x");
    emitter.instruction("lea r8, [rsi + 1]");                                   // colon after a
    emitter.instruction("cmp r8, rdx");                                         // is the colon in bounds?
    emitter.instruction("jae __rt_unser_validate_at_fail_x");                   // truncated array envelope
    emitter.instruction("cmp BYTE PTR [rdi + r8], 58");                         // exact colon
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // a must be followed by a colon
    emitter.instruction("lea rsi, [r8 + 1]");                                   // first count digit
    emitter.instruction("mov ecx, 58");                                         // count delimiter
    emitter.instruction("call __rt_unser_validate_uint");                       // parse the declared entry count
    emitter.instruction("test rax, rax");                                       // did the count validate?
    emitter.instruction("jz __rt_unser_validate_at_fail_x");                    // malformed count field
    emitter.instruction("mov QWORD PTR [rbp - 40], rsi");                       // entry count
    emitter.instruction("lea r8, [rdx + 1]");                                   // opening brace position
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // end after helper call
    emitter.instruction("cmp r8, r9");                                          // is the opening brace in bounds?
    emitter.instruction("jae __rt_unser_validate_at_fail_x");                   // truncated array envelope
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base after helper call
    emitter.instruction("cmp BYTE PTR [rdi + r8], 123");                        // exact opening brace
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // entries must open with a brace
    emitter.instruction("add r8, 1");                                           // step past the opening brace
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // body position
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // entry index
    emitter.label("__rt_unser_validate_array_loop_x");
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // entries validated so far
    emitter.instruction("cmp r8, QWORD PTR [rbp - 40]");                        // all declared entries seen?
    emitter.instruction("jae __rt_unser_validate_container_close_x");           // done: only the closing brace remains
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload base for the key call
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // key starts at the body cursor
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // key must stay inside the source
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // current recursion depth
    emitter.instruction("add rcx, 1");                                          // nested key depth
    emitter.instruction("call __rt_unser_validate_key");                        // bound one entry key
    emitter.instruction("test rax, rax");                                       // did the key validate?
    emitter.instruction("jz __rt_unser_validate_at_fail_x");                    // malformed entry key
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // position after key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload base for the value call
    emitter.instruction("mov rsi, rdx");                                        // value starts right after the key
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // value must stay inside the source
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // current recursion depth
    emitter.instruction("add rcx, 1");                                          // nested value depth
    emitter.instruction("call __rt_unser_validate_at");                         // bound one entry value
    emitter.instruction("test rax, rax");                                       // did the value validate?
    emitter.instruction("jz __rt_unser_validate_at_fail_x");                    // malformed entry value
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // position after value
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // count one validated entry
    emitter.instruction("jmp __rt_unser_validate_array_loop_x");                // validate the next entry

    emitter.label("__rt_unser_validate_object_x");
    emitter.instruction("lea r8, [rsi + 1]");                                   // colon after O
    emitter.instruction("cmp r8, rdx");                                         // is the colon in bounds?
    emitter.instruction("jae __rt_unser_validate_at_fail_x");                   // truncated object envelope
    emitter.instruction("cmp BYTE PTR [rdi + r8], 58");                         // exact colon
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // O must be followed by a colon
    emitter.instruction("lea rsi, [r8 + 1]");                                   // first class-name length digit
    emitter.instruction("mov ecx, 58");                                         // class-name length delimiter
    emitter.instruction("call __rt_unser_validate_uint");                       // parse the class-name length
    emitter.instruction("test rax, rax");                                       // did the length validate?
    emitter.instruction("jz __rt_unser_validate_at_fail_x");                    // malformed length field
    emitter.instruction("mov r11, rsi");                                        // class-name byte length
    emitter.instruction("lea r8, [rdx + 1]");                                   // opening quote position
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // end after helper call
    emitter.instruction("cmp r8, r9");                                          // is the opening quote in bounds?
    emitter.instruction("jae __rt_unser_validate_at_fail_x");                   // truncated object envelope
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base after helper call
    emitter.instruction("cmp BYTE PTR [rdi + r8], 34");                         // exact opening quote
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // the class name must be quoted
    emitter.instruction("add r8, 1");                                           // class-name bytes
    emitter.instruction("mov r10, r9");                                         // start the remaining count from end
    emitter.instruction("sub r10, r8");                                         // remaining bytes
    emitter.instruction("cmp r10, 2");                                          // closing quote and colon
    emitter.instruction("jb __rt_unser_validate_at_fail_x");                    // no room for quote and colon
    emitter.instruction("sub r10, 2");                                          // room left for name bytes alone
    emitter.instruction("cmp r11, r10");                                        // declared class name fits?
    emitter.instruction("ja __rt_unser_validate_at_fail_x");                    // declared length overruns the input
    emitter.instruction("xor r13d, r13d");                                     // class-name control-byte scan cursor
    emitter.label("__rt_unser_validate_object_name_scan_x");
    emitter.instruction("cmp r13, r11");                                       // scanned the full declared class name?
    emitter.instruction("jae __rt_unser_validate_object_name_scan_done_x");    // continue with the closing quote
    emitter.instruction("lea r14, [r8 + r13]");                                // source offset of this class-name byte
    emitter.instruction("movzx r12d, BYTE PTR [rdi + r14]");                   // load one already bounded class-name byte
    emitter.instruction("cmp r12d, 32");                                       // serialized class names cannot contain ASCII controls
    emitter.instruction("jb __rt_unser_validate_object_name_invalid_x");       // fail at the object's marker like php-src
    emitter.instruction("add r13, 1");                                         // scan the next byte
    emitter.instruction("jmp __rt_unser_validate_object_name_scan_x");         // continue the bounded scan
    emitter.label("__rt_unser_validate_object_name_invalid_x");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                      // report the malformed object's starting offset
    emitter.instruction("jmp __rt_unser_validate_at_fail_x");                  // reject the invalid class name
    emitter.label("__rt_unser_validate_object_name_scan_done_x");
    emitter.instruction("add r8, r11");                                         // closing quote
    emitter.instruction("cmp BYTE PTR [rdi + r8], 34");                         // exact closing quote
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // name must end at the declared length
    emitter.instruction("add r8, 1");                                           // colon before count
    emitter.instruction("cmp BYTE PTR [rdi + r8], 58");                         // exact colon before the count
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // count must follow the class name
    emitter.instruction("lea rsi, [r8 + 1]");                                   // first property-count digit
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // restore end for the count parse
    emitter.instruction("mov ecx, 58");                                         // property-count delimiter
    emitter.instruction("call __rt_unser_validate_uint");                       // parse the property count
    emitter.instruction("test rax, rax");                                       // did the count validate?
    emitter.instruction("jz __rt_unser_validate_at_fail_x");                    // malformed count field
    emitter.instruction("mov QWORD PTR [rbp - 40], rsi");                       // property count
    emitter.instruction("lea r8, [rdx + 1]");                                   // opening brace
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // end after helper call
    emitter.instruction("cmp r8, r9");                                          // is the opening brace in bounds?
    emitter.instruction("jae __rt_unser_validate_at_fail_x");                   // truncated object envelope
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base after helper call
    emitter.instruction("cmp BYTE PTR [rdi + r8], 123");                        // exact opening brace
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // properties must open with a brace
    emitter.instruction("add r8, 1");                                           // step past the opening brace
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // first property key
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // property index
    emitter.label("__rt_unser_validate_object_loop_x");
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // properties validated so far
    emitter.instruction("cmp r8, QWORD PTR [rbp - 40]");                        // all declared properties seen?
    emitter.instruction("jae __rt_unser_validate_container_close_x");           // done: only the closing brace remains
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload base for the key call
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // key starts at the body cursor
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // key must stay inside the source
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // current recursion depth
    emitter.instruction("add rcx, 1");                                          // nested key depth
    emitter.instruction("call __rt_unser_validate_key");                        // bound one property name
    emitter.instruction("test rax, rax");                                       // did the name validate?
    emitter.instruction("jz __rt_unser_validate_at_fail_x");                    // malformed property name
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // position after key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload base for the value call
    emitter.instruction("mov rsi, rdx");                                        // value starts right after the name
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // value must stay inside the source
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // current recursion depth
    emitter.instruction("add rcx, 1");                                          // nested value depth
    emitter.instruction("call __rt_unser_validate_at");                         // bound one property value
    emitter.instruction("test rax, rax");                                       // did the value validate?
    emitter.instruction("jz __rt_unser_validate_at_fail_x");                    // malformed property value
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // position after value
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // count one validated property
    emitter.instruction("jmp __rt_unser_validate_object_loop_x");               // validate the next property

    emitter.label("__rt_unser_validate_container_close_x");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // closing-brace position
    emitter.instruction("cmp rdx, QWORD PTR [rbp - 24]");                       // require the closing brace byte
    emitter.instruction("je __rt_unser_validate_at_ok_x");                     // a missing final brace is allocation-safe; let hydration run PHP hooks before failing
    emitter.instruction("ja __rt_unser_validate_at_fail_x");                   // reject a cursor that escaped the bounded source
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload base for the brace check
    emitter.instruction("cmp BYTE PTR [rdi + rdx], 125");                       // exact closing brace
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // body must end exactly at the brace
    emitter.instruction("add rdx, 1");                                          // position after complete container
    emitter.instruction("jmp __rt_unser_validate_at_ok_x");                     // container fully bounded

    emitter.label("__rt_unser_validate_ref_x");
    emitter.instruction("lea r8, [rsi + 1]");                                   // colon after r/R
    emitter.instruction("cmp r8, rdx");                                         // is the colon in bounds?
    emitter.instruction("jae __rt_unser_validate_at_fail_x");                   // truncated reference envelope
    emitter.instruction("cmp BYTE PTR [rdi + r8], 58");                         // exact colon
    emitter.instruction("jne __rt_unser_validate_at_fail_x");                   // r/R must be followed by a colon
    emitter.instruction("lea rsi, [r8 + 1]");                                   // first reference-index digit
    emitter.instruction("mov ecx, 59");                                         // reference terminator
    emitter.instruction("call __rt_unser_validate_uint");                       // parse the back-reference index
    emitter.instruction("test rax, rax");                                       // did the index validate?
    emitter.instruction("jz __rt_unser_validate_at_fail_x");                    // malformed index field
    emitter.instruction("test rsi, rsi");                                       // reference indices are one-based
    emitter.instruction("jz __rt_unser_validate_at_fail_x");                    // index zero targets no value
    emitter.instruction("add rdx, 1");                                          // skip semicolon

    emitter.label("__rt_unser_validate_at_ok_x");
    emitter.instruction("mov eax, 1");                                          // report a fully bounded value
    emitter.instruction("leave");                                               // restore recursive validator frame
    emitter.instruction("ret");                                                 // return success and the new cursor to the caller
    emitter.label("__rt_unser_validate_at_fail_x");
    emitter.instruction("xor eax, eax");                                        // report malformed/truncated wire data
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_unser_failure_offset");
    emitter.instruction("mov r10, QWORD PTR [r11]");                           // first failure cursor recorded by the innermost validator frame
    emitter.instruction("cmp r10, -1");                                        // is the no-failure sentinel still present?
    emitter.instruction("jne __rt_unser_validate_at_fail_offset_ready_x");     // recursive parents must preserve the original failure point
    emitter.instruction("mov r10, rsi");                                       // capture the bounded cursor at the first grammar failure
    emitter.instruction("mov QWORD PTR [r11], r10");                           // publish it for recursive propagation
    emitter.label("__rt_unser_validate_at_fail_offset_ready_x");
    emitter.instruction("mov rdx, r10");                                       // return the first failure offset to the caller
    emitter.instruction("leave");                                               // restore the frame on the failure path
    emitter.instruction("ret");                                                 // return the failure flag to the caller
}
