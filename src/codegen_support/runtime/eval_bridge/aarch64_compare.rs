//! Purpose:
//! Emits AArch64 concat, comparison, and spaceship wrappers.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Equality uses its dedicated helpers; relational and spaceship operations share
//!   the runtime PHP ordering table and preserve its unordered NaN flag.

use super::*;

/// Emits AArch64 concat, comparison, and spaceship wrappers.
pub(super) fn emit_aarch64_compare(emitter: &mut Emitter) {
    super::concat::emit(emitter);

    label_c_global(emitter, "__elephc_eval_value_compare");
    emitter.instruction("sub sp, sp, #64");                                     // allocate a wrapper frame for comparison operands and opcode
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address across comparison helpers
    emitter.instruction("add x29, sp, #48");                                    // establish a stable comparison wrapper frame
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand for later casts
    emitter.instruction("str x2, [sp, #8]");                                    // save the eval comparison opcode
    emitter.instruction("str x0, [sp, #16]");                                   // save the left boxed operand for equality helper calls
    emitter.instruction("cmp x2, #0");                                          // is this loose equality?
    emitter.instruction("b.eq __elephc_eval_value_compare_eq");                 // route == through the mixed loose-equality helper
    emitter.instruction("cmp x2, #1");                                          // is this loose inequality?
    emitter.instruction("b.eq __elephc_eval_value_compare_ne");                 // route != through the mixed loose-equality helper
    emitter.instruction("cmp x2, #6");                                          // is this strict equality?
    emitter.instruction("b.eq __elephc_eval_value_compare_strict_eq");          // route === through the mixed strict-equality helper
    emitter.instruction("cmp x2, #7");                                          // is this strict inequality?
    emitter.instruction("b.eq __elephc_eval_value_compare_strict_ne");          // route !== through the mixed strict-equality helper
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the left boxed operand for runtime-tag unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox the left eval operand into tag and payload words
    emitter.instruction("stp x0, x1, [sp, #24]");                               // save the left runtime tag and low payload word
    emitter.instruction("str x2, [sp, #40]");                                   // save the left high payload word
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for runtime-tag unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox the right eval operand into tag and payload words
    emitter.instruction("mov x3, x0");                                          // pass the right runtime tag to PHP ordering
    emitter.instruction("mov x4, x1");                                          // pass the right low payload word to PHP ordering
    emitter.instruction("mov x5, x2");                                          // pass the right high payload word to PHP ordering
    emitter.instruction("ldp x0, x1, [sp, #24]");                               // reload the left runtime tag and low payload word
    emitter.instruction("ldr x2, [sp, #40]");                                   // reload the left high payload word
    emitter.instruction("bl __rt_php_compare");                                 // apply PHP ordering and report unordered NaN separately
    emitter.instruction("mov x10, x0");                                         // preserve the normalized ordering result for opcode dispatch
    emitter.instruction("mov x11, x1");                                         // preserve the unordered flag for relational predicates
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the eval comparison opcode for dispatch
    emitter.instruction("cmp x9, #2");                                          // is this a less-than comparison?
    emitter.instruction("b.eq __elephc_eval_value_compare_lt");                 // materialize left < right from normalized PHP ordering
    emitter.instruction("cmp x9, #3");                                          // is this a less-than-or-equal comparison?
    emitter.instruction("b.eq __elephc_eval_value_compare_lte");                // materialize left <= right from normalized PHP ordering
    emitter.instruction("cmp x9, #4");                                          // is this a greater-than comparison?
    emitter.instruction("b.eq __elephc_eval_value_compare_gt");                 // materialize left > right from normalized PHP ordering
    emitter.instruction("cmp x9, #5");                                          // is this a greater-than-or-equal comparison?
    emitter.instruction("b.eq __elephc_eval_value_compare_gte");                // materialize left >= right from normalized PHP ordering
    emitter.instruction("mov x1, #0");                                          // unknown comparison opcodes fail closed as false
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the fallback false result
    emitter.label("__elephc_eval_value_compare_eq");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the left operand for loose equality
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the right operand for loose equality
    emitter.instruction("bl __elephc_eval_mixed_loose_eq");                     // compute scalar PHP loose equality
    emitter.instruction("mov x1, x0");                                          // move equality into the bool payload register
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the equality result
    emitter.label("__elephc_eval_value_compare_ne");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the left operand for loose inequality
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the right operand for loose inequality
    emitter.instruction("bl __elephc_eval_mixed_loose_eq");                     // compute scalar PHP loose equality before inversion
    emitter.instruction("eor x1, x0, #1");                                      // invert equality for the != operator
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the inequality result
    emitter.label("__elephc_eval_value_compare_strict_eq");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the left operand for strict equality
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the right operand for strict equality
    emitter.instruction("bl __rt_mixed_strict_eq");                             // compute PHP strict equality
    emitter.instruction("mov x1, x0");                                          // move strict equality into the bool payload register
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the strict-equality result
    emitter.label("__elephc_eval_value_compare_strict_ne");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the left operand for strict inequality
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the right operand for strict inequality
    emitter.instruction("bl __rt_mixed_strict_eq");                             // compute PHP strict equality before inversion
    emitter.instruction("eor x1, x0, #1");                                      // invert equality for the !== operator
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the strict-inequality result
    emitter.label("__elephc_eval_value_compare_lt");
    emitter.instruction("cbnz x11, __elephc_eval_value_compare_unordered");     // every relational predicate is false for unordered NaN
    emitter.instruction("cmp x10, #0");                                         // compare the PHP ordering result against zero for <
    emitter.instruction("cset x1, lt");                                         // materialize signed less-than as a PHP boolean
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the less-than result
    emitter.label("__elephc_eval_value_compare_lte");
    emitter.instruction("cbnz x11, __elephc_eval_value_compare_unordered");     // every relational predicate is false for unordered NaN
    emitter.instruction("cmp x10, #0");                                         // compare the PHP ordering result against zero for <=
    emitter.instruction("cset x1, le");                                         // materialize signed less-than-or-equal as a PHP boolean
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the less-than-or-equal result
    emitter.label("__elephc_eval_value_compare_gt");
    emitter.instruction("cbnz x11, __elephc_eval_value_compare_unordered");     // every relational predicate is false for unordered NaN
    emitter.instruction("cmp x10, #0");                                         // compare the PHP ordering result against zero for >
    emitter.instruction("cset x1, gt");                                         // materialize signed greater-than as a PHP boolean
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the greater-than result
    emitter.label("__elephc_eval_value_compare_gte");
    emitter.instruction("cbnz x11, __elephc_eval_value_compare_unordered");     // every relational predicate is false for unordered NaN
    emitter.instruction("cmp x10, #0");                                         // compare the PHP ordering result against zero for >=
    emitter.instruction("cset x1, ge");                                         // materialize signed greater-than-or-equal as a PHP boolean
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the greater-than-or-equal result
    emitter.label("__elephc_eval_value_compare_unordered");
    emitter.instruction("mov x1, #0");                                          // unordered NaN makes every PHP relational predicate false
    emitter.label("__elephc_eval_value_compare_box");
    emitter.instruction("mov x0, #3");                                          // runtime tag 3 = bool
    emitter.instruction("mov x2, xzr");                                         // bool payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the comparison result as a Mixed bool
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the comparison wrapper frame
    emitter.instruction("ret");                                                 // return the boxed comparison result to Rust

    emitter.label("__elephc_eval_mixed_loose_eq");
    emitter.instruction("sub sp, sp, #96");                                     // allocate helper slots for unboxed tags, payloads, and casts
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address across mixed helper calls
    emitter.instruction("add x29, sp, #80");                                    // establish a stable loose-equality helper frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save incoming boxed operands for later casts
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox the left eval operand into tag and payload words
    emitter.instruction("str x0, [sp, #16]");                                   // save the left runtime tag
    emitter.instruction("stp x1, x2, [sp, #24]");                               // save the left payload words
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed operand for unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox the right eval operand into tag and payload words
    emitter.instruction("str x0, [sp, #40]");                                   // save the right runtime tag
    emitter.instruction("stp x1, x2, [sp, #48]");                               // save the right payload words
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the left runtime tag for equality dispatch
    emitter.instruction("cmp x9, #3");                                          // does the left operand have PHP bool semantics?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_bool");              // bool comparisons use truthiness on both operands
    emitter.instruction("cmp x0, #3");                                          // does the right operand have PHP bool semantics?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_bool");              // bool comparisons use truthiness on both operands
    emitter.instruction("cmp x9, x0");                                          // do the operands have the same runtime tag?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_same_tag");          // same-tag scalars use focused payload comparisons
    emitter.instruction("cmp x9, #8");                                          // is the left operand null?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_left_null");         // null compares equal only to empty strings before numeric fallback
    emitter.instruction("cmp x0, #8");                                          // is the right operand null?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_right_null");        // null compares equal only to empty strings before numeric fallback
    emitter.instruction("cmp x9, #1");                                          // is a non-matching left operand a string?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_left_string");       // compare numeric strings against numeric scalars
    emitter.instruction("cmp x0, #1");                                          // is a non-matching right operand a string?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_right_string");      // compare numeric strings against numeric scalars
    emitter.instruction("b __elephc_eval_mixed_loose_eq_numeric");              // remaining scalar mismatches compare numerically
    emitter.label("__elephc_eval_mixed_loose_eq_same_tag");
    emitter.instruction("cmp x9, #8");                                          // are both operands null?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_true");              // null loosely equals null
    emitter.instruction("cmp x9, #1");                                          // are both operands strings?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_strings");           // strings use PHP loose string equality
    emitter.instruction("cmp x9, #2");                                          // are both operands floats?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_floats");            // floats compare with native floating equality
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the left low payload word
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the right low payload word
    emitter.instruction("cmp x10, x11");                                        // compare low payload words for int and pointer-like scalars
    emitter.instruction("b.ne __elephc_eval_mixed_loose_eq_false");             // mismatched low payloads are not equal
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the left high payload word
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload the right high payload word
    emitter.instruction("cmp x10, x11");                                        // compare high payload words for pointer-like scalars
    emitter.instruction("cset x0, eq");                                         // materialize same-tag payload equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the payload equality result
    emitter.label("__elephc_eval_mixed_loose_eq_strings");
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // reload the left string pointer and length
    emitter.instruction("ldp x3, x4, [sp, #48]");                               // reload the right string pointer and length
    emitter.instruction("bl __rt_str_loose_eq");                                // compare strings with PHP loose numeric-string rules
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the string loose-equality result
    emitter.label("__elephc_eval_mixed_loose_eq_floats");
    emitter.instruction("ldr d1, [sp, #24]");                                   // reload the left float payload
    emitter.instruction("ldr d0, [sp, #48]");                                   // reload the right float payload
    emitter.instruction("fcmp d1, d0");                                         // compare same-tag float payloads
    emitter.instruction("cset x0, eq");                                         // floats loosely equal only when ordered equal
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the float equality result
    emitter.label("__elephc_eval_mixed_loose_eq_left_null");
    emitter.instruction("cmp x0, #1");                                          // is null being compared with a string?
    emitter.instruction("b.ne __elephc_eval_mixed_loose_eq_numeric");           // non-string null comparisons fall back to numeric zero
    emitter.instruction("ldr x10, [sp, #56]");                                  // load the right string length
    emitter.instruction("cmp x10, #0");                                         // null loosely equals only the empty string
    emitter.instruction("cset x0, eq");                                         // materialize the null/string equality result
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the null/string equality result
    emitter.label("__elephc_eval_mixed_loose_eq_right_null");
    emitter.instruction("cmp x9, #1");                                          // is null being compared with a string?
    emitter.instruction("b.ne __elephc_eval_mixed_loose_eq_numeric");           // non-string null comparisons fall back to numeric zero
    emitter.instruction("ldr x10, [sp, #32]");                                  // load the left string length
    emitter.instruction("cmp x10, #0");                                         // null loosely equals only the empty string
    emitter.instruction("cset x0, eq");                                         // materialize the string/null equality result
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the string/null equality result
    emitter.label("__elephc_eval_mixed_loose_eq_left_string");
    emitter.instruction("cmp x0, #0");                                          // can the right operand be compared numerically as an int?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_left_string_numeric"); // parse the left string for numeric equality
    emitter.instruction("cmp x0, #2");                                          // can the right operand be compared numerically as a float?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_left_string_numeric"); // parse the left string for numeric equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_false");                // non-numeric string mismatches are not loosely equal here
    emitter.label("__elephc_eval_mixed_loose_eq_left_string_numeric");
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // reload the left string pointer and length for numeric parsing
    emitter.instruction("bl __rt_str_to_number");                               // parse the left string under PHP numeric-string rules
    emitter.instruction("cbz x0, __elephc_eval_mixed_loose_eq_false");          // non-numeric strings do not equal numeric scalars
    emitter.instruction("str d0, [sp, #64]");                                   // save the parsed left numeric-string value
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed operand for numeric casting
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the right operand to a comparison double
    emitter.instruction("ldr d1, [sp, #64]");                                   // reload the parsed left numeric-string value
    emitter.instruction("fcmp d1, d0");                                         // compare parsed string and numeric scalar values
    emitter.instruction("cset x0, eq");                                         // materialize string/numeric loose equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the string/numeric equality result
    emitter.label("__elephc_eval_mixed_loose_eq_right_string");
    emitter.instruction("cmp x9, #0");                                          // can the left operand be compared numerically as an int?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_right_string_numeric"); // parse the right string for numeric equality
    emitter.instruction("cmp x9, #2");                                          // can the left operand be compared numerically as a float?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_right_string_numeric"); // parse the right string for numeric equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_false");                // non-numeric string mismatches are not loosely equal here
    emitter.label("__elephc_eval_mixed_loose_eq_right_string_numeric");
    emitter.instruction("ldp x1, x2, [sp, #48]");                               // reload the right string pointer and length for numeric parsing
    emitter.instruction("bl __rt_str_to_number");                               // parse the right string under PHP numeric-string rules
    emitter.instruction("cbz x0, __elephc_eval_mixed_loose_eq_false");          // non-numeric strings do not equal numeric scalars
    emitter.instruction("str d0, [sp, #64]");                                   // save the parsed right numeric-string value
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left boxed operand for numeric casting
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the left operand to a comparison double
    emitter.instruction("ldr d1, [sp, #64]");                                   // reload the parsed right numeric-string value
    emitter.instruction("fcmp d0, d1");                                         // compare numeric scalar and parsed string values
    emitter.instruction("cset x0, eq");                                         // materialize numeric/string loose equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the numeric/string equality result
    emitter.label("__elephc_eval_mixed_loose_eq_bool");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left boxed operand for truthiness
    emitter.instruction("bl __rt_mixed_cast_bool");                             // cast the left operand to PHP truthiness
    emitter.instruction("str x0, [sp, #64]");                                   // save the left truthiness result
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed operand for truthiness
    emitter.instruction("bl __rt_mixed_cast_bool");                             // cast the right operand to PHP truthiness
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the left truthiness result
    emitter.instruction("cmp x9, x0");                                          // compare boolean truthiness for loose equality
    emitter.instruction("cset x0, eq");                                         // materialize bool loose equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the bool loose-equality result
    emitter.label("__elephc_eval_mixed_loose_eq_numeric");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left boxed operand for numeric equality
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the left operand to a comparison double
    emitter.instruction("str d0, [sp, #64]");                                   // save the left numeric equality operand
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed operand for numeric equality
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the right operand to a comparison double
    emitter.instruction("ldr d1, [sp, #64]");                                   // reload the left numeric equality operand
    emitter.instruction("fcmp d1, d0");                                         // compare numeric operands for loose equality
    emitter.instruction("cset x0, eq");                                         // materialize numeric loose equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the numeric loose-equality result
    emitter.label("__elephc_eval_mixed_loose_eq_true");
    emitter.instruction("mov x0, #1");                                          // materialize true for loose equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the true result
    emitter.label("__elephc_eval_mixed_loose_eq_false");
    emitter.instruction("mov x0, #0");                                          // materialize false for loose equality
    emitter.label("__elephc_eval_mixed_loose_eq_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the loose-equality helper frame
    emitter.instruction("ret");                                                 // return the loose-equality boolean in x0

    label_c_global(emitter, "__elephc_eval_value_regular_key_compare");
    emitter.instruction("sub sp, sp, #64");                                     // allocate normalized key slots and a saved-call frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve frame pointer and return address across unboxing and comparison
    emitter.instruction("add x29, sp, #48");                                    // establish a stable key-comparison wrapper frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save both boxed key operands before unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox the left integer or string key
    emitter.instruction("cmp x0, #0");                                          // does the left key carry the integer runtime tag?
    emitter.instruction("b.eq __elephc_eval_value_regular_key_left_int");       // normalize integer keys to the hash sentinel representation
    emitter.instruction("cmp x0, #1");                                          // does the left key carry the string runtime tag?
    emitter.instruction("b.ne __elephc_eval_value_regular_key_invalid");        // reject values that cannot be normalized array keys
    emitter.instruction("stp x1, x2, [sp, #16]");                               // save the left string pointer and bounded length
    emitter.instruction("b __elephc_eval_value_regular_key_right");             // continue with the right key
    emitter.label("__elephc_eval_value_regular_key_left_int");
    emitter.instruction("mov x2, #-1");                                         // integer hash keys use an all-ones high-word sentinel
    emitter.instruction("stp x1, x2, [sp, #16]");                               // save the normalized left integer key pair
    emitter.label("__elephc_eval_value_regular_key_right");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed right key for unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox the right integer or string key
    emitter.instruction("cmp x0, #0");                                          // does the right key carry the integer runtime tag?
    emitter.instruction("b.eq __elephc_eval_value_regular_key_right_int");      // normalize integer keys to the hash sentinel representation
    emitter.instruction("cmp x0, #1");                                          // does the right key carry the string runtime tag?
    emitter.instruction("b.ne __elephc_eval_value_regular_key_invalid");        // reject values that cannot be normalized array keys
    emitter.instruction("stp x1, x2, [sp, #32]");                               // save the right string pointer and bounded length
    emitter.instruction("b __elephc_eval_value_regular_key_call");              // compare the two normalized key pairs
    emitter.label("__elephc_eval_value_regular_key_right_int");
    emitter.instruction("mov x2, #-1");                                         // integer hash keys use an all-ones high-word sentinel
    emitter.instruction("stp x1, x2, [sp, #32]");                               // save the normalized right integer key pair
    emitter.label("__elephc_eval_value_regular_key_call");
    emitter.instruction("ldp x0, x1, [sp, #16]");                               // load the normalized left key for the native comparator
    emitter.instruction("ldp x2, x3, [sp, #32]");                               // load the normalized right key for the native comparator
    emitter.instruction("bl __rt_key_compare_regular");                         // apply the same SORT_REGULAR ordering used by AOT ksort
    emitter.instruction("b __elephc_eval_value_regular_key_done");              // preserve the normalized -1, 0, or 1 result
    emitter.label("__elephc_eval_value_regular_key_invalid");
    emitter.instruction("mov x0, #0");                                          // invalid key cells compare equal and fail closed
    emitter.label("__elephc_eval_value_regular_key_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the key-comparison wrapper frame
    emitter.instruction("ret");                                                 // return the signed comparison result to Rust

    label_c_global(emitter, "__elephc_eval_value_spaceship");
    emitter.instruction("sub sp, sp, #64");                                     // allocate wrapper slots for boxed operands and the left runtime triple
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox the left eval operand into tag and payload words
    emitter.instruction("stp x0, x1, [sp, #8]");                                // save the left runtime tag and low payload word
    emitter.instruction("str x2, [sp, #24]");                                   // save the left high payload word
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for runtime-tag unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox the right eval operand into tag and payload words
    emitter.instruction("mov x3, x0");                                          // pass the right runtime tag to PHP ordering
    emitter.instruction("mov x4, x1");                                          // pass the right low payload word to PHP ordering
    emitter.instruction("mov x5, x2");                                          // pass the right high payload word to PHP ordering
    emitter.instruction("ldp x0, x1, [sp, #8]");                                // reload the left runtime tag and low payload word
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload the left high payload word
    emitter.instruction("bl __rt_php_compare");                                 // compute PHP's normalized spaceship ordering
    emitter.instruction("mov x1, x0");                                          // move the ordering result into the Mixed integer payload
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("bl __rt_mixed_from_value");                            // box the spaceship result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the spaceship wrapper frame
    emitter.instruction("ret");                                                 // return the boxed spaceship result to Rust

}
