//! Purpose:
//! Emits the `array_count_values()` runtime helpers: `__rt_count_values_bump`,
//! `__rt_array_count_values` (indexed sources) and `__rt_hash_count_values` (associative
//! sources).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - `crate::codegen::lower_inst::builtins::arrays::lower_array_count_values()`.
//!
//! Key details:
//! - The destination is always a hash whose `value_type` is `0` (int): php-src's
//!   `array_count_values()` maps every distinct value to an `int` occurrence count.
//! - Keys go through `__rt_hash_normalize_key`, so PHP's numeric-string collapsing applies
//!   exactly as it does for `$a[$v]` (`array_count_values(["1", 1])` yields `[1 => 2]`).
//! - php-src warns and SKIPS any element that is neither int nor string; the `skip` arms
//!   reproduce that through `__rt_diag_warning` with `ARRAY_COUNT_VALUES_SKIPPED_MESSAGES`.
//! - OWNERSHIP: the source is only ever READ. `__rt_hash_set` persists the inserted string key
//!   itself and the stored value is a plain integer, so no refcount traffic crosses this helper.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// The `array_count_values()` warning text for a value PHP refuses to use as an array key.
///
/// Entries are `(symbol, message)`. `crate::codegen_support::runtime::data::fixed` emits them
/// verbatim as `.ascii` literals; this module derives the `write()` length from `message.len()`
/// so the bytes and the immediate can never drift apart.
///
/// Captured from PHP 8.4.20 with `LC_ALL=C php`; elephc does not synthesize the
/// ` in <file> on line <n>` tail that php-src appends to the message.
pub const ARRAY_COUNT_VALUES_SKIPPED_MESSAGES: &[(&str, &str)] = &[(
    "_diag_array_count_values_skipped",
    "Warning: array_count_values(): Can only count string and integer values, entry skipped\n",
)];

/// Returns the byte length of the single `array_count_values()` skip message.
fn skip_message_len() -> usize {
    ARRAY_COUNT_VALUES_SKIPPED_MESSAGES[0].1.len()
}

/// Emits every `array_count_values()` runtime helper for the active target.
pub fn emit_array_count_values(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_count_values_bump_x86_64(emitter);
        emit_array_count_values_x86_64(emitter);
        emit_hash_count_values_x86_64(emitter);
        return;
    }
    emit_count_values_bump_aarch64(emitter);
    emit_array_count_values_aarch64(emitter);
    emit_hash_count_values_aarch64(emitter);
}

/// Emits `__rt_count_values_bump`, the shared "increment the tally for one key" routine.
///
/// # ABI (AArch64)
/// - Input: `x0` = destination hash, `x1` = normalized key_lo, `x2` = normalized key_hi.
/// - Output: `x0` = destination hash (possibly reallocated by `__rt_hash_set`).
fn emit_count_values_bump_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: count_values_bump ---");
    emitter.label_global("__rt_count_values_bump");

    // Stack layout:
    //   [sp, #0]  = destination hash pointer
    //   [sp, #8]  = normalized key_lo
    //   [sp, #16] = normalized key_hi
    //   [sp, #32] = saved x29/x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate the tally-bump frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up the tally-bump frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the destination hash across the lookup
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the normalized key low word
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the normalized key high word

    emitter.instruction("bl __rt_hash_get");                                    // x0 = found, x1 = value_lo (the previous tally)
    emitter.instruction("mov x9, #1");                                          // a value seen for the first time starts at 1
    emitter.instruction("add x10, x1, #1");                                     // an already-tallied value grows by one
    emitter.instruction("cmp x0, #0");                                          // did the destination already hold this key?
    emitter.instruction("csel x3, x10, x9, ne");                                // pick the incremented tally only for an existing key

    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = destination hash pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = normalized key low word
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = normalized key high word
    emitter.instruction("mov x4, xzr");                                         // integer tallies carry no high word
    emitter.instruction("mov x5, xzr");                                         // runtime tag 0 marks the tally as an int
    emitter.instruction("bl __rt_hash_set");                                    // insert or overwrite the tally; hash_set persists string keys

    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the tally-bump frame
    emitter.instruction("ret");                                                 // return with x0 = destination hash pointer
}

/// Emits `__rt_array_count_values` for INDEXED array sources.
///
/// # ABI (AArch64)
/// - Input: `x0` = source indexed array, `x1` = compile-time element tag
///   (`0` int, `1` string, `7` boxed Mixed; any other tag makes every element skippable).
/// - Output: `x0` = fresh destination hash mapping each countable value to its tally.
fn emit_array_count_values_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_count_values ---");
    emitter.label_global("__rt_array_count_values");

    // Stack layout:
    //   [sp, #0]  = source array pointer
    //   [sp, #8]  = destination hash pointer
    //   [sp, #16] = loop index
    //   [sp, #24] = element tag
    //   [sp, #48] = saved x29/x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate the count-values frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up the count-values frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the source array across helper calls
    emitter.instruction("str x1, [sp, #24]");                                   // preserve the compile-time element tag

    emitter.instruction("ldr x0, [x0]");                                        // x0 = source element count
    emitter.instruction("lsl x0, x0, #1");                                      // double it so the destination has insertion headroom
    emitter.instruction("mov x9, #16");                                         // x9 = minimum destination bucket count
    emitter.instruction("cmp x0, x9");                                          // compare the derived capacity against the runtime minimum
    emitter.instruction("csel x0, x9, x0, lt");                                 // clamp very small sources up to the minimum bucket count
    emitter.instruction("mov x1, xzr");                                         // destination value_type 0: tallies are integers
    emitter.instruction("bl __rt_hash_new");                                    // allocate the destination hash
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the destination hash across insertions
    emitter.instruction("str xzr, [sp, #16]");                                  // start the walk at source index 0

    emitter.label("__rt_array_count_values_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = source array pointer
    emitter.instruction("ldr x9, [x0]");                                        // x9 = source element count
    emitter.instruction("ldr x10, [sp, #16]");                                  // x10 = current source index
    emitter.instruction("cmp x10, x9");                                         // has every source element been tallied?
    emitter.instruction("b.ge __rt_array_count_values_done");                   // yes - the destination hash is complete
    emitter.instruction("add x11, x0, #24");                                    // x11 = source payload base
    emitter.instruction("ldr x12, [sp, #24]");                                  // x12 = compile-time element tag
    emitter.instruction("cmp x12, #1");                                         // element tag 1 = string payload
    emitter.instruction("b.eq __rt_array_count_values_str");                    // string elements live in 16-byte slots
    emitter.instruction("cmp x12, #7");                                         // element tag 7 = boxed Mixed payload
    emitter.instruction("b.eq __rt_array_count_values_mixed");                  // boxed elements need a runtime tag dispatch
    emitter.instruction("cmp x12, #0");                                         // element tag 0 = plain integer payload
    emitter.instruction("b.ne __rt_array_count_values_skip");                   // float/bool/array/object elements are skipped
    emitter.instruction("ldr x1, [x11, x10, lsl #3]");                          // x1 = integer value becoming the tally key
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks an inline integer key
    emitter.instruction("b __rt_array_count_values_bump");                      // tally this integer value

    emitter.label("__rt_array_count_values_str");
    emitter.instruction("add x11, x11, x10, lsl #4");                           // advance to the selected 16-byte string slot
    emitter.instruction("ldr x1, [x11]");                                       // x1 = source string pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // x2 = source string length
    emitter.instruction("bl __rt_hash_normalize_key");                          // collapse PHP numeric-string values into integer keys
    emitter.instruction("b __rt_array_count_values_bump");                      // tally this string value

    emitter.label("__rt_array_count_values_mixed");
    emitter.instruction("ldr x0, [x11, x10, lsl #3]");                          // x0 = boxed Mixed cell for this element
    emitter.instruction("cbz x0, __rt_array_count_values_skip");                // a null cell is not a countable value
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0 = concrete tag, x1 = value_lo, x2 = value_hi
    emitter.instruction("cmp x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("b.eq __rt_array_count_values_mixed_int");              // integers become inline integer keys
    emitter.instruction("cmp x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("b.ne __rt_array_count_values_skip");                   // every other tag is skipped with a warning
    emitter.instruction("bl __rt_hash_normalize_key");                          // collapse PHP numeric-string values into integer keys
    emitter.instruction("b __rt_array_count_values_bump");                      // tally this unboxed string value

    emitter.label("__rt_array_count_values_mixed_int");
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks an inline integer key

    emitter.label("__rt_array_count_values_bump");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = destination hash pointer
    emitter.instruction("bl __rt_count_values_bump");                           // increment the tally for this key
    emitter.instruction("str x0, [sp, #8]");                                    // keep the destination pointer current after growth

    emitter.label("__rt_array_count_values_next");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the source index after the helper calls
    emitter.instruction("add x10, x10, #1");                                    // advance to the next source element
    emitter.instruction("str x10, [sp, #16]");                                  // persist the updated source index
    emitter.instruction("b __rt_array_count_values_loop");                      // continue tallying source elements

    emitter.label("__rt_array_count_values_skip");
    abi::emit_symbol_address(emitter, "x1", ARRAY_COUNT_VALUES_SKIPPED_MESSAGES[0].0);
    emitter.instruction(&format!("mov x2, #{}", skip_message_len()));           // pass the complete skip-warning length
    emitter.instruction("bl __rt_diag_warning");                                // emit or suppress the PHP skip warning
    emitter.instruction("b __rt_array_count_values_next");                      // skip this element and continue

    emitter.label("__rt_array_count_values_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the destination hash pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the count-values frame
    emitter.instruction("ret");                                                 // return with x0 = destination hash pointer
}

/// Emits `__rt_hash_count_values` for ASSOCIATIVE array sources.
///
/// # ABI (AArch64)
/// - Input: `x0` = source hash.
/// - Output: `x0` = fresh destination hash mapping each countable value to its tally.
///
/// Hash entries carry a per-entry runtime tag, so one routine covers `Int`, `Str`, and boxed
/// `Mixed` value types without a compile-time hint.
fn emit_hash_count_values_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_count_values ---");
    emitter.label_global("__rt_hash_count_values");

    // Stack layout:
    //   [sp, #0]  = source hash pointer
    //   [sp, #8]  = destination hash pointer
    //   [sp, #16] = insertion-order iterator cursor
    //   [sp, #48] = saved x29/x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate the hash count-values frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up the hash count-values frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the source hash across helper calls

    emitter.instruction("ldr x0, [x0]");                                        // x0 = source entry count
    emitter.instruction("lsl x0, x0, #1");                                      // double it so the destination has insertion headroom
    emitter.instruction("mov x9, #16");                                         // x9 = minimum destination bucket count
    emitter.instruction("cmp x0, x9");                                          // compare the derived capacity against the runtime minimum
    emitter.instruction("csel x0, x9, x0, lt");                                 // clamp very small sources up to the minimum bucket count
    emitter.instruction("mov x1, xzr");                                         // destination value_type 0: tallies are integers
    emitter.instruction("bl __rt_hash_new");                                    // allocate the destination hash
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the destination hash across insertions
    emitter.instruction("str xzr, [sp, #16]");                                  // iterator cursor = 0 (start from header.head)

    emitter.label("__rt_hash_count_values_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = source hash pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // x1 = current insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next_value");                        // fetch the next source entry
    emitter.instruction("cmn x0, #1");                                          // did the iterator signal end-of-walk?
    emitter.instruction("b.eq __rt_hash_count_values_done");                    // yes - the destination hash is complete
    emitter.instruction("str x0, [sp, #16]");                                   // save the next insertion-order cursor
    emitter.instruction("mov x0, x5");                                          // x0 = source value tag
    emitter.instruction("mov x1, x3");                                          // x1 = source value low word
    emitter.instruction("mov x2, x4");                                          // x2 = source value high word
    emitter.instruction("cmp x0, #7");                                          // runtime tag 7 = boxed mixed cell
    emitter.instruction("b.ne __rt_hash_count_values_tag_ready");               // concrete tags are already usable
    emitter.instruction("cbz x1, __rt_hash_count_values_skip");                 // a null cell is not a countable value
    emitter.instruction("mov x0, x1");                                          // x0 = boxed mixed pointer for unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0 = concrete tag, x1 = value_lo, x2 = value_hi

    emitter.label("__rt_hash_count_values_tag_ready");
    emitter.instruction("cmp x0, #0");                                          // runtime tag 0 = int
    emitter.instruction("b.eq __rt_hash_count_values_int");                     // integers become inline integer keys
    emitter.instruction("cmp x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("b.ne __rt_hash_count_values_skip");                    // every other tag is skipped with a warning
    emitter.instruction("bl __rt_hash_normalize_key");                          // collapse PHP numeric-string values into integer keys
    emitter.instruction("b __rt_hash_count_values_bump");                       // tally this string value

    emitter.label("__rt_hash_count_values_int");
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks an inline integer key

    emitter.label("__rt_hash_count_values_bump");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = destination hash pointer
    emitter.instruction("bl __rt_count_values_bump");                           // increment the tally for this key
    emitter.instruction("str x0, [sp, #8]");                                    // keep the destination pointer current after growth
    emitter.instruction("b __rt_hash_count_values_loop");                       // continue with the next source entry

    emitter.label("__rt_hash_count_values_skip");
    abi::emit_symbol_address(emitter, "x1", ARRAY_COUNT_VALUES_SKIPPED_MESSAGES[0].0);
    emitter.instruction(&format!("mov x2, #{}", skip_message_len()));           // pass the complete skip-warning length
    emitter.instruction("bl __rt_diag_warning");                                // emit or suppress the PHP skip warning
    emitter.instruction("b __rt_hash_count_values_loop");                       // skip this entry and continue

    emitter.label("__rt_hash_count_values_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the destination hash pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the hash count-values frame
    emitter.instruction("ret");                                                 // return with x0 = destination hash pointer
}

/// Emits the x86_64 System V variant of `__rt_count_values_bump`.
///
/// `__rt_hash_get` takes `(rdi = hash, rsi = key_lo, rdx = key_hi)` and returns
/// `rax = found`, `rdi = value_lo`; every field is spilled before the insertion call.
fn emit_count_values_bump_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: count_values_bump ---");
    emitter.label_global("__rt_count_values_bump");

    // Frame layout:
    //   [rbp - 8]  = destination hash pointer
    //   [rbp - 16] = normalized key_lo
    //   [rbp - 24] = normalized key_hi
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the tally bookkeeping
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the nested helper calls
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the destination hash across the lookup
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the normalized key low word
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the normalized key high word

    emitter.instruction("call __rt_hash_get");                                  // rax = found, rdi = value_lo (the previous tally)
    emitter.instruction("lea rcx, [rdi + 1]");                                  // an already-tallied value grows by one
    emitter.instruction("mov r10, 1");                                          // a value seen for the first time starts at 1
    emitter.instruction("test rax, rax");                                       // did the destination already hold this key?
    emitter.instruction("cmovne r10, rcx");                                     // pick the incremented tally only for an existing key

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // rdi = destination hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // rsi = normalized key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // rdx = normalized key high word
    emitter.instruction("mov rcx, r10");                                        // rcx = the tally being stored
    emitter.instruction("xor r8d, r8d");                                        // integer tallies carry no high word
    emitter.instruction("xor r9d, r9d");                                        // runtime tag 0 marks the tally as an int
    emitter.instruction("call __rt_hash_set");                                  // insert or overwrite the tally; hash_set persists string keys

    emitter.instruction("add rsp, 32");                                         // release the tally-bump spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with rax = destination hash pointer
}

/// Emits the x86_64 System V variant of `__rt_array_count_values`.
///
/// Mirrors the AArch64 logic; `__rt_hash_normalize_key` and `__rt_mixed_unbox` use the
/// `rax`/`rdx` convention rather than the System V argument registers, exactly as
/// `__rt_hash_flip` calls them.
fn emit_array_count_values_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_count_values ---");
    emitter.label_global("__rt_array_count_values");

    // Frame layout:
    //   [rbp - 8]  = source array pointer
    //   [rbp - 16] = destination hash pointer
    //   [rbp - 24] = loop index
    //   [rbp - 32] = element tag
    //   [rbp - 40] = normalized key_lo
    //   [rbp - 48] = normalized key_hi
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the walk bookkeeping
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for the nested helper calls
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source array across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // preserve the compile-time element tag

    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // rax = source element count
    emitter.instruction("shl rax, 1");                                          // double it so the destination has insertion headroom
    emitter.instruction("cmp rax, 16");                                         // compare the derived capacity against the runtime minimum
    emitter.instruction("jge __rt_array_count_values_capacity_x86");            // keep the doubled count when it already meets the minimum
    emitter.instruction("mov rax, 16");                                         // clamp very small sources up to the minimum bucket count
    emitter.label("__rt_array_count_values_capacity_x86");
    emitter.instruction("mov rdi, rax");                                        // rdi = destination bucket count
    emitter.instruction("xor esi, esi");                                        // destination value_type 0: tallies are integers
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the destination hash across insertions
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // start the walk at source index 0

    emitter.label("__rt_array_count_values_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // r10 = source array pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // rcx = current source index
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // has every source element been tallied?
    emitter.instruction("jge __rt_array_count_values_done_x86");                // yes - the destination hash is complete
    emitter.instruction("lea r11, [r10 + 24]");                                 // r11 = source payload base
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // r10 = compile-time element tag
    emitter.instruction("cmp r10, 1");                                          // element tag 1 = string payload
    emitter.instruction("je __rt_array_count_values_str_x86");                  // string elements live in 16-byte slots
    emitter.instruction("cmp r10, 7");                                          // element tag 7 = boxed Mixed payload
    emitter.instruction("je __rt_array_count_values_mixed_x86");                // boxed elements need a runtime tag dispatch
    emitter.instruction("cmp r10, 0");                                          // element tag 0 = plain integer payload
    emitter.instruction("jne __rt_array_count_values_skip_x86");                // float/bool/array/object elements are skipped
    emitter.instruction("mov rax, QWORD PTR [r11 + rcx * 8]");                  // rax = integer value becoming the tally key
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the tally key low word
    emitter.instruction("mov QWORD PTR [rbp - 48], -1");                        // key_hi sentinel marks an inline integer key
    emitter.instruction("jmp __rt_array_count_values_bump_x86");                // tally this integer value

    emitter.label("__rt_array_count_values_str_x86");
    emitter.instruction("shl rcx, 4");                                          // convert the element index into a 16-byte slot offset
    emitter.instruction("add r11, rcx");                                        // advance to the selected string slot
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // rax = source string pointer
    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                        // rdx = source string length
    emitter.instruction("call __rt_hash_normalize_key");                        // collapse PHP numeric-string values into integer keys
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the normalized key low word
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the normalized key high word
    emitter.instruction("jmp __rt_array_count_values_bump_x86");                // tally this string value

    emitter.label("__rt_array_count_values_mixed_x86");
    emitter.instruction("mov rax, QWORD PTR [r11 + rcx * 8]");                  // rax = boxed Mixed cell for this element
    emitter.instruction("test rax, rax");                                       // is the cell null?
    emitter.instruction("je __rt_array_count_values_skip_x86");                 // a null cell is not a countable value
    emitter.instruction("call __rt_mixed_unbox");                               // rax = concrete tag, rdi = value_lo, rdx = value_hi
    emitter.instruction("cmp rax, 0");                                          // runtime tag 0 = int
    emitter.instruction("je __rt_array_count_values_mixed_int_x86");            // integers become inline integer keys
    emitter.instruction("cmp rax, 1");                                          // runtime tag 1 = string
    emitter.instruction("jne __rt_array_count_values_skip_x86");                // every other tag is skipped with a warning
    emitter.instruction("mov rax, rdi");                                        // rax = unboxed string pointer
    emitter.instruction("call __rt_hash_normalize_key");                        // collapse PHP numeric-string values into integer keys
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the normalized key low word
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the normalized key high word
    emitter.instruction("jmp __rt_array_count_values_bump_x86");                // tally this unboxed string value

    emitter.label("__rt_array_count_values_mixed_int_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save the unboxed integer as the tally key
    emitter.instruction("mov QWORD PTR [rbp - 48], -1");                        // key_hi sentinel marks an inline integer key

    emitter.label("__rt_array_count_values_bump_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // rdi = destination hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // rsi = tally key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // rdx = tally key high word
    emitter.instruction("call __rt_count_values_bump");                         // increment the tally for this key
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // keep the destination pointer current after growth

    emitter.label("__rt_array_count_values_next_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the source index after the helper calls
    emitter.instruction("add r10, 1");                                          // advance to the next source element
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // persist the updated source index
    emitter.instruction("jmp __rt_array_count_values_loop_x86");                // continue tallying source elements

    emitter.label("__rt_array_count_values_skip_x86");
    abi::emit_symbol_address(emitter, "rdi", ARRAY_COUNT_VALUES_SKIPPED_MESSAGES[0].0);
    emitter.instruction(&format!("mov esi, {}", skip_message_len()));           // pass the complete skip-warning length
    emitter.instruction("call __rt_diag_warning");                              // emit or suppress the PHP skip warning
    emitter.instruction("jmp __rt_array_count_values_next_x86");                // skip this element and continue

    emitter.label("__rt_array_count_values_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the destination hash pointer
    emitter.instruction("add rsp, 64");                                         // release the count-values spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with rax = destination hash pointer
}

/// Emits the x86_64 System V variant of `__rt_hash_count_values`.
fn emit_hash_count_values_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_count_values ---");
    emitter.label_global("__rt_hash_count_values");

    // Frame layout:
    //   [rbp - 8]  = source hash pointer
    //   [rbp - 16] = destination hash pointer
    //   [rbp - 24] = insertion-order iterator cursor
    //   [rbp - 32] = normalized key_lo
    //   [rbp - 40] = normalized key_hi
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the walk bookkeeping
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for the nested helper calls
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source hash across helper calls

    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // rax = source entry count
    emitter.instruction("shl rax, 1");                                          // double it so the destination has insertion headroom
    emitter.instruction("cmp rax, 16");                                         // compare the derived capacity against the runtime minimum
    emitter.instruction("jge __rt_hash_count_values_capacity_x86");             // keep the doubled count when it already meets the minimum
    emitter.instruction("mov rax, 16");                                         // clamp very small sources up to the minimum bucket count
    emitter.label("__rt_hash_count_values_capacity_x86");
    emitter.instruction("mov rdi, rax");                                        // rdi = destination bucket count
    emitter.instruction("xor esi, esi");                                        // destination value_type 0: tallies are integers
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the destination hash across insertions
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // iterator cursor = 0 (start from header.head)

    emitter.label("__rt_hash_count_values_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // rdi = source hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // rsi = current insertion-order cursor
    emitter.instruction("call __rt_hash_iter_next_value");                      // rax=cursor, rcx=value_lo, r8=value_hi, r9=value_tag
    emitter.instruction("cmp rax, -1");                                         // did the iterator signal end-of-walk?
    emitter.instruction("je __rt_hash_count_values_done_x86");                  // yes - the destination hash is complete
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the next insertion-order cursor
    emitter.instruction("mov rax, r9");                                         // rax = source value tag
    emitter.instruction("mov rdi, rcx");                                        // rdi = source value low word
    emitter.instruction("mov rdx, r8");                                         // rdx = source value high word
    emitter.instruction("cmp rax, 7");                                          // runtime tag 7 = boxed mixed cell
    emitter.instruction("jne __rt_hash_count_values_tag_ready_x86");            // concrete tags are already usable
    emitter.instruction("test rdi, rdi");                                       // is the boxed cell null?
    emitter.instruction("je __rt_hash_count_values_skip_x86");                  // a null cell is not a countable value
    emitter.instruction("mov rax, rdi");                                        // rax = boxed mixed pointer for unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // rax = concrete tag, rdi = value_lo, rdx = value_hi

    emitter.label("__rt_hash_count_values_tag_ready_x86");
    emitter.instruction("cmp rax, 0");                                          // runtime tag 0 = int
    emitter.instruction("je __rt_hash_count_values_int_x86");                   // integers become inline integer keys
    emitter.instruction("cmp rax, 1");                                          // runtime tag 1 = string
    emitter.instruction("jne __rt_hash_count_values_skip_x86");                 // every other tag is skipped with a warning
    emitter.instruction("mov rax, rdi");                                        // rax = source string pointer
    emitter.instruction("call __rt_hash_normalize_key");                        // collapse PHP numeric-string values into integer keys
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the normalized key low word
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the normalized key high word
    emitter.instruction("jmp __rt_hash_count_values_bump_x86");                 // tally this string value

    emitter.label("__rt_hash_count_values_int_x86");
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the integer value as the tally key
    emitter.instruction("mov QWORD PTR [rbp - 40], -1");                        // key_hi sentinel marks an inline integer key

    emitter.label("__rt_hash_count_values_bump_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // rdi = destination hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // rsi = tally key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // rdx = tally key high word
    emitter.instruction("call __rt_count_values_bump");                         // increment the tally for this key
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // keep the destination pointer current after growth
    emitter.instruction("jmp __rt_hash_count_values_loop_x86");                 // continue with the next source entry

    emitter.label("__rt_hash_count_values_skip_x86");
    abi::emit_symbol_address(emitter, "rdi", ARRAY_COUNT_VALUES_SKIPPED_MESSAGES[0].0);
    emitter.instruction(&format!("mov esi, {}", skip_message_len()));           // pass the complete skip-warning length
    emitter.instruction("call __rt_diag_warning");                              // emit or suppress the PHP skip warning
    emitter.instruction("jmp __rt_hash_count_values_loop_x86");                 // skip this entry and continue

    emitter.label("__rt_hash_count_values_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the destination hash pointer
    emitter.instruction("add rsp, 64");                                         // release the count-values spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with rax = destination hash pointer
}
