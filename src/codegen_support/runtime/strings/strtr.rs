//! Purpose:
//! Emits the `strtr()` runtime helpers: `__rt_strtr_pairwise` for the three-argument byte
//! translation form, and `__rt_strtr_hash` / `__rt_strtr_array` plus the shared
//! `__rt_strtr_probe` and `__rt_strtr_int_key_len` for the replacement-pair form.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The pairwise form builds a 256-byte translation table (identity, then `$from[i] ->
//!   $to[i]` for `i < min(len($from), len($to))`, last mapping wins) and rewrites the subject
//!   byte by byte, so the result is always exactly as long as the subject.
//! - The pair form reproduces php-src's single left-to-right pass with longest-match-first
//!   selection and no re-substitution: at each position the longest key length that still
//!   fits is probed down to the shortest, the replacement is copied verbatim, and scanning
//!   resumes after the MATCHED key. Keys shorter than one byte or longer than the whole
//!   subject are ignored, exactly as php-src ignores them.
//! - Integer keys are matched through `__rt_hash_normalize_key`, which maps the probed
//!   substring back onto the canonical integer key the hash actually stores, so
//!   `strtr("12345", [1 => "one"])` behaves like php-src.
//! - The replacement result length is not known up front, so the pair form runs the match
//!   loop twice: once to size the result exactly and once to write it. That keeps the
//!   `__rt_concat_reserve` reservation exact instead of appending unbounded into the shared
//!   scratch buffer.
//! - Every result is copied into owned heap storage by `__rt_str_persist` and the superseded
//!   reservation is released through `__rt_heap_free_safe`, which matches the `Fresh`
//!   ownership contract on `RuntimeFnId::Strtr` and keeps results larger than the 64 KiB
//!   scratch buffer from leaking their heap fallback block.
//! - php-src also emits `Warning: strtr(): Ignoring replacement of empty string` for a
//!   zero-length key. elephc skips the key with the same observable result but does not
//!   emit that warning.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits every `strtr()` runtime helper for the active target.
pub fn emit_strtr(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strtr_int_key_len_x86_64(emitter);
        emit_strtr_probe_x86_64(emitter);
        emit_strtr_pairwise_x86_64(emitter);
        emit_strtr_hash_x86_64(emitter);
        emit_strtr_array_x86_64(emitter);
        return;
    }
    emit_strtr_int_key_len_aarch64(emitter);
    emit_strtr_probe_aarch64(emitter);
    emit_strtr_pairwise_aarch64(emitter);
    emit_strtr_hash_aarch64(emitter);
    emit_strtr_array_aarch64(emitter);
}

/// Emits the AArch64 `__rt_strtr_int_key_len` helper.
///
/// Returns how many bytes php-src would use to spell one integer array key, so an integer
/// key participates in the same longest-match-first length window as a string key.
///
/// - Input: `x0` = integer key.
/// - Output: `x0` = decimal digit count, plus one for a negative sign.
/// - Leaf routine: no frame, no calls.
fn emit_strtr_int_key_len_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtr_int_key_len ---");
    emitter.label_global("__rt_strtr_int_key_len");

    emitter.instruction("mov x9, #0");                                          // positive keys spend no byte on a sign
    emitter.instruction("cmp x0, #0");                                          // is this a negative integer key?
    emitter.instruction("b.ge __rt_strtr_int_key_len_magnitude");               // positive keys are already their own magnitude
    emitter.instruction("neg x0, x0");                                          // measure the magnitude of a negative key
    emitter.instruction("mov x9, #1");                                          // a negative key also spells a leading '-'

    emitter.label("__rt_strtr_int_key_len_magnitude");
    emitter.instruction("mov x10, #1");                                         // every integer spells at least one digit
    emitter.instruction("mov x11, #10");                                        // decimal radix

    emitter.label("__rt_strtr_int_key_len_loop");
    emitter.instruction("cmp x0, #10");                                         // is there another decimal digit left?
    emitter.instruction("b.lo __rt_strtr_int_key_len_done");                    // a value below ten has no further digits
    emitter.instruction("udiv x0, x0, x11");                                    // drop the lowest decimal digit
    emitter.instruction("add x10, x10, #1");                                    // count the dropped digit
    emitter.instruction("b __rt_strtr_int_key_len_loop");                       // keep counting decimal digits

    emitter.label("__rt_strtr_int_key_len_done");
    emitter.instruction("add x0, x10, x9");                                     // total spelled length = digits + optional sign
    emitter.instruction("ret");                                                 // return the spelled key length
}

/// Emits the AArch64 `__rt_strtr_probe` helper.
///
/// Finds php-src's longest matching replacement key at one subject position.
///
/// - Input: `x0` = pairs hash, `x1` = position pointer, `x2` = remaining bytes,
///   `x3` = shortest usable key length, `x4` = longest usable key length.
/// - Output: `x0` = matched key length (`0` when nothing matched), `x1` = replacement
///   pointer, `x2` = replacement length.
fn emit_strtr_probe_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtr_probe ---");
    emitter.label_global("__rt_strtr_probe");

    emitter.instruction("sub sp, sp, #64");                                     // allocate the probe frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save the frame pointer and return address across the lookup calls
    emitter.instruction("add x29, sp, #48");                                    // establish the probe helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the pairs hash across the lookup calls
    emitter.instruction("str x1, [sp, #8]");                                    // save the probed subject position
    emitter.instruction("str x3, [sp, #16]");                                   // save the shortest usable key length
    emitter.instruction("cmp x4, x2");                                          // does the longest key still fit the remaining subject?
    emitter.instruction("csel x5, x2, x4, hi");                                 // clamp the first probed length to what remains
    emitter.instruction("str x5, [sp, #24]");                                   // save the current candidate key length

    emitter.label("__rt_strtr_probe_loop");
    emitter.instruction("ldr x5, [sp, #24]");                                   // reload the current candidate key length
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the shortest usable key length
    emitter.instruction("cmp x5, x3");                                          // have all candidate lengths been tried?
    emitter.instruction("b.lo __rt_strtr_probe_miss");                          // nothing matches at this subject position
    emitter.instruction("ldr x1, [sp, #8]");                                    // the candidate substring starts at the probed position
    emitter.instruction("mov x2, x5");                                          // the candidate substring is as long as the current length
    emitter.instruction("bl __rt_hash_normalize_key");                          // map numeric substrings onto the integer keys the hash stores
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the pairs hash for the lookup
    emitter.instruction("bl __rt_hash_get");                                    // x0 = found, x1 = replacement pointer, x2 = replacement length
    emitter.instruction("cbnz x0, __rt_strtr_probe_hit");                       // the longest matching key wins
    emitter.instruction("ldr x5, [sp, #24]");                                   // reload the current candidate key length
    emitter.instruction("sub x5, x5, #1");                                      // try the next shorter key length
    emitter.instruction("str x5, [sp, #24]");                                   // publish the shortened candidate length
    emitter.instruction("b __rt_strtr_probe_loop");                             // keep probing shorter keys

    emitter.label("__rt_strtr_probe_hit");
    emitter.instruction("ldr x0, [sp, #24]");                                   // report the matched key length without disturbing the replacement pair
    emitter.instruction("b __rt_strtr_probe_done");                             // the probe is finished

    emitter.label("__rt_strtr_probe_miss");
    emitter.instruction("mov x0, xzr");                                         // report that no replacement key matched here
    emitter.instruction("mov x1, xzr");                                         // no replacement pointer for a miss
    emitter.instruction("mov x2, xzr");                                         // no replacement length for a miss

    emitter.label("__rt_strtr_probe_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the probe frame
    emitter.instruction("ret");                                                 // return the probe outcome
}

/// Emits the AArch64 `__rt_strtr_pairwise` helper for `strtr($string, $from, $to)`.
///
/// - Input: `x1`/`x2` = subject, `x3`/`x4` = `$from`, `x5`/`x6` = `$to`.
/// - Output: `x1`/`x2` = owned translated string.
fn emit_strtr_pairwise_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtr_pairwise ---");
    emitter.label_global("__rt_strtr_pairwise");

    // Frame layout (320 bytes):
    //   [sp, #0]   = 256-byte byte translation table
    //   [sp, #256] = subject pointer
    //   [sp, #264] = subject length
    //   [sp, #272] = reservation start
    //   [sp, #280] = owned result pointer
    //   [sp, #288] = owned result length
    //   [sp, #304] = saved x29/x30
    emitter.instruction("sub sp, sp, #320");                                    // allocate the translation frame
    emitter.instruction("stp x29, x30, [sp, #304]");                            // save the frame pointer and return address across the reservation calls
    emitter.instruction("add x29, sp, #304");                                   // establish the pairwise helper frame pointer
    emitter.instruction("str x1, [sp, #256]");                                  // save the subject pointer across the reservation call
    emitter.instruction("str x2, [sp, #264]");                                  // save the subject length across the reservation call

    // -- seed the translation table with the identity mapping --
    emitter.instruction("mov x10, sp");                                         // x10 = translation table base
    emitter.instruction("mov x9, #0");                                          // start at byte value zero
    emitter.label("__rt_strtr_pairwise_identity");
    emitter.instruction("strb w9, [x10, x9]");                                  // an unmapped byte translates to itself
    emitter.instruction("add x9, x9, #1");                                      // advance to the next byte value
    emitter.instruction("cmp x9, #256");                                        // is the identity table complete?
    emitter.instruction("b.lo __rt_strtr_pairwise_identity");                   // keep seeding the identity mapping

    // -- overwrite the mapped bytes; php-src truncates to the shorter of the two lists --
    emitter.instruction("cmp x4, x6");                                          // which of $from and $to is shorter?
    emitter.instruction("csel x7, x6, x4, hi");                                 // the mapping covers only min(len($from), len($to)) bytes
    emitter.instruction("mov x9, #0");                                          // start at the first mapped pair
    emitter.label("__rt_strtr_pairwise_map");
    emitter.instruction("cmp x9, x7");                                          // has the whole mapping been applied?
    emitter.instruction("b.hs __rt_strtr_pairwise_map_done");                   // the translation table is complete
    emitter.instruction("ldrb w11, [x3, x9]");                                  // load the source byte of this pair
    emitter.instruction("ldrb w12, [x5, x9]");                                  // load the destination byte of this pair
    emitter.instruction("strb w12, [x10, x11]");                                // a later pair for the same source byte wins, as in php-src
    emitter.instruction("add x9, x9, #1");                                      // advance to the next mapped pair
    emitter.instruction("b __rt_strtr_pairwise_map");                           // keep applying the mapping
    emitter.label("__rt_strtr_pairwise_map_done");

    // -- the result is always exactly as long as the subject --
    emitter.instruction("ldr x0, [sp, #264]");                                  // request exactly the subject byte count
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the translated result
    emitter.instruction("str x0, [sp, #272]");                                  // save the reservation start
    emitter.instruction("mov x13, x0");                                         // destination cursor
    emitter.instruction("mov x10, sp");                                         // restore the translation table base after the reservation call
    emitter.instruction("ldr x1, [sp, #256]");                                  // reload the borrowed subject pointer
    emitter.instruction("ldr x2, [sp, #264]");                                  // reload the subject length
    emitter.instruction("mov x9, #0");                                          // start translating at the first subject byte

    emitter.label("__rt_strtr_pairwise_translate");
    emitter.instruction("cmp x9, x2");                                          // has the whole subject been translated?
    emitter.instruction("b.hs __rt_strtr_pairwise_translate_done");             // the translated result is complete
    emitter.instruction("ldrb w11, [x1, x9]");                                  // load the next subject byte
    emitter.instruction("ldrb w12, [x10, x11]");                                // look up its translation
    emitter.instruction("strb w12, [x13, x9]");                                 // store the translated byte at the same offset
    emitter.instruction("add x9, x9, #1");                                      // advance to the next subject byte
    emitter.instruction("b __rt_strtr_pairwise_translate");                     // keep translating subject bytes

    emitter.label("__rt_strtr_pairwise_translate_done");
    emitter.instruction("ldr x1, [sp, #272]");                                  // the translated result starts at the reservation
    emitter.instruction("ldr x2, [sp, #264]");                                  // the translated result is as long as the subject
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("bl __rt_str_persist");                                 // hand back owned heap storage, matching the Fresh ownership contract
    emitter.instruction("str x1, [sp, #280]");                                  // save the owned result pointer across the reservation release
    emitter.instruction("str x2, [sp, #288]");                                  // save the owned result length across the reservation release
    emitter.instruction("ldr x0, [sp, #272]");                                  // reload the superseded reservation
    emitter.instruction("bl __rt_heap_free_safe");                              // release a heap-backed reservation; concat-scratch pointers are skipped
    emitter.instruction("ldr x1, [sp, #280]");                                  // restore the owned result pointer
    emitter.instruction("ldr x2, [sp, #288]");                                  // restore the owned result length
    emitter.instruction("ldp x29, x30, [sp, #304]");                            // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #320");                                    // release the translation frame
    emitter.instruction("ret");                                                 // return the translated string as a PHP string pair
}

/// Emits the AArch64 `__rt_strtr_hash` helper for `strtr($string, $pairs)`.
///
/// - Input: `x0` = pairs hash, `x1`/`x2` = subject.
/// - Output: `x1`/`x2` = owned replaced string.
fn emit_strtr_hash_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtr_hash ---");
    emitter.label_global("__rt_strtr_hash");

    // Frame layout (128 bytes):
    //   [sp, #0]   = pairs hash
    //   [sp, #8]   = subject pointer
    //   [sp, #16]  = subject length
    //   [sp, #24]  = shortest usable key length
    //   [sp, #32]  = longest usable key length (0 = no usable key at all)
    //   [sp, #40]  = hash iteration cursor
    //   [sp, #48]  = measured result length, then the destination cursor
    //   [sp, #56]  = reservation start
    //   [sp, #64]  = scan position inside the subject
    //   [sp, #72]  = owned result pointer
    //   [sp, #80]  = owned result length
    //   [sp, #112] = saved x29/x30
    emitter.instruction("sub sp, sp, #128");                                    // allocate the replacement frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save the frame pointer and return address across the helper calls
    emitter.instruction("add x29, sp, #112");                                   // establish the pair-form helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the pairs hash
    emitter.instruction("str x1, [sp, #8]");                                    // save the subject pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the subject length
    emitter.instruction("str xzr, [sp, #40]");                                  // start the hash walk from the head entry
    emitter.instruction("mov x9, #-1");                                         // seed the shortest key length with the largest possible value
    emitter.instruction("lsr x9, x9, #1");                                      // PHP_INT_MAX is the neutral element for the minimum
    emitter.instruction("str x9, [sp, #24]");                                   // publish the seeded shortest key length
    emitter.instruction("str xzr, [sp, #32]");                                  // no usable key has been seen yet

    // -- measure the usable key-length window php-src probes at every position --
    emitter.label("__rt_strtr_hash_keys");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the pairs hash for the walk
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next_value");                        // x0 = next cursor, x1 = key payload, x2 = key length
    emitter.instruction("cmn x0, #1");                                          // did the iterator signal end-of-walk?
    emitter.instruction("b.eq __rt_strtr_hash_keys_done");                      // the key-length window is complete
    emitter.instruction("str x0, [sp, #40]");                                   // save the next insertion-order cursor
    emitter.instruction("cmn x2, #1");                                          // is this an inline integer key?
    emitter.instruction("b.ne __rt_strtr_hash_key_len");                        // string keys already carry their byte length
    emitter.instruction("mov x0, x1");                                          // measure how php-src would spell this integer key
    emitter.instruction("bl __rt_strtr_int_key_len");                           // x0 = spelled key length
    emitter.instruction("mov x2, x0");                                          // treat the spelled length as this key's length

    emitter.label("__rt_strtr_hash_key_len");
    emitter.instruction("cmp x2, #1");                                          // is the key at least one byte long?
    emitter.instruction("b.lt __rt_strtr_hash_keys");                           // php-src ignores an empty replacement key
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the subject length
    emitter.instruction("cmp x2, x9");                                          // could this key ever fit inside the subject?
    emitter.instruction("b.hi __rt_strtr_hash_keys");                           // php-src skips keys longer than the whole subject
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the shortest usable key length
    emitter.instruction("cmp x2, x10");                                         // is this key shorter than every key seen so far?
    emitter.instruction("csel x10, x2, x10, lo");                               // keep the shortest usable key length
    emitter.instruction("str x10, [sp, #24]");                                  // publish the shortest usable key length
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the longest usable key length
    emitter.instruction("cmp x2, x11");                                         // is this key longer than every key seen so far?
    emitter.instruction("csel x11, x2, x11, hi");                               // keep the longest usable key length
    emitter.instruction("str x11, [sp, #32]");                                  // publish the longest usable key length
    emitter.instruction("b __rt_strtr_hash_keys");                              // consider the next replacement key
    emitter.label("__rt_strtr_hash_keys_done");

    // -- first pass: size the result exactly so the reservation stays bounded --
    emitter.instruction("str xzr, [sp, #48]");                                  // the measured result starts empty
    emitter.instruction("str xzr, [sp, #64]");                                  // start measuring at the first subject byte

    emitter.label("__rt_strtr_hash_measure");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the scan position
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the subject length
    emitter.instruction("cmp x9, x10");                                         // has the whole subject been measured?
    emitter.instruction("b.hs __rt_strtr_hash_measure_done");                   // the exact result size is known
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the longest usable key length
    emitter.instruction("cbz x11, __rt_strtr_hash_measure_plain");              // with no usable key every byte is copied verbatim
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass the pairs hash to the probe
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the subject base
    emitter.instruction("add x1, x1, x9");                                      // probe from the current scan position
    emitter.instruction("sub x2, x10, x9");                                     // tell the probe how many subject bytes remain
    emitter.instruction("ldr x3, [sp, #24]");                                   // pass the shortest usable key length
    emitter.instruction("mov x4, x11");                                         // pass the longest usable key length
    emitter.instruction("bl __rt_strtr_probe");                                 // x0 = matched key length, x2 = replacement length
    emitter.instruction("cbz x0, __rt_strtr_hash_measure_plain");               // no key matched here, so this byte is copied verbatim
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the scan position
    emitter.instruction("add x9, x9, x0");                                      // php-src resumes after the matched key, never re-substituting
    emitter.instruction("str x9, [sp, #64]");                                   // publish the advanced scan position
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the measured result length
    emitter.instruction("adds x10, x10, x2");                                   // the replacement contributes its own length
    emitter.instruction("b.cs __rt_strtr_hash_overflow");                       // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("str x10, [sp, #48]");                                  // publish the measured result length
    emitter.instruction("b __rt_strtr_hash_measure");                           // measure the next subject position

    emitter.label("__rt_strtr_hash_measure_plain");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the scan position
    emitter.instruction("add x9, x9, #1");                                      // an unmatched byte advances the scan by one
    emitter.instruction("str x9, [sp, #64]");                                   // publish the advanced scan position
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the measured result length
    emitter.instruction("adds x10, x10, #1");                                   // an unmatched byte contributes one byte
    emitter.instruction("b.cs __rt_strtr_hash_overflow");                       // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("str x10, [sp, #48]");                                  // publish the measured result length
    emitter.instruction("b __rt_strtr_hash_measure");                           // measure the next subject position
    emitter.label("__rt_strtr_hash_measure_done");

    emitter.instruction("ldr x0, [sp, #48]");                                   // request exactly the measured result size
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the replaced result
    emitter.instruction("str x0, [sp, #56]");                                   // save the reservation start
    emitter.instruction("str x0, [sp, #48]");                                   // the destination cursor starts at the reservation
    emitter.instruction("str xzr, [sp, #64]");                                  // restart the scan for the writing pass

    // -- second pass: replay the same matches, writing into the exact reservation --
    emitter.label("__rt_strtr_hash_write");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the scan position
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the subject length
    emitter.instruction("cmp x9, x10");                                         // has the whole subject been rewritten?
    emitter.instruction("b.hs __rt_strtr_hash_write_done");                     // the replaced result is complete
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the longest usable key length
    emitter.instruction("cbz x11, __rt_strtr_hash_write_plain");                // with no usable key every byte is copied verbatim
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass the pairs hash to the probe
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the subject base
    emitter.instruction("add x1, x1, x9");                                      // probe from the current scan position
    emitter.instruction("sub x2, x10, x9");                                     // tell the probe how many subject bytes remain
    emitter.instruction("ldr x3, [sp, #24]");                                   // pass the shortest usable key length
    emitter.instruction("mov x4, x11");                                         // pass the longest usable key length
    emitter.instruction("bl __rt_strtr_probe");                                 // x0 = matched key length, x1/x2 = replacement pair
    emitter.instruction("cbz x0, __rt_strtr_hash_write_plain");                 // no key matched here, so this byte is copied verbatim
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the scan position
    emitter.instruction("add x9, x9, x0");                                      // php-src resumes after the matched key, never re-substituting
    emitter.instruction("str x9, [sp, #64]");                                   // publish the advanced scan position
    emitter.instruction("ldr x12, [sp, #48]");                                  // reload the destination cursor
    emitter.instruction("mov x13, #0");                                         // start copying at the first replacement byte

    emitter.label("__rt_strtr_hash_copy");
    emitter.instruction("cmp x13, x2");                                         // has the whole replacement been copied?
    emitter.instruction("b.hs __rt_strtr_hash_copy_done");                      // the replacement is fully written
    emitter.instruction("ldrb w14, [x1, x13]");                                 // load the next replacement byte
    emitter.instruction("strb w14, [x12, x13]");                                // store it at the same offset inside the result
    emitter.instruction("add x13, x13, #1");                                    // advance the copy index
    emitter.instruction("b __rt_strtr_hash_copy");                              // copy the next replacement byte

    emitter.label("__rt_strtr_hash_copy_done");
    emitter.instruction("add x12, x12, x2");                                    // advance the destination cursor past the replacement
    emitter.instruction("str x12, [sp, #48]");                                  // publish the advanced destination cursor
    emitter.instruction("b __rt_strtr_hash_write");                             // rewrite the next subject position

    emitter.label("__rt_strtr_hash_write_plain");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the scan position
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the subject base clobbered by the probe
    emitter.instruction("ldrb w14, [x1, x9]");                                  // load the unmatched subject byte
    emitter.instruction("ldr x12, [sp, #48]");                                  // reload the destination cursor
    emitter.instruction("strb w14, [x12]");                                     // copy the unmatched byte verbatim
    emitter.instruction("add x12, x12, #1");                                    // advance the destination cursor by one byte
    emitter.instruction("str x12, [sp, #48]");                                  // publish the advanced destination cursor
    emitter.instruction("add x9, x9, #1");                                      // an unmatched byte advances the scan by one
    emitter.instruction("str x9, [sp, #64]");                                   // publish the advanced scan position
    emitter.instruction("b __rt_strtr_hash_write");                             // rewrite the next subject position

    emitter.label("__rt_strtr_hash_write_done");
    emitter.instruction("ldr x1, [sp, #56]");                                   // the replaced result starts at the reservation
    emitter.instruction("ldr x12, [sp, #48]");                                  // reload the final destination cursor
    emitter.instruction("sub x2, x12, x1");                                     // the written byte count is the result length
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("bl __rt_str_persist");                                 // hand back owned heap storage, matching the Fresh ownership contract
    emitter.instruction("str x1, [sp, #72]");                                   // save the owned result pointer across the reservation release
    emitter.instruction("str x2, [sp, #80]");                                   // save the owned result length across the reservation release
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload the superseded reservation
    emitter.instruction("bl __rt_heap_free_safe");                              // release a heap-backed reservation; concat-scratch pointers are skipped
    emitter.instruction("ldr x1, [sp, #72]");                                   // restore the owned result pointer
    emitter.instruction("ldr x2, [sp, #80]");                                   // restore the owned result length
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the replacement frame
    emitter.instruction("ret");                                                 // return the replaced string as a PHP string pair

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_strtr_hash_overflow");
    emitter.instruction("b __rt_alloc_overflow");                               // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits the AArch64 `__rt_strtr_array` helper for an indexed-array `$pairs` argument.
///
/// php-src treats an indexed array as the pair list `{"0": e0, "1": e1, ...}`, so the array
/// is converted into an owned temporary hash, replaced through `__rt_strtr_hash`, and the
/// temporary is released once the result has been copied into its own storage.
///
/// - Input: `x0` = indexed array, `x1`/`x2` = subject.
/// - Output: `x1`/`x2` = owned replaced string.
fn emit_strtr_array_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtr_array ---");
    emitter.label_global("__rt_strtr_array");

    emitter.instruction("sub sp, sp, #48");                                     // allocate the conversion frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save the frame pointer and return address across the helper calls
    emitter.instruction("add x29, sp, #32");                                    // establish the indexed-pairs helper frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save the borrowed subject across the conversion
    emitter.instruction("bl __rt_array_to_hash");                               // build the owned {0: e0, 1: e1, ...} pair hash
    emitter.instruction("str x0, [sp, #16]");                                   // save the temporary pair hash for release
    emitter.instruction("ldp x1, x2, [sp]");                                    // restore the borrowed subject
    emitter.instruction("bl __rt_strtr_hash");                                  // run the ordinary pair-form replacement
    emitter.instruction("stp x1, x2, [sp]");                                    // save the owned result across the temporary release
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the temporary pair hash
    emitter.instruction("bl __rt_hash_free_deep");                              // release the temporary pair hash and its persisted values
    emitter.instruction("ldp x1, x2, [sp]");                                    // restore the owned result
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the conversion frame
    emitter.instruction("ret");                                                 // return the replaced string as a PHP string pair
}

/// Emits the x86_64 `__rt_strtr_int_key_len` helper.
///
/// - Input: `rax` = integer key.
/// - Output: `rax` = decimal digit count, plus one for a negative sign.
fn emit_strtr_int_key_len_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtr_int_key_len ---");
    emitter.label_global("__rt_strtr_int_key_len");

    emitter.instruction("xor r9d, r9d");                                        // positive keys spend no byte on a sign
    emitter.instruction("test rax, rax");                                       // is this a negative integer key?
    emitter.instruction("jge __rt_strtr_int_key_len_magnitude_x86");            // positive keys are already their own magnitude
    emitter.instruction("neg rax");                                             // measure the magnitude of a negative key
    emitter.instruction("mov r9d, 1");                                          // a negative key also spells a leading '-'

    emitter.label("__rt_strtr_int_key_len_magnitude_x86");
    emitter.instruction("mov r10d, 1");                                         // every integer spells at least one digit
    emitter.instruction("mov r11, 10");                                         // decimal radix

    emitter.label("__rt_strtr_int_key_len_loop_x86");
    emitter.instruction("cmp rax, 10");                                         // is there another decimal digit left?
    emitter.instruction("jb __rt_strtr_int_key_len_done_x86");                  // a value below ten has no further digits
    emitter.instruction("xor edx, edx");                                        // clear the dividend high half before the unsigned division
    emitter.instruction("div r11");                                             // drop the lowest decimal digit
    emitter.instruction("add r10, 1");                                          // count the dropped digit
    emitter.instruction("jmp __rt_strtr_int_key_len_loop_x86");                 // keep counting decimal digits

    emitter.label("__rt_strtr_int_key_len_done_x86");
    emitter.instruction("mov rax, r10");                                        // total spelled length starts at the digit count
    emitter.instruction("add rax, r9");                                         // add the optional sign byte
    emitter.instruction("ret");                                                 // return the spelled key length
}

/// Emits the x86_64 `__rt_strtr_probe` helper.
///
/// - Input: `rdi` = pairs hash, `rsi` = position pointer, `rdx` = remaining bytes,
///   `rcx` = shortest usable key length, `r8` = longest usable key length.
/// - Output: `rax` = matched key length (`0` when nothing matched), `rdi` = replacement
///   pointer, `rsi` = replacement length.
fn emit_strtr_probe_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtr_probe ---");
    emitter.label_global("__rt_strtr_probe");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the lookup calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the probe state
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for the probe state
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the pairs hash across the lookup calls
    emitter.instruction("mov QWORD PTR [rbp - 40], rsi");                       // save the probed subject position
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save the shortest usable key length
    emitter.instruction("mov r9, r8");                                          // start from the longest usable key length
    emitter.instruction("cmp r9, rdx");                                         // does the longest key still fit the remaining subject?
    emitter.instruction("cmova r9, rdx");                                       // clamp the first probed length to what remains
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // save the current candidate key length

    emitter.label("__rt_strtr_probe_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // reload the current candidate key length
    emitter.instruction("cmp r9, QWORD PTR [rbp - 48]");                        // have all candidate lengths been tried?
    emitter.instruction("jb __rt_strtr_probe_miss_x86");                        // nothing matches at this subject position
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the candidate substring starts at the probed position
    emitter.instruction("mov rdx, r9");                                         // the candidate substring is as long as the current length
    emitter.instruction("call __rt_hash_normalize_key");                        // map numeric substrings onto the integer keys the hash stores
    emitter.instruction("mov rsi, rax");                                        // move the normalized key_lo into the lookup register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the pairs hash for the lookup
    emitter.instruction("call __rt_hash_get");                                  // rax = found, rdi = replacement pointer, rsi = replacement length
    emitter.instruction("test rax, rax");                                       // did this candidate length match a replacement key?
    emitter.instruction("jnz __rt_strtr_probe_hit_x86");                        // the longest matching key wins
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // reload the current candidate key length
    emitter.instruction("sub r9, 1");                                           // try the next shorter key length
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // publish the shortened candidate length
    emitter.instruction("jmp __rt_strtr_probe_loop_x86");                       // keep probing shorter keys

    emitter.label("__rt_strtr_probe_hit_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // report the matched key length without disturbing the replacement pair
    emitter.instruction("jmp __rt_strtr_probe_done_x86");                       // the probe is finished

    emitter.label("__rt_strtr_probe_miss_x86");
    emitter.instruction("xor eax, eax");                                        // report that no replacement key matched here
    emitter.instruction("xor edi, edi");                                        // no replacement pointer for a miss
    emitter.instruction("xor esi, esi");                                        // no replacement length for a miss

    emitter.label("__rt_strtr_probe_done_x86");
    emitter.instruction("add rsp, 64");                                         // release the probe frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the probe outcome
}

/// Emits the x86_64 `__rt_strtr_pairwise` helper for `strtr($string, $from, $to)`.
///
/// - Input: `rax`/`rdx` = subject, `rdi`/`rsi` = `$from`, `rcx`/`r8` = `$to`.
/// - Output: `rax`/`rdx` = owned translated string.
fn emit_strtr_pairwise_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtr_pairwise ---");
    emitter.label_global("__rt_strtr_pairwise");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the reservation calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the translation table
    emitter.instruction("sub rsp, 320");                                        // reserve the saved-argument slots plus the 256-byte translation table
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the subject pointer across the reservation call
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the subject length across the reservation call

    // -- seed the translation table with the identity mapping --
    emitter.instruction("xor r9d, r9d");                                        // start at byte value zero
    emitter.label("__rt_strtr_pairwise_identity_x86");
    emitter.instruction("mov BYTE PTR [rbp + r9 - 320], r9b");                  // an unmapped byte translates to itself
    emitter.instruction("add r9, 1");                                           // advance to the next byte value
    emitter.instruction("cmp r9, 256");                                         // is the identity table complete?
    emitter.instruction("jb __rt_strtr_pairwise_identity_x86");                 // keep seeding the identity mapping

    // -- overwrite the mapped bytes; php-src truncates to the shorter of the two lists --
    emitter.instruction("mov r10, rsi");                                        // start from the $from byte count
    emitter.instruction("cmp r10, r8");                                         // which of $from and $to is shorter?
    emitter.instruction("cmova r10, r8");                                       // the mapping covers only min(len($from), len($to)) bytes
    emitter.instruction("xor r9d, r9d");                                        // start at the first mapped pair
    emitter.label("__rt_strtr_pairwise_map_x86");
    emitter.instruction("cmp r9, r10");                                         // has the whole mapping been applied?
    emitter.instruction("jae __rt_strtr_pairwise_map_done_x86");                // the translation table is complete
    emitter.instruction("movzx r11d, BYTE PTR [rdi + r9]");                     // load the source byte of this pair
    emitter.instruction("movzx eax, BYTE PTR [rcx + r9]");                      // load the destination byte of this pair
    emitter.instruction("mov BYTE PTR [rbp + r11 - 320], al");                  // a later pair for the same source byte wins, as in php-src
    emitter.instruction("add r9, 1");                                           // advance to the next mapped pair
    emitter.instruction("jmp __rt_strtr_pairwise_map_x86");                     // keep applying the mapping
    emitter.label("__rt_strtr_pairwise_map_done_x86");

    // -- the result is always exactly as long as the subject --
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // request exactly the subject byte count
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the translated result
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the reservation start
    emitter.instruction("mov rsi, rax");                                        // destination cursor
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the borrowed subject pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the subject length
    emitter.instruction("xor r9d, r9d");                                        // start translating at the first subject byte

    emitter.label("__rt_strtr_pairwise_translate_x86");
    emitter.instruction("cmp r9, rcx");                                         // has the whole subject been translated?
    emitter.instruction("jae __rt_strtr_pairwise_translate_done_x86");          // the translated result is complete
    emitter.instruction("movzx r10d, BYTE PTR [rdi + r9]");                     // load the next subject byte
    emitter.instruction("mov r11b, BYTE PTR [rbp + r10 - 320]");                // look up its translation
    emitter.instruction("mov BYTE PTR [rsi + r9], r11b");                       // store the translated byte at the same offset
    emitter.instruction("add r9, 1");                                           // advance to the next subject byte
    emitter.instruction("jmp __rt_strtr_pairwise_translate_x86");               // keep translating subject bytes

    emitter.label("__rt_strtr_pairwise_translate_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // the translated result starts at the reservation
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // the translated result is as long as the subject
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("call __rt_str_persist");                               // hand back owned heap storage, matching the Fresh ownership contract
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the owned result pointer across the reservation release
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save the owned result length across the reservation release
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the superseded reservation
    emitter.instruction("call __rt_heap_free_safe");                            // release a heap-backed reservation; concat-scratch pointers are skipped
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // restore the owned result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // restore the owned result length
    emitter.instruction("add rsp, 320");                                        // release the translation frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the translated string as a PHP string pair
}

/// Emits the x86_64 `__rt_strtr_hash` helper for `strtr($string, $pairs)`.
///
/// - Input: `rdi` = pairs hash, `rax`/`rdx` = subject.
/// - Output: `rax`/`rdx` = owned replaced string.
fn emit_strtr_hash_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtr_hash ---");
    emitter.label_global("__rt_strtr_hash");

    // Frame layout:
    //   [rbp - 32]  = pairs hash
    //   [rbp - 40]  = subject pointer
    //   [rbp - 48]  = subject length
    //   [rbp - 56]  = shortest usable key length
    //   [rbp - 64]  = longest usable key length (0 = no usable key at all)
    //   [rbp - 72]  = hash iteration cursor
    //   [rbp - 80]  = measured result length, then the destination cursor
    //   [rbp - 88]  = reservation start
    //   [rbp - 96]  = scan position inside the subject
    //   [rbp - 104] = owned result pointer
    //   [rbp - 112] = owned result length
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the replacement state
    emitter.instruction("sub rsp, 112");                                        // reserve aligned spill slots for the replacement state
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the pairs hash
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the subject pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the subject length
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // start the hash walk from the head entry
    emitter.instruction("mov r9, 0x7fffffffffffffff");                          // seed the shortest key length with PHP_INT_MAX
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // publish the seeded shortest key length
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // no usable key has been seen yet

    // -- measure the usable key-length window php-src probes at every position --
    emitter.label("__rt_strtr_hash_keys_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the pairs hash for the walk
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // reload the insertion-order cursor
    emitter.instruction("call __rt_hash_iter_next_value");                      // rax = next cursor, rdi = key payload, rdx = key length
    emitter.instruction("cmp rax, -1");                                         // did the iterator signal end-of-walk?
    emitter.instruction("je __rt_strtr_hash_keys_done_x86");                    // the key-length window is complete
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the next insertion-order cursor
    emitter.instruction("cmp rdx, -1");                                         // is this an inline integer key?
    emitter.instruction("jne __rt_strtr_hash_key_len_x86");                     // string keys already carry their byte length
    emitter.instruction("mov rax, rdi");                                        // measure how php-src would spell this integer key
    emitter.instruction("call __rt_strtr_int_key_len");                         // rax = spelled key length
    emitter.instruction("mov rdx, rax");                                        // treat the spelled length as this key's length

    emitter.label("__rt_strtr_hash_key_len_x86");
    emitter.instruction("cmp rdx, 1");                                          // is the key at least one byte long?
    emitter.instruction("jl __rt_strtr_hash_keys_x86");                         // php-src ignores an empty replacement key
    emitter.instruction("cmp rdx, QWORD PTR [rbp - 48]");                       // could this key ever fit inside the subject?
    emitter.instruction("ja __rt_strtr_hash_keys_x86");                         // php-src skips keys longer than the whole subject
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the shortest usable key length
    emitter.instruction("cmp rdx, r10");                                        // is this key shorter than every key seen so far?
    emitter.instruction("cmovb r10, rdx");                                      // keep the shortest usable key length
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // publish the shortest usable key length
    emitter.instruction("mov r11, QWORD PTR [rbp - 64]");                       // reload the longest usable key length
    emitter.instruction("cmp rdx, r11");                                        // is this key longer than every key seen so far?
    emitter.instruction("cmova r11, rdx");                                      // keep the longest usable key length
    emitter.instruction("mov QWORD PTR [rbp - 64], r11");                       // publish the longest usable key length
    emitter.instruction("jmp __rt_strtr_hash_keys_x86");                        // consider the next replacement key
    emitter.label("__rt_strtr_hash_keys_done_x86");

    // -- first pass: size the result exactly so the reservation stays bounded --
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // the measured result starts empty
    emitter.instruction("mov QWORD PTR [rbp - 96], 0");                         // start measuring at the first subject byte

    emitter.label("__rt_strtr_hash_measure_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 96]");                        // reload the scan position
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the subject length
    emitter.instruction("cmp r9, r10");                                         // has the whole subject been measured?
    emitter.instruction("jae __rt_strtr_hash_measure_done_x86");                // the exact result size is known
    emitter.instruction("mov r11, QWORD PTR [rbp - 64]");                       // reload the longest usable key length
    emitter.instruction("test r11, r11");                                       // is there any usable replacement key at all?
    emitter.instruction("jz __rt_strtr_hash_measure_plain_x86");                // with no usable key every byte is copied verbatim
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the pairs hash to the probe
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload the subject base
    emitter.instruction("add rsi, r9");                                         // probe from the current scan position
    emitter.instruction("mov rdx, r10");                                        // copy the subject length before deriving what remains
    emitter.instruction("sub rdx, r9");                                         // tell the probe how many subject bytes remain
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // pass the shortest usable key length
    emitter.instruction("mov r8, r11");                                         // pass the longest usable key length
    emitter.instruction("call __rt_strtr_probe");                               // rax = matched key length, rsi = replacement length
    emitter.instruction("test rax, rax");                                       // did any replacement key match here?
    emitter.instruction("jz __rt_strtr_hash_measure_plain_x86");                // no key matched here, so this byte is copied verbatim
    emitter.instruction("mov r9, QWORD PTR [rbp - 96]");                        // reload the scan position
    emitter.instruction("add r9, rax");                                         // php-src resumes after the matched key, never re-substituting
    emitter.instruction("mov QWORD PTR [rbp - 96], r9");                        // publish the advanced scan position
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the measured result length
    emitter.instruction("add r10, rsi");                                        // the replacement contributes its own length
    emitter.instruction("jc __rt_strtr_hash_overflow_x86");                     // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // publish the measured result length
    emitter.instruction("jmp __rt_strtr_hash_measure_x86");                     // measure the next subject position

    emitter.label("__rt_strtr_hash_measure_plain_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 96]");                        // reload the scan position
    emitter.instruction("add r9, 1");                                           // an unmatched byte advances the scan by one
    emitter.instruction("mov QWORD PTR [rbp - 96], r9");                        // publish the advanced scan position
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the measured result length
    emitter.instruction("add r10, 1");                                          // an unmatched byte contributes one byte
    emitter.instruction("jc __rt_strtr_hash_overflow_x86");                     // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // publish the measured result length
    emitter.instruction("jmp __rt_strtr_hash_measure_x86");                     // measure the next subject position
    emitter.label("__rt_strtr_hash_measure_done_x86");

    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // request exactly the measured result size
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the replaced result
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // save the reservation start
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // the destination cursor starts at the reservation
    emitter.instruction("mov QWORD PTR [rbp - 96], 0");                         // restart the scan for the writing pass

    // -- second pass: replay the same matches, writing into the exact reservation --
    emitter.label("__rt_strtr_hash_write_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 96]");                        // reload the scan position
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the subject length
    emitter.instruction("cmp r9, r10");                                         // has the whole subject been rewritten?
    emitter.instruction("jae __rt_strtr_hash_write_done_x86");                  // the replaced result is complete
    emitter.instruction("mov r11, QWORD PTR [rbp - 64]");                       // reload the longest usable key length
    emitter.instruction("test r11, r11");                                       // is there any usable replacement key at all?
    emitter.instruction("jz __rt_strtr_hash_write_plain_x86");                  // with no usable key every byte is copied verbatim
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the pairs hash to the probe
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload the subject base
    emitter.instruction("add rsi, r9");                                         // probe from the current scan position
    emitter.instruction("mov rdx, r10");                                        // copy the subject length before deriving what remains
    emitter.instruction("sub rdx, r9");                                         // tell the probe how many subject bytes remain
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // pass the shortest usable key length
    emitter.instruction("mov r8, r11");                                         // pass the longest usable key length
    emitter.instruction("call __rt_strtr_probe");                               // rax = matched key length, rdi/rsi = replacement pair
    emitter.instruction("test rax, rax");                                       // did any replacement key match here?
    emitter.instruction("jz __rt_strtr_hash_write_plain_x86");                  // no key matched here, so this byte is copied verbatim
    emitter.instruction("mov r9, QWORD PTR [rbp - 96]");                        // reload the scan position
    emitter.instruction("add r9, rax");                                         // php-src resumes after the matched key, never re-substituting
    emitter.instruction("mov QWORD PTR [rbp - 96], r9");                        // publish the advanced scan position
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the destination cursor
    emitter.instruction("xor r11d, r11d");                                      // start copying at the first replacement byte

    emitter.label("__rt_strtr_hash_copy_x86");
    emitter.instruction("cmp r11, rsi");                                        // has the whole replacement been copied?
    emitter.instruction("jae __rt_strtr_hash_copy_done_x86");                   // the replacement is fully written
    emitter.instruction("mov cl, BYTE PTR [rdi + r11]");                        // load the next replacement byte
    emitter.instruction("mov BYTE PTR [r10 + r11], cl");                        // store it at the same offset inside the result
    emitter.instruction("add r11, 1");                                          // advance the copy index
    emitter.instruction("jmp __rt_strtr_hash_copy_x86");                        // copy the next replacement byte

    emitter.label("__rt_strtr_hash_copy_done_x86");
    emitter.instruction("add r10, rsi");                                        // advance the destination cursor past the replacement
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // publish the advanced destination cursor
    emitter.instruction("jmp __rt_strtr_hash_write_x86");                       // rewrite the next subject position

    emitter.label("__rt_strtr_hash_write_plain_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 96]");                        // reload the scan position
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the subject base clobbered by the probe
    emitter.instruction("mov cl, BYTE PTR [rdi + r9]");                         // load the unmatched subject byte
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the destination cursor
    emitter.instruction("mov BYTE PTR [r10], cl");                              // copy the unmatched byte verbatim
    emitter.instruction("add r10, 1");                                          // advance the destination cursor by one byte
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // publish the advanced destination cursor
    emitter.instruction("add r9, 1");                                           // an unmatched byte advances the scan by one
    emitter.instruction("mov QWORD PTR [rbp - 96], r9");                        // publish the advanced scan position
    emitter.instruction("jmp __rt_strtr_hash_write_x86");                       // rewrite the next subject position

    emitter.label("__rt_strtr_hash_write_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // the replaced result starts at the reservation
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // reload the final destination cursor
    emitter.instruction("sub rdx, rax");                                        // the written byte count is the result length
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("call __rt_str_persist");                               // hand back owned heap storage, matching the Fresh ownership contract
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // save the owned result pointer across the reservation release
    emitter.instruction("mov QWORD PTR [rbp - 112], rdx");                      // save the owned result length across the reservation release
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // reload the superseded reservation
    emitter.instruction("call __rt_heap_free_safe");                            // release a heap-backed reservation; concat-scratch pointers are skipped
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // restore the owned result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 112]");                      // restore the owned result length
    emitter.instruction("add rsp, 112");                                        // release the replacement frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the replaced string as a PHP string pair

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_strtr_hash_overflow_x86");
    emitter.instruction("jmp __rt_alloc_overflow");                             // unconditional branch keeps the fatal trampoline reachable from every caller
}

/// Emits the x86_64 `__rt_strtr_array` helper for an indexed-array `$pairs` argument.
///
/// - Input: `rdi` = indexed array, `rax`/`rdx` = subject.
/// - Output: `rax`/`rdx` = owned replaced string.
fn emit_strtr_array_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtr_array ---");
    emitter.label_global("__rt_strtr_array");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the borrowed subject
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the subject and temporary hash
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the borrowed subject pointer across the conversion
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the borrowed subject length across the conversion
    emitter.instruction("call __rt_array_to_hash");                             // build the owned {0: e0, 1: e1, ...} pair hash
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the temporary pair hash for release
    emitter.instruction("mov rdi, rax");                                        // pass the temporary pair hash to the replacement helper
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // restore the borrowed subject pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // restore the borrowed subject length
    emitter.instruction("call __rt_strtr_hash");                                // run the ordinary pair-form replacement
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the owned result pointer across the temporary release
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the owned result length across the temporary release
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the temporary pair hash
    emitter.instruction("call __rt_hash_free_deep");                            // release the temporary pair hash and its persisted values
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // restore the owned result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // restore the owned result length
    emitter.instruction("add rsp, 48");                                         // release the conversion frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the replaced string as a PHP string pair
}
