//! Purpose:
//! Emits x86_64 concat, comparison, and spaceship wrappers.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Equality uses its dedicated helpers; relational and spaceship operations share
//!   the runtime PHP ordering table and preserve its unordered NaN flag.

use super::*;

/// Emits x86_64 concat, comparison, and spaceship wrappers.
pub(super) fn emit_x86_64_compare(emitter: &mut Emitter) {
    super::concat::emit(emitter);

    label_c_global(emitter, "__elephc_eval_value_compare");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across comparison helpers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable comparison wrapper frame
    emitter.instruction("sub rsp, 64");                                         // reserve slots for operands, opcode, and the left runtime triple
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left boxed operand
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the right boxed operand
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the eval comparison opcode
    emitter.instruction("cmp rdx, 0");                                          // is this loose equality?
    emitter.instruction("je __elephc_eval_value_compare_eq");                   // route == through the mixed loose-equality helper
    emitter.instruction("cmp rdx, 1");                                          // is this loose inequality?
    emitter.instruction("je __elephc_eval_value_compare_ne");                   // route != through the mixed loose-equality helper
    emitter.instruction("cmp rdx, 6");                                          // is this strict equality?
    emitter.instruction("je __elephc_eval_value_compare_strict_eq");            // route === through the mixed strict-equality helper
    emitter.instruction("cmp rdx, 7");                                          // is this strict inequality?
    emitter.instruction("je __elephc_eval_value_compare_strict_ne");            // route !== through the mixed strict-equality helper
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the left boxed operand for runtime-tag unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // unbox the left eval operand into tag and payload words
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the left runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save the left low payload word
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the left high payload word
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the right boxed operand for runtime-tag unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // unbox the right eval operand into tag and payload words
    emitter.instruction("mov rcx, rax");                                        // pass the right runtime tag to PHP ordering
    emitter.instruction("mov r8, rdi");                                         // pass the right low payload word to PHP ordering
    emitter.instruction("mov r9, rdx");                                         // pass the right high payload word to PHP ordering
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the left runtime tag
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload the left low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reload the left high payload word
    emitter.instruction("call __rt_php_compare");                               // apply PHP ordering and report unordered NaN separately
    emitter.instruction("mov r10, rax");                                        // preserve the normalized ordering result for opcode dispatch
    emitter.instruction("mov r11, rdx");                                        // preserve the unordered flag for relational predicates
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the eval comparison opcode without clobbering ordering
    emitter.instruction("cmp rcx, 2");                                          // is this a less-than comparison?
    emitter.instruction("je __elephc_eval_value_compare_lt");                   // materialize left < right from normalized PHP ordering
    emitter.instruction("cmp rcx, 3");                                          // is this a less-than-or-equal comparison?
    emitter.instruction("je __elephc_eval_value_compare_lte");                  // materialize left <= right from normalized PHP ordering
    emitter.instruction("cmp rcx, 4");                                          // is this a greater-than comparison?
    emitter.instruction("je __elephc_eval_value_compare_gt");                   // materialize left > right from normalized PHP ordering
    emitter.instruction("cmp rcx, 5");                                          // is this a greater-than-or-equal comparison?
    emitter.instruction("je __elephc_eval_value_compare_gte");                  // materialize left >= right from normalized PHP ordering
    emitter.instruction("xor eax, eax");                                        // unknown comparison opcodes fail closed as false
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the fallback false result
    emitter.label("__elephc_eval_value_compare_eq");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left operand for loose equality
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the right operand for loose equality
    emitter.instruction("call __elephc_eval_mixed_loose_eq");                   // compute scalar PHP loose equality
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the equality result
    emitter.label("__elephc_eval_value_compare_ne");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left operand for loose inequality
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the right operand for loose inequality
    emitter.instruction("call __elephc_eval_mixed_loose_eq");                   // compute scalar PHP loose equality before inversion
    emitter.instruction("xor rax, 1");                                          // invert equality for the != operator
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the inequality result
    emitter.label("__elephc_eval_value_compare_strict_eq");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left operand for strict equality
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the right operand for strict equality
    emitter.instruction("call __rt_mixed_strict_eq");                           // compute PHP strict equality
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the strict-equality result
    emitter.label("__elephc_eval_value_compare_strict_ne");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left operand for strict inequality
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the right operand for strict inequality
    emitter.instruction("call __rt_mixed_strict_eq");                           // compute PHP strict equality before inversion
    emitter.instruction("xor rax, 1");                                          // invert equality for the !== operator
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the strict-inequality result
    emitter.label("__elephc_eval_value_compare_lt");
    emitter.instruction("test r11, r11");                                       // did PHP ordering encounter unordered NaN?
    emitter.instruction("jnz __elephc_eval_value_compare_unordered");           // every relational predicate is false for unordered NaN
    emitter.instruction("cmp r10, 0");                                          // compare the PHP ordering result against zero for <
    emitter.instruction("setl al");                                             // materialize signed less-than as a PHP boolean
    emitter.instruction("movzx rax, al");                                       // widen the less-than boolean result
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the less-than result
    emitter.label("__elephc_eval_value_compare_lte");
    emitter.instruction("test r11, r11");                                       // did PHP ordering encounter unordered NaN?
    emitter.instruction("jnz __elephc_eval_value_compare_unordered");           // every relational predicate is false for unordered NaN
    emitter.instruction("cmp r10, 0");                                          // compare the PHP ordering result against zero for <=
    emitter.instruction("setle al");                                            // materialize signed less-than-or-equal as a PHP boolean
    emitter.instruction("movzx rax, al");                                       // widen the less-than-or-equal boolean result
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the less-than-or-equal result
    emitter.label("__elephc_eval_value_compare_gt");
    emitter.instruction("test r11, r11");                                       // did PHP ordering encounter unordered NaN?
    emitter.instruction("jnz __elephc_eval_value_compare_unordered");           // every relational predicate is false for unordered NaN
    emitter.instruction("cmp r10, 0");                                          // compare the PHP ordering result against zero for >
    emitter.instruction("setg al");                                             // materialize signed greater-than as a PHP boolean
    emitter.instruction("movzx rax, al");                                       // widen the greater-than boolean result
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the greater-than result
    emitter.label("__elephc_eval_value_compare_gte");
    emitter.instruction("test r11, r11");                                       // did PHP ordering encounter unordered NaN?
    emitter.instruction("jnz __elephc_eval_value_compare_unordered");           // every relational predicate is false for unordered NaN
    emitter.instruction("cmp r10, 0");                                          // compare the PHP ordering result against zero for >=
    emitter.instruction("setge al");                                            // materialize signed greater-than-or-equal as a PHP boolean
    emitter.instruction("movzx rax, al");                                       // widen the greater-than-or-equal boolean result
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the greater-than-or-equal result
    emitter.label("__elephc_eval_value_compare_unordered");
    emitter.instruction("xor eax, eax");                                        // unordered NaN makes every PHP relational predicate false
    emitter.label("__elephc_eval_value_compare_box");
    emitter.instruction("mov rdi, rax");                                        // move the comparison boolean into the Mixed payload register
    emitter.instruction("mov eax, 3");                                          // runtime tag 3 = bool
    emitter.instruction("xor esi, esi");                                        // bool payloads do not use a high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the comparison result as a Mixed bool
    emitter.instruction("add rsp, 64");                                         // release the comparison wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed comparison result to Rust

    emitter.label("__elephc_eval_mixed_loose_eq");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across mixed helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable loose-equality helper frame
    emitter.instruction("sub rsp, 96");                                         // allocate helper slots for unboxed tags, payloads, and casts
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left boxed operand for later casts
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the right boxed operand for later casts
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_unbox input
    emitter.instruction("call __rt_mixed_unbox");                               // unbox the left eval operand into tag and payload words
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the left runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the left low payload word
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the left high payload word
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the right boxed operand for unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // unbox the right eval operand into tag and payload words
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the right runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // save the right low payload word
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save the right high payload word
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the left runtime tag for equality dispatch
    emitter.instruction("cmp r10, 3");                                          // does the left operand have PHP bool semantics?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_bool");                // bool comparisons use truthiness on both operands
    emitter.instruction("cmp rax, 3");                                          // does the right operand have PHP bool semantics?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_bool");                // bool comparisons use truthiness on both operands
    emitter.instruction("cmp r10, rax");                                        // do the operands have the same runtime tag?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_same_tag");            // same-tag scalars use focused payload comparisons
    emitter.instruction("cmp r10, 8");                                          // is the left operand null?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_left_null");           // null compares equal only to empty strings before numeric fallback
    emitter.instruction("cmp rax, 8");                                          // is the right operand null?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_right_null");          // null compares equal only to empty strings before numeric fallback
    emitter.instruction("cmp r10, 1");                                          // is a non-matching left operand a string?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_left_string");         // compare numeric strings against numeric scalars
    emitter.instruction("cmp rax, 1");                                          // is a non-matching right operand a string?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_right_string");        // compare numeric strings against numeric scalars
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_numeric");            // remaining scalar mismatches compare numerically
    emitter.label("__elephc_eval_mixed_loose_eq_same_tag");
    emitter.instruction("cmp r10, 8");                                          // are both operands null?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_true");                // null loosely equals null
    emitter.instruction("cmp r10, 1");                                          // are both operands strings?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_strings");             // strings use PHP loose string equality
    emitter.instruction("cmp r10, 2");                                          // are both operands floats?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_floats");              // floats compare with native floating equality
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the left low payload word
    emitter.instruction("cmp r11, QWORD PTR [rbp - 56]");                       // compare low payload words for int and pointer-like scalars
    emitter.instruction("jne __elephc_eval_mixed_loose_eq_false");              // mismatched low payloads are not equal
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the left high payload word
    emitter.instruction("cmp r11, QWORD PTR [rbp - 64]");                       // compare high payload words for pointer-like scalars
    emitter.instruction("sete al");                                             // materialize same-tag payload equality
    emitter.instruction("movzx rax, al");                                       // widen the payload equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the payload equality result
    emitter.label("__elephc_eval_mixed_loose_eq_strings");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the left string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload the left string length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // reload the right string pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // reload the right string length
    emitter.instruction("call __rt_str_loose_eq");                              // compare strings with PHP loose numeric-string rules
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the string loose-equality result
    emitter.label("__elephc_eval_mixed_loose_eq_floats");
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 32]");                    // reload the left float payload
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 56]");                    // reload the right float payload
    emitter.instruction("ucomisd xmm1, xmm0");                                  // compare same-tag float payloads
    emitter.instruction("sete al");                                             // set true for ordered float equality
    emitter.instruction("setnp r10b");                                          // require an ordered comparison
    emitter.instruction("and al, r10b");                                        // clear unordered NaN equality
    emitter.instruction("movzx rax, al");                                       // widen the float equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the float equality result
    emitter.label("__elephc_eval_mixed_loose_eq_left_null");
    emitter.instruction("cmp rax, 1");                                          // is null being compared with a string?
    emitter.instruction("jne __elephc_eval_mixed_loose_eq_numeric");            // non-string null comparisons fall back to numeric zero
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // null loosely equals only the empty string
    emitter.instruction("sete al");                                             // materialize the null/string equality result
    emitter.instruction("movzx rax, al");                                       // widen the null/string equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the null/string equality result
    emitter.label("__elephc_eval_mixed_loose_eq_right_null");
    emitter.instruction("cmp r10, 1");                                          // is null being compared with a string?
    emitter.instruction("jne __elephc_eval_mixed_loose_eq_numeric");            // non-string null comparisons fall back to numeric zero
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // null loosely equals only the empty string
    emitter.instruction("sete al");                                             // materialize the string/null equality result
    emitter.instruction("movzx rax, al");                                       // widen the string/null equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the string/null equality result
    emitter.label("__elephc_eval_mixed_loose_eq_left_string");
    emitter.instruction("cmp rax, 0");                                          // can the right operand be compared numerically as an int?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_left_string_numeric"); // parse the left string for numeric equality
    emitter.instruction("cmp rax, 2");                                          // can the right operand be compared numerically as a float?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_left_string_numeric"); // parse the left string for numeric equality
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_false");              // non-numeric string mismatches are not loosely equal here
    emitter.label("__elephc_eval_mixed_loose_eq_left_string_numeric");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the left string pointer for numeric parsing
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the left string length for numeric parsing
    emitter.instruction("call __rt_str_to_number");                             // parse the left string under PHP numeric-string rules
    emitter.instruction("test rax, rax");                                       // did the left string parse as numeric?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_false");               // non-numeric strings do not equal numeric scalars
    emitter.instruction("movsd QWORD PTR [rbp - 72], xmm0");                    // save the parsed left numeric-string value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the right boxed operand for numeric casting
    emitter.instruction("mov rax, rdi");                                        // move the right boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the right operand to a comparison double
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 72]");                    // reload the parsed left numeric-string value
    emitter.instruction("ucomisd xmm1, xmm0");                                  // compare parsed string and numeric scalar values
    emitter.instruction("sete al");                                             // set true for ordered string/numeric equality
    emitter.instruction("setnp r10b");                                          // require an ordered comparison
    emitter.instruction("and al, r10b");                                        // clear unordered NaN equality
    emitter.instruction("movzx rax, al");                                       // widen the string/numeric equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the string/numeric equality result
    emitter.label("__elephc_eval_mixed_loose_eq_right_string");
    emitter.instruction("cmp r10, 0");                                          // can the left operand be compared numerically as an int?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_right_string_numeric"); // parse the right string for numeric equality
    emitter.instruction("cmp r10, 2");                                          // can the left operand be compared numerically as a float?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_right_string_numeric"); // parse the right string for numeric equality
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_false");              // non-numeric string mismatches are not loosely equal here
    emitter.label("__elephc_eval_mixed_loose_eq_right_string_numeric");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the right string pointer for numeric parsing
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // reload the right string length for numeric parsing
    emitter.instruction("call __rt_str_to_number");                             // parse the right string under PHP numeric-string rules
    emitter.instruction("test rax, rax");                                       // did the right string parse as numeric?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_false");               // non-numeric strings do not equal numeric scalars
    emitter.instruction("movsd QWORD PTR [rbp - 72], xmm0");                    // save the parsed right numeric-string value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left boxed operand for numeric casting
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the left operand to a comparison double
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 72]");                    // reload the parsed right numeric-string value
    emitter.instruction("ucomisd xmm0, xmm1");                                  // compare numeric scalar and parsed string values
    emitter.instruction("sete al");                                             // set true for ordered numeric/string equality
    emitter.instruction("setnp r10b");                                          // require an ordered comparison
    emitter.instruction("and al, r10b");                                        // clear unordered NaN equality
    emitter.instruction("movzx rax, al");                                       // widen the numeric/string equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the numeric/string equality result
    emitter.label("__elephc_eval_mixed_loose_eq_bool");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left boxed operand for truthiness
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_bool input
    emitter.instruction("call __rt_mixed_cast_bool");                           // cast the left operand to PHP truthiness
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the left truthiness result
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the right boxed operand for truthiness
    emitter.instruction("mov rax, rdi");                                        // move the right boxed operand into mixed_cast_bool input
    emitter.instruction("call __rt_mixed_cast_bool");                           // cast the right operand to PHP truthiness
    emitter.instruction("cmp QWORD PTR [rbp - 72], rax");                       // compare boolean truthiness for loose equality
    emitter.instruction("sete al");                                             // materialize bool loose equality
    emitter.instruction("movzx rax, al");                                       // widen the bool equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the bool loose-equality result
    emitter.label("__elephc_eval_mixed_loose_eq_numeric");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left boxed operand for numeric equality
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the left operand to a comparison double
    emitter.instruction("movsd QWORD PTR [rbp - 72], xmm0");                    // save the left numeric equality operand
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the right boxed operand for numeric equality
    emitter.instruction("mov rax, rdi");                                        // move the right boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the right operand to a comparison double
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 72]");                    // reload the left numeric equality operand
    emitter.instruction("ucomisd xmm1, xmm0");                                  // compare numeric operands for loose equality
    emitter.instruction("sete al");                                             // set true for ordered numeric equality
    emitter.instruction("setnp r10b");                                          // require an ordered comparison
    emitter.instruction("and al, r10b");                                        // clear unordered NaN equality
    emitter.instruction("movzx rax, al");                                       // widen the numeric equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the numeric loose-equality result
    emitter.label("__elephc_eval_mixed_loose_eq_true");
    emitter.instruction("mov rax, 1");                                          // materialize true for loose equality
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the true result
    emitter.label("__elephc_eval_mixed_loose_eq_false");
    emitter.instruction("xor eax, eax");                                        // materialize false for loose equality
    emitter.label("__elephc_eval_mixed_loose_eq_done");
    emitter.instruction("add rsp, 96");                                         // release the loose-equality helper slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the loose-equality boolean in rax

    label_c_global(emitter, "__elephc_eval_value_regular_key_compare");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable key-comparison wrapper frame
    emitter.instruction("sub rsp, 48");                                         // reserve boxed operands and normalized key pairs
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the boxed left key
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the boxed right key
    emitter.instruction("mov rax, rdi");                                        // move the left key into the internal unbox input register
    emitter.instruction("call __rt_mixed_unbox");                               // unbox the left integer or string key
    emitter.instruction("cmp rax, 0");                                          // does the left key carry the integer runtime tag?
    emitter.instruction("je __elephc_eval_value_regular_key_left_int_x86");     // normalize integer keys to the hash sentinel representation
    emitter.instruction("cmp rax, 1");                                          // does the left key carry the string runtime tag?
    emitter.instruction("jne __elephc_eval_value_regular_key_invalid_x86");     // reject values that cannot be normalized array keys
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the left string pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the left bounded string length
    emitter.instruction("jmp __elephc_eval_value_regular_key_right_x86");       // continue with the right key
    emitter.label("__elephc_eval_value_regular_key_left_int_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the left integer payload
    emitter.instruction("mov QWORD PTR [rbp - 32], -1");                        // integer hash keys use an all-ones high-word sentinel
    emitter.label("__elephc_eval_value_regular_key_right_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed right key for unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // unbox the right integer or string key
    emitter.instruction("cmp rax, 0");                                          // does the right key carry the integer runtime tag?
    emitter.instruction("je __elephc_eval_value_regular_key_right_int_x86");    // normalize integer keys to the hash sentinel representation
    emitter.instruction("cmp rax, 1");                                          // does the right key carry the string runtime tag?
    emitter.instruction("jne __elephc_eval_value_regular_key_invalid_x86");     // reject values that cannot be normalized array keys
    emitter.instruction("mov rcx, rdx");                                        // pass the right bounded string length as key_hi
    emitter.instruction("jmp __elephc_eval_value_regular_key_call_x86");        // compare the two normalized key pairs
    emitter.label("__elephc_eval_value_regular_key_right_int_x86");
    emitter.instruction("mov rcx, -1");                                         // integer hash keys use an all-ones high-word sentinel
    emitter.label("__elephc_eval_value_regular_key_call_x86");
    emitter.instruction("mov rdx, rdi");                                        // pass the right key low word to the native comparator
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the saved left key low word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the saved left key high word or integer sentinel
    emitter.instruction("call __rt_key_compare_regular");                       // apply the same SORT_REGULAR ordering used by AOT ksort
    emitter.instruction("jmp __elephc_eval_value_regular_key_done_x86");        // preserve the normalized -1, 0, or 1 result
    emitter.label("__elephc_eval_value_regular_key_invalid_x86");
    emitter.instruction("xor eax, eax");                                        // invalid key cells compare equal and fail closed
    emitter.label("__elephc_eval_value_regular_key_done_x86");
    emitter.instruction("add rsp, 48");                                         // release the key-comparison wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the signed comparison result to Rust

    label_c_global(emitter, "__elephc_eval_value_spaceship");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for the right operand and left runtime triple
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed-unbox input
    emitter.instruction("call __rt_mixed_unbox");                               // unbox the left eval operand into tag and payload words
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the left runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the left low payload word
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the left high payload word
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for runtime-tag unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // unbox the right eval operand into tag and payload words
    emitter.instruction("mov rcx, rax");                                        // pass the right runtime tag to PHP ordering
    emitter.instruction("mov r8, rdi");                                         // pass the right low payload word to PHP ordering
    emitter.instruction("mov r9, rdx");                                         // pass the right high payload word to PHP ordering
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the left runtime tag
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the left low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the left high payload word
    emitter.instruction("call __rt_php_compare");                               // compute PHP's normalized spaceship ordering
    emitter.instruction("mov rdi, rax");                                        // move the ordering result into the Mixed integer payload
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a high word
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the spaceship result into a Mixed cell
    emitter.instruction("add rsp, 48");                                         // release the spaceship wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed spaceship result to Rust

}
