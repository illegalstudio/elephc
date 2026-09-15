//! Purpose:
//! Emits `__rt_php_compare`, the runtime implementation of PHP 8's *ordering*
//! comparison (`zend_compare`, the engine routine behind `<`, `>` and `<=>`) for two
//! unboxed runtime value triples, plus the `__rt_php_truthy` helper it needs for the
//! bool/null coercion rule.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::compare`.
//! - `__rt_min_max_mixed` / `__rt_min_max_str` / `__rt_min_max_hash`
//!   (`crate::codegen_support::runtime::arrays::min_max_container`).
//!
//! Key details:
//! - Operands are `(tag, lo, hi)` triples, not boxed cells: tag 0 = int (`lo`),
//!   1 = string (`lo` = bytes, `hi` = length), 2 = float (`lo` = raw `f64` bits),
//!   3 = bool (`lo`), 8 = null. Callers must peel boxed `Mixed` cells with
//!   `__rt_mixed_unbox` first.
//! - PHP 8 rule order, which is observable: a `bool` on either side coerces BOTH
//!   sides to bool; then `null` (against a string it becomes `""` and a *string*
//!   comparison happens, so `null < "0"` but `null == ""`); then two strings use
//!   numeric-string promotion; then a number against a string parses the string and,
//!   when that fails, compares the number's *string form* byte-wise (`0 < "a"`).
//! - Number-to-string conversion goes through `__rt_itoa` / `__rt_ftoa`, which append
//!   to the shared `_concat_buf` scratch. The cursor is saved before and restored
//!   after the comparison, so a reduction loop cannot exhaust the buffer.
//! - The primary result remains the spaceship ordering value. A secondary flag reports
//!   unordered IEEE comparisons so relational consumers can return `false` for every NaN
//!   predicate without changing spaceship's PHP result of `1`.
//! - Known deviations: comparisons that involve a numeric *string* are resolved as
//!   `double`s, so two integer strings beyond 2^53 can compare equal where PHP
//!   compares them exactly (the same simplification `__rt_mixed_loose_eq` already
//!   makes); arrays, objects, resources and callables only rank above the scalar
//!   tags and compare equal to each other.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_php_compare` and `__rt_php_truthy` for the active target.
///
/// `__rt_php_compare` inputs are two runtime value triples — AArch64
/// `x0`/`x1`/`x2` and `x3`/`x4`/`x5`, x86_64 `rdi`/`rsi`/`rdx` and
/// `rcx`/`r8`/`r9` — and the result is `-1`, `0` or `1` in `x0`/`rax`.
/// `x1`/`rdx` returns 1 when the comparison encountered unordered NaN and 0 otherwise.
/// String payloads stay borrowed: the helper never releases or persists them.
pub fn emit_php_compare(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_php_truthy_x86_64(emitter);
        emit_php_compare_x86_64(emitter);
        return;
    }
    emit_php_truthy_aarch64(emitter);
    emit_php_compare_aarch64(emitter);
}

/// Emits the AArch64 PHP truthiness helper over one runtime value triple.
///
/// Input `x0` = tag, `x1` = low payload word, `x2` = high payload word; output
/// `x0` = 0 or 1. Leaf routine: it never calls out, so callers only need `x30`
/// saved once. Container, object, resource and callable tags report `true`.
fn emit_php_truthy_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: php_truthy ---");
    emitter.label_global("__rt_php_truthy");

    emitter.instruction("cmp x0, #8");                                          // is the operand PHP null?
    emitter.instruction("b.eq __rt_pt_false");                                  // null is the only always-falsy tag
    emitter.instruction("cmp x0, #2");                                          // is the operand a float?
    emitter.instruction("b.eq __rt_pt_float");                                  // floats need a numeric zero test
    emitter.instruction("cmp x0, #1");                                          // is the operand a string?
    emitter.instruction("b.eq __rt_pt_string");                                 // strings use PHP's "" / "0" rule
    emitter.instruction("cmp x0, #0");                                          // is the operand an int?
    emitter.instruction("b.eq __rt_pt_word");                                   // ints are falsy only at zero
    emitter.instruction("cmp x0, #3");                                          // is the operand a bool?
    emitter.instruction("b.eq __rt_pt_word");                                   // bools carry their truth value in the low word
    emitter.instruction("b __rt_pt_true");                                      // arrays, objects, resources and callables report true here

    emitter.label("__rt_pt_word");
    emitter.instruction("cmp x1, #0");                                          // compare the integer-like payload against zero
    emitter.instruction("cset x0, ne");                                         // any non-zero integer-like payload is truthy
    emitter.instruction("ret");                                                 // return the truthiness flag

    emitter.label("__rt_pt_float");
    emitter.instruction("fmov d0, x1");                                         // reinterpret the payload word as the double it encodes
    emitter.instruction("fcmp d0, #0.0");                                       // compare the double against positive zero
    emitter.instruction("cset x0, ne");                                         // NaN stays unordered and therefore truthy, like PHP
    emitter.instruction("ret");                                                 // return the truthiness flag

    emitter.label("__rt_pt_string");
    emitter.instruction("cbz x2, __rt_pt_false");                               // the empty string is falsy
    emitter.instruction("cmp x2, #1");                                          // only a one-byte string can be the falsy "0"
    emitter.instruction("b.ne __rt_pt_true");                                   // every other non-empty string is truthy
    emitter.instruction("ldrb w9, [x1]");                                       // load the single byte of a one-character string
    emitter.instruction("cmp w9, #48");                                         // is that byte the ASCII digit zero?
    emitter.instruction("b.eq __rt_pt_false");                                  // "0" is PHP's other falsy string

    emitter.label("__rt_pt_true");
    emitter.instruction("mov x0, #1");                                          // report a truthy operand
    emitter.instruction("ret");                                                 // return the truthiness flag

    emitter.label("__rt_pt_false");
    emitter.instruction("mov x0, #0");                                          // report a falsy operand
    emitter.instruction("ret");                                                 // return the truthiness flag
}

/// Emits the AArch64 PHP ordering comparison over two runtime value triples.
///
/// Frame (128 bytes): `[sp,#0..#16]` left tag/lo/hi, `[sp,#24..#40]` right
/// tag/lo/hi, `[sp,#48]` a parsed-double scratch slot, `[sp,#56]` the
/// "operands were swapped" flag used by the number-versus-string leg,
/// `[sp,#64..#88]` the normalized number/string operands of that leg,
/// `[sp,#96]` the saved `_concat_off` cursor, `[sp,#104]` the unordered flag,
/// and `[sp,#112]` saved `x29`/`x30`.
fn emit_php_compare_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: php_compare ---");
    emitter.label_global("__rt_php_compare");

    emitter.instruction("sub sp, sp, #128");                                    // allocate the ordering-comparison frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish the comparison frame pointer
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save the left runtime tag and low payload word
    emitter.instruction("str x2, [sp, #16]");                                   // save the left high payload word
    emitter.instruction("stp x3, x4, [sp, #24]");                               // save the right runtime tag and low payload word
    emitter.instruction("str x5, [sp, #40]");                                   // save the right high payload word
    emitter.instruction("str xzr, [sp, #104]");                                 // initialize the unordered IEEE result flag

    // -- PHP rule 1: a bool operand converts BOTH sides to bool --
    emitter.instruction("cmp x0, #3");                                          // is the left operand a bool?
    emitter.instruction("b.eq __rt_pcmp_bools");                                // bool comparisons use truthiness on both sides
    emitter.instruction("cmp x3, #3");                                          // is the right operand a bool?
    emitter.instruction("b.eq __rt_pcmp_bools");                                // bool comparisons use truthiness on both sides

    // -- PHP rule 2: null becomes "" against a string and bool against everything else --
    emitter.instruction("cmp x0, #8");                                          // is the left operand PHP null?
    emitter.instruction("b.eq __rt_pcmp_left_null");                            // null has its own conversion rules
    emitter.instruction("cmp x3, #8");                                          // is the right operand PHP null?
    emitter.instruction("b.eq __rt_pcmp_right_null");                           // null has its own conversion rules
    emitter.instruction("b __rt_pcmp_no_null");                                 // neither operand needs the bool/null coercions

    // -- bool coercion of both operands --
    emitter.label("__rt_pcmp_bools");
    emitter.instruction("bl __rt_php_truthy");                                  // PHP truthiness of the left operand
    emitter.instruction("str x0, [sp, #48]");                                   // save the left truthiness while the right one is computed
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the right runtime tag
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload the right low payload word
    emitter.instruction("ldr x2, [sp, #40]");                                   // reload the right high payload word
    emitter.instruction("bl __rt_php_truthy");                                  // PHP truthiness of the right operand
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the left truthiness value
    emitter.instruction("cmp x9, x0");                                          // false sorts below true in PHP's bool comparison
    emitter.instruction("b.lt __rt_pcmp_neg");                                  // false versus true is "less than"
    emitter.instruction("b.gt __rt_pcmp_pos");                                  // true versus false is "greater than"
    emitter.instruction("b __rt_pcmp_zero");                                    // equal truthiness compares equal

    // -- null on the left --
    emitter.label("__rt_pcmp_left_null");
    emitter.instruction("cmp x3, #8");                                          // is the right operand also null?
    emitter.instruction("b.eq __rt_pcmp_zero");                                 // null compares equal to null
    emitter.instruction("cmp x3, #1");                                          // is the right operand a string?
    emitter.instruction("b.ne __rt_pcmp_null_vs_right");                        // non-string operands coerce to bool
    emitter.instruction("cbz x5, __rt_pcmp_zero");                              // null converts to "" and equals the empty string
    emitter.instruction("b __rt_pcmp_neg");                                     // "" sorts below every non-empty string
    emitter.label("__rt_pcmp_null_vs_right");
    emitter.instruction("mov x0, x3");                                          // pass the right runtime tag to the truthiness helper
    emitter.instruction("mov x1, x4");                                          // pass the right low payload word
    emitter.instruction("mov x2, x5");                                          // pass the right high payload word
    emitter.instruction("bl __rt_php_truthy");                                  // PHP truthiness of the right operand
    emitter.instruction("cbz x0, __rt_pcmp_zero");                              // null equals every falsy operand
    emitter.instruction("b __rt_pcmp_neg");                                     // null sorts below every truthy operand

    // -- null on the right --
    emitter.label("__rt_pcmp_right_null");
    emitter.instruction("cmp x0, #1");                                          // is the left operand a string?
    emitter.instruction("b.ne __rt_pcmp_left_vs_null");                         // non-string operands coerce to bool
    emitter.instruction("cbz x2, __rt_pcmp_zero");                              // the empty string equals null
    emitter.instruction("b __rt_pcmp_pos");                                     // every non-empty string sorts above ""
    emitter.label("__rt_pcmp_left_vs_null");
    emitter.instruction("bl __rt_php_truthy");                                  // PHP truthiness of the left operand
    emitter.instruction("cbz x0, __rt_pcmp_zero");                              // every falsy operand equals null
    emitter.instruction("b __rt_pcmp_pos");                                     // every truthy operand sorts above null

    // -- neither operand is bool or null --
    emitter.label("__rt_pcmp_no_null");
    emitter.instruction("cmp x0, #2");                                          // tags 0, 1 and 2 are the comparable scalar payloads
    emitter.instruction("cset x9, ls");                                         // record whether the left operand is a scalar
    emitter.instruction("cmp x3, #2");                                          // tags 0, 1 and 2 are the comparable scalar payloads
    emitter.instruction("cset x10, ls");                                        // record whether the right operand is a scalar
    emitter.instruction("and x11, x9, x10");                                    // are both operands comparable scalars?
    emitter.instruction("cbz x11, __rt_pcmp_nonscalar");                        // containers and objects use the coarse ranking below
    emitter.instruction("cmp x0, #1");                                          // is the left operand a string?
    emitter.instruction("b.ne __rt_pcmp_left_number");                          // only the right operand can still be a string
    emitter.instruction("cmp x3, #1");                                          // is the right operand also a string?
    emitter.instruction("b.eq __rt_pcmp_strings");                              // two strings use PHP's numeric-string promotion
    emitter.instruction("b __rt_pcmp_str_vs_num");                              // a string against a number parses the string
    emitter.label("__rt_pcmp_left_number");
    emitter.instruction("cmp x3, #1");                                          // is the right operand a string?
    emitter.instruction("b.eq __rt_pcmp_num_vs_str");                           // a number against a string parses the string
    emitter.instruction("cmp x0, #0");                                          // is the left operand an int?
    emitter.instruction("b.ne __rt_pcmp_numeric");                              // any float operand promotes both sides to double
    emitter.instruction("cmp x3, #0");                                          // is the right operand an int?
    emitter.instruction("b.ne __rt_pcmp_numeric");                              // any float operand promotes both sides to double
    emitter.instruction("cmp x1, x4");                                          // compare two ints exactly, without losing precision
    emitter.instruction("b.lt __rt_pcmp_neg");                                  // the left int is smaller
    emitter.instruction("b.gt __rt_pcmp_pos");                                  // the left int is larger
    emitter.instruction("b __rt_pcmp_zero");                                    // both ints are equal

    // -- containers, objects, resources and callables --
    emitter.label("__rt_pcmp_nonscalar");
    emitter.instruction("orr x11, x9, x10");                                    // is exactly one operand a comparable scalar?
    emitter.instruction("cbz x11, __rt_pcmp_zero");                             // two non-scalars are reported equal (documented limitation)
    emitter.instruction("cbz x9, __rt_pcmp_pos");                               // a non-scalar left operand ranks above a scalar
    emitter.instruction("b __rt_pcmp_neg");                                     // a non-scalar right operand ranks above a scalar

    // -- both operands are numbers: promote to double --
    emitter.label("__rt_pcmp_numeric");
    emitter.instruction("cmp x0, #2");                                          // is the left operand already a double?
    emitter.instruction("b.eq __rt_pcmp_numeric_left_double");                  // reinterpret its payload word
    emitter.instruction("scvtf d0, x1");                                        // widen the left int payload into a double
    emitter.instruction("b __rt_pcmp_numeric_right");                           // continue with the right operand
    emitter.label("__rt_pcmp_numeric_left_double");
    emitter.instruction("fmov d0, x1");                                         // reinterpret the left payload word as a double
    emitter.label("__rt_pcmp_numeric_right");
    emitter.instruction("cmp x3, #2");                                          // is the right operand already a double?
    emitter.instruction("b.eq __rt_pcmp_numeric_right_double");                 // reinterpret its payload word
    emitter.instruction("scvtf d1, x4");                                        // widen the right int payload into a double
    emitter.instruction("b __rt_pcmp_fcmp");                                    // compare both operands as doubles
    emitter.label("__rt_pcmp_numeric_right_double");
    emitter.instruction("fmov d1, x4");                                         // reinterpret the right payload word as a double

    emitter.label("__rt_pcmp_fcmp");
    emitter.instruction("fcmp d0, d1");                                         // compare both numeric operands as doubles
    emitter.instruction("b.vs __rt_pcmp_unordered");                            // report unordered NaN separately from signed ordering
    emitter.instruction("b.mi __rt_pcmp_neg");                                  // an ordered less-than result
    emitter.instruction("b.eq __rt_pcmp_zero");                                 // an ordered equal result
    emitter.instruction("b __rt_pcmp_pos");                                     // an ordered greater-than result

    emitter.label("__rt_pcmp_unordered");
    emitter.instruction("mov x9, #1");                                          // mark the comparison as IEEE unordered
    emitter.instruction("str x9, [sp, #104]");                                  // expose unordered state to relational consumers
    emitter.instruction("b __rt_pcmp_pos");                                     // PHP spaceship orders unordered NaN as greater

    // -- string versus string --
    emitter.label("__rt_pcmp_strings");
    emitter.instruction("bl __rt_str_to_number");                               // parse the left string under PHP's numeric-string grammar
    emitter.instruction("cbz x0, __rt_pcmp_str_bytes");                         // a non-numeric operand forces the byte comparison
    emitter.instruction("str d0, [sp, #48]");                                   // save the parsed left value across the second parse
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload the right string pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // reload the right string length
    emitter.instruction("bl __rt_str_to_number");                               // parse the right string under PHP's numeric-string grammar
    emitter.instruction("cbz x0, __rt_pcmp_str_bytes");                         // a non-numeric operand forces the byte comparison
    emitter.instruction("fmov d1, d0");                                         // move the parsed right value into the comparison register
    emitter.instruction("ldr d0, [sp, #48]");                                   // reload the parsed left value
    emitter.instruction("b __rt_pcmp_fcmp");                                    // two numeric strings compare numerically
    emitter.label("__rt_pcmp_str_bytes");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the left string pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the left string length
    emitter.instruction("ldr x3, [sp, #32]");                                   // reload the right string pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the right string length
    emitter.instruction("bl __rt_strcmp");                                      // compare both strings byte-wise, then by length
    emitter.instruction("cmp x0, #0");                                          // normalize the byte difference into a three-way result
    emitter.instruction("b.lt __rt_pcmp_neg");                                  // the left string sorts first
    emitter.instruction("b.gt __rt_pcmp_pos");                                  // the right string sorts first
    emitter.instruction("b __rt_pcmp_zero");                                    // both strings are byte-identical

    // -- number versus string, normalized so the number is always the left operand --
    emitter.label("__rt_pcmp_num_vs_str");
    emitter.instruction("str xzr, [sp, #56]");                                  // the operands are already in number/string order
    emitter.instruction("stp x0, x1, [sp, #64]");                               // stage the number's tag and payload word
    emitter.instruction("stp x4, x5, [sp, #80]");                               // stage the string's pointer and length
    emitter.instruction("b __rt_pcmp_num_str_body");                            // run the shared number-versus-string comparison
    emitter.label("__rt_pcmp_str_vs_num");
    emitter.instruction("mov x9, #1");                                          // the string is the left operand, so the result is negated
    emitter.instruction("str x9, [sp, #56]");                                   // record that the normalized result must be negated
    emitter.instruction("stp x3, x4, [sp, #64]");                               // stage the number's tag and payload word
    emitter.instruction("stp x1, x2, [sp, #80]");                               // stage the string's pointer and length

    emitter.label("__rt_pcmp_num_str_body");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the staged number's runtime tag
    emitter.instruction("cmp x9, #2");                                          // is the numeric operand a double that may be NaN?
    emitter.instruction("b.ne __rt_pcmp_num_str_parse");                        // integers cannot be unordered
    emitter.instruction("ldr x10, [sp, #72]");                                  // reload the staged double payload bits
    emitter.instruction("fmov d0, x10");                                        // reinterpret the payload for the NaN self-comparison
    emitter.instruction("fcmp d0, d0");                                         // NaN is the only double unordered with itself
    emitter.instruction("b.vs __rt_pcmp_num_str_unordered");                    // bypass parsing, formatting, and swap correction for NaN
    emitter.label("__rt_pcmp_num_str_parse");
    emitter.instruction("ldr x1, [sp, #80]");                                   // pass the string pointer to the numeric parser
    emitter.instruction("ldr x2, [sp, #88]");                                   // pass the string length to the numeric parser
    emitter.instruction("bl __rt_str_to_number");                               // parse the string under PHP's numeric-string grammar
    emitter.instruction("cbz x0, __rt_pcmp_num_str_bytes");                     // PHP 8 compares a number with a non-numeric string as strings
    emitter.instruction("str d0, [sp, #48]");                                   // save the parsed string value
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the number's runtime tag
    emitter.instruction("ldr x10, [sp, #72]");                                  // reload the number's payload word
    emitter.instruction("cmp x9, #2");                                          // is the number already a double?
    emitter.instruction("b.eq __rt_pcmp_num_str_double");                       // reinterpret its payload word
    emitter.instruction("scvtf d0, x10");                                       // widen the int payload into a double
    emitter.instruction("b __rt_pcmp_num_str_cmp");                             // compare the number against the parsed string
    emitter.label("__rt_pcmp_num_str_double");
    emitter.instruction("fmov d0, x10");                                        // reinterpret the payload word as a double
    emitter.label("__rt_pcmp_num_str_cmp");
    emitter.instruction("ldr d1, [sp, #48]");                                   // reload the parsed string value
    emitter.instruction("fcmp d0, d1");                                         // compare the number against the numeric string
    emitter.instruction("b.vs __rt_pcmp_num_str_unordered");                    // report unordered NaN separately from signed ordering
    emitter.instruction("b.mi __rt_pcmp_maybe_neg");                            // the number is smaller, before any swap correction
    emitter.instruction("b.eq __rt_pcmp_zero");                                 // both values are numerically equal
    emitter.instruction("b __rt_pcmp_maybe_pos");                               // the number is larger, before any swap correction
    emitter.label("__rt_pcmp_num_str_unordered");
    emitter.instruction("mov x9, #1");                                          // mark the number/string comparison as IEEE unordered
    emitter.instruction("str x9, [sp, #104]");                                  // expose unordered state to relational consumers
    emitter.instruction("b __rt_pcmp_pos");                                     // PHP spaceship returns 1 whenever either operand is NaN

    emitter.label("__rt_pcmp_num_str_bytes");
    crate::codegen_support::runtime::ctx::emit_concat_off_load(emitter, "x10");
    emitter.instruction("str x10, [sp, #96]");                                  // save it so the rendered number cannot leak scratch space
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the number's runtime tag
    emitter.instruction("ldr x10, [sp, #72]");                                  // reload the number's payload word
    emitter.instruction("cmp x9, #2");                                          // is the number a double?
    emitter.instruction("b.eq __rt_pcmp_num_str_ftoa");                         // doubles render through PHP's precision-14 formatter
    emitter.instruction("mov x0, x10");                                         // pass the int payload to the decimal formatter
    emitter.instruction("bl __rt_itoa");                                        // render the int exactly the way PHP casts it to string
    emitter.instruction("b __rt_pcmp_num_str_strcmp");                          // compare the rendered number with the string
    emitter.label("__rt_pcmp_num_str_ftoa");
    emitter.instruction("fmov d0, x10");                                        // reinterpret the payload word as the double to render
    emitter.instruction("bl __rt_ftoa");                                        // render the double at PHP's default precision of 14
    emitter.label("__rt_pcmp_num_str_strcmp");
    emitter.instruction("ldr x3, [sp, #80]");                                   // reload the string pointer for the byte comparison
    emitter.instruction("ldr x4, [sp, #88]");                                   // reload the string length for the byte comparison
    emitter.instruction("bl __rt_strcmp");                                      // compare the rendered number with the string byte-wise
        emitter.instruction("ldr x10, [sp, #96]");                                  // reload the saved concat scratch cursor
    crate::codegen_support::runtime::ctx::emit_concat_off_store(emitter, "x10"); // release the scratch the rendered number occupied (ctx-relative in ctx mode)
    emitter.instruction("cmp x0, #0");                                          // normalize the byte difference into a three-way result
    emitter.instruction("b.lt __rt_pcmp_maybe_neg");                            // the rendered number sorts first, before any swap correction
    emitter.instruction("b.gt __rt_pcmp_maybe_pos");                            // the string sorts first, before any swap correction
    emitter.instruction("b __rt_pcmp_zero");                                    // both byte sequences are identical

    emitter.label("__rt_pcmp_maybe_neg");
    emitter.instruction("ldr x9, [sp, #56]");                                   // was the string the original left operand?
    emitter.instruction("cbz x9, __rt_pcmp_neg");                               // no swap: keep the normalized result
    emitter.instruction("b __rt_pcmp_pos");                                     // swapped operands invert the normalized result
    emitter.label("__rt_pcmp_maybe_pos");
    emitter.instruction("ldr x9, [sp, #56]");                                   // was the string the original left operand?
    emitter.instruction("cbz x9, __rt_pcmp_pos");                               // no swap: keep the normalized result
    emitter.instruction("b __rt_pcmp_neg");                                     // swapped operands invert the normalized result

    emitter.label("__rt_pcmp_neg");
    emitter.instruction("mov x0, #-1");                                         // the left operand sorts before the right one
    emitter.instruction("b __rt_pcmp_done");                                    // return the three-way result
    emitter.label("__rt_pcmp_pos");
    emitter.instruction("mov x0, #1");                                          // the left operand sorts after the right one
    emitter.instruction("b __rt_pcmp_done");                                    // return the three-way result
    emitter.label("__rt_pcmp_zero");
    emitter.instruction("mov x0, #0");                                          // both operands compare equal

    emitter.label("__rt_pcmp_done");
    emitter.instruction("ldr x1, [sp, #104]");                                  // return whether the comparison was IEEE unordered
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the ordering-comparison frame
    emitter.instruction("ret");                                                 // return the three-way comparison result
}

/// Emits the x86_64 PHP truthiness helper over one runtime value triple.
///
/// Input `rdi` = tag, `rsi` = low payload word, `rdx` = high payload word;
/// output `rax` = 0 or 1. Leaf routine, so it needs no frame of its own.
fn emit_php_truthy_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: php_truthy ---");
    emitter.label_global("__rt_php_truthy");

    emitter.instruction("cmp rdi, 8");                                          // is the operand PHP null?
    emitter.instruction("je __rt_pt_false");                                    // null is the only always-falsy tag
    emitter.instruction("cmp rdi, 2");                                          // is the operand a float?
    emitter.instruction("je __rt_pt_float");                                    // floats need a numeric zero test
    emitter.instruction("cmp rdi, 1");                                          // is the operand a string?
    emitter.instruction("je __rt_pt_string");                                   // strings use PHP's "" / "0" rule
    emitter.instruction("cmp rdi, 0");                                          // is the operand an int?
    emitter.instruction("je __rt_pt_word");                                     // ints are falsy only at zero
    emitter.instruction("cmp rdi, 3");                                          // is the operand a bool?
    emitter.instruction("je __rt_pt_word");                                     // bools carry their truth value in the low word
    emitter.instruction("jmp __rt_pt_true");                                    // arrays, objects, resources and callables report true here

    emitter.label("__rt_pt_word");
    emitter.instruction("test rsi, rsi");                                       // compare the integer-like payload against zero
    emitter.instruction("setne al");                                            // any non-zero integer-like payload is truthy
    emitter.instruction("movzx rax, al");                                       // widen the predicate byte into the result register
    emitter.instruction("ret");                                                 // return the truthiness flag

    emitter.label("__rt_pt_float");
    emitter.instruction("movq xmm0, rsi");                                      // reinterpret the payload word as the double it encodes
    emitter.instruction("xorpd xmm1, xmm1");                                    // materialize positive zero for the comparison
    emitter.instruction("ucomisd xmm0, xmm1");                                  // compare the double against positive zero
    emitter.instruction("setne al");                                            // a non-zero double is truthy
    emitter.instruction("setp cl");                                             // an unordered NaN comparison is truthy in PHP too
    emitter.instruction("or al, cl");                                           // combine the non-zero and unordered predicates
    emitter.instruction("movzx rax, al");                                       // widen the predicate byte into the result register
    emitter.instruction("ret");                                                 // return the truthiness flag

    emitter.label("__rt_pt_string");
    emitter.instruction("test rdx, rdx");                                       // does the string have any bytes at all?
    emitter.instruction("jz __rt_pt_false");                                    // the empty string is falsy
    emitter.instruction("cmp rdx, 1");                                          // only a one-byte string can be the falsy "0"
    emitter.instruction("jne __rt_pt_true");                                    // every other non-empty string is truthy
    emitter.instruction("movzx rax, BYTE PTR [rsi]");                           // load the single byte of a one-character string
    emitter.instruction("cmp rax, 48");                                         // is that byte the ASCII digit zero?
    emitter.instruction("je __rt_pt_false");                                    // "0" is PHP's other falsy string

    emitter.label("__rt_pt_true");
    emitter.instruction("mov rax, 1");                                          // report a truthy operand
    emitter.instruction("ret");                                                 // return the truthiness flag

    emitter.label("__rt_pt_false");
    emitter.instruction("xor eax, eax");                                        // report a falsy operand
    emitter.instruction("ret");                                                 // return the truthiness flag
}

/// Emits the x86_64 PHP ordering comparison over two runtime value triples.
///
/// Frame (112 bytes below `rbp`): `[rbp-8..-24]` left tag/lo/hi,
/// `[rbp-32..-48]` right tag/lo/hi, `[rbp-56]` a parsed-double scratch slot,
/// `[rbp-64]` the "operands were swapped" flag, `[rbp-72..-96]` the normalized
/// number/string operands, `[rbp-104]` the saved `_concat_off` cursor, and
/// `[rbp-112]` the unordered flag. The
/// `push rbp` plus the 112-byte reservation keep `rsp` 16-byte aligned for the
/// nested libc-backed calls.
fn emit_php_compare_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: php_compare ---");
    emitter.label_global("__rt_php_compare");

    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the comparison frame pointer
    emitter.instruction("sub rsp, 112");                                        // allocate the aligned ordering-comparison frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the left low payload word
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the left high payload word
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the right runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the right low payload word
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the right high payload word
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                        // initialize the unordered IEEE result flag

    // -- PHP rule 1: a bool operand converts BOTH sides to bool --
    emitter.instruction("cmp rdi, 3");                                          // is the left operand a bool?
    emitter.instruction("je __rt_pcmp_bools");                                  // bool comparisons use truthiness on both sides
    emitter.instruction("cmp rcx, 3");                                          // is the right operand a bool?
    emitter.instruction("je __rt_pcmp_bools");                                  // bool comparisons use truthiness on both sides

    // -- PHP rule 2: null becomes "" against a string and bool against everything else --
    emitter.instruction("cmp rdi, 8");                                          // is the left operand PHP null?
    emitter.instruction("je __rt_pcmp_left_null");                              // null has its own conversion rules
    emitter.instruction("cmp rcx, 8");                                          // is the right operand PHP null?
    emitter.instruction("je __rt_pcmp_right_null");                             // null has its own conversion rules
    emitter.instruction("jmp __rt_pcmp_no_null");                               // neither operand needs the bool/null coercions

    // -- bool coercion of both operands --
    emitter.label("__rt_pcmp_bools");
    emitter.instruction("call __rt_php_truthy");                                // PHP truthiness of the left operand
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the left truthiness while the right one is computed
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the right runtime tag
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload the right low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reload the right high payload word
    emitter.instruction("call __rt_php_truthy");                                // PHP truthiness of the right operand
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the left truthiness value
    emitter.instruction("cmp r10, rax");                                        // false sorts below true in PHP's bool comparison
    emitter.instruction("jl __rt_pcmp_neg");                                    // false versus true is "less than"
    emitter.instruction("jg __rt_pcmp_pos");                                    // true versus false is "greater than"
    emitter.instruction("jmp __rt_pcmp_zero");                                  // equal truthiness compares equal

    // -- null on the left --
    emitter.label("__rt_pcmp_left_null");
    emitter.instruction("cmp rcx, 8");                                          // is the right operand also null?
    emitter.instruction("je __rt_pcmp_zero");                                   // null compares equal to null
    emitter.instruction("cmp rcx, 1");                                          // is the right operand a string?
    emitter.instruction("jne __rt_pcmp_null_vs_right");                         // non-string operands coerce to bool
    emitter.instruction("test r9, r9");                                         // does the right string hold any byte?
    emitter.instruction("jz __rt_pcmp_zero");                                   // null converts to "" and equals the empty string
    emitter.instruction("jmp __rt_pcmp_neg");                                   // "" sorts below every non-empty string
    emitter.label("__rt_pcmp_null_vs_right");
    emitter.instruction("mov rdi, rcx");                                        // pass the right runtime tag to the truthiness helper
    emitter.instruction("mov rsi, r8");                                         // pass the right low payload word
    emitter.instruction("mov rdx, r9");                                         // pass the right high payload word
    emitter.instruction("call __rt_php_truthy");                                // PHP truthiness of the right operand
    emitter.instruction("test rax, rax");                                       // is the right operand falsy?
    emitter.instruction("jz __rt_pcmp_zero");                                   // null equals every falsy operand
    emitter.instruction("jmp __rt_pcmp_neg");                                   // null sorts below every truthy operand

    // -- null on the right --
    emitter.label("__rt_pcmp_right_null");
    emitter.instruction("cmp rdi, 1");                                          // is the left operand a string?
    emitter.instruction("jne __rt_pcmp_left_vs_null");                          // non-string operands coerce to bool
    emitter.instruction("test rdx, rdx");                                       // does the left string hold any byte?
    emitter.instruction("jz __rt_pcmp_zero");                                   // the empty string equals null
    emitter.instruction("jmp __rt_pcmp_pos");                                   // every non-empty string sorts above ""
    emitter.label("__rt_pcmp_left_vs_null");
    emitter.instruction("call __rt_php_truthy");                                // PHP truthiness of the left operand
    emitter.instruction("test rax, rax");                                       // is the left operand falsy?
    emitter.instruction("jz __rt_pcmp_zero");                                   // every falsy operand equals null
    emitter.instruction("jmp __rt_pcmp_pos");                                   // every truthy operand sorts above null

    // -- neither operand is bool or null --
    emitter.label("__rt_pcmp_no_null");
    emitter.instruction("cmp rdi, 2");                                          // tags 0, 1 and 2 are the comparable scalar payloads
    emitter.instruction("setbe al");                                            // record whether the left operand is a scalar
    emitter.instruction("movzx r10, al");                                       // widen the left scalar predicate
    emitter.instruction("cmp rcx, 2");                                          // tags 0, 1 and 2 are the comparable scalar payloads
    emitter.instruction("setbe al");                                            // record whether the right operand is a scalar
    emitter.instruction("movzx r11, al");                                       // widen the right scalar predicate
    emitter.instruction("mov rax, r10");                                        // copy the left predicate for the combined tests
    emitter.instruction("and rax, r11");                                        // are both operands comparable scalars?
    emitter.instruction("jz __rt_pcmp_nonscalar");                              // containers and objects use the coarse ranking below
    emitter.instruction("cmp rdi, 1");                                          // is the left operand a string?
    emitter.instruction("jne __rt_pcmp_left_number");                           // only the right operand can still be a string
    emitter.instruction("cmp rcx, 1");                                          // is the right operand also a string?
    emitter.instruction("je __rt_pcmp_strings");                                // two strings use PHP's numeric-string promotion
    emitter.instruction("jmp __rt_pcmp_str_vs_num");                            // a string against a number parses the string
    emitter.label("__rt_pcmp_left_number");
    emitter.instruction("cmp rcx, 1");                                          // is the right operand a string?
    emitter.instruction("je __rt_pcmp_num_vs_str");                             // a number against a string parses the string
    emitter.instruction("cmp rdi, 0");                                          // is the left operand an int?
    emitter.instruction("jne __rt_pcmp_numeric");                               // any float operand promotes both sides to double
    emitter.instruction("cmp rcx, 0");                                          // is the right operand an int?
    emitter.instruction("jne __rt_pcmp_numeric");                               // any float operand promotes both sides to double
    emitter.instruction("cmp rsi, r8");                                         // compare two ints exactly, without losing precision
    emitter.instruction("jl __rt_pcmp_neg");                                    // the left int is smaller
    emitter.instruction("jg __rt_pcmp_pos");                                    // the left int is larger
    emitter.instruction("jmp __rt_pcmp_zero");                                  // both ints are equal

    // -- containers, objects, resources and callables --
    emitter.label("__rt_pcmp_nonscalar");
    emitter.instruction("mov rax, r10");                                        // copy the left predicate for the combined tests
    emitter.instruction("or rax, r11");                                         // is exactly one operand a comparable scalar?
    emitter.instruction("jz __rt_pcmp_zero");                                   // two non-scalars are reported equal (documented limitation)
    emitter.instruction("test r10, r10");                                       // is the left operand the non-scalar one?
    emitter.instruction("jz __rt_pcmp_pos");                                    // a non-scalar left operand ranks above a scalar
    emitter.instruction("jmp __rt_pcmp_neg");                                   // a non-scalar right operand ranks above a scalar

    // -- both operands are numbers: promote to double --
    emitter.label("__rt_pcmp_numeric");
    emitter.instruction("cmp rdi, 2");                                          // is the left operand already a double?
    emitter.instruction("je __rt_pcmp_numeric_left_double");                    // reinterpret its payload word
    emitter.instruction("cvtsi2sd xmm0, rsi");                                  // widen the left int payload into a double
    emitter.instruction("jmp __rt_pcmp_numeric_right");                         // continue with the right operand
    emitter.label("__rt_pcmp_numeric_left_double");
    emitter.instruction("movq xmm0, rsi");                                      // reinterpret the left payload word as a double
    emitter.label("__rt_pcmp_numeric_right");
    emitter.instruction("cmp rcx, 2");                                          // is the right operand already a double?
    emitter.instruction("je __rt_pcmp_numeric_right_double");                   // reinterpret its payload word
    emitter.instruction("cvtsi2sd xmm1, r8");                                   // widen the right int payload into a double
    emitter.instruction("jmp __rt_pcmp_fcmp");                                  // compare both operands as doubles
    emitter.label("__rt_pcmp_numeric_right_double");
    emitter.instruction("movq xmm1, r8");                                       // reinterpret the right payload word as a double

    emitter.label("__rt_pcmp_fcmp");
    emitter.instruction("ucomisd xmm0, xmm1");                                  // compare both numeric operands as doubles
    emitter.instruction("jp __rt_pcmp_unordered");                              // report unordered NaN separately from signed ordering
    emitter.instruction("jb __rt_pcmp_neg");                                    // an ordered less-than result
    emitter.instruction("je __rt_pcmp_zero");                                   // an ordered equal result
    emitter.instruction("jmp __rt_pcmp_pos");                                   // an ordered greater-than result

    emitter.label("__rt_pcmp_unordered");
    emitter.instruction("mov QWORD PTR [rbp - 112], 1");                        // expose unordered state to relational consumers
    emitter.instruction("jmp __rt_pcmp_pos");                                   // PHP spaceship orders unordered NaN as greater

    // -- string versus string --
    emitter.label("__rt_pcmp_strings");
    emitter.instruction("mov rax, rsi");                                        // pass the left string pointer to the numeric parser
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // pass the left string length to the numeric parser
    emitter.instruction("call __rt_str_to_number");                             // parse the left string under PHP's numeric-string grammar
    emitter.instruction("test rax, rax");                                       // was the left string fully numeric?
    emitter.instruction("jz __rt_pcmp_str_bytes");                              // a non-numeric operand forces the byte comparison
    emitter.instruction("movsd QWORD PTR [rbp - 56], xmm0");                    // save the parsed left value across the second parse
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // pass the right string pointer to the numeric parser
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // pass the right string length to the numeric parser
    emitter.instruction("call __rt_str_to_number");                             // parse the right string under PHP's numeric-string grammar
    emitter.instruction("test rax, rax");                                       // was the right string fully numeric?
    emitter.instruction("jz __rt_pcmp_str_bytes");                              // a non-numeric operand forces the byte comparison
    emitter.instruction("movapd xmm1, xmm0");                                   // move the parsed right value into the comparison register
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 56]");                    // reload the parsed left value
    emitter.instruction("jmp __rt_pcmp_fcmp");                                  // two numeric strings compare numerically
    emitter.label("__rt_pcmp_str_bytes");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the left string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the left string length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the right string pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the right string length
    emitter.instruction("call __rt_strcmp");                                    // compare both strings byte-wise, then by length
    emitter.instruction("cmp rax, 0");                                          // normalize the byte difference into a three-way result
    emitter.instruction("jl __rt_pcmp_neg");                                    // the left string sorts first
    emitter.instruction("jg __rt_pcmp_pos");                                    // the right string sorts first
    emitter.instruction("jmp __rt_pcmp_zero");                                  // both strings are byte-identical

    // -- number versus string, normalized so the number is always the left operand --
    emitter.label("__rt_pcmp_num_vs_str");
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // the operands are already in number/string order
    emitter.instruction("mov QWORD PTR [rbp - 72], rdi");                       // stage the number's runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 80], rsi");                       // stage the number's payload word
    emitter.instruction("mov QWORD PTR [rbp - 88], r8");                        // stage the string's pointer
    emitter.instruction("mov QWORD PTR [rbp - 96], r9");                        // stage the string's length
    emitter.instruction("jmp __rt_pcmp_num_str_body");                          // run the shared number-versus-string comparison
    emitter.label("__rt_pcmp_str_vs_num");
    emitter.instruction("mov QWORD PTR [rbp - 64], 1");                         // the string is the left operand, so the result is negated
    emitter.instruction("mov QWORD PTR [rbp - 72], rcx");                       // stage the number's runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 80], r8");                        // stage the number's payload word
    emitter.instruction("mov QWORD PTR [rbp - 88], rsi");                       // stage the string's pointer
    emitter.instruction("mov QWORD PTR [rbp - 96], rdx");                       // stage the string's length

    emitter.label("__rt_pcmp_num_str_body");
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the staged number's runtime tag
    emitter.instruction("cmp r10, 2");                                          // is the numeric operand a double that may be NaN?
    emitter.instruction("jne __rt_pcmp_num_str_parse");                         // integers cannot be unordered
    emitter.instruction("mov r11, QWORD PTR [rbp - 80]");                       // reload the staged double payload bits
    emitter.instruction("movq xmm0, r11");                                      // reinterpret the payload for the NaN self-comparison
    emitter.instruction("ucomisd xmm0, xmm0");                                  // NaN is the only double unordered with itself
    emitter.instruction("jp __rt_pcmp_num_str_unordered");                      // bypass parsing, formatting, and swap correction for NaN
    emitter.label("__rt_pcmp_num_str_parse");
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // pass the string pointer to the numeric parser
    emitter.instruction("mov rdx, QWORD PTR [rbp - 96]");                       // pass the string length to the numeric parser
    emitter.instruction("call __rt_str_to_number");                             // parse the string under PHP's numeric-string grammar
    emitter.instruction("test rax, rax");                                       // was the string fully numeric?
    emitter.instruction("jz __rt_pcmp_num_str_bytes");                          // PHP 8 compares a number with a non-numeric string as strings
    emitter.instruction("movsd QWORD PTR [rbp - 56], xmm0");                    // save the parsed string value
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the number's runtime tag
    emitter.instruction("mov r11, QWORD PTR [rbp - 80]");                       // reload the number's payload word
    emitter.instruction("cmp r10, 2");                                          // is the number already a double?
    emitter.instruction("je __rt_pcmp_num_str_double");                         // reinterpret its payload word
    emitter.instruction("cvtsi2sd xmm0, r11");                                  // widen the int payload into a double
    emitter.instruction("jmp __rt_pcmp_num_str_cmp");                           // compare the number against the parsed string
    emitter.label("__rt_pcmp_num_str_double");
    emitter.instruction("movq xmm0, r11");                                      // reinterpret the payload word as a double
    emitter.label("__rt_pcmp_num_str_cmp");
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 56]");                    // reload the parsed string value
    emitter.instruction("ucomisd xmm0, xmm1");                                  // compare the number against the numeric string
    emitter.instruction("jp __rt_pcmp_num_str_unordered");                      // report unordered NaN separately from signed ordering
    emitter.instruction("jb __rt_pcmp_maybe_neg");                              // the number is smaller, before any swap correction
    emitter.instruction("je __rt_pcmp_zero");                                   // both values are numerically equal
    emitter.instruction("jmp __rt_pcmp_maybe_pos");                             // the number is larger, before any swap correction
    emitter.label("__rt_pcmp_num_str_unordered");
    emitter.instruction("mov QWORD PTR [rbp - 112], 1");                        // expose unordered state to relational consumers
    emitter.instruction("jmp __rt_pcmp_pos");                                   // PHP spaceship returns 1 whenever either operand is NaN

    emitter.label("__rt_pcmp_num_str_bytes");
    crate::codegen_support::runtime::ctx::emit_concat_off_load(emitter, "r11");
    emitter.instruction("mov QWORD PTR [rbp - 104], r11");                      // save it so the rendered number cannot leak scratch space
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the number's runtime tag
    emitter.instruction("mov r11, QWORD PTR [rbp - 80]");                       // reload the number's payload word
    emitter.instruction("cmp r10, 2");                                          // is the number a double?
    emitter.instruction("je __rt_pcmp_num_str_ftoa");                           // doubles render through PHP's precision-14 formatter
    emitter.instruction("mov rax, r11");                                        // pass the int payload to the decimal formatter
    emitter.instruction("call __rt_itoa");                                      // render the int exactly the way PHP casts it to string
    emitter.instruction("jmp __rt_pcmp_num_str_strcmp");                        // compare the rendered number with the string
    emitter.label("__rt_pcmp_num_str_ftoa");
    emitter.instruction("movq xmm0, r11");                                      // reinterpret the payload word as the double to render
    emitter.instruction("call __rt_ftoa");                                      // render the double at PHP's default precision of 14
    emitter.label("__rt_pcmp_num_str_strcmp");
    emitter.instruction("mov rdi, rax");                                        // move the rendered number pointer into the strcmp argument
    emitter.instruction("mov rsi, rdx");                                        // move the rendered number length into the strcmp argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // reload the string pointer for the byte comparison
    emitter.instruction("mov rcx, QWORD PTR [rbp - 96]");                       // reload the string length for the byte comparison
    emitter.instruction("call __rt_strcmp");                                    // compare the rendered number with the string byte-wise
        emitter.instruction("mov r11, QWORD PTR [rbp - 104]");                      // reload the saved concat scratch cursor
    crate::codegen_support::runtime::ctx::emit_concat_off_store(emitter, "r11"); // release the scratch the rendered number occupied (ctx-relative in ctx mode)
    emitter.instruction("cmp rax, 0");                                          // normalize the byte difference into a three-way result
    emitter.instruction("jl __rt_pcmp_maybe_neg");                              // the rendered number sorts first, before any swap correction
    emitter.instruction("jg __rt_pcmp_maybe_pos");                              // the string sorts first, before any swap correction
    emitter.instruction("jmp __rt_pcmp_zero");                                  // both byte sequences are identical

    emitter.label("__rt_pcmp_maybe_neg");
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // was the string the original left operand?
    emitter.instruction("je __rt_pcmp_neg");                                    // no swap: keep the normalized result
    emitter.instruction("jmp __rt_pcmp_pos");                                   // swapped operands invert the normalized result
    emitter.label("__rt_pcmp_maybe_pos");
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // was the string the original left operand?
    emitter.instruction("je __rt_pcmp_pos");                                    // no swap: keep the normalized result
    emitter.instruction("jmp __rt_pcmp_neg");                                   // swapped operands invert the normalized result

    emitter.label("__rt_pcmp_neg");
    emitter.instruction("mov rax, -1");                                         // the left operand sorts before the right one
    emitter.instruction("jmp __rt_pcmp_done");                                  // return the three-way result
    emitter.label("__rt_pcmp_pos");
    emitter.instruction("mov rax, 1");                                          // the left operand sorts after the right one
    emitter.instruction("jmp __rt_pcmp_done");                                  // return the three-way result
    emitter.label("__rt_pcmp_zero");
    emitter.instruction("xor eax, eax");                                        // both operands compare equal

    emitter.label("__rt_pcmp_done");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 112]");                      // return whether the comparison was IEEE unordered
    emitter.instruction("add rsp, 112");                                        // release the ordering-comparison frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the three-way comparison result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies both target ABIs expose unordered NaN without changing the ordering result.
    #[test]
    fn php_compare_returns_unordered_flag_on_both_targets() {
        let mut aarch64 = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_php_compare(&mut aarch64);
        let aarch64 = aarch64.output();
        assert!(aarch64.contains("str xzr, [sp, #104]"), "{aarch64}");
        assert!(aarch64.contains("b.vs __rt_pcmp_unordered"), "{aarch64}");
        assert!(aarch64.contains("ldr x1, [sp, #104]"), "{aarch64}");

        let mut x86_64 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_php_compare(&mut x86_64);
        let x86_64 = x86_64.output();
        assert!(x86_64.contains("mov QWORD PTR [rbp - 112], 0"), "{x86_64}");
        assert!(x86_64.contains("jp __rt_pcmp_unordered"), "{x86_64}");
        assert!(x86_64.contains("mov rdx, QWORD PTR [rbp - 112]"), "{x86_64}");
    }

    /// Verifies number/string NaN comparisons bypass operand-swap negation on both targets.
    #[test]
    fn php_compare_number_string_unordered_is_always_positive_on_both_targets() {
        let mut aarch64 = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_php_compare(&mut aarch64);
        let aarch64 = aarch64.output();
        assert!(
            aarch64.contains(
                "fcmp d0, d0\n    b.vs __rt_pcmp_num_str_unordered\n__rt_pcmp_num_str_parse:"
            ),
            "{aarch64}"
        );
        assert!(
            aarch64.contains(
                "__rt_pcmp_num_str_unordered:\n    mov x9, #1\n    str x9, [sp, #104]\n    b __rt_pcmp_pos"
            ),
            "{aarch64}"
        );

        let mut x86_64 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_php_compare(&mut x86_64);
        let x86_64 = x86_64.output();
        assert!(
            x86_64.contains(
                "ucomisd xmm0, xmm0\n    jp __rt_pcmp_num_str_unordered\n__rt_pcmp_num_str_parse:"
            ),
            "{x86_64}"
        );
        assert!(
            x86_64.contains(
                "__rt_pcmp_num_str_unordered:\n    mov QWORD PTR [rbp - 112], 1\n    jmp __rt_pcmp_pos"
            ),
            "{x86_64}"
        );
    }
}
