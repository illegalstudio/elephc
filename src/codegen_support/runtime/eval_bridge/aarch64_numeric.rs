//! Purpose:
//! Emits AArch64 numeric, unary, and bitwise eval wrappers.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - PHP coercions and boxed result shapes remain unchanged.

use super::*;

/// Emits AArch64 numeric, unary, and bitwise eval wrappers.
pub(super) fn emit_aarch64_numeric(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_abs");
    emitter.instruction("b __rt_abs_mixed");                                    // compute PHP abs() for one boxed eval value

    label_c_global(emitter, "__elephc_eval_value_ceil");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while casting and boxing ceil
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across helper calls
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the boxed eval argument to a PHP double for ceil
    emitter.bl_c("ceil");
    emitter.instruction("fmov x1, d0");                                         // move the ceil result bits into mixed value_lo
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the ceil result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the ceil wrapper frame
    emitter.instruction("ret");                                                 // return the boxed ceil result to Rust

    label_c_global(emitter, "__elephc_eval_value_floor");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while casting and boxing floor
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across helper calls
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the boxed eval argument to a PHP double for floor
    emitter.bl_c("floor");
    emitter.instruction("fmov x1, d0");                                         // move the floor result bits into mixed value_lo
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the floor result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the floor wrapper frame
    emitter.instruction("ret");                                                 // return the boxed floor result to Rust

    label_c_global(emitter, "__elephc_eval_value_sqrt");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while casting and boxing sqrt
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across helper calls
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the boxed eval argument to a PHP double for sqrt
    emitter.bl_c("sqrt");
    emitter.instruction("fmov x1, d0");                                         // move the sqrt result bits into mixed value_lo
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the sqrt result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the sqrt wrapper frame
    emitter.instruction("ret");                                                 // return the boxed sqrt result to Rust

    label_c_global(emitter, "__elephc_eval_value_strrev");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the borrowed input across tag inspection
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x0, [sp]");                                        // preserve the source cell for non-string coercion
    emitter.instruction("bl __rt_mixed_unbox");                                 // borrow the canonical tag and payload without copying strings
    emitter.instruction("cmp x0, #1");                                          // existing string payloads remain owned by the caller lease
    emitter.instruction("b.eq __elephc_eval_value_strrev_reverse");             // reverse borrowed bytes without an unowned persisted copy
    emitter.instruction("ldr x0, [sp]");                                        // reload non-string input for PHP scalar stringification
    emitter.instruction("bl __rt_mixed_cast_string");                           // non-string arms return borrowed formatting or fixed storage
    emitter.label("__elephc_eval_value_strrev_reverse");
    emitter.instruction("bl __rt_strrev");                                      // reverse the PHP byte string into concat storage
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the reversed string for Rust
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the strrev wrapper frame
    emitter.instruction("ret");                                                 // return the boxed reversed string to Rust

    label_c_global(emitter, "__elephc_eval_value_fdiv");
    emitter.instruction("sub sp, sp, #32");                                     // allocate wrapper slots for the right operand and left double
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the left boxed operand to a PHP numeric double
    emitter.instruction("str d0, [sp, #8]");                                    // save the left double across the right cast
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for numeric casting
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the right boxed operand to a PHP numeric double
    emitter.instruction("fmov d1, d0");                                         // keep the right divisor in d1
    emitter.instruction("ldr d0, [sp, #8]");                                    // reload the left dividend into d0
    emitter.instruction("fdiv d0, d0, d1");                                     // compute fdiv() with IEEE zero handling
    emitter.instruction("fcmp d0, d0");                                         // detect NaN so PHP echo prints NAN without a sign
    emitter.instruction("b.vs __elephc_eval_value_fdiv_nan");                   // normalize unordered fdiv results before boxing
    emitter.instruction("fmov x1, d0");                                         // move the fdiv result bits into mixed value_lo
    emitter.instruction("b __elephc_eval_value_fdiv_box");                      // skip the canonical NaN payload path
    emitter.label("__elephc_eval_value_fdiv_nan");
    emitter.instruction("mov x1, xzr");                                         // start the canonical quiet NaN payload from zero bits
    emitter.instruction("movk x1, #0x7ff8, lsl #48");                           // install the positive quiet NaN exponent/significand
    emitter.label("__elephc_eval_value_fdiv_box");
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the fdiv result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the fdiv wrapper frame
    emitter.instruction("ret");                                                 // return the boxed fdiv result to Rust

    label_c_global(emitter, "__elephc_eval_value_fmod");
    emitter.instruction("sub sp, sp, #32");                                     // allocate wrapper slots for the right operand and left double
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the left boxed operand to a PHP numeric double
    emitter.instruction("str d0, [sp, #8]");                                    // save the left double across the right cast
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for numeric casting
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the right boxed operand to a PHP numeric double
    emitter.instruction("fmov d1, d0");                                         // keep the right divisor in d1
    emitter.instruction("ldr d0, [sp, #8]");                                    // reload the left dividend into d0
    emitter.bl_c("fmod");                                                       // libc fmod keeps the dividend's sign, so -0.0 survives like PHP
    emitter.instruction("fcmp d0, d0");                                         // detect NaN so PHP echo prints NAN without a sign
    emitter.instruction("b.vs __elephc_eval_value_fmod_nan");                   // normalize unordered fmod results before boxing
    emitter.instruction("fmov x1, d0");                                         // move the fmod result bits into mixed value_lo
    emitter.instruction("b __elephc_eval_value_fmod_box");                      // skip the canonical NaN payload path
    emitter.label("__elephc_eval_value_fmod_nan");
    emitter.instruction("mov x1, xzr");                                         // start the canonical quiet NaN payload from zero bits
    emitter.instruction("movk x1, #0x7ff8, lsl #48");                           // install the positive quiet NaN exponent/significand
    emitter.label("__elephc_eval_value_fmod_box");
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the fmod result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the fmod wrapper frame
    emitter.instruction("ret");                                                 // return the boxed fmod result to Rust

    label_c_global(emitter, "__elephc_eval_value_add");
    emitter.instruction("b __rt_mixed_numeric_add");                            // add two boxed mixed values and return the boxed result

    label_c_global(emitter, "__elephc_eval_value_sub");
    emitter.instruction("b __rt_mixed_numeric_sub");                            // subtract two boxed mixed values and return the boxed result

    label_c_global(emitter, "__elephc_eval_value_mul");
    emitter.instruction("b __rt_mixed_numeric_mul");                            // multiply two boxed mixed values and return the boxed result

    label_c_global(emitter, "__elephc_eval_value_div");
    emitter.instruction("sub sp, sp, #32");                                     // allocate wrapper slots for the right operand and left double
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the left boxed operand to a PHP numeric double
    emitter.instruction("str d0, [sp, #8]");                                    // save the left double across the right cast
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for numeric casting
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the right boxed operand to a PHP numeric double
    emitter.instruction("fcmp d0, #0.0");                                       // detect division by zero before the hardware divide
    emitter.instruction("b.eq __elephc_eval_value_div_null");                   // return null until eval has throwable propagation
    emitter.instruction("fmov d1, d0");                                         // keep the right divisor in d1
    emitter.instruction("ldr d0, [sp, #8]");                                    // reload the left dividend into d0
    emitter.instruction("fdiv d0, d0, d1");                                     // compute PHP division as a double result
    emitter.instruction("fmov x1, d0");                                         // move the double bits into mixed value_lo
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the division result into a Mixed cell
    emitter.instruction("b __elephc_eval_value_div_done");                      // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_div_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null fallback for division by zero
    emitter.instruction("mov x1, xzr");                                         // null has no low payload word
    emitter.instruction("mov x2, xzr");                                         // null has no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box null for unsupported division-by-zero propagation
    emitter.label("__elephc_eval_value_div_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the division wrapper frame
    emitter.instruction("ret");                                                 // return the boxed division result to Rust

    label_c_global(emitter, "__elephc_eval_value_mod");
    emitter.instruction("sub sp, sp, #32");                                     // allocate wrapper slots for the right operand and left integer
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("bl __rt_mixed_cast_int");                              // cast the left boxed operand to a PHP integer
    emitter.instruction("str x0, [sp, #8]");                                    // save the left integer across the right cast
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for integer casting
    emitter.instruction("bl __rt_mixed_cast_int");                              // cast the right boxed operand to a PHP integer
    emitter.instruction("cbz x0, __elephc_eval_value_mod_null");                // return null until eval has throwable propagation
    emitter.instruction("mov x2, x0");                                          // keep the integer divisor in x2
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the integer dividend into x1
    emitter.instruction("sdiv x3, x1, x2");                                     // compute the signed integer quotient
    emitter.instruction("msub x1, x3, x2, x1");                                 // compute dividend - quotient * divisor
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("bl __rt_mixed_from_value");                            // box the modulo result into a Mixed cell
    emitter.instruction("b __elephc_eval_value_mod_done");                      // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_mod_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null fallback for modulo by zero
    emitter.instruction("mov x1, xzr");                                         // null has no low payload word
    emitter.instruction("mov x2, xzr");                                         // null has no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box null for unsupported modulo-by-zero propagation
    emitter.label("__elephc_eval_value_mod_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the modulo wrapper frame
    emitter.instruction("ret");                                                 // return the boxed modulo result to Rust

    label_c_global(emitter, "__elephc_eval_value_pow");
    emitter.instruction("sub sp, sp, #32");                                     // allocate wrapper slots for the right operand and left double
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the left boxed operand to a PHP numeric double
    emitter.instruction("str d0, [sp, #8]");                                    // save the exponentiation base across the right cast
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for numeric casting
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the right boxed operand to a PHP numeric double
    emitter.instruction("fmov d1, d0");                                         // move the exponent into libc pow's second argument
    emitter.instruction("ldr d0, [sp, #8]");                                    // reload the base into libc pow's first argument
    emitter.bl_c("pow");
    emitter.instruction("fmov x1, d0");                                         // move the pow result bits into mixed value_lo
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the exponentiation result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the exponentiation wrapper frame
    emitter.instruction("ret");                                                 // return the boxed exponentiation result to Rust

    label_c_global(emitter, "__elephc_eval_value_round");
    emitter.instruction("sub sp, sp, #48");                                     // allocate wrapper slots for precision state and saved doubles
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the optional precision cell while casting the value
    emitter.instruction("str x2, [sp, #8]");                                    // save whether the caller supplied a precision argument
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the boxed eval value to a PHP numeric double
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the precision-presence flag after the value cast
    emitter.instruction("cbnz x2, __elephc_eval_value_round_precision");        // use the precision path when a second argument is present
    emitter.bl_c("round");
    emitter.instruction("b __elephc_eval_value_round_box");                     // box the default-precision round result
    emitter.label("__elephc_eval_value_round_precision");
    emitter.instruction("str d0, [sp, #16]");                                   // save the original value while casting the precision
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the precision cell for integer casting
    emitter.instruction("bl __rt_mixed_cast_int");                              // cast the optional precision to a PHP integer
    emitter.instruction("scvtf d1, x0");                                        // convert the precision to a floating exponent for pow
    emitter.instruction("fmov d0, #10.0");                                      // materialize 10.0 as the precision multiplier base
    emitter.bl_c("pow");
    emitter.instruction("ldr d1, [sp, #16]");                                   // reload the original value after pow returns the multiplier
    emitter.instruction("fmul d1, d1, d0");                                     // scale the value by the precision multiplier
    emitter.instruction("str d0, [sp, #24]");                                   // save the multiplier for rescaling after round
    emitter.instruction("fmov d0, d1");                                         // move the scaled value into the round argument
    emitter.bl_c("round");
    emitter.instruction("ldr d1, [sp, #24]");                                   // reload the precision multiplier for rescaling
    emitter.instruction("fdiv d0, d0, d1");                                     // scale the rounded value back to requested precision
    emitter.label("__elephc_eval_value_round_box");
    emitter.instruction("fmov x1, d0");                                         // move the round result bits into mixed value_lo
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the round result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the round wrapper frame
    emitter.instruction("ret");                                                 // return the boxed round result to Rust

    label_c_global(emitter, "__elephc_eval_value_bit_not");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame for the cast helper call
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across the cast
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_cast_int");                              // cast the boxed operand to a PHP integer
    emitter.instruction("mvn x1, x0");                                          // compute bitwise complement of the integer payload
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("bl __rt_mixed_from_value");                            // box the bitwise NOT result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the bitwise NOT wrapper frame
    emitter.instruction("ret");                                                 // return the boxed bitwise NOT result to Rust

    label_c_global(emitter, "__elephc_eval_value_bitwise");
    emitter.instruction("sub sp, sp, #48");                                     // allocate wrapper slots for right operand, opcode, and left integer
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("str x2, [sp, #8]");                                    // save the eval bitwise opcode across helper calls
    emitter.instruction("bl __rt_mixed_cast_int");                              // cast the left boxed operand to a PHP integer
    emitter.instruction("str x0, [sp, #16]");                                   // save the left integer across the right cast
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for integer casting
    emitter.instruction("bl __rt_mixed_cast_int");                              // cast the right boxed operand to a PHP integer
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the left integer into the payload register
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the eval bitwise opcode for dispatch
    emitter.instruction("cmp x2, #0");                                          // is this integer bitwise AND?
    emitter.instruction("b.eq __elephc_eval_value_bitwise_and");                // route opcode 0 to integer AND
    emitter.instruction("cmp x2, #1");                                          // is this integer bitwise OR?
    emitter.instruction("b.eq __elephc_eval_value_bitwise_or");                 // route opcode 1 to integer OR
    emitter.instruction("cmp x2, #2");                                          // is this integer bitwise XOR?
    emitter.instruction("b.eq __elephc_eval_value_bitwise_xor");                // route opcode 2 to integer XOR
    emitter.instruction("cmp x2, #3");                                          // is this integer left shift?
    emitter.instruction("b.eq __elephc_eval_value_bitwise_shl");                // route opcode 3 to integer left shift
    emitter.instruction("cmp x2, #4");                                          // is this integer right shift?
    emitter.instruction("b.eq __elephc_eval_value_bitwise_shr");                // route opcode 4 to integer right shift
    emitter.instruction("b __elephc_eval_value_bitwise_null");                  // fail closed for unknown bitwise opcodes
    emitter.label("__elephc_eval_value_bitwise_and");
    emitter.instruction("and x1, x1, x0");                                      // compute integer bitwise AND
    emitter.instruction("b __elephc_eval_value_bitwise_box");                   // box the integer bitwise result
    emitter.label("__elephc_eval_value_bitwise_or");
    emitter.instruction("orr x1, x1, x0");                                      // compute integer bitwise OR
    emitter.instruction("b __elephc_eval_value_bitwise_box");                   // box the integer bitwise result
    emitter.label("__elephc_eval_value_bitwise_xor");
    emitter.instruction("eor x1, x1, x0");                                      // compute integer bitwise XOR
    emitter.instruction("b __elephc_eval_value_bitwise_box");                   // box the integer bitwise result
    emitter.label("__elephc_eval_value_bitwise_shl");
    emitter.instruction("cmp x0, #0");                                          // negative shift counts are runtime errors in PHP
    emitter.instruction("b.lt __elephc_eval_value_bitwise_null");               // return null until eval has throwable propagation
    emitter.instruction("lsl x1, x1, x0");                                      // shift the integer payload left
    emitter.instruction("b __elephc_eval_value_bitwise_box");                   // box the integer shift result
    emitter.label("__elephc_eval_value_bitwise_shr");
    emitter.instruction("cmp x0, #0");                                          // negative shift counts are runtime errors in PHP
    emitter.instruction("b.lt __elephc_eval_value_bitwise_null");               // return null until eval has throwable propagation
    emitter.instruction("asr x1, x1, x0");                                      // shift the integer payload right arithmetically
    emitter.instruction("b __elephc_eval_value_bitwise_box");                   // box the integer shift result
    emitter.label("__elephc_eval_value_bitwise_box");
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("bl __rt_mixed_from_value");                            // box the bitwise result into a Mixed cell
    emitter.instruction("b __elephc_eval_value_bitwise_done");                  // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_bitwise_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null fallback for unsupported bitwise errors
    emitter.instruction("mov x1, xzr");                                         // null has no low payload word
    emitter.instruction("mov x2, xzr");                                         // null has no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box null for unsupported bitwise error propagation
    emitter.label("__elephc_eval_value_bitwise_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the bitwise wrapper frame
    emitter.instruction("ret");                                                 // return the boxed bitwise result to Rust

}
