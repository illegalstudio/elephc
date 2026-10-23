//! Purpose:
//! Emits the `__rt_version_compare` runtime helper assembly for PHP's `version_compare`,
//! with its canonicalization, special-form and token-comparison subroutines.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Mirrors php-src's `php_version_compare`: both versions are read as C strings (up to the
//!   first NUL), an empty version orders below any other, and a version that does not start
//!   with `#` is canonicalized first: `-`, `_`, `+` and any other non-alphanumeric byte become
//!   one `.`, a `.` is inserted between digit and non-digit runs, and trailing separators are
//!   dropped.
//! - Tokens compare numerically (`strtol`, saturating at `PHP_INT_MAX`) when both are numeric,
//!   by special-form order otherwise: anything else < `dev` < `alpha` = `a` < `beta` = `b` <
//!   `RC` = `rc` < `#` (and every number) < `pl` = `p`. Forms match by prefix, as in php-src.
//! - When one version runs out, its rest is compared token by token against the `#` form.
//! - With an operator, the result is the matching predicate (`<`, `lt`, `<=`, `le`, `>`, `gt`,
//!   `>=`, `ge`, `==`, `=`, `eq`, `!=`, `<>`, `ne`); any other operator is reported to the call
//!   site, which raises php-src's `ValueError`.
//! - The canonical forms live in one scratch heap block released before returning.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_version_compare` and its subroutines.
///
/// ABI (AArch64):
///   Input:  `x1`/`x2` = first version, `x3`/`x4` = second version, `x5`/`x6` = operator,
///           `x7` = 1 when an operator was given.
///   Output: `x0` = -1/0/1 (no operator) or the predicate as 0/1, `x1` = 1 for an invalid
///           operator, 0 otherwise.
///
/// ABI (x86_64, internal):
///   Input:  `rdi`/`rsi` = first version, `rdx`/`rcx` = second version, `r8`/`r9` = operator,
///           `r10` = 1 when an operator was given.
///   Output: `rax` = -1/0/1 or the predicate as 0/1, `rdx` = 1 for an invalid operator.
pub fn emit_version_compare(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_version_compare_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: version_compare canonicalization ---");
    // -- entry: canonicalize --
    emitter.label_global("__rt_vc_canon");
    emitter.instruction("cbz x2, __rt_vc_canon_empty");                         // an empty version canonicalizes to nothing
    emitter.instruction("ldrb w9, [x1]");                                       // load the first version byte
    emitter.instruction("cmp w9, #35");                                         // a version starting with '#' is compared verbatim
    emitter.instruction("b.eq __rt_vc_canon_raw");                              // copy it without canonicalization, as php-src does
    emitter.instruction("strb w9, [x3]");                                       // the first byte is always copied as is
    emitter.instruction("mov x10, #1");                                         // canonical output length
    emitter.instruction("mov x11, #1");                                         // next source byte index
    emitter.instruction("mov w12, w9");                                         // previous source byte
    // -- canonicalize loop --
    emitter.label("__rt_vc_canon_loop");
    emitter.instruction("cmp x11, x2");                                         // has every source byte been consumed?
    emitter.instruction("b.hs __rt_vc_canon_strip");                            // drop trailing separators and return
    emitter.instruction("ldrb w9, [x1, x11]");                                  // load the next source byte
    emitter.instruction("add x11, x11, #1");                                    // advance the source index
    emitter.instruction("sub x14, x10, #1");                                    // index of the last canonical byte
    emitter.instruction("ldrb w13, [x3, x14]");                                 // load the last canonical byte
    emitter.instruction("cmp w9, #45");                                         // '-' is a version separator
    emitter.instruction("b.eq __rt_vc_canon_sep");                              // separators become a single '.'
    emitter.instruction("cmp w9, #95");                                         // '_' is a version separator
    emitter.instruction("b.eq __rt_vc_canon_sep");                              // separators become a single '.'
    emitter.instruction("cmp w9, #43");                                         // '+' is a version separator
    emitter.instruction("b.eq __rt_vc_canon_sep");                              // separators become a single '.'
    emitter.instruction("sub w14, w9, #48");                                    // classify the current byte
    emitter.instruction("cmp w14, #10");                                        // is the current byte a digit?
    emitter.instruction("cset w5, lo");                                         // w5 = current byte is a digit
    emitter.instruction("sub w14, w12, #48");                                   // classify the previous byte
    emitter.instruction("cmp w14, #10");                                        // is the previous byte a digit?
    emitter.instruction("cset w6, lo");                                         // w6 = previous byte is a digit
    emitter.instruction("cmp w12, #46");                                        // is the previous byte a '.'?
    emitter.instruction("cset w7, ne");                                         // w7 = previous byte is not '.'
    emitter.instruction("bic w7, w7, w6");                                      // w7 = previous byte is a non-digit other than '.'
    emitter.instruction("and w14, w7, w5");                                     // non-digit followed by digit?
    emitter.instruction("cbnz w14, __rt_vc_canon_transition");                  // a digit/non-digit boundary inserts a '.'
    emitter.instruction("cmp w9, #46");                                         // is the current byte a '.'?
    emitter.instruction("cset w7, ne");                                         // w7 = current byte is not '.'
    emitter.instruction("bic w7, w7, w5");                                      // w7 = current byte is a non-digit other than '.'
    emitter.instruction("and w14, w6, w7");                                     // digit followed by non-digit?
    emitter.instruction("cbnz w14, __rt_vc_canon_transition");                  // a digit/non-digit boundary inserts a '.'
    emitter.instruction("cbnz w5, __rt_vc_canon_copy");                         // digits are copied
    emitter.instruction("orr w14, w9, #32");                                    // fold ASCII case for the letter test
    emitter.instruction("sub w14, w14, #97");                                   // letters map to 0..25
    emitter.instruction("cmp w14, #26");                                        // is the current byte an ASCII letter?
    emitter.instruction("b.hs __rt_vc_canon_sep");                              // any other non-alphanumeric byte is a separator
    // -- canonicalize copy --
    emitter.label("__rt_vc_canon_copy");
    emitter.instruction("strb w9, [x3, x10]");                                  // copy the alphanumeric byte
    emitter.instruction("add x10, x10, #1");                                    // grow the canonical output
    emitter.instruction("b __rt_vc_canon_next");                                // continue with the next source byte
    // -- canonicalize transition --
    emitter.label("__rt_vc_canon_transition");
    emitter.instruction("cmp w13, #46");                                        // does the output already end in '.'?
    emitter.instruction("b.eq __rt_vc_canon_copy");                             // never emit two separators in a row
    emitter.instruction("mov w14, #46");                                        // ASCII '.'
    emitter.instruction("strb w14, [x3, x10]");                                 // separate the digit and non-digit runs
    emitter.instruction("add x10, x10, #1");                                    // grow the canonical output
    emitter.instruction("b __rt_vc_canon_copy");                                // then copy the byte itself
    // -- canonicalize separator --
    emitter.label("__rt_vc_canon_sep");
    emitter.instruction("cmp w13, #46");                                        // does the output already end in '.'?
    emitter.instruction("b.eq __rt_vc_canon_next");                             // never emit two separators in a row
    emitter.instruction("mov w14, #46");                                        // ASCII '.'
    emitter.instruction("strb w14, [x3, x10]");                                 // emit the separator
    emitter.instruction("add x10, x10, #1");                                    // grow the canonical output
    // -- canonicalize next --
    emitter.label("__rt_vc_canon_next");
    emitter.instruction("mov w12, w9");                                         // remember the byte just consumed
    emitter.instruction("b __rt_vc_canon_loop");                                // continue with the next source byte
    // -- canonicalize strip --
    emitter.label("__rt_vc_canon_strip");
    emitter.instruction("cbz x10, __rt_vc_canon_done");                         // nothing left to strip
    emitter.instruction("sub x14, x10, #1");                                    // index of the last canonical byte
    emitter.instruction("ldrb w13, [x3, x14]");                                 // load the last canonical byte
    emitter.instruction("cmp w13, #46");                                        // is it a trailing separator?
    emitter.instruction("b.ne __rt_vc_canon_done");                             // keep a real last token
    emitter.instruction("mov x10, x14");                                        // drop the trailing separator
    emitter.instruction("b __rt_vc_canon_strip");                               // drop every trailing separator
    // -- canonicalize done --
    emitter.label("__rt_vc_canon_done");
    emitter.instruction("mov x0, x10");                                         // return the canonical length
    emitter.instruction("ret");                                                 // return to the comparator
    // -- canonicalize raw --
    emitter.label("__rt_vc_canon_raw");
    emitter.instruction("mov x10, #0");                                         // verbatim copy index
    // -- canonicalize raw loop --
    emitter.label("__rt_vc_canon_raw_loop");
    emitter.instruction("cmp x10, x2");                                         // has the whole version been copied?
    emitter.instruction("b.hs __rt_vc_canon_done");                             // return the verbatim length
    emitter.instruction("ldrb w9, [x1, x10]");                                  // load the next byte
    emitter.instruction("strb w9, [x3, x10]");                                  // copy it verbatim
    emitter.instruction("add x10, x10, #1");                                    // advance the copy index
    emitter.instruction("b __rt_vc_canon_raw_loop");                            // continue copying
    // -- canonicalize empty --
    emitter.label("__rt_vc_canon_empty");
    emitter.instruction("mov x0, #0");                                          // an empty version has no canonical bytes
    emitter.instruction("ret");                                                 // return to the comparator
    emitter.blank();
    emitter.comment("--- runtime: version_compare special forms ---");
    // -- entry: special --
    emitter.label_global("__rt_vc_special");
    emitter.instruction("cmp x2, #3");                                          // is the token long enough for 'dev'?
    emitter.instruction("b.lo __rt_vc_special_skip_0");                         // a shorter token cannot start with 'dev'
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load token byte 0
    emitter.instruction("cmp w9, #100");                                        // compare it with 'd'
    emitter.instruction("b.ne __rt_vc_special_skip_0");                         // the token does not start with 'dev'
    emitter.instruction("ldrb w9, [x1, #1]");                                   // load token byte 1
    emitter.instruction("cmp w9, #101");                                        // compare it with 'e'
    emitter.instruction("b.ne __rt_vc_special_skip_0");                         // the token does not start with 'dev'
    emitter.instruction("ldrb w9, [x1, #2]");                                   // load token byte 2
    emitter.instruction("cmp w9, #118");                                        // compare it with 'v'
    emitter.instruction("b.ne __rt_vc_special_skip_0");                         // the token does not start with 'dev'
    emitter.instruction("mov x0, #0");                                          // 'dev' orders as 0
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 0 --
    emitter.label("__rt_vc_special_skip_0");
    emitter.instruction("cmp x2, #5");                                          // is the token long enough for 'alpha'?
    emitter.instruction("b.lo __rt_vc_special_skip_1");                         // a shorter token cannot start with 'alpha'
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load token byte 0
    emitter.instruction("cmp w9, #97");                                         // compare it with 'a'
    emitter.instruction("b.ne __rt_vc_special_skip_1");                         // the token does not start with 'alpha'
    emitter.instruction("ldrb w9, [x1, #1]");                                   // load token byte 1
    emitter.instruction("cmp w9, #108");                                        // compare it with 'l'
    emitter.instruction("b.ne __rt_vc_special_skip_1");                         // the token does not start with 'alpha'
    emitter.instruction("ldrb w9, [x1, #2]");                                   // load token byte 2
    emitter.instruction("cmp w9, #112");                                        // compare it with 'p'
    emitter.instruction("b.ne __rt_vc_special_skip_1");                         // the token does not start with 'alpha'
    emitter.instruction("ldrb w9, [x1, #3]");                                   // load token byte 3
    emitter.instruction("cmp w9, #104");                                        // compare it with 'h'
    emitter.instruction("b.ne __rt_vc_special_skip_1");                         // the token does not start with 'alpha'
    emitter.instruction("ldrb w9, [x1, #4]");                                   // load token byte 4
    emitter.instruction("cmp w9, #97");                                         // compare it with 'a'
    emitter.instruction("b.ne __rt_vc_special_skip_1");                         // the token does not start with 'alpha'
    emitter.instruction("mov x0, #1");                                          // 'alpha' orders as 1
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 1 --
    emitter.label("__rt_vc_special_skip_1");
    emitter.instruction("cmp x2, #1");                                          // is the token long enough for 'a'?
    emitter.instruction("b.lo __rt_vc_special_skip_2");                         // a shorter token cannot start with 'a'
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load token byte 0
    emitter.instruction("cmp w9, #97");                                         // compare it with 'a'
    emitter.instruction("b.ne __rt_vc_special_skip_2");                         // the token does not start with 'a'
    emitter.instruction("mov x0, #1");                                          // 'a' orders as 1
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 2 --
    emitter.label("__rt_vc_special_skip_2");
    emitter.instruction("cmp x2, #4");                                          // is the token long enough for 'beta'?
    emitter.instruction("b.lo __rt_vc_special_skip_3");                         // a shorter token cannot start with 'beta'
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load token byte 0
    emitter.instruction("cmp w9, #98");                                         // compare it with 'b'
    emitter.instruction("b.ne __rt_vc_special_skip_3");                         // the token does not start with 'beta'
    emitter.instruction("ldrb w9, [x1, #1]");                                   // load token byte 1
    emitter.instruction("cmp w9, #101");                                        // compare it with 'e'
    emitter.instruction("b.ne __rt_vc_special_skip_3");                         // the token does not start with 'beta'
    emitter.instruction("ldrb w9, [x1, #2]");                                   // load token byte 2
    emitter.instruction("cmp w9, #116");                                        // compare it with 't'
    emitter.instruction("b.ne __rt_vc_special_skip_3");                         // the token does not start with 'beta'
    emitter.instruction("ldrb w9, [x1, #3]");                                   // load token byte 3
    emitter.instruction("cmp w9, #97");                                         // compare it with 'a'
    emitter.instruction("b.ne __rt_vc_special_skip_3");                         // the token does not start with 'beta'
    emitter.instruction("mov x0, #2");                                          // 'beta' orders as 2
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 3 --
    emitter.label("__rt_vc_special_skip_3");
    emitter.instruction("cmp x2, #1");                                          // is the token long enough for 'b'?
    emitter.instruction("b.lo __rt_vc_special_skip_4");                         // a shorter token cannot start with 'b'
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load token byte 0
    emitter.instruction("cmp w9, #98");                                         // compare it with 'b'
    emitter.instruction("b.ne __rt_vc_special_skip_4");                         // the token does not start with 'b'
    emitter.instruction("mov x0, #2");                                          // 'b' orders as 2
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 4 --
    emitter.label("__rt_vc_special_skip_4");
    emitter.instruction("cmp x2, #2");                                          // is the token long enough for 'RC'?
    emitter.instruction("b.lo __rt_vc_special_skip_5");                         // a shorter token cannot start with 'RC'
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load token byte 0
    emitter.instruction("cmp w9, #82");                                         // compare it with 'R'
    emitter.instruction("b.ne __rt_vc_special_skip_5");                         // the token does not start with 'RC'
    emitter.instruction("ldrb w9, [x1, #1]");                                   // load token byte 1
    emitter.instruction("cmp w9, #67");                                         // compare it with 'C'
    emitter.instruction("b.ne __rt_vc_special_skip_5");                         // the token does not start with 'RC'
    emitter.instruction("mov x0, #3");                                          // 'RC' orders as 3
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 5 --
    emitter.label("__rt_vc_special_skip_5");
    emitter.instruction("cmp x2, #2");                                          // is the token long enough for 'rc'?
    emitter.instruction("b.lo __rt_vc_special_skip_6");                         // a shorter token cannot start with 'rc'
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load token byte 0
    emitter.instruction("cmp w9, #114");                                        // compare it with 'r'
    emitter.instruction("b.ne __rt_vc_special_skip_6");                         // the token does not start with 'rc'
    emitter.instruction("ldrb w9, [x1, #1]");                                   // load token byte 1
    emitter.instruction("cmp w9, #99");                                         // compare it with 'c'
    emitter.instruction("b.ne __rt_vc_special_skip_6");                         // the token does not start with 'rc'
    emitter.instruction("mov x0, #3");                                          // 'rc' orders as 3
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 6 --
    emitter.label("__rt_vc_special_skip_6");
    emitter.instruction("cmp x2, #1");                                          // is the token long enough for '#'?
    emitter.instruction("b.lo __rt_vc_special_skip_7");                         // a shorter token cannot start with '#'
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load token byte 0
    emitter.instruction("cmp w9, #35");                                         // compare it with '#'
    emitter.instruction("b.ne __rt_vc_special_skip_7");                         // the token does not start with '#'
    emitter.instruction("mov x0, #4");                                          // '#' orders as 4
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 7 --
    emitter.label("__rt_vc_special_skip_7");
    emitter.instruction("cmp x2, #2");                                          // is the token long enough for 'pl'?
    emitter.instruction("b.lo __rt_vc_special_skip_8");                         // a shorter token cannot start with 'pl'
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load token byte 0
    emitter.instruction("cmp w9, #112");                                        // compare it with 'p'
    emitter.instruction("b.ne __rt_vc_special_skip_8");                         // the token does not start with 'pl'
    emitter.instruction("ldrb w9, [x1, #1]");                                   // load token byte 1
    emitter.instruction("cmp w9, #108");                                        // compare it with 'l'
    emitter.instruction("b.ne __rt_vc_special_skip_8");                         // the token does not start with 'pl'
    emitter.instruction("mov x0, #5");                                          // 'pl' orders as 5
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 8 --
    emitter.label("__rt_vc_special_skip_8");
    emitter.instruction("cmp x2, #1");                                          // is the token long enough for 'p'?
    emitter.instruction("b.lo __rt_vc_special_skip_9");                         // a shorter token cannot start with 'p'
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load token byte 0
    emitter.instruction("cmp w9, #112");                                        // compare it with 'p'
    emitter.instruction("b.ne __rt_vc_special_skip_9");                         // the token does not start with 'p'
    emitter.instruction("mov x0, #5");                                          // 'p' orders as 5
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 9 --
    emitter.label("__rt_vc_special_skip_9");
    emitter.instruction("mov x0, #-1");                                         // any other token orders below 'dev'
    emitter.instruction("ret");                                                 // return the special-form order
    emitter.blank();
    emitter.comment("--- runtime: version_compare core ---");
    // -- entry: core --
    emitter.label_global("__rt_vc_core");
    emitter.instruction("sub sp, sp, #96");                                     // allocate the comparator frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save the frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the comparator frame pointer
    emitter.instruction("stp x19, x20, [sp]");                                  // save callee-saved registers used for the cursors
    emitter.instruction("stp x21, x22, [sp, #16]");                             // save callee-saved registers used for the cursors
    emitter.instruction("stp x23, x24, [sp, #32]");                             // save callee-saved registers used for the token state
    emitter.instruction("stp x25, x26, [sp, #48]");                             // save callee-saved registers used for the token state
    emitter.instruction("stp x27, x28, [sp, #64]");                             // save callee-saved registers used for the token bounds
    emitter.instruction("mov x19, x1");                                         // x19 = first version cursor
    emitter.instruction("add x20, x1, x2");                                     // x20 = end of the first version
    emitter.instruction("mov x21, x3");                                         // x21 = second version cursor
    emitter.instruction("add x22, x3, x4");                                     // x22 = end of the second version
    emitter.instruction("mov x23, #1");                                         // the first version may have more tokens
    emitter.instruction("mov x24, #1");                                         // the second version may have more tokens
    emitter.instruction("mov x25, #0");                                         // the versions compare equal so far
    // -- core loop --
    emitter.label("__rt_vc_core_loop");
    emitter.instruction("cmp x19, x20");                                        // is the first version exhausted?
    emitter.instruction("b.hs __rt_vc_core_tail");                              // compare what remains of the other one
    emitter.instruction("cmp x21, x22");                                        // is the second version exhausted?
    emitter.instruction("b.hs __rt_vc_core_tail");                              // compare what remains of the other one
    emitter.instruction("cbz x23, __rt_vc_core_tail");                          // the first version had no further token
    emitter.instruction("cbz x24, __rt_vc_core_tail");                          // the second version had no further token
    emitter.instruction("mov x26, x19");                                        // the token starts at the cursor
    emitter.instruction("mov x9, x19");                                         // scan for the token's '.'
    // -- core scan version-1 token scan --
    emitter.label("__rt_vc_core_t1_scan");
    emitter.instruction("cmp x9, x20");                                         // reached the end of the version?
    emitter.instruction("b.hs __rt_vc_core_t1_last");                           // this is the last token
    emitter.instruction("ldrb w10, [x9]");                                      // load the next byte
    emitter.instruction("cmp w10, #46");                                        // is it the separator?
    emitter.instruction("b.eq __rt_vc_core_t1_dot");                            // the token ends here
    emitter.instruction("add x9, x9, #1");                                      // keep scanning
    emitter.instruction("b __rt_vc_core_t1_scan");                              // continue the scan
    // -- core scan version-1 token dot --
    emitter.label("__rt_vc_core_t1_dot");
    emitter.instruction("mov x27, x9");                                         // the token ends at the separator
    emitter.instruction("add x19, x9, #1");                                     // the cursor moves past the separator
    emitter.instruction("mov x23, #1");                                         // another token may follow
    emitter.instruction("b __rt_vc_core_t1_done");                              // the token is delimited
    // -- core scan version-1 token last --
    emitter.label("__rt_vc_core_t1_last");
    emitter.instruction("mov x27, x9");                                         // the token runs to the end of the version
    emitter.instruction("mov x23, #0");                                         // no token follows
    // -- core scan version-1 token done --
    emitter.label("__rt_vc_core_t1_done");
    emitter.instruction("mov x28, x21");                                        // the token starts at the cursor
    emitter.instruction("mov x9, x21");                                         // scan for the token's '.'
    // -- core scan version-2 token scan --
    emitter.label("__rt_vc_core_t2_scan");
    emitter.instruction("cmp x9, x22");                                         // reached the end of the version?
    emitter.instruction("b.hs __rt_vc_core_t2_last");                           // this is the last token
    emitter.instruction("ldrb w10, [x9]");                                      // load the next byte
    emitter.instruction("cmp w10, #46");                                        // is it the separator?
    emitter.instruction("b.eq __rt_vc_core_t2_dot");                            // the token ends here
    emitter.instruction("add x9, x9, #1");                                      // keep scanning
    emitter.instruction("b __rt_vc_core_t2_scan");                              // continue the scan
    // -- core scan version-2 token dot --
    emitter.label("__rt_vc_core_t2_dot");
    emitter.instruction("mov x12, x9");                                         // the token ends at the separator
    emitter.instruction("add x21, x9, #1");                                     // the cursor moves past the separator
    emitter.instruction("mov x24, #1");                                         // another token may follow
    emitter.instruction("b __rt_vc_core_t2_done");                              // the token is delimited
    // -- core scan version-2 token last --
    emitter.label("__rt_vc_core_t2_last");
    emitter.instruction("mov x12, x9");                                         // the token runs to the end of the version
    emitter.instruction("mov x24, #0");                                         // no token follows
    // -- core scan version-2 token done --
    emitter.label("__rt_vc_core_t2_done");
    emitter.instruction("mov x13, #0");                                         // assume the token does not start with a digit
    emitter.instruction("cmp x26, x27");                                        // is the token empty?
    emitter.instruction("b.hs __rt_vc_core_d1");                                // an empty token is not numeric
    emitter.instruction("ldrb w10, [x26]");                                     // load the token's first byte
    emitter.instruction("sub w10, w10, #48");                                   // classify it
    emitter.instruction("cmp w10, #10");                                        // is it a digit?
    emitter.instruction("cset x13, lo");                                        // the token is numeric when it starts with a digit
    // -- core classify version-1 token --
    emitter.label("__rt_vc_core_d1");
    emitter.instruction("mov x14, #0");                                         // assume the token does not start with a digit
    emitter.instruction("cmp x28, x12");                                        // is the token empty?
    emitter.instruction("b.hs __rt_vc_core_d2");                                // an empty token is not numeric
    emitter.instruction("ldrb w10, [x28]");                                     // load the token's first byte
    emitter.instruction("sub w10, w10, #48");                                   // classify it
    emitter.instruction("cmp w10, #10");                                        // is it a digit?
    emitter.instruction("cset x14, lo");                                        // the token is numeric when it starts with a digit
    // -- core classify version-2 token --
    emitter.label("__rt_vc_core_d2");
    emitter.instruction("and x10, x13, x14");                                   // are both tokens numeric?
    emitter.instruction("cbnz x10, __rt_vc_core_numbers");                      // compare them as integers
    emitter.instruction("orr x10, x13, x14");                                   // is exactly one token numeric?
    emitter.instruction("cbnz x10, __rt_vc_core_mixed");                        // a number orders like the special form '#'
    emitter.instruction("mov x1, x26");                                         // first token start
    emitter.instruction("sub x2, x27, x26");                                    // first token length
    emitter.instruction("bl __rt_vc_special");                                  // order of the first token
    emitter.instruction("mov x15, x0");                                         // keep the first token's order
    emitter.instruction("mov x1, x28");                                         // second token start
    emitter.instruction("sub x2, x12, x28");                                    // second token length
    emitter.instruction("bl __rt_vc_special");                                  // order of the second token
    emitter.instruction("cmp x15, x0");                                         // compare the two orders
    emitter.instruction("b __rt_vc_core_sign");                                 // turn the comparison into -1/0/1
    // -- core mixed --
    emitter.label("__rt_vc_core_mixed");
    emitter.instruction("cbz x13, __rt_vc_core_mixed_second");                  // the second token is the numeric one
    emitter.instruction("mov x1, x28");                                         // second token start
    emitter.instruction("sub x2, x12, x28");                                    // second token length
    emitter.instruction("bl __rt_vc_special");                                  // order of the non-numeric second token
    emitter.instruction("mov x15, #4");                                         // a number orders like '#'
    emitter.instruction("cmp x15, x0");                                         // compare the number with the second token
    emitter.instruction("b __rt_vc_core_sign");                                 // turn the comparison into -1/0/1
    // -- core mixed second --
    emitter.label("__rt_vc_core_mixed_second");
    emitter.instruction("mov x1, x26");                                         // first token start
    emitter.instruction("sub x2, x27, x26");                                    // first token length
    emitter.instruction("bl __rt_vc_special");                                  // order of the non-numeric first token
    emitter.instruction("cmp x0, #4");                                          // compare the first token with a number
    emitter.instruction("b __rt_vc_core_sign");                                 // turn the comparison into -1/0/1
    // -- core numbers --
    emitter.label("__rt_vc_core_numbers");
    emitter.instruction("mov x15, #0");                                         // accumulated token value
    emitter.instruction("mov x9, x26");                                         // digit cursor
    // -- core parse version-1 number loop --
    emitter.label("__rt_vc_core_n1_loop");
    emitter.instruction("cmp x9, x27");                                         // has every digit been read?
    emitter.instruction("b.hs __rt_vc_core_n1_done");                           // the value is complete
    emitter.instruction("ldrb w10, [x9]");                                      // load the next byte
    emitter.instruction("sub w10, w10, #48");                                   // convert it to a digit value
    emitter.instruction("cmp w10, #10");                                        // is it a digit?
    emitter.instruction("b.hs __rt_vc_core_n1_done");                           // strtol() stops at the first non-digit
    emitter.instruction("add x9, x9, #1");                                      // advance the digit cursor
    emitter.instruction("mov x11, #52428");                                     // load PHP_INT_MAX / 10 (low half)
    emitter.instruction("movk x11, #52428, lsl #16");                           // load PHP_INT_MAX / 10
    emitter.instruction("movk x11, #52428, lsl #32");                           // load PHP_INT_MAX / 10
    emitter.instruction("movk x11, #3276, lsl #48");                            // load PHP_INT_MAX / 10
    emitter.instruction("cmp x15, x11");                                        // would the next digit overflow?
    emitter.instruction("b.hi __rt_vc_core_n1_saturate");                       // strtol() saturates at PHP_INT_MAX
    emitter.instruction("b.lo __rt_vc_core_n1_accumulate");                     // plenty of room for another digit
    emitter.instruction("cmp w10, #7");                                         // only digits up to 7 still fit
    emitter.instruction("b.hi __rt_vc_core_n1_saturate");                       // strtol() saturates at PHP_INT_MAX
    // -- core parse version-1 number accumulate --
    emitter.label("__rt_vc_core_n1_accumulate");
    emitter.instruction("mov x11, #10");                                        // decimal base
    emitter.instruction("madd x15, x15, x11, x10");                             // value = value * 10 + digit
    emitter.instruction("b __rt_vc_core_n1_loop");                              // read the next digit
    // -- core parse version-1 number saturate --
    emitter.label("__rt_vc_core_n1_saturate");
    emitter.instruction("mov x15, #0x7fffffffffffffff");                        // the token saturates at PHP_INT_MAX
    emitter.instruction("b __rt_vc_core_n1_loop");                              // consume the remaining digits
    // -- core parse version-1 number done --
    emitter.label("__rt_vc_core_n1_done");
    emitter.instruction("mov x13, #0");                                         // accumulated token value
    emitter.instruction("mov x9, x28");                                         // digit cursor
    // -- core parse version-2 number loop --
    emitter.label("__rt_vc_core_n2_loop");
    emitter.instruction("cmp x9, x12");                                         // has every digit been read?
    emitter.instruction("b.hs __rt_vc_core_n2_done");                           // the value is complete
    emitter.instruction("ldrb w10, [x9]");                                      // load the next byte
    emitter.instruction("sub w10, w10, #48");                                   // convert it to a digit value
    emitter.instruction("cmp w10, #10");                                        // is it a digit?
    emitter.instruction("b.hs __rt_vc_core_n2_done");                           // strtol() stops at the first non-digit
    emitter.instruction("add x9, x9, #1");                                      // advance the digit cursor
    emitter.instruction("mov x11, #52428");                                     // load PHP_INT_MAX / 10 (low half)
    emitter.instruction("movk x11, #52428, lsl #16");                           // load PHP_INT_MAX / 10
    emitter.instruction("movk x11, #52428, lsl #32");                           // load PHP_INT_MAX / 10
    emitter.instruction("movk x11, #3276, lsl #48");                            // load PHP_INT_MAX / 10
    emitter.instruction("cmp x13, x11");                                        // would the next digit overflow?
    emitter.instruction("b.hi __rt_vc_core_n2_saturate");                       // strtol() saturates at PHP_INT_MAX
    emitter.instruction("b.lo __rt_vc_core_n2_accumulate");                     // plenty of room for another digit
    emitter.instruction("cmp w10, #7");                                         // only digits up to 7 still fit
    emitter.instruction("b.hi __rt_vc_core_n2_saturate");                       // strtol() saturates at PHP_INT_MAX
    // -- core parse version-2 number accumulate --
    emitter.label("__rt_vc_core_n2_accumulate");
    emitter.instruction("mov x11, #10");                                        // decimal base
    emitter.instruction("madd x13, x13, x11, x10");                             // value = value * 10 + digit
    emitter.instruction("b __rt_vc_core_n2_loop");                              // read the next digit
    // -- core parse version-2 number saturate --
    emitter.label("__rt_vc_core_n2_saturate");
    emitter.instruction("mov x13, #0x7fffffffffffffff");                        // the token saturates at PHP_INT_MAX
    emitter.instruction("b __rt_vc_core_n2_loop");                              // consume the remaining digits
    // -- core parse version-2 number done --
    emitter.label("__rt_vc_core_n2_done");
    emitter.instruction("cmp x15, x13");                                        // compare the two numbers
    // -- core sign --
    emitter.label("__rt_vc_core_sign");
    emitter.instruction("cset x25, gt");                                        // 1 when the first side is greater
    emitter.instruction("csinv x25, x25, xzr, ge");                             // -1 when it is smaller, 0 when equal
    emitter.instruction("cbnz x25, __rt_vc_core_done");                         // the first difference decides
    emitter.instruction("b __rt_vc_core_loop");                                 // compare the next pair of tokens
    // -- core tail --
    emitter.label("__rt_vc_core_tail");
    emitter.instruction("cbnz x23, __rt_vc_core_tail_first");                   // the first version has tokens left
    emitter.instruction("cbnz x24, __rt_vc_core_tail_second");                  // the second version has tokens left
    emitter.instruction("mov x25, #0");                                         // both versions ended together
    emitter.instruction("b __rt_vc_core_done");                                 // the versions are equal
    // -- core tail second --
    emitter.label("__rt_vc_core_tail_second");
    emitter.instruction("mov x19, x21");                                        // continue with the second version's rest
    emitter.instruction("mov x20, x22");                                        // and its end
    emitter.instruction("mov x26, #-1");                                        // the remaining side is the second one
    emitter.instruction("b __rt_vc_core_rest");                                 // compare the rest against the '#' form
    // -- core tail first --
    emitter.label("__rt_vc_core_tail_first");
    emitter.instruction("mov x26, #1");                                         // the remaining side is the first one
    // -- core rest --
    emitter.label("__rt_vc_core_rest");
    emitter.instruction("cmp x19, x20");                                        // is the remaining version empty?
    emitter.instruction("b.lo __rt_vc_core_rest_token");                        // compare its next token
    emitter.instruction("neg x25, x26");                                        // an empty rest orders below the '#' form
    emitter.instruction("b __rt_vc_core_done");                                 // the comparison is decided
    // -- core rest token --
    emitter.label("__rt_vc_core_rest_token");
    emitter.instruction("mov x27, x19");                                        // the token starts at the cursor
    emitter.instruction("mov x9, x19");                                         // scan for the token's '.'
    // -- core rest token scan --
    emitter.label("__rt_vc_core_r_scan");
    emitter.instruction("cmp x9, x20");                                         // reached the end of the version?
    emitter.instruction("b.hs __rt_vc_core_r_last");                            // this is the last token
    emitter.instruction("ldrb w10, [x9]");                                      // load the next byte
    emitter.instruction("cmp w10, #46");                                        // is it the separator?
    emitter.instruction("b.eq __rt_vc_core_r_dot");                             // the token ends here
    emitter.instruction("add x9, x9, #1");                                      // keep scanning
    emitter.instruction("b __rt_vc_core_r_scan");                               // continue the scan
    // -- core rest token dot --
    emitter.label("__rt_vc_core_r_dot");
    emitter.instruction("mov x28, x9");                                         // the token ends at the separator
    emitter.instruction("add x19, x9, #1");                                     // the cursor moves past the separator
    emitter.instruction("mov x23, #1");                                         // another token may follow
    emitter.instruction("b __rt_vc_core_r_done");                               // the token is delimited
    // -- core rest token last --
    emitter.label("__rt_vc_core_r_last");
    emitter.instruction("mov x28, x9");                                         // the token runs to the end of the version
    emitter.instruction("mov x23, #0");                                         // no token follows
    // -- core rest token done --
    emitter.label("__rt_vc_core_r_done");
    emitter.instruction("mov x13, #0");                                         // assume the token does not start with a digit
    emitter.instruction("cmp x27, x28");                                        // is the token empty?
    emitter.instruction("b.hs __rt_vc_core_rd");                                // an empty token is not numeric
    emitter.instruction("ldrb w10, [x27]");                                     // load the token's first byte
    emitter.instruction("sub w10, w10, #48");                                   // classify it
    emitter.instruction("cmp w10, #10");                                        // is it a digit?
    emitter.instruction("cset x13, lo");                                        // the token is numeric when it starts with a digit
    // -- core rest digits --
    emitter.label("__rt_vc_core_rd");
    emitter.instruction("cbz x13, __rt_vc_core_rest_special");                  // a non-numeric token is compared by its order
    emitter.instruction("mov x25, x26");                                        // a numeric token makes its side the greater one
    emitter.instruction("b __rt_vc_core_done");                                 // the comparison is decided
    // -- core rest special --
    emitter.label("__rt_vc_core_rest_special");
    emitter.instruction("mov x1, x27");                                         // token start
    emitter.instruction("sub x2, x28, x27");                                    // token length
    emitter.instruction("bl __rt_vc_special");                                  // order of the remaining token
    emitter.instruction("cmp x0, #4");                                          // compare it with the '#' form
    emitter.instruction("cset x25, gt");                                        // 1 when the token orders above '#'
    emitter.instruction("csinv x25, x25, xzr, ge");                             // -1 below, 0 equal
    emitter.instruction("mul x25, x25, x26");                                   // orient the result by the remaining side
    emitter.instruction("cbnz x25, __rt_vc_core_done");                         // the first difference decides
    emitter.instruction("cbz x23, __rt_vc_core_done");                          // no token is left: the versions are equal
    emitter.instruction("b __rt_vc_core_rest");                                 // compare the next remaining token
    // -- core done --
    emitter.label("__rt_vc_core_done");
    emitter.instruction("mov x0, x25");                                         // return -1, 0 or 1
    emitter.instruction("ldp x19, x20, [sp]");                                  // restore callee-saved registers
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore callee-saved registers
    emitter.instruction("ldp x23, x24, [sp, #32]");                             // restore callee-saved registers
    emitter.instruction("ldp x25, x26, [sp, #48]");                             // restore callee-saved registers
    emitter.instruction("ldp x27, x28, [sp, #64]");                             // restore callee-saved registers
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the comparator frame
    emitter.instruction("ret");                                                 // return the comparison
    emitter.blank();
    emitter.comment("--- runtime: version_compare ---");
    // -- entry: entry --
    emitter.label_global("__rt_version_compare");
    emitter.instruction("sub sp, sp, #96");                                     // allocate the entry frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save the frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the entry frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save the first version
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save the second version
    emitter.instruction("stp x5, x6, [sp, #32]");                               // save the operator
    emitter.instruction("str x7, [sp, #48]");                                   // save whether an operator was given
    emitter.instruction("ldp x9, x10, [sp, #0]");                               // reload the version pointer and length
    emitter.instruction("mov x11, #0");                                         // scan for an embedded NUL
    // -- version-1 length --
    emitter.label("__rt_version_compare_len1");
    emitter.instruction("cmp x11, x10");                                        // reached the end of the PHP string?
    emitter.instruction("b.hs __rt_version_compare_len1_done");                 // no NUL byte inside
    emitter.instruction("ldrb w12, [x9, x11]");                                 // load the next byte
    emitter.instruction("cbz w12, __rt_version_compare_len1_done");             // php-src stops at the first NUL byte
    emitter.instruction("add x11, x11, #1");                                    // advance the scan
    emitter.instruction("b __rt_version_compare_len1");                         // keep scanning
    // -- version-1 length done --
    emitter.label("__rt_version_compare_len1_done");
    emitter.instruction("str x11, [sp, #8]");                                   // keep the C-string length
    emitter.instruction("ldp x9, x10, [sp, #16]");                              // reload the version pointer and length
    emitter.instruction("mov x11, #0");                                         // scan for an embedded NUL
    // -- version-2 length --
    emitter.label("__rt_version_compare_len2");
    emitter.instruction("cmp x11, x10");                                        // reached the end of the PHP string?
    emitter.instruction("b.hs __rt_version_compare_len2_done");                 // no NUL byte inside
    emitter.instruction("ldrb w12, [x9, x11]");                                 // load the next byte
    emitter.instruction("cbz w12, __rt_version_compare_len2_done");             // php-src stops at the first NUL byte
    emitter.instruction("add x11, x11, #1");                                    // advance the scan
    emitter.instruction("b __rt_version_compare_len2");                         // keep scanning
    // -- version-2 length done --
    emitter.label("__rt_version_compare_len2_done");
    emitter.instruction("str x11, [sp, #24]");                                  // keep the C-string length
    emitter.instruction("ldr x9, [sp, #8]");                                    // first version length
    emitter.instruction("ldr x10, [sp, #24]");                                  // second version length
    emitter.instruction("cbnz x9, __rt_version_compare_first_present");         // a non-empty first version is canonicalized
    emitter.instruction("cmp x10, #0");                                         // is the second version empty too?
    emitter.instruction("csetm x0, ne");                                        // an empty version orders below any other
    emitter.instruction("b __rt_version_compare_result");                       // apply the operator
    // -- first present --
    emitter.label("__rt_version_compare_first_present");
    emitter.instruction("mov x0, #1");                                          // any version orders above an empty one
    emitter.instruction("cbz x10, __rt_version_compare_result");                // apply the operator
    emitter.instruction("add x0, x9, x10");                                     // total source length
    emitter.instruction("lsl x0, x0, #1");                                      // canonicalization at most doubles each version
    emitter.instruction("add x0, x0, #2");                                      // room for both canonical buffers
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = scratch buffer for both canonical forms
    emitter.instruction("str x0, [sp, #56]");                                   // save the scratch buffer
    emitter.instruction("ldp x1, x2, [sp]");                                    // first version
    emitter.instruction("mov x3, x0");                                          // its canonical form starts the buffer
    emitter.instruction("bl __rt_vc_canon");                                    // x0 = first canonical length
    emitter.instruction("str x0, [sp, #64]");                                   // save the first canonical length
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the first version length
    emitter.instruction("ldr x3, [sp, #56]");                                   // reload the scratch buffer
    emitter.instruction("add x3, x3, x9, lsl #1");                              // skip the first canonical area
    emitter.instruction("add x3, x3, #1");                                      // past its worst-case length
    emitter.instruction("str x3, [sp, #72]");                                   // save the second canonical start
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // second version
    emitter.instruction("bl __rt_vc_canon");                                    // x0 = second canonical length
    emitter.instruction("mov x4, x0");                                          // second canonical length
    emitter.instruction("ldr x3, [sp, #72]");                                   // second canonical start
    emitter.instruction("ldr x1, [sp, #56]");                                   // first canonical start
    emitter.instruction("ldr x2, [sp, #64]");                                   // first canonical length
    emitter.instruction("bl __rt_vc_core");                                     // x0 = -1, 0 or 1
    emitter.instruction("str x0, [sp, #64]");                                   // keep the comparison across the release
    emitter.instruction("ldr x0, [sp, #56]");                                   // the scratch buffer
    emitter.instruction("bl __rt_heap_free");                                   // release the canonical forms
    emitter.instruction("ldr x0, [sp, #64]");                                   // reload the comparison
    // -- result --
    emitter.label("__rt_version_compare_result");
    emitter.instruction("ldr x9, [sp, #48]");                                   // was an operator given?
    emitter.instruction("cbz x9, __rt_version_compare_int");                    // without one the comparison is the result
    emitter.instruction("ldp x5, x6, [sp, #32]");                               // reload the operator
    emitter.instruction("cmp x6, #1");                                          // is the operator as long as '<'?
    emitter.instruction("b.ne __rt_version_compare_op_skip_0");                 // a different length cannot be '<'
    emitter.instruction("ldrb w9, [x5, #0]");                                   // load operator byte 0
    emitter.instruction("cmp w9, #60");                                         // compare it with '<'
    emitter.instruction("b.ne __rt_version_compare_op_skip_0");                 // the operator is not '<'
    emitter.instruction("b __rt_version_compare_lt");                           // '<' selects the lt predicate
    // -- operator skip 0 --
    emitter.label("__rt_version_compare_op_skip_0");
    emitter.instruction("cmp x6, #2");                                          // is the operator as long as 'lt'?
    emitter.instruction("b.ne __rt_version_compare_op_skip_1");                 // a different length cannot be 'lt'
    emitter.instruction("ldrb w9, [x5, #0]");                                   // load operator byte 0
    emitter.instruction("cmp w9, #108");                                        // compare it with 'l'
    emitter.instruction("b.ne __rt_version_compare_op_skip_1");                 // the operator is not 'lt'
    emitter.instruction("ldrb w9, [x5, #1]");                                   // load operator byte 1
    emitter.instruction("cmp w9, #116");                                        // compare it with 't'
    emitter.instruction("b.ne __rt_version_compare_op_skip_1");                 // the operator is not 'lt'
    emitter.instruction("b __rt_version_compare_lt");                           // 'lt' selects the lt predicate
    // -- operator skip 1 --
    emitter.label("__rt_version_compare_op_skip_1");
    emitter.instruction("cmp x6, #2");                                          // is the operator as long as '<='?
    emitter.instruction("b.ne __rt_version_compare_op_skip_2");                 // a different length cannot be '<='
    emitter.instruction("ldrb w9, [x5, #0]");                                   // load operator byte 0
    emitter.instruction("cmp w9, #60");                                         // compare it with '<'
    emitter.instruction("b.ne __rt_version_compare_op_skip_2");                 // the operator is not '<='
    emitter.instruction("ldrb w9, [x5, #1]");                                   // load operator byte 1
    emitter.instruction("cmp w9, #61");                                         // compare it with '='
    emitter.instruction("b.ne __rt_version_compare_op_skip_2");                 // the operator is not '<='
    emitter.instruction("b __rt_version_compare_le");                           // '<=' selects the le predicate
    // -- operator skip 2 --
    emitter.label("__rt_version_compare_op_skip_2");
    emitter.instruction("cmp x6, #2");                                          // is the operator as long as 'le'?
    emitter.instruction("b.ne __rt_version_compare_op_skip_3");                 // a different length cannot be 'le'
    emitter.instruction("ldrb w9, [x5, #0]");                                   // load operator byte 0
    emitter.instruction("cmp w9, #108");                                        // compare it with 'l'
    emitter.instruction("b.ne __rt_version_compare_op_skip_3");                 // the operator is not 'le'
    emitter.instruction("ldrb w9, [x5, #1]");                                   // load operator byte 1
    emitter.instruction("cmp w9, #101");                                        // compare it with 'e'
    emitter.instruction("b.ne __rt_version_compare_op_skip_3");                 // the operator is not 'le'
    emitter.instruction("b __rt_version_compare_le");                           // 'le' selects the le predicate
    // -- operator skip 3 --
    emitter.label("__rt_version_compare_op_skip_3");
    emitter.instruction("cmp x6, #1");                                          // is the operator as long as '>'?
    emitter.instruction("b.ne __rt_version_compare_op_skip_4");                 // a different length cannot be '>'
    emitter.instruction("ldrb w9, [x5, #0]");                                   // load operator byte 0
    emitter.instruction("cmp w9, #62");                                         // compare it with '>'
    emitter.instruction("b.ne __rt_version_compare_op_skip_4");                 // the operator is not '>'
    emitter.instruction("b __rt_version_compare_gt");                           // '>' selects the gt predicate
    // -- operator skip 4 --
    emitter.label("__rt_version_compare_op_skip_4");
    emitter.instruction("cmp x6, #2");                                          // is the operator as long as 'gt'?
    emitter.instruction("b.ne __rt_version_compare_op_skip_5");                 // a different length cannot be 'gt'
    emitter.instruction("ldrb w9, [x5, #0]");                                   // load operator byte 0
    emitter.instruction("cmp w9, #103");                                        // compare it with 'g'
    emitter.instruction("b.ne __rt_version_compare_op_skip_5");                 // the operator is not 'gt'
    emitter.instruction("ldrb w9, [x5, #1]");                                   // load operator byte 1
    emitter.instruction("cmp w9, #116");                                        // compare it with 't'
    emitter.instruction("b.ne __rt_version_compare_op_skip_5");                 // the operator is not 'gt'
    emitter.instruction("b __rt_version_compare_gt");                           // 'gt' selects the gt predicate
    // -- operator skip 5 --
    emitter.label("__rt_version_compare_op_skip_5");
    emitter.instruction("cmp x6, #2");                                          // is the operator as long as '>='?
    emitter.instruction("b.ne __rt_version_compare_op_skip_6");                 // a different length cannot be '>='
    emitter.instruction("ldrb w9, [x5, #0]");                                   // load operator byte 0
    emitter.instruction("cmp w9, #62");                                         // compare it with '>'
    emitter.instruction("b.ne __rt_version_compare_op_skip_6");                 // the operator is not '>='
    emitter.instruction("ldrb w9, [x5, #1]");                                   // load operator byte 1
    emitter.instruction("cmp w9, #61");                                         // compare it with '='
    emitter.instruction("b.ne __rt_version_compare_op_skip_6");                 // the operator is not '>='
    emitter.instruction("b __rt_version_compare_ge");                           // '>=' selects the ge predicate
    // -- operator skip 6 --
    emitter.label("__rt_version_compare_op_skip_6");
    emitter.instruction("cmp x6, #2");                                          // is the operator as long as 'ge'?
    emitter.instruction("b.ne __rt_version_compare_op_skip_7");                 // a different length cannot be 'ge'
    emitter.instruction("ldrb w9, [x5, #0]");                                   // load operator byte 0
    emitter.instruction("cmp w9, #103");                                        // compare it with 'g'
    emitter.instruction("b.ne __rt_version_compare_op_skip_7");                 // the operator is not 'ge'
    emitter.instruction("ldrb w9, [x5, #1]");                                   // load operator byte 1
    emitter.instruction("cmp w9, #101");                                        // compare it with 'e'
    emitter.instruction("b.ne __rt_version_compare_op_skip_7");                 // the operator is not 'ge'
    emitter.instruction("b __rt_version_compare_ge");                           // 'ge' selects the ge predicate
    // -- operator skip 7 --
    emitter.label("__rt_version_compare_op_skip_7");
    emitter.instruction("cmp x6, #2");                                          // is the operator as long as '=='?
    emitter.instruction("b.ne __rt_version_compare_op_skip_8");                 // a different length cannot be '=='
    emitter.instruction("ldrb w9, [x5, #0]");                                   // load operator byte 0
    emitter.instruction("cmp w9, #61");                                         // compare it with '='
    emitter.instruction("b.ne __rt_version_compare_op_skip_8");                 // the operator is not '=='
    emitter.instruction("ldrb w9, [x5, #1]");                                   // load operator byte 1
    emitter.instruction("cmp w9, #61");                                         // compare it with '='
    emitter.instruction("b.ne __rt_version_compare_op_skip_8");                 // the operator is not '=='
    emitter.instruction("b __rt_version_compare_eq");                           // '==' selects the eq predicate
    // -- operator skip 8 --
    emitter.label("__rt_version_compare_op_skip_8");
    emitter.instruction("cmp x6, #1");                                          // is the operator as long as '='?
    emitter.instruction("b.ne __rt_version_compare_op_skip_9");                 // a different length cannot be '='
    emitter.instruction("ldrb w9, [x5, #0]");                                   // load operator byte 0
    emitter.instruction("cmp w9, #61");                                         // compare it with '='
    emitter.instruction("b.ne __rt_version_compare_op_skip_9");                 // the operator is not '='
    emitter.instruction("b __rt_version_compare_eq");                           // '=' selects the eq predicate
    // -- operator skip 9 --
    emitter.label("__rt_version_compare_op_skip_9");
    emitter.instruction("cmp x6, #2");                                          // is the operator as long as 'eq'?
    emitter.instruction("b.ne __rt_version_compare_op_skip_10");                // a different length cannot be 'eq'
    emitter.instruction("ldrb w9, [x5, #0]");                                   // load operator byte 0
    emitter.instruction("cmp w9, #101");                                        // compare it with 'e'
    emitter.instruction("b.ne __rt_version_compare_op_skip_10");                // the operator is not 'eq'
    emitter.instruction("ldrb w9, [x5, #1]");                                   // load operator byte 1
    emitter.instruction("cmp w9, #113");                                        // compare it with 'q'
    emitter.instruction("b.ne __rt_version_compare_op_skip_10");                // the operator is not 'eq'
    emitter.instruction("b __rt_version_compare_eq");                           // 'eq' selects the eq predicate
    // -- operator skip 10 --
    emitter.label("__rt_version_compare_op_skip_10");
    emitter.instruction("cmp x6, #2");                                          // is the operator as long as '!='?
    emitter.instruction("b.ne __rt_version_compare_op_skip_11");                // a different length cannot be '!='
    emitter.instruction("ldrb w9, [x5, #0]");                                   // load operator byte 0
    emitter.instruction("cmp w9, #33");                                         // compare it with '!'
    emitter.instruction("b.ne __rt_version_compare_op_skip_11");                // the operator is not '!='
    emitter.instruction("ldrb w9, [x5, #1]");                                   // load operator byte 1
    emitter.instruction("cmp w9, #61");                                         // compare it with '='
    emitter.instruction("b.ne __rt_version_compare_op_skip_11");                // the operator is not '!='
    emitter.instruction("b __rt_version_compare_ne");                           // '!=' selects the ne predicate
    // -- operator skip 11 --
    emitter.label("__rt_version_compare_op_skip_11");
    emitter.instruction("cmp x6, #2");                                          // is the operator as long as '<>'?
    emitter.instruction("b.ne __rt_version_compare_op_skip_12");                // a different length cannot be '<>'
    emitter.instruction("ldrb w9, [x5, #0]");                                   // load operator byte 0
    emitter.instruction("cmp w9, #60");                                         // compare it with '<'
    emitter.instruction("b.ne __rt_version_compare_op_skip_12");                // the operator is not '<>'
    emitter.instruction("ldrb w9, [x5, #1]");                                   // load operator byte 1
    emitter.instruction("cmp w9, #62");                                         // compare it with '>'
    emitter.instruction("b.ne __rt_version_compare_op_skip_12");                // the operator is not '<>'
    emitter.instruction("b __rt_version_compare_ne");                           // '<>' selects the ne predicate
    // -- operator skip 12 --
    emitter.label("__rt_version_compare_op_skip_12");
    emitter.instruction("cmp x6, #2");                                          // is the operator as long as 'ne'?
    emitter.instruction("b.ne __rt_version_compare_op_skip_13");                // a different length cannot be 'ne'
    emitter.instruction("ldrb w9, [x5, #0]");                                   // load operator byte 0
    emitter.instruction("cmp w9, #110");                                        // compare it with 'n'
    emitter.instruction("b.ne __rt_version_compare_op_skip_13");                // the operator is not 'ne'
    emitter.instruction("ldrb w9, [x5, #1]");                                   // load operator byte 1
    emitter.instruction("cmp w9, #101");                                        // compare it with 'e'
    emitter.instruction("b.ne __rt_version_compare_op_skip_13");                // the operator is not 'ne'
    emitter.instruction("b __rt_version_compare_ne");                           // 'ne' selects the ne predicate
    // -- operator skip 13 --
    emitter.label("__rt_version_compare_op_skip_13");
    emitter.instruction("mov x0, #0");                                          // no result for an unknown operator
    emitter.instruction("mov x1, #1");                                          // report the invalid operator so the caller throws
    emitter.instruction("b __rt_version_compare_return");                       // leave the entry frame
    // -- lt --
    emitter.label("__rt_version_compare_lt");
    emitter.instruction("cmn x0, #1");                                          // compare the result with -1
    emitter.instruction("cset x0, eq");                                         // evaluate the 'lt' predicate
    emitter.instruction("b __rt_version_compare_ok");                           // return the boolean
    // -- le --
    emitter.label("__rt_version_compare_le");
    emitter.instruction("cmp x0, #1");                                          // compare the result with 1
    emitter.instruction("cset x0, ne");                                         // evaluate the 'le' predicate
    emitter.instruction("b __rt_version_compare_ok");                           // return the boolean
    // -- gt --
    emitter.label("__rt_version_compare_gt");
    emitter.instruction("cmp x0, #1");                                          // compare the result with 1
    emitter.instruction("cset x0, eq");                                         // evaluate the 'gt' predicate
    emitter.instruction("b __rt_version_compare_ok");                           // return the boolean
    // -- ge --
    emitter.label("__rt_version_compare_ge");
    emitter.instruction("cmn x0, #1");                                          // compare the result with -1
    emitter.instruction("cset x0, ne");                                         // evaluate the 'ge' predicate
    emitter.instruction("b __rt_version_compare_ok");                           // return the boolean
    // -- eq --
    emitter.label("__rt_version_compare_eq");
    emitter.instruction("cmp x0, #0");                                          // compare the result with 0
    emitter.instruction("cset x0, eq");                                         // evaluate the 'eq' predicate
    emitter.instruction("b __rt_version_compare_ok");                           // return the boolean
    // -- ne --
    emitter.label("__rt_version_compare_ne");
    emitter.instruction("cmp x0, #0");                                          // compare the result with 0
    emitter.instruction("cset x0, ne");                                         // evaluate the 'ne' predicate
    emitter.instruction("b __rt_version_compare_ok");                           // return the boolean
    // -- int --
    emitter.label("__rt_version_compare_int");
    // -- ok --
    emitter.label("__rt_version_compare_ok");
    emitter.instruction("mov x1, #0");                                          // report success
    // -- return --
    emitter.label("__rt_version_compare_return");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the entry frame
    emitter.instruction("ret");                                                 // return the comparison or predicate
}

/// Emits `__rt_version_compare` and its subroutines for x86_64 Linux.
fn emit_version_compare_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: version_compare canonicalization ---");
    // -- entry: canonicalize --
    emitter.label_global("__rt_vc_canon");
    emitter.instruction("test rsi, rsi");                                       // is the version empty?
    emitter.instruction("jz __rt_vc_canon_empty_x86");                          // an empty version canonicalizes to nothing
    emitter.instruction("movzx r8d, BYTE PTR [rdi]");                           // load the first version byte
    emitter.instruction("cmp r8d, 35");                                         // a version starting with '#' is compared verbatim
    emitter.instruction("je __rt_vc_canon_raw_x86");                            // copy it without canonicalization, as php-src does
    emitter.instruction("mov BYTE PTR [rdx], r8b");                             // the first byte is always copied as is
    emitter.instruction("mov eax, 1");                                          // canonical output length
    emitter.instruction("mov ecx, 1");                                          // next source byte index
    emitter.instruction("mov r9d, r8d");                                        // previous source byte
    // -- canonicalize loop --
    emitter.label("__rt_vc_canon_loop_x86");
    emitter.instruction("cmp rcx, rsi");                                        // has every source byte been consumed?
    emitter.instruction("jae __rt_vc_canon_strip_x86");                         // drop trailing separators and return
    emitter.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                     // load the next source byte
    emitter.instruction("add rcx, 1");                                          // advance the source index
    emitter.instruction("movzx r10d, BYTE PTR [rdx + rax - 1]");                // load the last canonical byte
    emitter.instruction("cmp r8d, 45");                                         // '-' is a version separator
    emitter.instruction("je __rt_vc_canon_sep_x86");                            // separators become a single '.'
    emitter.instruction("cmp r8d, 95");                                         // '_' is a version separator
    emitter.instruction("je __rt_vc_canon_sep_x86");                            // separators become a single '.'
    emitter.instruction("cmp r8d, 43");                                         // '+' is a version separator
    emitter.instruction("je __rt_vc_canon_sep_x86");                            // separators become a single '.'
    emitter.instruction("mov r11d, r8d");                                       // copy the current byte
    emitter.instruction("sub r11d, 48");                                        // classify the current byte
    emitter.instruction("cmp r11d, 10");                                        // is the current byte a digit?
    emitter.instruction("setb r11b");                                           // r11b = current byte is a digit
    emitter.instruction("mov r10d, r9d");                                       // copy the previous byte
    emitter.instruction("sub r10d, 48");                                        // classify the previous byte
    emitter.instruction("cmp r10d, 10");                                        // is the previous byte a digit?
    emitter.instruction("jb __rt_vc_canon_prev_digit_x86");                     // a digit run may end here
    emitter.instruction("cmp r9d, 46");                                         // was the previous byte a '.'?
    emitter.instruction("je __rt_vc_canon_classified_x86");                     // after a '.' no boundary is inserted
    emitter.instruction("test r11b, r11b");                                     // is the current byte a digit after a non-digit?
    emitter.instruction("jnz __rt_vc_canon_transition_x86");                    // a non-digit/digit boundary inserts a '.'
    emitter.instruction("jmp __rt_vc_canon_classified_x86");                    // no boundary
    // -- canonicalize prev digit --
    emitter.label("__rt_vc_canon_prev_digit_x86");
    emitter.instruction("test r11b, r11b");                                     // is the current byte a digit too?
    emitter.instruction("jnz __rt_vc_canon_classified_x86");                    // digits continue the numeric run
    emitter.instruction("cmp r8d, 46");                                         // is the current byte a '.'?
    emitter.instruction("jne __rt_vc_canon_transition_x86");                    // a digit/non-digit boundary inserts a '.'
    // -- canonicalize classified --
    emitter.label("__rt_vc_canon_classified_x86");
    emitter.instruction("movzx r10d, BYTE PTR [rdx + rax - 1]");                // reload the last canonical byte
    emitter.instruction("test r11b, r11b");                                     // digits are alphanumeric
    emitter.instruction("jnz __rt_vc_canon_copy_x86");                          // copy the digit
    emitter.instruction("mov r11d, r8d");                                       // copy the byte for the letter test
    emitter.instruction("or r11d, 32");                                         // fold ASCII case
    emitter.instruction("sub r11d, 97");                                        // letters map to 0..25
    emitter.instruction("cmp r11d, 26");                                        // is the current byte an ASCII letter?
    emitter.instruction("jae __rt_vc_canon_sep_x86");                           // any other non-alphanumeric byte is a separator
    // -- canonicalize copy --
    emitter.label("__rt_vc_canon_copy_x86");
    emitter.instruction("mov BYTE PTR [rdx + rax], r8b");                       // copy the alphanumeric byte
    emitter.instruction("add rax, 1");                                          // grow the canonical output
    emitter.instruction("jmp __rt_vc_canon_next_x86");                          // continue with the next source byte
    // -- canonicalize transition --
    emitter.label("__rt_vc_canon_transition_x86");
    emitter.instruction("movzx r10d, BYTE PTR [rdx + rax - 1]");                // reload the last canonical byte
    emitter.instruction("cmp r10d, 46");                                        // does the output already end in '.'?
    emitter.instruction("je __rt_vc_canon_copy_x86");                           // never emit two separators in a row
    emitter.instruction("mov BYTE PTR [rdx + rax], 46");                        // separate the digit and non-digit runs
    emitter.instruction("add rax, 1");                                          // grow the canonical output
    emitter.instruction("jmp __rt_vc_canon_copy_x86");                          // then copy the byte itself
    // -- canonicalize separator --
    emitter.label("__rt_vc_canon_sep_x86");
    emitter.instruction("cmp r10d, 46");                                        // does the output already end in '.'?
    emitter.instruction("je __rt_vc_canon_next_x86");                           // never emit two separators in a row
    emitter.instruction("mov BYTE PTR [rdx + rax], 46");                        // emit the separator
    emitter.instruction("add rax, 1");                                          // grow the canonical output
    // -- canonicalize next --
    emitter.label("__rt_vc_canon_next_x86");
    emitter.instruction("mov r9d, r8d");                                        // remember the byte just consumed
    emitter.instruction("jmp __rt_vc_canon_loop_x86");                          // continue with the next source byte
    // -- canonicalize strip --
    emitter.label("__rt_vc_canon_strip_x86");
    emitter.instruction("test rax, rax");                                       // anything left to strip?
    emitter.instruction("jz __rt_vc_canon_done_x86");                           // nothing left
    emitter.instruction("cmp BYTE PTR [rdx + rax - 1], 46");                    // is the last canonical byte a separator?
    emitter.instruction("jne __rt_vc_canon_done_x86");                          // keep a real last token
    emitter.instruction("sub rax, 1");                                          // drop the trailing separator
    emitter.instruction("jmp __rt_vc_canon_strip_x86");                         // drop every trailing separator
    // -- canonicalize done --
    emitter.label("__rt_vc_canon_done_x86");
    emitter.instruction("ret");                                                 // return the canonical length in rax
    // -- canonicalize raw --
    emitter.label("__rt_vc_canon_raw_x86");
    emitter.instruction("xor eax, eax");                                        // verbatim copy index
    // -- canonicalize raw loop --
    emitter.label("__rt_vc_canon_raw_loop_x86");
    emitter.instruction("cmp rax, rsi");                                        // has the whole version been copied?
    emitter.instruction("jae __rt_vc_canon_done_x86");                          // return the verbatim length
    emitter.instruction("movzx r8d, BYTE PTR [rdi + rax]");                     // load the next byte
    emitter.instruction("mov BYTE PTR [rdx + rax], r8b");                       // copy it verbatim
    emitter.instruction("add rax, 1");                                          // advance the copy index
    emitter.instruction("jmp __rt_vc_canon_raw_loop_x86");                      // continue copying
    // -- canonicalize empty --
    emitter.label("__rt_vc_canon_empty_x86");
    emitter.instruction("xor eax, eax");                                        // an empty version has no canonical bytes
    emitter.instruction("ret");                                                 // return to the comparator
    emitter.blank();
    emitter.comment("--- runtime: version_compare special forms ---");
    // -- entry: special --
    emitter.label_global("__rt_vc_special");
    emitter.instruction("cmp rsi, 3");                                          // is the token long enough for 'dev'?
    emitter.instruction("jb __rt_vc_special_skip_0_x86");                       // a shorter token cannot start with 'dev'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 0]");                       // load token byte 0
    emitter.instruction("cmp r8d, 100");                                        // compare it with 'd'
    emitter.instruction("jne __rt_vc_special_skip_0_x86");                      // the token does not start with 'dev'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 1]");                       // load token byte 1
    emitter.instruction("cmp r8d, 101");                                        // compare it with 'e'
    emitter.instruction("jne __rt_vc_special_skip_0_x86");                      // the token does not start with 'dev'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 2]");                       // load token byte 2
    emitter.instruction("cmp r8d, 118");                                        // compare it with 'v'
    emitter.instruction("jne __rt_vc_special_skip_0_x86");                      // the token does not start with 'dev'
    emitter.instruction("mov eax, 0");                                          // 'dev' orders as 0
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 0 --
    emitter.label("__rt_vc_special_skip_0_x86");
    emitter.instruction("cmp rsi, 5");                                          // is the token long enough for 'alpha'?
    emitter.instruction("jb __rt_vc_special_skip_1_x86");                       // a shorter token cannot start with 'alpha'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 0]");                       // load token byte 0
    emitter.instruction("cmp r8d, 97");                                         // compare it with 'a'
    emitter.instruction("jne __rt_vc_special_skip_1_x86");                      // the token does not start with 'alpha'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 1]");                       // load token byte 1
    emitter.instruction("cmp r8d, 108");                                        // compare it with 'l'
    emitter.instruction("jne __rt_vc_special_skip_1_x86");                      // the token does not start with 'alpha'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 2]");                       // load token byte 2
    emitter.instruction("cmp r8d, 112");                                        // compare it with 'p'
    emitter.instruction("jne __rt_vc_special_skip_1_x86");                      // the token does not start with 'alpha'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 3]");                       // load token byte 3
    emitter.instruction("cmp r8d, 104");                                        // compare it with 'h'
    emitter.instruction("jne __rt_vc_special_skip_1_x86");                      // the token does not start with 'alpha'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 4]");                       // load token byte 4
    emitter.instruction("cmp r8d, 97");                                         // compare it with 'a'
    emitter.instruction("jne __rt_vc_special_skip_1_x86");                      // the token does not start with 'alpha'
    emitter.instruction("mov eax, 1");                                          // 'alpha' orders as 1
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 1 --
    emitter.label("__rt_vc_special_skip_1_x86");
    emitter.instruction("cmp rsi, 1");                                          // is the token long enough for 'a'?
    emitter.instruction("jb __rt_vc_special_skip_2_x86");                       // a shorter token cannot start with 'a'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 0]");                       // load token byte 0
    emitter.instruction("cmp r8d, 97");                                         // compare it with 'a'
    emitter.instruction("jne __rt_vc_special_skip_2_x86");                      // the token does not start with 'a'
    emitter.instruction("mov eax, 1");                                          // 'a' orders as 1
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 2 --
    emitter.label("__rt_vc_special_skip_2_x86");
    emitter.instruction("cmp rsi, 4");                                          // is the token long enough for 'beta'?
    emitter.instruction("jb __rt_vc_special_skip_3_x86");                       // a shorter token cannot start with 'beta'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 0]");                       // load token byte 0
    emitter.instruction("cmp r8d, 98");                                         // compare it with 'b'
    emitter.instruction("jne __rt_vc_special_skip_3_x86");                      // the token does not start with 'beta'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 1]");                       // load token byte 1
    emitter.instruction("cmp r8d, 101");                                        // compare it with 'e'
    emitter.instruction("jne __rt_vc_special_skip_3_x86");                      // the token does not start with 'beta'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 2]");                       // load token byte 2
    emitter.instruction("cmp r8d, 116");                                        // compare it with 't'
    emitter.instruction("jne __rt_vc_special_skip_3_x86");                      // the token does not start with 'beta'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 3]");                       // load token byte 3
    emitter.instruction("cmp r8d, 97");                                         // compare it with 'a'
    emitter.instruction("jne __rt_vc_special_skip_3_x86");                      // the token does not start with 'beta'
    emitter.instruction("mov eax, 2");                                          // 'beta' orders as 2
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 3 --
    emitter.label("__rt_vc_special_skip_3_x86");
    emitter.instruction("cmp rsi, 1");                                          // is the token long enough for 'b'?
    emitter.instruction("jb __rt_vc_special_skip_4_x86");                       // a shorter token cannot start with 'b'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 0]");                       // load token byte 0
    emitter.instruction("cmp r8d, 98");                                         // compare it with 'b'
    emitter.instruction("jne __rt_vc_special_skip_4_x86");                      // the token does not start with 'b'
    emitter.instruction("mov eax, 2");                                          // 'b' orders as 2
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 4 --
    emitter.label("__rt_vc_special_skip_4_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the token long enough for 'RC'?
    emitter.instruction("jb __rt_vc_special_skip_5_x86");                       // a shorter token cannot start with 'RC'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 0]");                       // load token byte 0
    emitter.instruction("cmp r8d, 82");                                         // compare it with 'R'
    emitter.instruction("jne __rt_vc_special_skip_5_x86");                      // the token does not start with 'RC'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 1]");                       // load token byte 1
    emitter.instruction("cmp r8d, 67");                                         // compare it with 'C'
    emitter.instruction("jne __rt_vc_special_skip_5_x86");                      // the token does not start with 'RC'
    emitter.instruction("mov eax, 3");                                          // 'RC' orders as 3
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 5 --
    emitter.label("__rt_vc_special_skip_5_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the token long enough for 'rc'?
    emitter.instruction("jb __rt_vc_special_skip_6_x86");                       // a shorter token cannot start with 'rc'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 0]");                       // load token byte 0
    emitter.instruction("cmp r8d, 114");                                        // compare it with 'r'
    emitter.instruction("jne __rt_vc_special_skip_6_x86");                      // the token does not start with 'rc'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 1]");                       // load token byte 1
    emitter.instruction("cmp r8d, 99");                                         // compare it with 'c'
    emitter.instruction("jne __rt_vc_special_skip_6_x86");                      // the token does not start with 'rc'
    emitter.instruction("mov eax, 3");                                          // 'rc' orders as 3
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 6 --
    emitter.label("__rt_vc_special_skip_6_x86");
    emitter.instruction("cmp rsi, 1");                                          // is the token long enough for '#'?
    emitter.instruction("jb __rt_vc_special_skip_7_x86");                       // a shorter token cannot start with '#'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 0]");                       // load token byte 0
    emitter.instruction("cmp r8d, 35");                                         // compare it with '#'
    emitter.instruction("jne __rt_vc_special_skip_7_x86");                      // the token does not start with '#'
    emitter.instruction("mov eax, 4");                                          // '#' orders as 4
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 7 --
    emitter.label("__rt_vc_special_skip_7_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the token long enough for 'pl'?
    emitter.instruction("jb __rt_vc_special_skip_8_x86");                       // a shorter token cannot start with 'pl'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 0]");                       // load token byte 0
    emitter.instruction("cmp r8d, 112");                                        // compare it with 'p'
    emitter.instruction("jne __rt_vc_special_skip_8_x86");                      // the token does not start with 'pl'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 1]");                       // load token byte 1
    emitter.instruction("cmp r8d, 108");                                        // compare it with 'l'
    emitter.instruction("jne __rt_vc_special_skip_8_x86");                      // the token does not start with 'pl'
    emitter.instruction("mov eax, 5");                                          // 'pl' orders as 5
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 8 --
    emitter.label("__rt_vc_special_skip_8_x86");
    emitter.instruction("cmp rsi, 1");                                          // is the token long enough for 'p'?
    emitter.instruction("jb __rt_vc_special_skip_9_x86");                       // a shorter token cannot start with 'p'
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 0]");                       // load token byte 0
    emitter.instruction("cmp r8d, 112");                                        // compare it with 'p'
    emitter.instruction("jne __rt_vc_special_skip_9_x86");                      // the token does not start with 'p'
    emitter.instruction("mov eax, 5");                                          // 'p' orders as 5
    emitter.instruction("ret");                                                 // return the special-form order
    // -- special skip 9 --
    emitter.label("__rt_vc_special_skip_9_x86");
    emitter.instruction("mov rax, -1");                                         // any other token orders below 'dev'
    emitter.instruction("ret");                                                 // return the special-form order
    emitter.blank();
    emitter.comment("--- runtime: version_compare core ---");
    // -- entry: core --
    emitter.label_global("__rt_vc_core");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the comparator frame
    emitter.instruction("push rbx");                                            // save callee-saved rbx (comparison result)
    emitter.instruction("push r12");                                            // save callee-saved r12 (first cursor)
    emitter.instruction("push r13");                                            // save callee-saved r13 (first end)
    emitter.instruction("push r14");                                            // save callee-saved r14 (second cursor)
    emitter.instruction("push r15");                                            // save callee-saved r15 (second end)
    emitter.instruction("sub rsp, 72");                                         // spill slots for the token state, keeping the stack aligned
    emitter.instruction("mov r12, rdi");                                        // r12 = first version cursor
    emitter.instruction("lea r13, [rdi + rsi]");                                // r13 = end of the first version
    emitter.instruction("mov r14, rdx");                                        // r14 = second version cursor
    emitter.instruction("lea r15, [rdx + rcx]");                                // r15 = end of the second version
    emitter.instruction("mov QWORD PTR [rbp - 48], 1");                         // the first version may have more tokens
    emitter.instruction("mov QWORD PTR [rbp - 56], 1");                         // the second version may have more tokens
    emitter.instruction("xor ebx, ebx");                                        // the versions compare equal so far
    // -- core loop --
    emitter.label("__rt_vc_core_loop_x86");
    emitter.instruction("cmp r12, r13");                                        // is the first version exhausted?
    emitter.instruction("jae __rt_vc_core_tail_x86");                           // compare what remains of the other one
    emitter.instruction("cmp r14, r15");                                        // is the second version exhausted?
    emitter.instruction("jae __rt_vc_core_tail_x86");                           // compare what remains of the other one
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // did the first version have a further token?
    emitter.instruction("je __rt_vc_core_tail_x86");                            // no: compare the tails
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // did the second version have a further token?
    emitter.instruction("je __rt_vc_core_tail_x86");                            // no: compare the tails
    emitter.instruction("mov QWORD PTR [rbp - 64], r12");                       // the token starts at the cursor
    emitter.instruction("mov rax, r12");                                        // scan for the token's '.'
    // -- core scan version-1 token scan --
    emitter.label("__rt_vc_core_t1_scan_x86");
    emitter.instruction("cmp rax, r13");                                        // reached the end of the version?
    emitter.instruction("jae __rt_vc_core_t1_last_x86");                        // this is the last token
    emitter.instruction("cmp BYTE PTR [rax], 46");                              // is the next byte the separator?
    emitter.instruction("je __rt_vc_core_t1_dot_x86");                          // the token ends here
    emitter.instruction("add rax, 1");                                          // keep scanning
    emitter.instruction("jmp __rt_vc_core_t1_scan_x86");                        // continue the scan
    // -- core scan version-1 token dot --
    emitter.label("__rt_vc_core_t1_dot_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // the token ends at the separator
    emitter.instruction("lea r12, [rax + 1]");                                  // the cursor moves past the separator
    emitter.instruction("mov QWORD PTR [rbp - 48], 1");                         // another token may follow
    emitter.instruction("jmp __rt_vc_core_t1_done_x86");                        // the token is delimited
    // -- core scan version-1 token last --
    emitter.label("__rt_vc_core_t1_last_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // the token runs to the end of the version
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // no token follows
    // -- core scan version-1 token done --
    emitter.label("__rt_vc_core_t1_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 80], r14");                       // the token starts at the cursor
    emitter.instruction("mov rax, r14");                                        // scan for the token's '.'
    // -- core scan version-2 token scan --
    emitter.label("__rt_vc_core_t2_scan_x86");
    emitter.instruction("cmp rax, r15");                                        // reached the end of the version?
    emitter.instruction("jae __rt_vc_core_t2_last_x86");                        // this is the last token
    emitter.instruction("cmp BYTE PTR [rax], 46");                              // is the next byte the separator?
    emitter.instruction("je __rt_vc_core_t2_dot_x86");                          // the token ends here
    emitter.instruction("add rax, 1");                                          // keep scanning
    emitter.instruction("jmp __rt_vc_core_t2_scan_x86");                        // continue the scan
    // -- core scan version-2 token dot --
    emitter.label("__rt_vc_core_t2_dot_x86");
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // the token ends at the separator
    emitter.instruction("lea r14, [rax + 1]");                                  // the cursor moves past the separator
    emitter.instruction("mov QWORD PTR [rbp - 56], 1");                         // another token may follow
    emitter.instruction("jmp __rt_vc_core_t2_done_x86");                        // the token is delimited
    // -- core scan version-2 token last --
    emitter.label("__rt_vc_core_t2_last_x86");
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // the token runs to the end of the version
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // no token follows
    // -- core scan version-2 token done --
    emitter.label("__rt_vc_core_t2_done_x86");
    emitter.instruction("xor r10, r10");                                        // assume the token does not start with a digit
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // token start
    emitter.instruction("cmp rax, QWORD PTR [rbp - 72]");                       // is the token empty?
    emitter.instruction("jae __rt_vc_core_d1_x86");                             // an empty token is not numeric
    emitter.instruction("movzx eax, BYTE PTR [rax]");                           // load the token's first byte
    emitter.instruction("sub eax, 48");                                         // classify it
    emitter.instruction("cmp eax, 10");                                         // is it a digit?
    emitter.instruction("jae __rt_vc_core_d1_x86");                             // the token is not numeric
    emitter.instruction("mov r10, 1");                                          // the token starts with a digit
    // -- core classify version-1 token --
    emitter.label("__rt_vc_core_d1_x86");
    emitter.instruction("xor r11, r11");                                        // assume the token does not start with a digit
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // token start
    emitter.instruction("cmp rax, QWORD PTR [rbp - 88]");                       // is the token empty?
    emitter.instruction("jae __rt_vc_core_d2_x86");                             // an empty token is not numeric
    emitter.instruction("movzx eax, BYTE PTR [rax]");                           // load the token's first byte
    emitter.instruction("sub eax, 48");                                         // classify it
    emitter.instruction("cmp eax, 10");                                         // is it a digit?
    emitter.instruction("jae __rt_vc_core_d2_x86");                             // the token is not numeric
    emitter.instruction("mov r11, 1");                                          // the token starts with a digit
    // -- core classify version-2 token --
    emitter.label("__rt_vc_core_d2_x86");
    emitter.instruction("mov rax, r10");                                        // first numeric flag
    emitter.instruction("and rax, r11");                                        // are both tokens numeric?
    emitter.instruction("jnz __rt_vc_core_numbers_x86");                        // compare them as integers
    emitter.instruction("mov rax, r10");                                        // first numeric flag
    emitter.instruction("or rax, r11");                                         // is exactly one token numeric?
    emitter.instruction("jnz __rt_vc_core_mixed_x86");                          // a number orders like the special form '#'
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // token start
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // token end
    emitter.instruction("sub rsi, rdi");                                        // token length
    emitter.instruction("call __rt_vc_special");                                // rax = the token's special-form order
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // keep the first token's order
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // token start
    emitter.instruction("mov rsi, QWORD PTR [rbp - 88]");                       // token end
    emitter.instruction("sub rsi, rdi");                                        // token length
    emitter.instruction("call __rt_vc_special");                                // rax = the token's special-form order
    emitter.instruction("mov rcx, rax");                                        // second token's order
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // first token's order
    emitter.instruction("cmp rax, rcx");                                        // compare the two orders
    emitter.instruction("jmp __rt_vc_core_sign_x86");                           // turn the comparison into -1/0/1
    // -- core mixed --
    emitter.label("__rt_vc_core_mixed_x86");
    emitter.instruction("test r10, r10");                                       // which token is numeric?
    emitter.instruction("jz __rt_vc_core_mixed_second_x86");                    // the second token is the numeric one
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // token start
    emitter.instruction("mov rsi, QWORD PTR [rbp - 88]");                       // token end
    emitter.instruction("sub rsi, rdi");                                        // token length
    emitter.instruction("call __rt_vc_special");                                // rax = the token's special-form order
    emitter.instruction("mov rcx, rax");                                        // order of the non-numeric second token
    emitter.instruction("mov eax, 4");                                          // a number orders like '#'
    emitter.instruction("cmp rax, rcx");                                        // compare the number with the second token
    emitter.instruction("jmp __rt_vc_core_sign_x86");                           // turn the comparison into -1/0/1
    // -- core mixed second --
    emitter.label("__rt_vc_core_mixed_second_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // token start
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // token end
    emitter.instruction("sub rsi, rdi");                                        // token length
    emitter.instruction("call __rt_vc_special");                                // rax = the token's special-form order
    emitter.instruction("cmp rax, 4");                                          // compare the first token with a number
    emitter.instruction("jmp __rt_vc_core_sign_x86");                           // turn the comparison into -1/0/1
    // -- core numbers --
    emitter.label("__rt_vc_core_numbers_x86");
    emitter.instruction("xor r10, r10");                                        // accumulated token value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // digit cursor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // token end
    // -- core parse version-1 number loop --
    emitter.label("__rt_vc_core_n1_loop_x86");
    emitter.instruction("cmp rdi, rsi");                                        // has every digit been read?
    emitter.instruction("jae __rt_vc_core_n1_done_x86");                        // the value is complete
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load the next byte
    emitter.instruction("sub eax, 48");                                         // convert it to a digit value
    emitter.instruction("cmp eax, 10");                                         // is it a digit?
    emitter.instruction("jae __rt_vc_core_n1_done_x86");                        // strtol() stops at the first non-digit
    emitter.instruction("add rdi, 1");                                          // advance the digit cursor
    emitter.instruction("mov rcx, 922337203685477580");                         // PHP_INT_MAX / 10
    emitter.instruction("cmp r10, rcx");                                        // would the next digit overflow?
    emitter.instruction("ja __rt_vc_core_n1_saturate_x86");                     // strtol() saturates at PHP_INT_MAX
    emitter.instruction("jb __rt_vc_core_n1_accumulate_x86");                   // plenty of room for another digit
    emitter.instruction("cmp eax, 7");                                          // only digits up to 7 still fit
    emitter.instruction("ja __rt_vc_core_n1_saturate_x86");                     // strtol() saturates at PHP_INT_MAX
    // -- core parse version-1 number accumulate --
    emitter.label("__rt_vc_core_n1_accumulate_x86");
    emitter.instruction("imul r10, r10, 10");                                   // value = value * 10
    emitter.instruction("add r10, rax");                                        // value += digit
    emitter.instruction("jmp __rt_vc_core_n1_loop_x86");                        // read the next digit
    // -- core parse version-1 number saturate --
    emitter.label("__rt_vc_core_n1_saturate_x86");
    emitter.instruction("mov r10, 0x7fffffffffffffff");                         // the token saturates at PHP_INT_MAX
    emitter.instruction("jmp __rt_vc_core_n1_loop_x86");                        // consume the remaining digits
    // -- core parse version-1 number done --
    emitter.label("__rt_vc_core_n1_done_x86");
    emitter.instruction("xor r11, r11");                                        // accumulated token value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // digit cursor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 88]");                       // token end
    // -- core parse version-2 number loop --
    emitter.label("__rt_vc_core_n2_loop_x86");
    emitter.instruction("cmp rdi, rsi");                                        // has every digit been read?
    emitter.instruction("jae __rt_vc_core_n2_done_x86");                        // the value is complete
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load the next byte
    emitter.instruction("sub eax, 48");                                         // convert it to a digit value
    emitter.instruction("cmp eax, 10");                                         // is it a digit?
    emitter.instruction("jae __rt_vc_core_n2_done_x86");                        // strtol() stops at the first non-digit
    emitter.instruction("add rdi, 1");                                          // advance the digit cursor
    emitter.instruction("mov rcx, 922337203685477580");                         // PHP_INT_MAX / 10
    emitter.instruction("cmp r11, rcx");                                        // would the next digit overflow?
    emitter.instruction("ja __rt_vc_core_n2_saturate_x86");                     // strtol() saturates at PHP_INT_MAX
    emitter.instruction("jb __rt_vc_core_n2_accumulate_x86");                   // plenty of room for another digit
    emitter.instruction("cmp eax, 7");                                          // only digits up to 7 still fit
    emitter.instruction("ja __rt_vc_core_n2_saturate_x86");                     // strtol() saturates at PHP_INT_MAX
    // -- core parse version-2 number accumulate --
    emitter.label("__rt_vc_core_n2_accumulate_x86");
    emitter.instruction("imul r11, r11, 10");                                   // value = value * 10
    emitter.instruction("add r11, rax");                                        // value += digit
    emitter.instruction("jmp __rt_vc_core_n2_loop_x86");                        // read the next digit
    // -- core parse version-2 number saturate --
    emitter.label("__rt_vc_core_n2_saturate_x86");
    emitter.instruction("mov r11, 0x7fffffffffffffff");                         // the token saturates at PHP_INT_MAX
    emitter.instruction("jmp __rt_vc_core_n2_loop_x86");                        // consume the remaining digits
    // -- core parse version-2 number done --
    emitter.label("__rt_vc_core_n2_done_x86");
    emitter.instruction("cmp r10, r11");                                        // compare the two numbers
    // -- core sign --
    emitter.label("__rt_vc_core_sign_x86");
    emitter.instruction("mov ebx, 0");                                          // equal compares as 0 (mov keeps the flags)
    emitter.instruction("setg bl");                                             // 1 when the first side is greater
    emitter.instruction("mov rcx, -1");                                         // -1 for a smaller first side (mov keeps the flags)
    emitter.instruction("cmovl rbx, rcx");                                      // select -1 when the first side is smaller
    emitter.instruction("test rbx, rbx");                                       // did this pair decide the comparison?
    emitter.instruction("jnz __rt_vc_core_done_x86");                           // the first difference decides
    emitter.instruction("jmp __rt_vc_core_loop_x86");                           // compare the next pair of tokens
    // -- core tail --
    emitter.label("__rt_vc_core_tail_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // does the first version have tokens left?
    emitter.instruction("jne __rt_vc_core_tail_first_x86");                     // compare the first version's rest
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // does the second version have tokens left?
    emitter.instruction("jne __rt_vc_core_tail_second_x86");                    // compare the second version's rest
    emitter.instruction("xor ebx, ebx");                                        // both versions ended together
    emitter.instruction("jmp __rt_vc_core_done_x86");                           // the versions are equal
    // -- core tail second --
    emitter.label("__rt_vc_core_tail_second_x86");
    emitter.instruction("mov r12, r14");                                        // continue with the second version's rest
    emitter.instruction("mov r13, r15");                                        // and its end
    emitter.instruction("mov QWORD PTR [rbp - 96], -1");                        // the remaining side is the second one
    emitter.instruction("jmp __rt_vc_core_rest_x86");                           // compare the rest against the '#' form
    // -- core tail first --
    emitter.label("__rt_vc_core_tail_first_x86");
    emitter.instruction("mov QWORD PTR [rbp - 96], 1");                         // the remaining side is the first one
    // -- core rest --
    emitter.label("__rt_vc_core_rest_x86");
    emitter.instruction("cmp r12, r13");                                        // is the remaining version empty?
    emitter.instruction("jb __rt_vc_core_rest_token_x86");                      // compare its next token
    emitter.instruction("mov rbx, QWORD PTR [rbp - 96]");                       // the remaining side
    emitter.instruction("neg rbx");                                             // an empty rest orders below the '#' form
    emitter.instruction("jmp __rt_vc_core_done_x86");                           // the comparison is decided
    // -- core rest token --
    emitter.label("__rt_vc_core_rest_token_x86");
    emitter.instruction("mov QWORD PTR [rbp - 64], r12");                       // the token starts at the cursor
    emitter.instruction("mov rax, r12");                                        // scan for the token's '.'
    // -- core rest token scan --
    emitter.label("__rt_vc_core_r_scan_x86");
    emitter.instruction("cmp rax, r13");                                        // reached the end of the version?
    emitter.instruction("jae __rt_vc_core_r_last_x86");                         // this is the last token
    emitter.instruction("cmp BYTE PTR [rax], 46");                              // is the next byte the separator?
    emitter.instruction("je __rt_vc_core_r_dot_x86");                           // the token ends here
    emitter.instruction("add rax, 1");                                          // keep scanning
    emitter.instruction("jmp __rt_vc_core_r_scan_x86");                         // continue the scan
    // -- core rest token dot --
    emitter.label("__rt_vc_core_r_dot_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // the token ends at the separator
    emitter.instruction("lea r12, [rax + 1]");                                  // the cursor moves past the separator
    emitter.instruction("mov QWORD PTR [rbp - 48], 1");                         // another token may follow
    emitter.instruction("jmp __rt_vc_core_r_done_x86");                         // the token is delimited
    // -- core rest token last --
    emitter.label("__rt_vc_core_r_last_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // the token runs to the end of the version
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // no token follows
    // -- core rest token done --
    emitter.label("__rt_vc_core_r_done_x86");
    emitter.instruction("xor r10, r10");                                        // assume the token does not start with a digit
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // token start
    emitter.instruction("cmp rax, QWORD PTR [rbp - 72]");                       // is the token empty?
    emitter.instruction("jae __rt_vc_core_rd_x86");                             // an empty token is not numeric
    emitter.instruction("movzx eax, BYTE PTR [rax]");                           // load the token's first byte
    emitter.instruction("sub eax, 48");                                         // classify it
    emitter.instruction("cmp eax, 10");                                         // is it a digit?
    emitter.instruction("jae __rt_vc_core_rd_x86");                             // the token is not numeric
    emitter.instruction("mov r10, 1");                                          // the token starts with a digit
    // -- core rest digits --
    emitter.label("__rt_vc_core_rd_x86");
    emitter.instruction("test r10, r10");                                       // is the remaining token numeric?
    emitter.instruction("jz __rt_vc_core_rest_special_x86");                    // a non-numeric token is compared by its order
    emitter.instruction("mov rbx, QWORD PTR [rbp - 96]");                       // a numeric token makes its side the greater one
    emitter.instruction("jmp __rt_vc_core_done_x86");                           // the comparison is decided
    // -- core rest special --
    emitter.label("__rt_vc_core_rest_special_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // token start
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // token end
    emitter.instruction("sub rsi, rdi");                                        // token length
    emitter.instruction("call __rt_vc_special");                                // rax = the token's special-form order
    emitter.instruction("cmp rax, 4");                                          // compare the token with the '#' form
    emitter.instruction("mov ebx, 0");                                          // equal compares as 0 (mov keeps the flags)
    emitter.instruction("setg bl");                                             // 1 when the token orders above '#'
    emitter.instruction("mov rcx, -1");                                         // -1 below (mov keeps the flags)
    emitter.instruction("cmovl rbx, rcx");                                      // select -1 when the token orders below '#'
    emitter.instruction("imul rbx, QWORD PTR [rbp - 96]");                      // orient the result by the remaining side
    emitter.instruction("test rbx, rbx");                                       // did this token decide the comparison?
    emitter.instruction("jnz __rt_vc_core_done_x86");                           // the first difference decides
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // is another token left?
    emitter.instruction("je __rt_vc_core_done_x86");                            // no token is left: the versions are equal
    emitter.instruction("jmp __rt_vc_core_rest_x86");                           // compare the next remaining token
    // -- core done --
    emitter.label("__rt_vc_core_done_x86");
    emitter.instruction("mov rax, rbx");                                        // return -1, 0 or 1
    emitter.instruction("add rsp, 72");                                         // release the spill slots
    emitter.instruction("pop r15");                                             // restore callee-saved r15
    emitter.instruction("pop r14");                                             // restore callee-saved r14
    emitter.instruction("pop r13");                                             // restore callee-saved r13
    emitter.instruction("pop r12");                                             // restore callee-saved r12
    emitter.instruction("pop rbx");                                             // restore callee-saved rbx
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the comparison
    emitter.blank();
    emitter.comment("--- runtime: version_compare ---");
    // -- entry: entry --
    emitter.label_global("__rt_version_compare");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the entry frame
    emitter.instruction("sub rsp, 96");                                         // spill slots for the arguments and scratch buffer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the first version pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the first version length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the second version pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the second version length
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the operator pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the operator length
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // save whether an operator was given
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the version pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the version length
    emitter.instruction("xor eax, eax");                                        // scan for an embedded NUL
    // -- version-1 length --
    emitter.label("__rt_version_compare_len1_x86");
    emitter.instruction("cmp rax, rsi");                                        // reached the end of the PHP string?
    emitter.instruction("jae __rt_version_compare_len1_done_x86");              // no NUL byte inside
    emitter.instruction("cmp BYTE PTR [rdi + rax], 0");                         // is this a NUL byte?
    emitter.instruction("je __rt_version_compare_len1_done_x86");               // php-src stops at the first NUL byte
    emitter.instruction("add rax, 1");                                          // advance the scan
    emitter.instruction("jmp __rt_version_compare_len1_x86");                   // keep scanning
    // -- version-1 length done --
    emitter.label("__rt_version_compare_len1_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // keep the C-string length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the version pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the version length
    emitter.instruction("xor eax, eax");                                        // scan for an embedded NUL
    // -- version-2 length --
    emitter.label("__rt_version_compare_len2_x86");
    emitter.instruction("cmp rax, rsi");                                        // reached the end of the PHP string?
    emitter.instruction("jae __rt_version_compare_len2_done_x86");              // no NUL byte inside
    emitter.instruction("cmp BYTE PTR [rdi + rax], 0");                         // is this a NUL byte?
    emitter.instruction("je __rt_version_compare_len2_done_x86");               // php-src stops at the first NUL byte
    emitter.instruction("add rax, 1");                                          // advance the scan
    emitter.instruction("jmp __rt_version_compare_len2_x86");                   // keep scanning
    // -- version-2 length done --
    emitter.label("__rt_version_compare_len2_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // keep the C-string length
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // is the first version empty?
    emitter.instruction("jne __rt_version_compare_first_present_x86");          // a non-empty first version is canonicalized
    emitter.instruction("xor eax, eax");                                        // two empty versions are equal
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // is the second version empty too?
    emitter.instruction("je __rt_version_compare_result_x86");                  // apply the operator
    emitter.instruction("mov rax, -1");                                         // an empty version orders below any other
    emitter.instruction("jmp __rt_version_compare_result_x86");                 // apply the operator
    // -- first present --
    emitter.label("__rt_version_compare_first_present_x86");
    emitter.instruction("mov eax, 1");                                          // any version orders above an empty one
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // is the second version empty?
    emitter.instruction("je __rt_version_compare_result_x86");                  // apply the operator
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // first version length
    emitter.instruction("add rax, QWORD PTR [rbp - 32]");                       // plus the second version length
    emitter.instruction("lea rax, [rax * 2 + 2]");                              // canonicalization at most doubles each version
    emitter.instruction("call __rt_heap_alloc");                                // rax = scratch buffer for both canonical forms
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the scratch buffer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // first version
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // first version length
    emitter.instruction("mov rdx, rax");                                        // its canonical form starts the buffer
    emitter.instruction("call __rt_vc_canon");                                  // rax = first canonical length
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the first canonical length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // first version length
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // scratch buffer
    emitter.instruction("lea rdx, [rax + rdx * 2 + 1]");                        // skip the first canonical area
    emitter.instruction("mov QWORD PTR [rbp - 80], rdx");                       // save the second canonical start
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // second version
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // second version length
    emitter.instruction("call __rt_vc_canon");                                  // rax = second canonical length
    emitter.instruction("mov rcx, rax");                                        // second canonical length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // second canonical start
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // first canonical start
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // first canonical length
    emitter.instruction("call __rt_vc_core");                                   // rax = -1, 0 or 1
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // keep the comparison across the release
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // the scratch buffer
    emitter.instruction("call __rt_heap_free");                                 // release the canonical forms
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the comparison
    // -- result --
    emitter.label("__rt_version_compare_result_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // was an operator given?
    emitter.instruction("je __rt_version_compare_ok_x86");                      // without one the comparison is the result
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // operator pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // operator length
    emitter.instruction("cmp rsi, 1");                                          // is the operator as long as '<'?
    emitter.instruction("jne __rt_version_compare_op_skip_0_x86");              // a different length cannot be '<'
    emitter.instruction("cmp BYTE PTR [rdi + 0], 60");                          // compare operator byte 0 with '<'
    emitter.instruction("jne __rt_version_compare_op_skip_0_x86");              // the operator is not '<'
    emitter.instruction("jmp __rt_version_compare_lt_x86");                     // '<' selects the lt predicate
    // -- operator skip 0 --
    emitter.label("__rt_version_compare_op_skip_0_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the operator as long as 'lt'?
    emitter.instruction("jne __rt_version_compare_op_skip_1_x86");              // a different length cannot be 'lt'
    emitter.instruction("cmp BYTE PTR [rdi + 0], 108");                         // compare operator byte 0 with 'l'
    emitter.instruction("jne __rt_version_compare_op_skip_1_x86");              // the operator is not 'lt'
    emitter.instruction("cmp BYTE PTR [rdi + 1], 116");                         // compare operator byte 1 with 't'
    emitter.instruction("jne __rt_version_compare_op_skip_1_x86");              // the operator is not 'lt'
    emitter.instruction("jmp __rt_version_compare_lt_x86");                     // 'lt' selects the lt predicate
    // -- operator skip 1 --
    emitter.label("__rt_version_compare_op_skip_1_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the operator as long as '<='?
    emitter.instruction("jne __rt_version_compare_op_skip_2_x86");              // a different length cannot be '<='
    emitter.instruction("cmp BYTE PTR [rdi + 0], 60");                          // compare operator byte 0 with '<'
    emitter.instruction("jne __rt_version_compare_op_skip_2_x86");              // the operator is not '<='
    emitter.instruction("cmp BYTE PTR [rdi + 1], 61");                          // compare operator byte 1 with '='
    emitter.instruction("jne __rt_version_compare_op_skip_2_x86");              // the operator is not '<='
    emitter.instruction("jmp __rt_version_compare_le_x86");                     // '<=' selects the le predicate
    // -- operator skip 2 --
    emitter.label("__rt_version_compare_op_skip_2_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the operator as long as 'le'?
    emitter.instruction("jne __rt_version_compare_op_skip_3_x86");              // a different length cannot be 'le'
    emitter.instruction("cmp BYTE PTR [rdi + 0], 108");                         // compare operator byte 0 with 'l'
    emitter.instruction("jne __rt_version_compare_op_skip_3_x86");              // the operator is not 'le'
    emitter.instruction("cmp BYTE PTR [rdi + 1], 101");                         // compare operator byte 1 with 'e'
    emitter.instruction("jne __rt_version_compare_op_skip_3_x86");              // the operator is not 'le'
    emitter.instruction("jmp __rt_version_compare_le_x86");                     // 'le' selects the le predicate
    // -- operator skip 3 --
    emitter.label("__rt_version_compare_op_skip_3_x86");
    emitter.instruction("cmp rsi, 1");                                          // is the operator as long as '>'?
    emitter.instruction("jne __rt_version_compare_op_skip_4_x86");              // a different length cannot be '>'
    emitter.instruction("cmp BYTE PTR [rdi + 0], 62");                          // compare operator byte 0 with '>'
    emitter.instruction("jne __rt_version_compare_op_skip_4_x86");              // the operator is not '>'
    emitter.instruction("jmp __rt_version_compare_gt_x86");                     // '>' selects the gt predicate
    // -- operator skip 4 --
    emitter.label("__rt_version_compare_op_skip_4_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the operator as long as 'gt'?
    emitter.instruction("jne __rt_version_compare_op_skip_5_x86");              // a different length cannot be 'gt'
    emitter.instruction("cmp BYTE PTR [rdi + 0], 103");                         // compare operator byte 0 with 'g'
    emitter.instruction("jne __rt_version_compare_op_skip_5_x86");              // the operator is not 'gt'
    emitter.instruction("cmp BYTE PTR [rdi + 1], 116");                         // compare operator byte 1 with 't'
    emitter.instruction("jne __rt_version_compare_op_skip_5_x86");              // the operator is not 'gt'
    emitter.instruction("jmp __rt_version_compare_gt_x86");                     // 'gt' selects the gt predicate
    // -- operator skip 5 --
    emitter.label("__rt_version_compare_op_skip_5_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the operator as long as '>='?
    emitter.instruction("jne __rt_version_compare_op_skip_6_x86");              // a different length cannot be '>='
    emitter.instruction("cmp BYTE PTR [rdi + 0], 62");                          // compare operator byte 0 with '>'
    emitter.instruction("jne __rt_version_compare_op_skip_6_x86");              // the operator is not '>='
    emitter.instruction("cmp BYTE PTR [rdi + 1], 61");                          // compare operator byte 1 with '='
    emitter.instruction("jne __rt_version_compare_op_skip_6_x86");              // the operator is not '>='
    emitter.instruction("jmp __rt_version_compare_ge_x86");                     // '>=' selects the ge predicate
    // -- operator skip 6 --
    emitter.label("__rt_version_compare_op_skip_6_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the operator as long as 'ge'?
    emitter.instruction("jne __rt_version_compare_op_skip_7_x86");              // a different length cannot be 'ge'
    emitter.instruction("cmp BYTE PTR [rdi + 0], 103");                         // compare operator byte 0 with 'g'
    emitter.instruction("jne __rt_version_compare_op_skip_7_x86");              // the operator is not 'ge'
    emitter.instruction("cmp BYTE PTR [rdi + 1], 101");                         // compare operator byte 1 with 'e'
    emitter.instruction("jne __rt_version_compare_op_skip_7_x86");              // the operator is not 'ge'
    emitter.instruction("jmp __rt_version_compare_ge_x86");                     // 'ge' selects the ge predicate
    // -- operator skip 7 --
    emitter.label("__rt_version_compare_op_skip_7_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the operator as long as '=='?
    emitter.instruction("jne __rt_version_compare_op_skip_8_x86");              // a different length cannot be '=='
    emitter.instruction("cmp BYTE PTR [rdi + 0], 61");                          // compare operator byte 0 with '='
    emitter.instruction("jne __rt_version_compare_op_skip_8_x86");              // the operator is not '=='
    emitter.instruction("cmp BYTE PTR [rdi + 1], 61");                          // compare operator byte 1 with '='
    emitter.instruction("jne __rt_version_compare_op_skip_8_x86");              // the operator is not '=='
    emitter.instruction("jmp __rt_version_compare_eq_x86");                     // '==' selects the eq predicate
    // -- operator skip 8 --
    emitter.label("__rt_version_compare_op_skip_8_x86");
    emitter.instruction("cmp rsi, 1");                                          // is the operator as long as '='?
    emitter.instruction("jne __rt_version_compare_op_skip_9_x86");              // a different length cannot be '='
    emitter.instruction("cmp BYTE PTR [rdi + 0], 61");                          // compare operator byte 0 with '='
    emitter.instruction("jne __rt_version_compare_op_skip_9_x86");              // the operator is not '='
    emitter.instruction("jmp __rt_version_compare_eq_x86");                     // '=' selects the eq predicate
    // -- operator skip 9 --
    emitter.label("__rt_version_compare_op_skip_9_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the operator as long as 'eq'?
    emitter.instruction("jne __rt_version_compare_op_skip_10_x86");             // a different length cannot be 'eq'
    emitter.instruction("cmp BYTE PTR [rdi + 0], 101");                         // compare operator byte 0 with 'e'
    emitter.instruction("jne __rt_version_compare_op_skip_10_x86");             // the operator is not 'eq'
    emitter.instruction("cmp BYTE PTR [rdi + 1], 113");                         // compare operator byte 1 with 'q'
    emitter.instruction("jne __rt_version_compare_op_skip_10_x86");             // the operator is not 'eq'
    emitter.instruction("jmp __rt_version_compare_eq_x86");                     // 'eq' selects the eq predicate
    // -- operator skip 10 --
    emitter.label("__rt_version_compare_op_skip_10_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the operator as long as '!='?
    emitter.instruction("jne __rt_version_compare_op_skip_11_x86");             // a different length cannot be '!='
    emitter.instruction("cmp BYTE PTR [rdi + 0], 33");                          // compare operator byte 0 with '!'
    emitter.instruction("jne __rt_version_compare_op_skip_11_x86");             // the operator is not '!='
    emitter.instruction("cmp BYTE PTR [rdi + 1], 61");                          // compare operator byte 1 with '='
    emitter.instruction("jne __rt_version_compare_op_skip_11_x86");             // the operator is not '!='
    emitter.instruction("jmp __rt_version_compare_ne_x86");                     // '!=' selects the ne predicate
    // -- operator skip 11 --
    emitter.label("__rt_version_compare_op_skip_11_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the operator as long as '<>'?
    emitter.instruction("jne __rt_version_compare_op_skip_12_x86");             // a different length cannot be '<>'
    emitter.instruction("cmp BYTE PTR [rdi + 0], 60");                          // compare operator byte 0 with '<'
    emitter.instruction("jne __rt_version_compare_op_skip_12_x86");             // the operator is not '<>'
    emitter.instruction("cmp BYTE PTR [rdi + 1], 62");                          // compare operator byte 1 with '>'
    emitter.instruction("jne __rt_version_compare_op_skip_12_x86");             // the operator is not '<>'
    emitter.instruction("jmp __rt_version_compare_ne_x86");                     // '<>' selects the ne predicate
    // -- operator skip 12 --
    emitter.label("__rt_version_compare_op_skip_12_x86");
    emitter.instruction("cmp rsi, 2");                                          // is the operator as long as 'ne'?
    emitter.instruction("jne __rt_version_compare_op_skip_13_x86");             // a different length cannot be 'ne'
    emitter.instruction("cmp BYTE PTR [rdi + 0], 110");                         // compare operator byte 0 with 'n'
    emitter.instruction("jne __rt_version_compare_op_skip_13_x86");             // the operator is not 'ne'
    emitter.instruction("cmp BYTE PTR [rdi + 1], 101");                         // compare operator byte 1 with 'e'
    emitter.instruction("jne __rt_version_compare_op_skip_13_x86");             // the operator is not 'ne'
    emitter.instruction("jmp __rt_version_compare_ne_x86");                     // 'ne' selects the ne predicate
    // -- operator skip 13 --
    emitter.label("__rt_version_compare_op_skip_13_x86");
    emitter.instruction("xor eax, eax");                                        // no result for an unknown operator
    emitter.instruction("mov edx, 1");                                          // report the invalid operator so the caller throws
    emitter.instruction("jmp __rt_version_compare_return_x86");                 // leave the entry frame
    // -- lt --
    emitter.label("__rt_version_compare_lt_x86");
    emitter.instruction("cmp rax, -1");                                         // compare the result with -1
    emitter.instruction("sete al");                                             // evaluate the 'lt' predicate
    emitter.instruction("movzx eax, al");                                       // widen the boolean
    emitter.instruction("jmp __rt_version_compare_ok_x86");                     // return the boolean
    // -- le --
    emitter.label("__rt_version_compare_le_x86");
    emitter.instruction("cmp rax, 1");                                          // compare the result with 1
    emitter.instruction("setne al");                                            // evaluate the 'le' predicate
    emitter.instruction("movzx eax, al");                                       // widen the boolean
    emitter.instruction("jmp __rt_version_compare_ok_x86");                     // return the boolean
    // -- gt --
    emitter.label("__rt_version_compare_gt_x86");
    emitter.instruction("cmp rax, 1");                                          // compare the result with 1
    emitter.instruction("sete al");                                             // evaluate the 'gt' predicate
    emitter.instruction("movzx eax, al");                                       // widen the boolean
    emitter.instruction("jmp __rt_version_compare_ok_x86");                     // return the boolean
    // -- ge --
    emitter.label("__rt_version_compare_ge_x86");
    emitter.instruction("cmp rax, -1");                                         // compare the result with -1
    emitter.instruction("setne al");                                            // evaluate the 'ge' predicate
    emitter.instruction("movzx eax, al");                                       // widen the boolean
    emitter.instruction("jmp __rt_version_compare_ok_x86");                     // return the boolean
    // -- eq --
    emitter.label("__rt_version_compare_eq_x86");
    emitter.instruction("cmp rax, 0");                                          // compare the result with 0
    emitter.instruction("sete al");                                             // evaluate the 'eq' predicate
    emitter.instruction("movzx eax, al");                                       // widen the boolean
    emitter.instruction("jmp __rt_version_compare_ok_x86");                     // return the boolean
    // -- ne --
    emitter.label("__rt_version_compare_ne_x86");
    emitter.instruction("cmp rax, 0");                                          // compare the result with 0
    emitter.instruction("setne al");                                            // evaluate the 'ne' predicate
    emitter.instruction("movzx eax, al");                                       // widen the boolean
    emitter.instruction("jmp __rt_version_compare_ok_x86");                     // return the boolean
    // -- ok --
    emitter.label("__rt_version_compare_ok_x86");
    emitter.instruction("xor edx, edx");                                        // report success
    // -- return --
    emitter.label("__rt_version_compare_return_x86");
    emitter.instruction("add rsp, 96");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the comparison or predicate
}
