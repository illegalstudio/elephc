//! Purpose:
//! Emits x86_64 numeric, unary, and bitwise eval wrappers.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - PHP coercions and boxed result shapes remain unchanged.

use super::*;

/// Emits x86_64 numeric, unary, and bitwise eval wrappers.
pub(super) fn emit_x86_64_numeric(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_abs");
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into abs_mixed input
    emitter.instruction("jmp __rt_abs_mixed");                                  // compute PHP abs() for one boxed eval value

    label_c_global(emitter, "__elephc_eval_value_ceil");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the boxed eval argument to a PHP double for ceil
    emitter.bl_c("ceil");
    emitter.instruction("movq rdi, xmm0");                                      // move the ceil result bits into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the ceil result into a Mixed cell
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed ceil result to Rust

    label_c_global(emitter, "__elephc_eval_value_floor");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the boxed eval argument to a PHP double for floor
    emitter.bl_c("floor");
    emitter.instruction("movq rdi, xmm0");                                      // move the floor result bits into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the floor result into a Mixed cell
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed floor result to Rust

    label_c_global(emitter, "__elephc_eval_value_sqrt");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the boxed eval argument to a PHP double for sqrt
    emitter.bl_c("sqrt");
    emitter.instruction("movq rdi, xmm0");                                      // move the sqrt result bits into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the sqrt result into a Mixed cell
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed sqrt result to Rust

    label_c_global(emitter, "__elephc_eval_value_strrev");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve the borrowed input across tag inspection
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source cell for non-string coercion
    emitter.instruction("mov rax, rdi");                                        // pass the borrowed cell to canonical unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // inspect the tag and borrow both payload words
    emitter.instruction("cmp rax, 1");                                          // existing string payloads remain owned by the caller lease
    emitter.instruction("jne __elephc_eval_value_strrev_convert");              // coerce only non-string inputs through formatting storage
    emitter.instruction("mov rax, rdi");                                        // borrow the string pointer while preserving its length in rdx
    emitter.instruction("jmp __elephc_eval_value_strrev_reverse");              // skip the allocating string arm of mixed_cast_string
    emitter.label("__elephc_eval_value_strrev_convert");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload non-string input for PHP scalar stringification
    emitter.instruction("call __rt_mixed_cast_string");                         // non-string arms return borrowed formatting or fixed storage
    emitter.label("__elephc_eval_value_strrev_reverse");
    emitter.instruction("call __rt_strrev");                                    // reverse the PHP byte string into concat storage
    emitter.instruction("mov rdi, rax");                                        // move the reversed string pointer into mixed value_lo
    emitter.instruction("mov rsi, rdx");                                        // move the reversed string length into mixed value_hi
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the reversed string for Rust
    emitter.instruction("add rsp, 16");                                         // retire the borrowed input spill without releasing its owner
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed reversed string to Rust

    label_c_global(emitter, "__elephc_eval_value_fdiv");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the right operand and left double
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the left boxed operand to a PHP numeric double
    emitter.instruction("movsd QWORD PTR [rbp - 16], xmm0");                    // save the left double across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for numeric casting
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the right boxed operand to a PHP numeric double
    emitter.instruction("movapd xmm1, xmm0");                                   // keep the right divisor in xmm1
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 16]");                    // reload the left dividend into xmm0
    emitter.instruction("divsd xmm0, xmm1");                                    // compute fdiv() with IEEE zero handling
    emitter.instruction("ucomisd xmm0, xmm0");                                  // detect NaN so PHP echo prints NAN without a sign
    emitter.instruction("jp __elephc_eval_value_fdiv_nan_x86");                 // normalize unordered fdiv results before boxing
    emitter.instruction("movq rdi, xmm0");                                      // move the fdiv result bits into mixed value_lo
    emitter.instruction("jmp __elephc_eval_value_fdiv_box_x86");                // skip the canonical NaN payload path
    emitter.label("__elephc_eval_value_fdiv_nan_x86");
    emitter.instruction("movabs rdi, 0x7ff8000000000000");                      // use a positive quiet NaN payload for PHP output
    emitter.label("__elephc_eval_value_fdiv_box_x86");
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the fdiv result into a Mixed cell
    emitter.instruction("add rsp, 32");                                         // release the fdiv wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed fdiv result to Rust

    label_c_global(emitter, "__elephc_eval_value_fmod");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the right operand and left double
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the left boxed operand to a PHP numeric double
    emitter.instruction("movsd QWORD PTR [rbp - 16], xmm0");                    // save the left double across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for numeric casting
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the right boxed operand to a PHP numeric double
    emitter.instruction("movapd xmm1, xmm0");                                   // move the right divisor into the second fmod argument
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 16]");                    // move the left dividend into the first fmod argument
    emitter.bl_c("fmod");
    emitter.instruction("ucomisd xmm0, xmm0");                                  // detect NaN so PHP echo prints NAN without a sign
    emitter.instruction("jp __elephc_eval_value_fmod_nan_x86");                 // normalize unordered fmod results before boxing
    emitter.instruction("movq rdi, xmm0");                                      // move the fmod result bits into mixed value_lo
    emitter.instruction("jmp __elephc_eval_value_fmod_box_x86");                // skip the canonical NaN payload path
    emitter.label("__elephc_eval_value_fmod_nan_x86");
    emitter.instruction("movabs rdi, 0x7ff8000000000000");                      // use a positive quiet NaN payload for PHP output
    emitter.label("__elephc_eval_value_fmod_box_x86");
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the fmod result into a Mixed cell
    emitter.instruction("add rsp, 32");                                         // release the fmod wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed fmod result to Rust

    label_c_global(emitter, "__elephc_eval_value_add");
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into the internal result register
    emitter.instruction("mov rdi, rsi");                                        // move the right boxed operand into the internal argument register
    emitter.instruction("jmp __rt_mixed_numeric_add");                          // add two boxed mixed values and return the boxed result

    label_c_global(emitter, "__elephc_eval_value_sub");
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into the internal result register
    emitter.instruction("mov rdi, rsi");                                        // move the right boxed operand into the internal argument register
    emitter.instruction("jmp __rt_mixed_numeric_sub");                          // subtract two boxed mixed values and return the boxed result

    label_c_global(emitter, "__elephc_eval_value_mul");
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into the internal result register
    emitter.instruction("mov rdi, rsi");                                        // move the right boxed operand into the internal argument register
    emitter.instruction("jmp __rt_mixed_numeric_mul");                          // multiply two boxed mixed values and return the boxed result

    label_c_global(emitter, "__elephc_eval_value_div");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the right operand and left double
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the left boxed operand to a PHP numeric double
    emitter.instruction("movsd QWORD PTR [rbp - 16], xmm0");                    // save the left double across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for numeric casting
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the right boxed operand to a PHP numeric double
    emitter.instruction("pxor xmm1, xmm1");                                     // materialize a zero double for divisor checking
    emitter.instruction("ucomisd xmm0, xmm1");                                  // detect division by zero before the hardware divide
    emitter.instruction("je __elephc_eval_value_div_null_x86");                 // return null until eval has throwable propagation
    emitter.instruction("movapd xmm1, xmm0");                                   // keep the right divisor in xmm1
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 16]");                    // reload the left dividend into xmm0
    emitter.instruction("divsd xmm0, xmm1");                                    // compute PHP division as a double result
    emitter.instruction("movq rdi, xmm0");                                      // move the double bits into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the division result into a Mixed cell
    emitter.instruction("jmp __elephc_eval_value_div_done_x86");                // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_div_null_x86");
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = null fallback for division by zero
    emitter.instruction("xor edi, edi");                                        // null has no low payload word
    emitter.instruction("xor esi, esi");                                        // null has no high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box null for unsupported division-by-zero propagation
    emitter.label("__elephc_eval_value_div_done_x86");
    emitter.instruction("add rsp, 32");                                         // release the division wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed division result to Rust

    label_c_global(emitter, "__elephc_eval_value_mod");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the right operand and left integer
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_int input
    emitter.instruction("call __rt_mixed_cast_int");                            // cast the left boxed operand to a PHP integer
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the left integer across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for integer casting
    emitter.instruction("call __rt_mixed_cast_int");                            // cast the right boxed operand to a PHP integer
    emitter.instruction("test rax, rax");                                       // detect modulo by zero before the hardware divide
    emitter.instruction("jz __elephc_eval_value_mod_null_x86");                 // return null until eval has throwable propagation
    emitter.instruction("mov rdi, rax");                                        // keep the integer divisor in rdi
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the integer dividend into rax
    emitter.instruction("cqo");                                                 // sign-extend the dividend for signed division
    emitter.instruction("idiv rdi");                                            // compute quotient in rax and remainder in rdx
    emitter.instruction("mov rdi, rdx");                                        // move the integer remainder into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a high word
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the modulo result into a Mixed cell
    emitter.instruction("jmp __elephc_eval_value_mod_done_x86");                // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_mod_null_x86");
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = null fallback for modulo by zero
    emitter.instruction("xor edi, edi");                                        // null has no low payload word
    emitter.instruction("xor esi, esi");                                        // null has no high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box null for unsupported modulo-by-zero propagation
    emitter.label("__elephc_eval_value_mod_done_x86");
    emitter.instruction("add rsp, 32");                                         // release the modulo wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed modulo result to Rust

    label_c_global(emitter, "__elephc_eval_value_pow");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the right operand and left double
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the left boxed operand to a PHP numeric double
    emitter.instruction("movsd QWORD PTR [rbp - 16], xmm0");                    // save the exponentiation base across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for numeric casting
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the right boxed operand to a PHP numeric double
    emitter.instruction("movapd xmm1, xmm0");                                   // move the exponent into libc pow's second argument
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 16]");                    // reload the base into libc pow's first argument
    emitter.bl_c("pow");
    emitter.instruction("movq rdi, xmm0");                                      // move the pow result bits into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the exponentiation result into a Mixed cell
    emitter.instruction("add rsp, 32");                                         // release the exponentiation wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed exponentiation result to Rust

    label_c_global(emitter, "__elephc_eval_value_round");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for precision state and saved doubles
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the optional precision cell while casting the value
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save whether the caller supplied a precision argument
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the boxed eval value to a PHP numeric double
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // check whether a precision argument was supplied
    emitter.instruction("jne __elephc_eval_value_round_precision_x86");         // use the precision path when a second argument is present
    emitter.bl_c("round");
    emitter.instruction("jmp __elephc_eval_value_round_box_x86");               // box the default-precision round result
    emitter.label("__elephc_eval_value_round_precision_x86");
    emitter.instruction("movsd QWORD PTR [rbp - 24], xmm0");                    // save the original value while casting the precision
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the precision cell for integer casting
    emitter.instruction("call __rt_mixed_cast_int");                            // cast the optional precision to a PHP integer
    emitter.instruction("cvtsi2sd xmm1, rax");                                  // convert the precision to a floating exponent for pow
    emitter.instruction("mov rax, 0x4024000000000000");                         // materialize the IEEE-754 payload for 10.0
    emitter.instruction("movq xmm0, rax");                                      // move 10.0 into the pow base argument
    emitter.bl_c("pow");
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 24]");                    // reload the original value after pow returns the multiplier
    emitter.instruction("mulsd xmm1, xmm0");                                    // scale the value by the precision multiplier
    emitter.instruction("movsd QWORD PTR [rbp - 32], xmm0");                    // save the multiplier for rescaling after round
    emitter.instruction("movsd xmm0, xmm1");                                    // move the scaled value into the round argument
    emitter.bl_c("round");
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 32]");                    // reload the precision multiplier for rescaling
    emitter.instruction("divsd xmm0, xmm1");                                    // scale the rounded value back to requested precision
    emitter.label("__elephc_eval_value_round_box_x86");
    emitter.instruction("movq rdi, xmm0");                                      // move the round result bits into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the round result into a Mixed cell
    emitter.instruction("add rsp, 48");                                         // release the round wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed round result to Rust

    label_c_global(emitter, "__elephc_eval_value_bit_not");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // keep stack alignment for the cast and boxing calls
    emitter.instruction("mov rax, rdi");                                        // move the boxed operand into mixed_cast_int input
    emitter.instruction("call __rt_mixed_cast_int");                            // cast the boxed operand to a PHP integer
    emitter.instruction("not rax");                                             // compute bitwise complement of the integer payload
    emitter.instruction("mov rdi, rax");                                        // move the complement into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a high word
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the bitwise NOT result into a Mixed cell
    emitter.instruction("add rsp, 16");                                         // release the bitwise NOT wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed bitwise NOT result to Rust

    label_c_global(emitter, "__elephc_eval_value_bitwise");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve slots for right operand, opcode, and left integer
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the eval bitwise opcode across helper calls
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_int input
    emitter.instruction("call __rt_mixed_cast_int");                            // cast the left boxed operand to a PHP integer
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the left integer across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for integer casting
    emitter.instruction("call __rt_mixed_cast_int");                            // cast the right boxed operand to a PHP integer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the left integer into the payload register
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the eval bitwise opcode for dispatch
    emitter.instruction("cmp r10, 0");                                          // is this integer bitwise AND?
    emitter.instruction("je __elephc_eval_value_bitwise_and_x86");              // route opcode 0 to integer AND
    emitter.instruction("cmp r10, 1");                                          // is this integer bitwise OR?
    emitter.instruction("je __elephc_eval_value_bitwise_or_x86");               // route opcode 1 to integer OR
    emitter.instruction("cmp r10, 2");                                          // is this integer bitwise XOR?
    emitter.instruction("je __elephc_eval_value_bitwise_xor_x86");              // route opcode 2 to integer XOR
    emitter.instruction("cmp r10, 3");                                          // is this integer left shift?
    emitter.instruction("je __elephc_eval_value_bitwise_shl_x86");              // route opcode 3 to integer left shift
    emitter.instruction("cmp r10, 4");                                          // is this integer right shift?
    emitter.instruction("je __elephc_eval_value_bitwise_shr_x86");              // route opcode 4 to integer right shift
    emitter.instruction("jmp __elephc_eval_value_bitwise_null_x86");            // fail closed for unknown bitwise opcodes
    emitter.label("__elephc_eval_value_bitwise_and_x86");
    emitter.instruction("and rdi, rax");                                        // compute integer bitwise AND
    emitter.instruction("jmp __elephc_eval_value_bitwise_box_x86");             // box the integer bitwise result
    emitter.label("__elephc_eval_value_bitwise_or_x86");
    emitter.instruction("or rdi, rax");                                         // compute integer bitwise OR
    emitter.instruction("jmp __elephc_eval_value_bitwise_box_x86");             // box the integer bitwise result
    emitter.label("__elephc_eval_value_bitwise_xor_x86");
    emitter.instruction("xor rdi, rax");                                        // compute integer bitwise XOR
    emitter.instruction("jmp __elephc_eval_value_bitwise_box_x86");             // box the integer bitwise result
    emitter.label("__elephc_eval_value_bitwise_shl_x86");
    emitter.instruction("test rax, rax");                                       // negative shift counts are runtime errors in PHP
    emitter.instruction("js __elephc_eval_value_bitwise_null_x86");             // return null until eval has throwable propagation
    emitter.instruction("mov rcx, rax");                                        // move the shift count into the x86 shift-count register
    emitter.instruction("shl rdi, cl");                                         // shift the integer payload left
    emitter.instruction("jmp __elephc_eval_value_bitwise_box_x86");             // box the integer shift result
    emitter.label("__elephc_eval_value_bitwise_shr_x86");
    emitter.instruction("test rax, rax");                                       // negative shift counts are runtime errors in PHP
    emitter.instruction("js __elephc_eval_value_bitwise_null_x86");             // return null until eval has throwable propagation
    emitter.instruction("mov rcx, rax");                                        // move the shift count into the x86 shift-count register
    emitter.instruction("sar rdi, cl");                                         // shift the integer payload right arithmetically
    emitter.instruction("jmp __elephc_eval_value_bitwise_box_x86");             // box the integer shift result
    emitter.label("__elephc_eval_value_bitwise_box_x86");
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a high word
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the bitwise result into a Mixed cell
    emitter.instruction("jmp __elephc_eval_value_bitwise_done_x86");            // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_bitwise_null_x86");
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = null fallback for unsupported bitwise errors
    emitter.instruction("xor edi, edi");                                        // null has no low payload word
    emitter.instruction("xor esi, esi");                                        // null has no high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box null for unsupported bitwise error propagation
    emitter.label("__elephc_eval_value_bitwise_done_x86");
    emitter.instruction("add rsp, 32");                                         // release the bitwise wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed bitwise result to Rust

}
