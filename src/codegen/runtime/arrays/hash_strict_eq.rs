//! Purpose:
//! Emits the `__rt_hash_strict_eq` runtime helper for associative-array (hash)
//! strict equality (`===`) comparisons. Operates on the hash table layout
//! `[count:8][capacity:8][value_type:8][head:8][tail:8][entries...]` produced
//! by `__rt_hash_new` / `__rt_hash_set`, where each 64-byte entry is
//! `[occupied:8][key_ptr:8][key_len:8][value_lo:8][value_hi:8][value_tag:8][prev:8][next:8]`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Compares two hashes for PHP strict equality: identical count, identical
//!   keys in the same insertion order, and identical value types and values.
//! - Both hashes are walked in parallel through their `head`/`next`
//!   insertion-order chains. The right hash is NOT indexed by a sequential
//!   position counter, because hash slots are placed by hashing, not by
//!   insertion order; only the `head`/`next` chain preserves insertion order.
//! - A pointer-identity short-circuit handles aliases and cycles.
//! - Value comparison dispatches on the runtime value tag: scalar tags compare
//!   the low/high payload words; string tags compare by value through
//!   `__rt_str_eq`; boxed Mixed tags delegate to `__rt_mixed_strict_eq`.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the hash strict equality helper for the current target.
///
/// Dispatches to the x86_64 Linux variant when targeting that architecture;
/// otherwise emits the portable ARM64 implementation.
///
/// # Inputs (ARM64)
/// - `x0`: left hash table pointer
/// - `x1`: right hash table pointer
///
/// # Output
/// - `x0` (ARM64) / `rax` (x86_64): `1` when strictly equal, `0` otherwise.
pub fn emit_hash_strict_eq(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_strict_eq_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_strict_eq ---");
    emitter.label_global("__rt_hash_strict_eq");

    // -- pointer-identity short-circuit (aliases and cycles) --
    emitter.instruction("cmp x0, x1");                                          // compare the left and right hash pointers for identity
    emitter.instruction("b.eq __rt_hash_strict_eq_true_fast");                  // identical pointers are strictly equal

    // -- set up stack frame and save inputs --
    // [sp,#0]   = left hash pointer
    // [sp,#8]   = right hash pointer
    // [sp,#16]  = current left slot index (insertion-order walk of left hash)
    // [sp,#24]  = current right slot index (insertion-order walk of right hash)
    // [sp,#32]  = saved x29
    // [sp,#40]  = saved x30
    // [sp,#48]  = left value_lo
    // [sp,#56]  = left value_hi
    // [sp,#64]  = left value_tag
    // [sp,#72]  = right value_lo
    // [sp,#80]  = right value_hi
    // [sp,#88]  = right value_tag
    emitter.instruction("sub sp, sp, #96");                                     // reserve the helper frame for inputs, slots, values, and saved registers
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save the left and right hash pointers

    // -- compare counts --
    emitter.instruction("ldr x2, [x0]");                                        // load the left hash count
    emitter.instruction("ldr x3, [x1]");                                        // load the right hash count
    emitter.instruction("cmp x2, x3");                                          // compare left and right counts
    emitter.instruction("b.ne __rt_hash_strict_eq_false_restore");              // different counts are never strictly equal

    // -- count zero short-circuits to true --
    emitter.instruction("cbz x2, __rt_hash_strict_eq_true_restore");            // empty hashes with matching counts are strictly equal

    // -- begin insertion-order walk from both hash heads --
    emitter.instruction("ldr x4, [x0, #24]");                                   // load the left hash head slot index
    emitter.instruction("ldr x5, [x1, #24]");                                   // load the right hash head slot index
    emitter.instruction("stp x4, x5, [sp, #16]");                               // save the current left and right slot indices

    emitter.label("__rt_hash_strict_eq_slot");
    emitter.instruction("ldr x4, [sp, #16]");                                   // reload the current left slot index
    emitter.instruction("ldr x5, [sp, #24]");                                   // reload the current right slot index

    // -- both chains ended together means a full match --
    emitter.instruction("cmp x4, #-1");                                         // has the left insertion-order chain ended?
    emitter.instruction("b.ne __rt_hash_strict_eq_left_alive");                 // left chain still has entries
    emitter.instruction("cmp x5, #-1");                                         // has the right insertion-order chain ended?
    emitter.instruction("b.eq __rt_hash_strict_eq_true_restore");               // both chains ended together: hashes are strictly equal
    emitter.instruction("b __rt_hash_strict_eq_false_restore");                 // left ended before right: insertion order differs

    emitter.label("__rt_hash_strict_eq_left_alive");
    emitter.instruction("cmp x5, #-1");                                         // has the right insertion-order chain ended early?
    emitter.instruction("b.eq __rt_hash_strict_eq_false_restore");              // right ended before left: insertion order differs

    // -- compute the left entry address: left_base + 40 + left_slot * 64 --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left hash pointer
    emitter.instruction("mov x6, #64");                                         // x6 = hash entry size in bytes
    emitter.instruction("mul x7, x4, x6");                                      // x7 = left slot index * 64
    emitter.instruction("add x7, x0, x7");                                      // x7 = left hash base + slot offset
    emitter.instruction("add x7, x7, #40");                                     // x7 = left entry address (skip header)

    // -- read the left key, value, and next link --
    emitter.instruction("ldr x8, [x7, #8]");                                    // x8 = left key_ptr
    emitter.instruction("ldr x9, [x7, #16]");                                   // x9 = left key_len
    emitter.instruction("ldr x10, [x7, #24]");                                  // x10 = left value_lo
    emitter.instruction("ldr x11, [x7, #32]");                                  // x11 = left value_hi
    emitter.instruction("ldr x12, [x7, #40]");                                  // x12 = left value_tag
    emitter.instruction("ldr x13, [x7, #56]");                                  // x13 = left next slot index

    // -- compute the right entry address: right_base + 40 + right_slot * 64 --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the right hash pointer
    emitter.instruction("mov x6, #64");                                         // x6 = hash entry size in bytes
    emitter.instruction("mul x14, x5, x6");                                     // x14 = right slot index * 64
    emitter.instruction("add x14, x1, x14");                                    // x14 = right hash base + slot offset
    emitter.instruction("add x14, x14, #40");                                   // x14 = right entry address (skip header)

    // -- read the right key, value, and next link --
    emitter.instruction("ldr x15, [x14, #8]");                                  // x15 = right key_ptr
    emitter.instruction("ldr x16, [x14, #16]");                                 // x16 = right key_len
    emitter.instruction("ldr x17, [x14, #24]");                                 // x17 = right value_lo
    emitter.instruction("ldr x19, [x14, #32]");                                 // x19 = right value_hi
    emitter.instruction("ldr x20, [x14, #40]");                                 // x20 = right value_tag
    emitter.instruction("ldr x21, [x14, #56]");                                 // x21 = right next slot index

    // -- save next links and value payloads across the key-eq call --
    emitter.instruction("stp x13, x21, [sp, #16]");                             // store left and right next slot indices into the slot-index slots
    emitter.instruction("stp x10, x11, [sp, #48]");                             // save left value_lo and value_hi across the nested call
    emitter.instruction("str x12, [sp, #64]");                                  // save left value_tag across the nested call
    emitter.instruction("stp x17, x19, [sp, #72]");                             // save right value_lo and value_hi across the nested call
    emitter.instruction("str x20, [sp, #88]");                                  // save right value_tag across the nested call
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve frame registers around the nested helper call

    // -- compare the keys via __rt_hash_key_eq(left_lo, left_hi, right_lo, right_hi) --
    emitter.instruction("mov x1, x8");                                          // left key_ptr into the first key-eq argument
    emitter.instruction("mov x2, x9");                                          // left key_len into the second key-eq argument
    emitter.instruction("mov x3, x15");                                         // right key_ptr into the third key-eq argument
    emitter.instruction("mov x4, x16");                                         // right key_len into the fourth key-eq argument
    emitter.instruction("bl __rt_hash_key_eq");                                 // compare the two keys for equality
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame registers after the nested helper call
    emitter.instruction("cbz x0, __rt_hash_strict_eq_false_restore");           // a mismatched key makes the hashes unequal

    // -- reload saved value payloads and tags --
    emitter.instruction("ldr x7, [sp, #48]");                                   // x7 = left value_lo
    emitter.instruction("ldr x8, [sp, #56]");                                   // x8 = left value_hi
    emitter.instruction("ldr x9, [sp, #64]");                                   // x9 = left value_tag
    emitter.instruction("ldr x10, [sp, #72]");                                  // x10 = right value_lo
    emitter.instruction("ldr x11, [sp, #80]");                                  // x11 = right value_hi
    emitter.instruction("ldr x12, [sp, #88]");                                  // x12 = right value_tag

    // -- value tags must match --
    emitter.instruction("cmp x9, x12");                                         // compare left and right value tags
    emitter.instruction("b.ne __rt_hash_strict_eq_false_restore");              // different value tags are never strictly equal

    // -- dispatch on the shared value tag --
    emitter.instruction("cmp x9, #1");                                          // value tag 1 marks string payloads
    emitter.instruction("b.eq __rt_hash_strict_eq_value_str");                  // route string values through __rt_str_eq
    emitter.instruction("cmp x9, #7");                                          // value tag 7 marks boxed Mixed payloads
    emitter.instruction("b.eq __rt_hash_strict_eq_value_mixed");                // route Mixed values through __rt_mixed_strict_eq

    // -- scalar value comparison: low and high payload words --
    emitter.instruction("cmp x7, x10");                                         // compare the low payload words
    emitter.instruction("b.ne __rt_hash_strict_eq_false_restore");              // mismatched low words are not equal
    emitter.instruction("cmp x8, x11");                                         // compare the high payload words
    emitter.instruction("b.ne __rt_hash_strict_eq_false_restore");              // mismatched high words are not equal
    emitter.instruction("b __rt_hash_strict_eq_advance");                       // scalar values matched

    // -- string value comparison via __rt_str_eq(ptr_a, len_a, ptr_b, len_b) --
    emitter.label("__rt_hash_strict_eq_value_str");
    emitter.instruction("mov x1, x7");                                          // left string pointer into the first str_eq argument
    emitter.instruction("mov x2, x8");                                          // left string length into the second str_eq argument
    emitter.instruction("mov x3, x10");                                         // right string pointer into the third str_eq argument
    emitter.instruction("mov x4, x11");                                         // right string length into the fourth str_eq argument
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve frame registers around the nested helper call
    emitter.instruction("bl __rt_str_eq");                                      // compare the two string payloads byte-by-byte
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame registers after the nested helper call
    emitter.instruction("cbz x0, __rt_hash_strict_eq_false_restore");           // a mismatched string makes the hashes unequal
    emitter.instruction("b __rt_hash_strict_eq_advance");                       // string values matched

    // -- boxed Mixed value comparison via __rt_mixed_strict_eq --
    emitter.label("__rt_hash_strict_eq_value_mixed");
    emitter.instruction("mov x0, x7");                                          // left boxed Mixed pointer into the first mixed-eq argument
    emitter.instruction("mov x1, x10");                                         // right boxed Mixed pointer into the second mixed-eq argument
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve frame registers around the nested helper call
    emitter.instruction("bl __rt_mixed_strict_eq");                             // compare the boxed Mixed cells by tag and payload
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame registers after the nested helper call
    emitter.instruction("cbz x0, __rt_hash_strict_eq_false_restore");           // a mismatched Mixed cell makes the hashes unequal
    emitter.instruction("b __rt_hash_strict_eq_advance");                       // Mixed values matched

    // -- advance both chains to the next insertion-order slot --
    emitter.label("__rt_hash_strict_eq_advance");
    emitter.instruction("ldr x4, [sp, #16]");                                   // reload the left next slot index
    emitter.instruction("ldr x5, [sp, #24]");                                   // reload the right next slot index
    emitter.instruction("stp x4, x5, [sp, #16]");                               // store the updated left and right slot indices
    emitter.instruction("b __rt_hash_strict_eq_slot");                          // continue the parallel walk

    // -- result paths --
    emitter.label("__rt_hash_strict_eq_true_restore");
    emitter.instruction("mov x0, #1");                                          // materialize the strict-equality true result
    emitter.instruction("b __rt_hash_strict_eq_epilogue");                      // skip the false path

    emitter.label("__rt_hash_strict_eq_false_restore");
    emitter.instruction("mov x0, #0");                                          // materialize the strict-equality false result

    emitter.label("__rt_hash_strict_eq_epilogue");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the strict-equality boolean in x0

    // -- no-frame fast path for identical pointers --
    emitter.label("__rt_hash_strict_eq_true_fast");
    emitter.instruction("mov x0, #1");                                          // materialize true for identical pointers
    emitter.instruction("ret");                                                 // return true without allocating a frame
}

/// Emits the x86_64 Linux variant of the hash strict equality helper.
///
/// Mirrors the ARM64 algorithm using the System V AMD64 ABI: `rdi` for the left
/// hash pointer and `rsi` for the right, with the boolean result in `rax`.
/// Both hashes are walked in parallel through their `head`/`next` chains so
/// insertion order is compared correctly regardless of slot placement.
fn emit_hash_strict_eq_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_strict_eq ---");
    emitter.label_global("__rt_hash_strict_eq");

    // -- pointer-identity short-circuit (aliases and cycles) --
    emitter.instruction("cmp rdi, rsi");                                        // compare the left and right hash pointers for identity
    emitter.instruction("je __rt_hash_strict_eq_true_fast");                    // identical pointers are strictly equal

    // -- set up stack frame and save inputs --
    // [rbp - 8]   = left hash pointer
    // [rbp - 16]  = right hash pointer
    // [rbp - 24]  = current left slot index
    // [rbp - 32]  = current right slot index
    // [rbp - 40]  = left next slot index (saved across key-eq call)
    // [rbp - 48]  = right next slot index (saved across key-eq call)
    // [rbp - 56]  = left value_lo
    // [rbp - 64]  = left value_hi
    // [rbp - 72]  = left value_tag
    // [rbp - 80]  = right value_lo
    // [rbp - 88]  = right value_hi
    // [rbp - 96]  = right value_tag
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame base
    emitter.instruction("sub rsp, 96");                                         // reserve spill slots for inputs, slots, and value payloads
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the right hash pointer

    // -- compare counts --
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the left hash count
    emitter.instruction("mov r11, QWORD PTR [rsi]");                            // load the right hash count
    emitter.instruction("cmp r10, r11");                                        // compare left and right counts
    emitter.instruction("jne __rt_hash_strict_eq_false_restore");               // different counts are never strictly equal

    // -- count zero short-circuits to true --
    emitter.instruction("test r10, r10");                                       // is the shared count zero?
    emitter.instruction("jz __rt_hash_strict_eq_true_restore");                 // empty hashes with matching counts are strictly equal

    // -- begin insertion-order walk from both hash heads --
    emitter.instruction("mov r10, QWORD PTR [rdi + 24]");                       // load the left hash head slot index
    emitter.instruction("mov r11, QWORD PTR [rsi + 24]");                       // load the right hash head slot index
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the current left slot index
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the current right slot index

    emitter.label("__rt_hash_strict_eq_slot");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the current left slot index
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the current right slot index

    // -- both chains ended together means a full match --
    emitter.instruction("cmp r10, -1");                                         // has the left insertion-order chain ended?
    emitter.instruction("jne __rt_hash_strict_eq_left_alive");                  // left chain still has entries
    emitter.instruction("cmp r11, -1");                                         // has the right insertion-order chain ended?
    emitter.instruction("je __rt_hash_strict_eq_true_restore");                 // both chains ended together: hashes are strictly equal
    emitter.instruction("jmp __rt_hash_strict_eq_false_restore");               // left ended before right: insertion order differs

    emitter.label("__rt_hash_strict_eq_left_alive");
    emitter.instruction("cmp r11, -1");                                         // has the right insertion-order chain ended early?
    emitter.instruction("je __rt_hash_strict_eq_false_restore");                // right ended before left: insertion order differs

    // -- compute the left entry address: left_base + 40 + left_slot * 64 --
    emitter.instruction("mov rax, r10");                                        // copy the left slot index before scaling it into a byte offset
    emitter.instruction("shl rax, 6");                                          // convert the left slot index into a 64-byte hash-entry offset
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the left hash pointer
    emitter.instruction("lea r8, [rcx + rax + 40]");                            // r8 = left entry address (skip header)

    // -- read the left key, value, and next link --
    emitter.instruction("mov rdi, QWORD PTR [r8 + 8]");                         // rdi = left key_ptr (first key-eq argument)
    emitter.instruction("mov rsi, QWORD PTR [r8 + 16]");                        // rsi = left key_len (second key-eq argument)
    emitter.instruction("mov r9, QWORD PTR [r8 + 24]");                         // r9 = left value_lo
    emitter.instruction("mov r10, QWORD PTR [r8 + 32]");                        // r10 = left value_hi
    emitter.instruction("mov r11, QWORD PTR [r8 + 40]");                        // r11 = left value_tag
    emitter.instruction("mov rcx, QWORD PTR [r8 + 56]");                        // rcx = left next slot index

    // -- compute the right entry address: right_base + 40 + right_slot * 64 --
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the current right slot index
    emitter.instruction("shl rax, 6");                                          // convert the right slot index into a 64-byte hash-entry offset
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the right hash pointer
    emitter.instruction("lea r8, [rdx + rax + 40]");                            // r8 = right entry address (skip header)

    // -- read the right key, value, and next link --
    emitter.instruction("mov rdx, QWORD PTR [r8 + 8]");                         // rdx = right key_ptr (third key-eq argument)
    emitter.instruction("mov rcx, QWORD PTR [r8 + 16]");                        // rcx = right key_len (fourth key-eq argument)
    emitter.instruction("mov r12, QWORD PTR [r8 + 24]");                        // r12 = right value_lo
    emitter.instruction("mov r13, QWORD PTR [r8 + 32]");                        // r13 = right value_hi
    emitter.instruction("mov r14, QWORD PTR [r8 + 40]");                        // r14 = right value_tag
    emitter.instruction("mov r15, QWORD PTR [r8 + 56]");                        // r15 = right next slot index

    // -- save next links and value payloads across the key-eq call --
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save left next slot index (rcx holds left next from above)
    emitter.instruction("mov QWORD PTR [rbp - 48], r15");                       // save right next slot index
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // save left value_lo
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // save left value_hi
    emitter.instruction("mov QWORD PTR [rbp - 72], r11");                       // save left value_tag
    emitter.instruction("mov QWORD PTR [rbp - 80], r12");                       // save right value_lo
    emitter.instruction("mov QWORD PTR [rbp - 88], r13");                       // save right value_hi
    emitter.instruction("mov QWORD PTR [rbp - 96], r14");                       // save right value_tag

    // -- compare the keys via __rt_hash_key_eq(left_lo, left_hi, right_lo, right_hi) --
    emitter.instruction("call __rt_hash_key_eq");                               // compare the two keys for equality
    emitter.instruction("test rax, rax");                                       // check the key-equality helper result
    emitter.instruction("jz __rt_hash_strict_eq_false_restore");                // a mismatched key makes the hashes unequal

    // -- reload saved value payloads and tags --
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // r8 = left value_lo
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // rcx = left value_hi
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // r9 = left value_tag
    emitter.instruction("mov r12, QWORD PTR [rbp - 80]");                       // r12 = right value_lo
    emitter.instruction("mov r13, QWORD PTR [rbp - 88]");                       // r13 = right value_hi
    emitter.instruction("mov r14, QWORD PTR [rbp - 96]");                       // r14 = right value_tag

    // -- value tags must match --
    emitter.instruction("cmp r9, r14");                                         // compare left and right value tags
    emitter.instruction("jne __rt_hash_strict_eq_false_restore");               // different value tags are never strictly equal

    // -- dispatch on the shared value tag --
    emitter.instruction("cmp r9, 1");                                           // value tag 1 marks string payloads
    emitter.instruction("je __rt_hash_strict_eq_value_str");                    // route string values through __rt_str_eq
    emitter.instruction("cmp r9, 7");                                           // value tag 7 marks boxed Mixed payloads
    emitter.instruction("je __rt_hash_strict_eq_value_mixed");                  // route Mixed values through __rt_mixed_strict_eq

    // -- scalar value comparison: low and high payload words --
    emitter.instruction("cmp r8, r12");                                         // compare the low payload words
    emitter.instruction("jne __rt_hash_strict_eq_false_restore");               // mismatched low words are not equal
    emitter.instruction("cmp rcx, r13");                                        // compare the high payload words
    emitter.instruction("jne __rt_hash_strict_eq_false_restore");               // mismatched high words are not equal
    emitter.instruction("jmp __rt_hash_strict_eq_advance");                     // scalar values matched

    // -- string value comparison via __rt_str_eq(ptr_a, len_a, ptr_b, len_b) --
    emitter.label("__rt_hash_strict_eq_value_str");
    emitter.instruction("mov rdi, r8");                                         // left string pointer into the first str_eq argument
    emitter.instruction("mov rsi, rcx");                                        // left string length into the second str_eq argument
    emitter.instruction("mov rdx, r12");                                        // right string pointer into the third str_eq argument
    emitter.instruction("mov rcx, r13");                                        // right string length into the fourth str_eq argument
    emitter.instruction("call __rt_str_eq");                                    // compare the two string payloads byte-by-byte
    emitter.instruction("test rax, rax");                                       // check the string-equality helper result
    emitter.instruction("jz __rt_hash_strict_eq_false_restore");                // a mismatched string makes the hashes unequal
    emitter.instruction("jmp __rt_hash_strict_eq_advance");                     // string values matched

    // -- boxed Mixed value comparison via __rt_mixed_strict_eq --
    emitter.label("__rt_hash_strict_eq_value_mixed");
    emitter.instruction("mov rdi, r8");                                         // left boxed Mixed pointer into the first mixed-eq argument
    emitter.instruction("mov rsi, r12");                                        // right boxed Mixed pointer into the second mixed-eq argument
    emitter.instruction("call __rt_mixed_strict_eq");                           // compare the boxed Mixed cells by tag and payload
    emitter.instruction("test rax, rax");                                       // check the mixed-equality helper result
    emitter.instruction("jz __rt_hash_strict_eq_false_restore");                // a mismatched Mixed cell makes the hashes unequal
    emitter.instruction("jmp __rt_hash_strict_eq_advance");                     // Mixed values matched

    // -- advance both chains to the next insertion-order slot --
    emitter.label("__rt_hash_strict_eq_advance");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the left next slot index
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the right next slot index
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // store the updated left slot index
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // store the updated right slot index
    emitter.instruction("jmp __rt_hash_strict_eq_slot");                        // continue the parallel walk

    // -- result paths --
    emitter.label("__rt_hash_strict_eq_true_restore");
    emitter.instruction("mov rax, 1");                                          // materialize the strict-equality true result
    emitter.instruction("jmp __rt_hash_strict_eq_epilogue");                    // skip the false path

    emitter.label("__rt_hash_strict_eq_false_restore");
    emitter.instruction("xor rax, rax");                                        // materialize the strict-equality false result

    emitter.label("__rt_hash_strict_eq_epilogue");
    emitter.instruction("add rsp, 96");                                         // release the helper spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the strict-equality boolean in rax

    // -- no-frame fast path for identical pointers --
    emitter.label("__rt_hash_strict_eq_true_fast");
    emitter.instruction("mov rax, 1");                                          // materialize true for identical pointers
    emitter.instruction("ret");                                                 // return true without allocating a frame
}