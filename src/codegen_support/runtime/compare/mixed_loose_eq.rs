//! Purpose:
//! Emits `__rt_mixed_loose_eq`, the runtime implementation of PHP's `==` for two
//! boxed `Mixed` cells. It encodes PHP 8's comparison table end to end: bool/null
//! coercion, numeric-string parsing, array-vs-array structural comparison and
//! object-vs-object class/property comparison.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::compare`.
//! - Generated code, through the `lower_loose_eq` fallback in
//!   `crate::codegen::lower_inst::comparisons`, which boxes both operands first.
//!
//! Key details:
//! - PHP 8 rule order, and why it matters: a `bool` operand wins over everything
//!   (`[0] == true`), then `null` (`[] == null` is true, `null == "0"` is false
//!   because null converts to `""`, not to `0`), then containers (an array is
//!   loosely equal only to another array), then objects, then strings, then a
//!   numeric comparison. Reordering any two of those changes observable results.
//! - `__rt_mixed_loose_eq_d` carries the recursion depth; `__rt_mixed_loose_eq` is
//!   the depth-0 entry point every caller uses.
//! - Same-tag int/resource/callable payloads compare word-for-word rather than
//!   through `double`, so large integers do not lose precision.
//! - Internal object adapters run before the ordinary PHP comparison table, including
//!   bool/null conversion; unordered adapter results never mean equality.

use super::MAX_LOOSE_EQ_DEPTH;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_mixed_loose_eq` / `__rt_mixed_loose_eq_d` for the active target.
///
/// Public entry: AArch64 `x0`/`x1` = the two boxed `Mixed` operands, result in `x0`;
/// x86_64 `rdi`/`rsi`, result in `rax`. The `_d` form takes the recursion depth in
/// `x2` / `rdx`. Both operands stay borrowed: the helper never releases them.
pub fn emit_mixed_loose_eq(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_loose_eq_x86_64(emitter);
        return;
    }
    emit_mixed_loose_eq_aarch64(emitter);
}

/// Emits the AArch64 loose-equality dispatcher.
///
/// Frame (112 bytes): `[sp,#0]` left cell, `[sp,#8]` right cell, `[sp,#16]` depth,
/// `[sp,#24..#40]` left tag/lo/hi, `[sp,#48..#64]` right tag/lo/hi, `[sp,#72]` a
/// scalar scratch slot, `[sp,#96]` saved `x29`/`x30`.
fn emit_mixed_loose_eq_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_loose_eq ---");
    emitter.label_global("__rt_mixed_loose_eq");
    emitter.instruction("mov x2, #0");                                          // every public comparison starts at recursion depth zero
    emitter.instruction("b __rt_mixed_loose_eq_d");                             // share the depth-carrying comparison body

    emitter.label_global("__rt_mixed_loose_eq_d");
    emitter.instruction("sub sp, sp, #112");                                    // allocate the loose-equality comparison frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the comparison frame pointer
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save both boxed operands for the later helper calls
    emitter.instruction("str x2, [sp, #16]");                                   // save the current recursion depth
    abi::emit_load_int_immediate(emitter, "x9", MAX_LOOSE_EQ_DEPTH);
    emitter.instruction("cmp x2, x9");                                          // has the walk reached the cyclic-structure cap?
    emitter.instruction("b.ge __rt_mle_false");                                 // stop instead of recursing into a cycle forever

    // -- unbox both operands into concrete tag/payload triples --
    emitter.instruction("bl __rt_mixed_unbox");                                 // left cell -> x0=tag, x1=lo, x2=hi
    emitter.instruction("str x0, [sp, #24]");                                   // save the left runtime tag
    emitter.instruction("stp x1, x2, [sp, #32]");                               // save the left payload words
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed operand
    emitter.instruction("bl __rt_mixed_unbox");                                 // right cell -> x0=tag, x1=lo, x2=hi
    emitter.instruction("str x0, [sp, #48]");                                   // save the right runtime tag
    emitter.instruction("stp x1, x2, [sp, #56]");                               // save the right payload words

    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the left runtime tag for dispatch
    emitter.instruction("mov x10, x0");                                         // keep the right runtime tag in a scratch register

    // -- internal object comparison overrides the ordinary object conversion rules --
    emitter.instruction("cmp x9, #6");                                          // can the left operand select an internal class adapter?
    emitter.instruction("b.eq __rt_mle_internal_object");                       // ask the program-specific typed dispatcher
    emitter.instruction("cmp x10, #6");                                         // can the right operand select an internal class adapter?
    emitter.instruction("b.ne __rt_mle_standard");                              // scalar pairs keep the allocation-free comparison table
    emitter.label("__rt_mle_internal_object");
    emitter.instruction("ldp x0, x1, [sp, #0]");                                // borrow both original boxed operands
    abi::emit_call_label(emitter, &crate::names::function_symbol("__elephc_internal_object_compare"));
    emitter.instruction("cmp x0, #3");                                          // three means neither class provided an adapter
    emitter.instruction("b.eq __rt_mle_standard");                              // ordinary objects retain their existing PHP rules
    emitter.instruction("cmp x0, #0");                                          // only an ordered equal result represents loose equality
    emitter.instruction("cset x0, eq");                                         // unordered and unequal adapter results are false
    emitter.instruction("b __rt_mle_done");                                     // return the internal class comparison
    emitter.label("__rt_mle_standard");
    emitter.instruction("ldr x9, [sp, #24]");                                   // restore the left tag after a possible adapter call
    emitter.instruction("ldr x10, [sp, #48]");                                  // restore the right tag after a possible adapter call

    // -- PHP rule 1: a bool operand converts BOTH sides to bool --
    emitter.instruction("cmp x9, #3");                                          // is the left operand a bool?
    emitter.instruction("b.eq __rt_mle_bool");                                  // bool comparisons use truthiness on both sides
    emitter.instruction("cmp x10, #3");                                         // is the right operand a bool?
    emitter.instruction("b.eq __rt_mle_bool");                                  // bool comparisons use truthiness on both sides

    // -- PHP rule 2: null converts to "" against a string and to bool otherwise --
    emitter.instruction("cmp x9, #8");                                          // is the left operand PHP null?
    emitter.instruction("b.eq __rt_mle_left_null");                             // null has its own conversion rules
    emitter.instruction("cmp x10, #8");                                         // is the right operand PHP null?
    emitter.instruction("b.eq __rt_mle_right_null");                            // null has its own conversion rules

    // -- PHP rule 3: an array is loosely equal only to another array --
    emitter.instruction("sub x11, x9, #4");                                     // normalize the left tag against the array range
    emitter.instruction("cmp x11, #1");                                         // tags 4 and 5 are indexed arrays and hashes
    emitter.instruction("cset x11, ls");                                        // record whether the left operand is array-like
    emitter.instruction("sub x12, x10, #4");                                    // normalize the right tag against the array range
    emitter.instruction("cmp x12, #1");                                         // tags 4 and 5 are indexed arrays and hashes
    emitter.instruction("cset x12, ls");                                        // record whether the right operand is array-like
    emitter.instruction("and x13, x11, x12");                                   // are both operands array-like?
    emitter.instruction("cbnz x13, __rt_mle_arrays");                           // two arrays compare structurally
    emitter.instruction("orr x13, x11, x12");                                   // is exactly one operand array-like?
    emitter.instruction("cbnz x13, __rt_mle_false");                            // an array never equals a non-array here

    // -- PHP rule 4: objects compare by class then by properties --
    emitter.instruction("cmp x9, #6");                                          // is the left operand an object?
    emitter.instruction("b.ne __rt_mle_left_not_object");                       // fall through to the string/number rules
    emitter.instruction("cmp x10, #6");                                         // is the right operand also an object?
    emitter.instruction("b.eq __rt_mle_objects");                               // two objects compare class-then-property
    emitter.instruction("b __rt_mle_object_vs_number_right");                   // an object only equals the number 1
    emitter.label("__rt_mle_left_not_object");
    emitter.instruction("cmp x10, #6");                                         // is the right operand an object?
    emitter.instruction("b.eq __rt_mle_object_vs_number_left");                 // an object only equals the number 1

    // -- PHP rule 5: strings compare with numeric-string promotion --
    emitter.instruction("cmp x9, #1");                                          // is the left operand a string?
    emitter.instruction("b.ne __rt_mle_left_not_string");                       // only the right operand can still be a string
    emitter.instruction("cmp x10, #1");                                         // is the right operand also a string?
    emitter.instruction("b.eq __rt_mle_strings");                               // two strings use PHP loose string equality
    emitter.instruction("b __rt_mle_left_string_number");                       // a string vs a number parses the string
    emitter.label("__rt_mle_left_not_string");
    emitter.instruction("cmp x10, #1");                                         // is the right operand a string?
    emitter.instruction("b.eq __rt_mle_right_string_number");                   // a number vs a string parses the string
    emitter.instruction("b __rt_mle_numeric");                                  // everything left over compares numerically

    // -- bool coercion of both operands --
    emitter.label("__rt_mle_bool");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left boxed operand
    emitter.instruction("bl __rt_mixed_cast_bool");                             // PHP truthiness of the left operand
    emitter.instruction("str x0, [sp, #72]");                                   // save the left truthiness result
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed operand
    emitter.instruction("bl __rt_mixed_cast_bool");                             // PHP truthiness of the right operand
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload the left truthiness result
    emitter.instruction("cmp x9, x0");                                          // compare the two truthiness values
    emitter.instruction("cset x0, eq");                                         // materialize the bool-coerced equality
    emitter.instruction("b __rt_mle_done");                                     // return the bool-coerced result

    // -- null on the left --
    emitter.label("__rt_mle_left_null");
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the right runtime tag
    emitter.instruction("cmp x10, #8");                                         // is the right operand also null?
    emitter.instruction("b.eq __rt_mle_true");                                  // null is loosely equal to null
    emitter.instruction("cmp x10, #1");                                         // is the right operand a string?
    emitter.instruction("b.ne __rt_mle_null_vs_right_bool");                    // non-string operands coerce to bool
    emitter.instruction("ldr x11, [sp, #64]");                                  // load the right string length
    emitter.instruction("cmp x11, #0");                                         // null converts to "" and equals only the empty string
    emitter.instruction("cset x0, eq");                                         // materialize the null-versus-string result
    emitter.instruction("b __rt_mle_done");                                     // return the null-versus-string result
    emitter.label("__rt_mle_null_vs_right_bool");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed operand
    emitter.instruction("bl __rt_mixed_cast_bool");                             // PHP truthiness of the right operand
    emitter.instruction("eor x0, x0, #1");                                      // null equals exactly the falsy values
    emitter.instruction("b __rt_mle_done");                                     // return the null-coerced result

    // -- null on the right --
    emitter.label("__rt_mle_right_null");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the left runtime tag
    emitter.instruction("cmp x9, #1");                                          // is the left operand a string?
    emitter.instruction("b.ne __rt_mle_null_vs_left_bool");                     // non-string operands coerce to bool
    emitter.instruction("ldr x11, [sp, #40]");                                  // load the left string length
    emitter.instruction("cmp x11, #0");                                         // null converts to "" and equals only the empty string
    emitter.instruction("cset x0, eq");                                         // materialize the string-versus-null result
    emitter.instruction("b __rt_mle_done");                                     // return the string-versus-null result
    emitter.label("__rt_mle_null_vs_left_bool");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left boxed operand
    emitter.instruction("bl __rt_mixed_cast_bool");                             // PHP truthiness of the left operand
    emitter.instruction("eor x0, x0, #1");                                      // null equals exactly the falsy values
    emitter.instruction("b __rt_mle_done");                                     // return the null-coerced result

    // -- container and object recursion --
    emitter.label("__rt_mle_arrays");
    emitter.instruction("ldp x0, x1, [sp, #0]");                                // reload both boxed array operands
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the current recursion depth
    emitter.instruction("add x2, x2, #1");                                      // descend one level for the element walk
    emitter.instruction("bl __rt_mixed_array_loose_eq");                        // compare the two arrays entry by entry
    emitter.instruction("b __rt_mle_done");                                     // return the structural array result

    emitter.label("__rt_mle_objects");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the left object pointer
    emitter.instruction("ldr x1, [sp, #56]");                                   // reload the right object pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the current recursion depth
    emitter.instruction("add x2, x2, #1");                                      // descend one level for the property walk
    emitter.instruction("bl __rt_obj_loose_eq");                                // compare class identity then properties
    emitter.instruction("b __rt_mle_done");                                     // return the object comparison result

    // -- PHP converts an object to the integer 1 when compared with a number --
    emitter.label("__rt_mle_object_vs_number_right");
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the right runtime tag
    emitter.instruction("cmp x10, #0");                                         // is the right operand an int?
    emitter.instruction("b.eq __rt_mle_object_one_right");                      // compare the number against 1
    emitter.instruction("cmp x10, #2");                                         // is the right operand a float?
    emitter.instruction("b.ne __rt_mle_false");                                 // an object never equals a string or resource
    emitter.label("__rt_mle_object_one_right");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // read the numeric value of the right operand
    emitter.instruction("fmov d1, #1.0");                                       // PHP converts the object operand to 1
    emitter.instruction("fcmp d0, d1");                                         // compare the number against the converted object
    emitter.instruction("cset x0, eq");                                         // materialize the object-versus-number result
    emitter.instruction("b __rt_mle_done");                                     // return the object-versus-number result

    emitter.label("__rt_mle_object_vs_number_left");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the left runtime tag
    emitter.instruction("cmp x9, #0");                                          // is the left operand an int?
    emitter.instruction("b.eq __rt_mle_object_one_left");                       // compare the number against 1
    emitter.instruction("cmp x9, #2");                                          // is the left operand a float?
    emitter.instruction("b.ne __rt_mle_false");                                 // an object never equals a string or resource
    emitter.label("__rt_mle_object_one_left");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left boxed operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // read the numeric value of the left operand
    emitter.instruction("fmov d1, #1.0");                                       // PHP converts the object operand to 1
    emitter.instruction("fcmp d0, d1");                                         // compare the number against the converted object
    emitter.instruction("cset x0, eq");                                         // materialize the number-versus-object result
    emitter.instruction("b __rt_mle_done");                                     // return the number-versus-object result

    // -- string comparisons --
    emitter.label("__rt_mle_strings");
    emitter.instruction("ldp x1, x2, [sp, #32]");                               // reload the left string pointer and length
    emitter.instruction("ldp x3, x4, [sp, #56]");                               // reload the right string pointer and length
    emitter.instruction("bl __rt_str_loose_eq");                                // apply PHP's numeric-string string equality
    emitter.instruction("b __rt_mle_done");                                     // return the string comparison result

    emitter.label("__rt_mle_left_string_number");
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the right runtime tag
    emitter.instruction("cmp x10, #0");                                         // is the right operand an int?
    emitter.instruction("b.eq __rt_mle_left_string_parse");                     // parse the string for a numeric comparison
    emitter.instruction("cmp x10, #2");                                         // is the right operand a float?
    emitter.instruction("b.ne __rt_mle_false");                                 // strings never equal resources or callables
    emitter.label("__rt_mle_left_string_parse");
    emitter.instruction("ldp x1, x2, [sp, #32]");                               // reload the left string pointer and length
    emitter.instruction("bl __rt_str_to_number");                               // parse the left string under PHP numeric rules
    emitter.instruction("cbz x0, __rt_mle_false");                              // a non-numeric string never equals a number
    emitter.instruction("str d0, [sp, #72]");                                   // save the parsed left numeric value
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // read the numeric value of the right operand
    emitter.instruction("ldr d1, [sp, #72]");                                   // reload the parsed left numeric value
    emitter.instruction("fcmp d1, d0");                                         // compare the parsed string against the number
    emitter.instruction("cset x0, eq");                                         // materialize the string-versus-number result
    emitter.instruction("b __rt_mle_done");                                     // return the string-versus-number result

    emitter.label("__rt_mle_right_string_number");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the left runtime tag
    emitter.instruction("cmp x9, #0");                                          // is the left operand an int?
    emitter.instruction("b.eq __rt_mle_right_string_parse");                    // parse the string for a numeric comparison
    emitter.instruction("cmp x9, #2");                                          // is the left operand a float?
    emitter.instruction("b.ne __rt_mle_false");                                 // strings never equal resources or callables
    emitter.label("__rt_mle_right_string_parse");
    emitter.instruction("ldp x1, x2, [sp, #56]");                               // reload the right string pointer and length
    emitter.instruction("bl __rt_str_to_number");                               // parse the right string under PHP numeric rules
    emitter.instruction("cbz x0, __rt_mle_false");                              // a non-numeric string never equals a number
    emitter.instruction("str d0, [sp, #72]");                                   // save the parsed right numeric value
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left boxed operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // read the numeric value of the left operand
    emitter.instruction("ldr d1, [sp, #72]");                                   // reload the parsed right numeric value
    emitter.instruction("fcmp d0, d1");                                         // compare the number against the parsed string
    emitter.instruction("cset x0, eq");                                         // materialize the number-versus-string result
    emitter.instruction("b __rt_mle_done");                                     // return the number-versus-string result

    // -- numeric and identity fallbacks --
    emitter.label("__rt_mle_numeric");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the left runtime tag
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the right runtime tag
    emitter.instruction("cmp x9, x10");                                         // do both operands carry the same runtime tag?
    emitter.instruction("b.ne __rt_mle_numeric_float");                         // mixed int/float pairs promote to double
    emitter.instruction("cmp x9, #2");                                          // is the shared tag float?
    emitter.instruction("b.eq __rt_mle_numeric_float");                         // floats need signed-zero and NaN semantics
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the left payload word
    emitter.instruction("ldr x12, [sp, #56]");                                  // reload the right payload word
    emitter.instruction("cmp x11, x12");                                        // compare int/resource/callable payloads exactly
    emitter.instruction("cset x0, eq");                                         // materialize the same-tag payload equality
    emitter.instruction("b __rt_mle_done");                                     // return the exact payload comparison

    emitter.label("__rt_mle_numeric_float");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left boxed operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // read the numeric value of the left operand
    emitter.instruction("str d0, [sp, #72]");                                   // save the left numeric operand
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // read the numeric value of the right operand
    emitter.instruction("ldr d1, [sp, #72]");                                   // reload the left numeric operand
    emitter.instruction("fcmp d1, d0");                                         // compare both operands as doubles
    emitter.instruction("cset x0, eq");                                         // NaN stays unordered and therefore unequal
    emitter.instruction("b __rt_mle_done");                                     // return the numeric comparison result

    emitter.label("__rt_mle_true");
    emitter.instruction("mov x0, #1");                                          // report loose equality
    emitter.instruction("b __rt_mle_done");                                     // return the true result

    emitter.label("__rt_mle_false");
    emitter.instruction("mov x0, #0");                                          // report loose inequality

    emitter.label("__rt_mle_done");
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the comparison frame
    emitter.instruction("ret");                                                 // return the loose-equality boolean
}

/// Emits the x86_64 loose-equality dispatcher.
///
/// Frame (80 bytes below `rbp`): `[rbp-8]` left cell, `[rbp-16]` right cell,
/// `[rbp-24]` depth, `[rbp-32..-48]` left tag/lo/hi, `[rbp-56..-72]` right
/// tag/lo/hi, `[rbp-80]` a scalar scratch slot. The `push rbp` plus the 80-byte
/// reservation keep `rsp` 16-byte aligned for the nested libc-backed calls.
fn emit_mixed_loose_eq_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_loose_eq ---");
    emitter.label_global("__rt_mixed_loose_eq");
    emitter.instruction("xor edx, edx");                                        // every public comparison starts at recursion depth zero
    emitter.instruction("jmp __rt_mixed_loose_eq_d");                           // share the depth-carrying comparison body

    emitter.label_global("__rt_mixed_loose_eq_d");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the comparison frame pointer
    emitter.instruction("sub rsp, 80");                                         // allocate the aligned loose-equality comparison frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left boxed operand
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the right boxed operand
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the current recursion depth
    abi::emit_load_int_immediate(emitter, "r10", MAX_LOOSE_EQ_DEPTH);
    emitter.instruction("cmp rdx, r10");                                        // has the walk reached the cyclic-structure cap?
    emitter.instruction("jge __rt_mle_false");                                  // stop instead of recursing into a cycle forever

    // -- unbox both operands into concrete tag/payload triples --
    emitter.instruction("mov rax, rdi");                                        // move the left cell into the unbox input register
    abi::emit_call_label(emitter, "__rt_mixed_unbox"); // left cell -> rax=tag, rdi=lo, rdx=hi
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the left runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save the left payload low word
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the left payload high word
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the right cell for unboxing
    abi::emit_call_label(emitter, "__rt_mixed_unbox"); // right cell -> rax=tag, rdi=lo, rdx=hi
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the right runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                       // save the right payload low word
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // save the right payload high word

    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the left runtime tag for dispatch
    emitter.instruction("mov r11, rax");                                        // keep the right runtime tag in a scratch register

    // -- internal object comparison overrides the ordinary object conversion rules --
    emitter.instruction("cmp r10, 6");                                          // can the left operand select an internal class adapter?
    emitter.instruction("je __rt_mle_internal_object");                         // ask the program-specific typed dispatcher
    emitter.instruction("cmp r11, 6");                                          // can the right operand select an internal class adapter?
    emitter.instruction("jne __rt_mle_standard");                               // scalar pairs keep the allocation-free comparison table
    emitter.label("__rt_mle_internal_object");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // borrow the original left boxed operand
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // borrow the original right boxed operand
    abi::emit_call_label(emitter, &crate::names::function_symbol("__elephc_internal_object_compare"));
    emitter.instruction("cmp rax, 3");                                          // three means neither class provided an adapter
    emitter.instruction("je __rt_mle_standard");                                // ordinary objects retain their existing PHP rules
    emitter.instruction("test rax, rax");                                       // only an ordered equal result represents loose equality
    emitter.instruction("sete al");                                             // unordered and unequal adapter results are false
    emitter.instruction("movzx rax, al");                                       // widen the equality result to a PHP boolean
    emitter.instruction("jmp __rt_mle_done");                                   // return the internal class comparison
    emitter.label("__rt_mle_standard");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // restore the left tag after a possible adapter call
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // restore the right tag after a possible adapter call

    // -- PHP rule 1: a bool operand converts BOTH sides to bool --
    emitter.instruction("cmp r10, 3");                                          // is the left operand a bool?
    emitter.instruction("je __rt_mle_bool");                                    // bool comparisons use truthiness on both sides
    emitter.instruction("cmp r11, 3");                                          // is the right operand a bool?
    emitter.instruction("je __rt_mle_bool");                                    // bool comparisons use truthiness on both sides

    // -- PHP rule 2: null converts to "" against a string and to bool otherwise --
    emitter.instruction("cmp r10, 8");                                          // is the left operand PHP null?
    emitter.instruction("je __rt_mle_left_null");                               // null has its own conversion rules
    emitter.instruction("cmp r11, 8");                                          // is the right operand PHP null?
    emitter.instruction("je __rt_mle_right_null");                              // null has its own conversion rules

    // -- PHP rule 3: an array is loosely equal only to another array --
    emitter.instruction("lea rax, [r10 - 4]");                                  // normalize the left tag against the array range
    emitter.instruction("cmp rax, 1");                                          // tags 4 and 5 are indexed arrays and hashes
    emitter.instruction("setbe al");                                            // record whether the left operand is array-like
    emitter.instruction("movzx rax, al");                                       // widen the left array-like predicate
    emitter.instruction("lea rcx, [r11 - 4]");                                  // normalize the right tag against the array range
    emitter.instruction("cmp rcx, 1");                                          // tags 4 and 5 are indexed arrays and hashes
    emitter.instruction("setbe cl");                                            // record whether the right operand is array-like
    emitter.instruction("movzx rcx, cl");                                       // widen the right array-like predicate
    emitter.instruction("mov rsi, rax");                                        // copy the left predicate for the combined tests
    emitter.instruction("and rsi, rcx");                                        // are both operands array-like?
    emitter.instruction("jnz __rt_mle_arrays");                                 // two arrays compare structurally
    emitter.instruction("or rax, rcx");                                         // is exactly one operand array-like?
    emitter.instruction("jnz __rt_mle_false");                                  // an array never equals a non-array here

    // -- PHP rule 4: objects compare by class then by properties --
    emitter.instruction("cmp r10, 6");                                          // is the left operand an object?
    emitter.instruction("jne __rt_mle_left_not_object");                        // fall through to the string/number rules
    emitter.instruction("cmp r11, 6");                                          // is the right operand also an object?
    emitter.instruction("je __rt_mle_objects");                                 // two objects compare class-then-property
    emitter.instruction("jmp __rt_mle_object_vs_number_right");                 // an object only equals the number 1
    emitter.label("__rt_mle_left_not_object");
    emitter.instruction("cmp r11, 6");                                          // is the right operand an object?
    emitter.instruction("je __rt_mle_object_vs_number_left");                   // an object only equals the number 1

    // -- PHP rule 5: strings compare with numeric-string promotion --
    emitter.instruction("cmp r10, 1");                                          // is the left operand a string?
    emitter.instruction("jne __rt_mle_left_not_string");                        // only the right operand can still be a string
    emitter.instruction("cmp r11, 1");                                          // is the right operand also a string?
    emitter.instruction("je __rt_mle_strings");                                 // two strings use PHP loose string equality
    emitter.instruction("jmp __rt_mle_left_string_number");                     // a string vs a number parses the string
    emitter.label("__rt_mle_left_not_string");
    emitter.instruction("cmp r11, 1");                                          // is the right operand a string?
    emitter.instruction("je __rt_mle_right_string_number");                     // a number vs a string parses the string
    emitter.instruction("jmp __rt_mle_numeric");                                // everything left over compares numerically

    // -- bool coercion of both operands --
    emitter.label("__rt_mle_bool");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the left boxed operand
    abi::emit_call_label(emitter, "__rt_mixed_cast_bool"); // PHP truthiness of the left operand
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the left truthiness result
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the right boxed operand
    abi::emit_call_label(emitter, "__rt_mixed_cast_bool"); // PHP truthiness of the right operand
    emitter.instruction("cmp QWORD PTR [rbp - 80], rax");                       // compare the two truthiness values
    emitter.instruction("sete al");                                             // materialize the bool-coerced equality
    emitter.instruction("movzx rax, al");                                       // widen the boolean byte into the result register
    emitter.instruction("jmp __rt_mle_done");                                   // return the bool-coerced result

    // -- null on the left --
    emitter.label("__rt_mle_left_null");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the right runtime tag
    emitter.instruction("cmp r11, 8");                                          // is the right operand also null?
    emitter.instruction("je __rt_mle_true");                                    // null is loosely equal to null
    emitter.instruction("cmp r11, 1");                                          // is the right operand a string?
    emitter.instruction("jne __rt_mle_null_vs_right_bool");                     // non-string operands coerce to bool
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                         // null converts to "" and equals only the empty string
    emitter.instruction("sete al");                                             // materialize the null-versus-string result
    emitter.instruction("movzx rax, al");                                       // widen the boolean byte into the result register
    emitter.instruction("jmp __rt_mle_done");                                   // return the null-versus-string result
    emitter.label("__rt_mle_null_vs_right_bool");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the right boxed operand
    abi::emit_call_label(emitter, "__rt_mixed_cast_bool"); // PHP truthiness of the right operand
    emitter.instruction("xor rax, 1");                                          // null equals exactly the falsy values
    emitter.instruction("jmp __rt_mle_done");                                   // return the null-coerced result

    // -- null on the right --
    emitter.label("__rt_mle_right_null");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the left runtime tag
    emitter.instruction("cmp r10, 1");                                          // is the left operand a string?
    emitter.instruction("jne __rt_mle_null_vs_left_bool");                      // non-string operands coerce to bool
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // null converts to "" and equals only the empty string
    emitter.instruction("sete al");                                             // materialize the string-versus-null result
    emitter.instruction("movzx rax, al");                                       // widen the boolean byte into the result register
    emitter.instruction("jmp __rt_mle_done");                                   // return the string-versus-null result
    emitter.label("__rt_mle_null_vs_left_bool");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the left boxed operand
    abi::emit_call_label(emitter, "__rt_mixed_cast_bool"); // PHP truthiness of the left operand
    emitter.instruction("xor rax, 1");                                          // null equals exactly the falsy values
    emitter.instruction("jmp __rt_mle_done");                                   // return the null-coerced result

    // -- container and object recursion --
    emitter.label("__rt_mle_arrays");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left boxed array operand
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the right boxed array operand
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the current recursion depth
    emitter.instruction("add rdx, 1");                                          // descend one level for the element walk
    abi::emit_call_label(emitter, "__rt_mixed_array_loose_eq"); // compare the two arrays entry by entry
    emitter.instruction("jmp __rt_mle_done");                                   // return the structural array result

    emitter.label("__rt_mle_objects");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the left object pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // reload the right object pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the current recursion depth
    emitter.instruction("add rdx, 1");                                          // descend one level for the property walk
    abi::emit_call_label(emitter, "__rt_obj_loose_eq"); // compare class identity then properties
    emitter.instruction("jmp __rt_mle_done");                                   // return the object comparison result

    // -- PHP converts an object to the integer 1 when compared with a number --
    emitter.label("__rt_mle_object_vs_number_right");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the right runtime tag
    emitter.instruction("cmp r11, 0");                                          // is the right operand an int?
    emitter.instruction("je __rt_mle_object_one_right");                        // compare the number against 1
    emitter.instruction("cmp r11, 2");                                          // is the right operand a float?
    emitter.instruction("jne __rt_mle_false");                                  // an object never equals a string or resource
    emitter.label("__rt_mle_object_one_right");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the right boxed operand
    abi::emit_call_label(emitter, "__rt_mixed_cast_float"); // read the numeric value of the right operand
    emitter.instruction("jmp __rt_mle_compare_one");                            // compare the number against the converted object

    emitter.label("__rt_mle_object_vs_number_left");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the left runtime tag
    emitter.instruction("cmp r10, 0");                                          // is the left operand an int?
    emitter.instruction("je __rt_mle_object_one_left");                         // compare the number against 1
    emitter.instruction("cmp r10, 2");                                          // is the left operand a float?
    emitter.instruction("jne __rt_mle_false");                                  // an object never equals a string or resource
    emitter.label("__rt_mle_object_one_left");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the left boxed operand
    abi::emit_call_label(emitter, "__rt_mixed_cast_float"); // read the numeric value of the left operand

    emitter.label("__rt_mle_compare_one");
    emitter.instruction("movabs rax, 0x3ff0000000000000");                      // materialize the double bit pattern for 1.0
    emitter.instruction("movq xmm1, rax");                                      // PHP converts the object operand to 1
    emitter.instruction("ucomisd xmm0, xmm1");                                  // compare the number against the converted object
    emitter.instruction("sete al");                                             // equality requires matching numeric values
    emitter.instruction("setnp cl");                                            // an unordered NaN comparison is never equal
    emitter.instruction("and al, cl");                                          // combine the ordered and equal predicates
    emitter.instruction("movzx rax, al");                                       // widen the boolean byte into the result register
    emitter.instruction("jmp __rt_mle_done");                                   // return the object-versus-number result

    // -- string comparisons --
    emitter.label("__rt_mle_strings");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the left string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // reload the left string length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // reload the right string pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 72]");                       // reload the right string length
    abi::emit_call_label(emitter, "__rt_str_loose_eq"); // apply PHP's numeric-string string equality
    emitter.instruction("jmp __rt_mle_done");                                   // return the string comparison result

    emitter.label("__rt_mle_left_string_number");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the right runtime tag
    emitter.instruction("cmp r11, 0");                                          // is the right operand an int?
    emitter.instruction("je __rt_mle_left_string_parse");                       // parse the string for a numeric comparison
    emitter.instruction("cmp r11, 2");                                          // is the right operand a float?
    emitter.instruction("jne __rt_mle_false");                                  // strings never equal resources or callables
    emitter.label("__rt_mle_left_string_parse");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the left string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reload the left string length
    abi::emit_call_label(emitter, "__rt_str_to_number"); // parse the left string under PHP numeric rules
    emitter.instruction("test rax, rax");                                       // did the left string parse as fully numeric?
    emitter.instruction("jz __rt_mle_false");                                   // a non-numeric string never equals a number
    emitter.instruction("movsd QWORD PTR [rbp - 80], xmm0");                    // save the parsed left numeric value
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the right boxed operand
    abi::emit_call_label(emitter, "__rt_mixed_cast_float"); // read the numeric value of the right operand
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 80]");                    // reload the parsed left numeric value
    emitter.instruction("jmp __rt_mle_compare_doubles");                        // compare the parsed string against the number

    emitter.label("__rt_mle_right_string_number");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the left runtime tag
    emitter.instruction("cmp r10, 0");                                          // is the left operand an int?
    emitter.instruction("je __rt_mle_right_string_parse");                      // parse the string for a numeric comparison
    emitter.instruction("cmp r10, 2");                                          // is the left operand a float?
    emitter.instruction("jne __rt_mle_false");                                  // strings never equal resources or callables
    emitter.label("__rt_mle_right_string_parse");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the right string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // reload the right string length
    abi::emit_call_label(emitter, "__rt_str_to_number"); // parse the right string under PHP numeric rules
    emitter.instruction("test rax, rax");                                       // did the right string parse as fully numeric?
    emitter.instruction("jz __rt_mle_false");                                   // a non-numeric string never equals a number
    emitter.instruction("movsd QWORD PTR [rbp - 80], xmm0");                    // save the parsed right numeric value
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the left boxed operand
    abi::emit_call_label(emitter, "__rt_mixed_cast_float"); // read the numeric value of the left operand
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 80]");                    // reload the parsed right numeric value
    emitter.instruction("jmp __rt_mle_compare_doubles");                        // compare the number against the parsed string

    // -- numeric and identity fallbacks --
    emitter.label("__rt_mle_numeric");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the left runtime tag
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the right runtime tag
    emitter.instruction("cmp r10, r11");                                        // do both operands carry the same runtime tag?
    emitter.instruction("jne __rt_mle_numeric_float");                          // mixed int/float pairs promote to double
    emitter.instruction("cmp r10, 2");                                          // is the shared tag float?
    emitter.instruction("je __rt_mle_numeric_float");                           // floats need signed-zero and NaN semantics
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the left payload word
    emitter.instruction("cmp rax, QWORD PTR [rbp - 64]");                       // compare int/resource/callable payloads exactly
    emitter.instruction("sete al");                                             // materialize the same-tag payload equality
    emitter.instruction("movzx rax, al");                                       // widen the boolean byte into the result register
    emitter.instruction("jmp __rt_mle_done");                                   // return the exact payload comparison

    emitter.label("__rt_mle_numeric_float");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the left boxed operand
    abi::emit_call_label(emitter, "__rt_mixed_cast_float"); // read the numeric value of the left operand
    emitter.instruction("movsd QWORD PTR [rbp - 80], xmm0");                    // save the left numeric operand
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the right boxed operand
    abi::emit_call_label(emitter, "__rt_mixed_cast_float"); // read the numeric value of the right operand
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 80]");                    // reload the left numeric operand

    emitter.label("__rt_mle_compare_doubles");
    emitter.instruction("ucomisd xmm1, xmm0");                                  // compare both operands as doubles
    emitter.instruction("sete al");                                             // equality requires matching numeric values
    emitter.instruction("setnp cl");                                            // an unordered NaN comparison is never equal
    emitter.instruction("and al, cl");                                          // combine the ordered and equal predicates
    emitter.instruction("movzx rax, al");                                       // widen the boolean byte into the result register
    emitter.instruction("jmp __rt_mle_done");                                   // return the numeric comparison result

    emitter.label("__rt_mle_true");
    emitter.instruction("mov rax, 1");                                          // report loose equality
    emitter.instruction("jmp __rt_mle_done");                                   // return the true result

    emitter.label("__rt_mle_false");
    emitter.instruction("xor rax, rax");                                        // report loose inequality

    emitter.label("__rt_mle_done");
    emitter.instruction("add rsp, 80");                                         // release the comparison frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the loose-equality boolean
}
