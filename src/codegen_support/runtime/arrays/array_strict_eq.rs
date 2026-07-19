//! Purpose:
//! Emits the `__rt_array_strict_eq`, `__rt_array_iter_next` runtime helper assembly for deep PHP
//! `===` structural equality of two arrays.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - `__rt_mixed_strict_eq` dispatches array/hash payload tags (4/5) here so that `$a === $b` and
//!   every nested array element compares by structure rather than heap-pointer identity.
//!
//! Key details:
//! - PHP `===` on arrays requires the same element count, the same key => value pairs in the same
//!   insertion order, and each value strictly equal (recursively). Keys are strict-typed (an int key
//!   never equals a string key). Two arrays stored in different representations (a packed indexed
//!   array vs. a hash) still compare equal when they yield the same ordered (key, value) sequence, so
//!   comparison is driven by a uniform logical iterator rather than by matching representations.
//! - The value comparison materializes each element as a temporary 24-byte Mixed cell on the stack
//!   and delegates to `__rt_mixed_strict_eq`, which recurses back here for nested array values. No
//!   heap allocation and no refcount mutation occur, so the comparison is ownership-neutral.
//! - Homogeneous packed scalar arrays store int/float/bool inline under one array-wide value_type
//!   (0), so their elements compare by raw payload bits; heterogeneous arrays box each element
//!   (value_type 7) and keep full per-element type precision.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_array_strict_eq` and its private `__rt_array_iter_next` helper for the host target.
///
/// `__rt_array_strict_eq` implements deep PHP `===` for two arrays. It short-circuits on pointer
/// identity, rejects on unequal element counts, then walks both operands in lock-step through a
/// uniform logical iterator, comparing each key with `__rt_hash_key_eq` and each value through a
/// stack-materialized Mixed cell passed to `__rt_mixed_strict_eq` (which recurses back here for
/// nested arrays). `__rt_array_iter_next` abstracts the packed-array and hash representations behind
/// a single ordered `(next_cursor, key, value)` protocol.
///
/// # Inputs / outputs (AArch64)
/// - `__rt_array_strict_eq`: `x0` = left array/hash pointer, `x1` = right array/hash pointer →
///   `x0` = 1 when deeply equal, 0 otherwise.
/// - `__rt_array_iter_next`: `x0` = array/hash pointer, `x1` = cursor (0 to start) → `x0` = next
///   cursor (`-1` when exhausted), `x1` = key low word, `x2` = key high word (`-1` for an int key),
///   `x3` = value tag, `x4` = value low word, `x5` = value high word.
///
/// Delegates to `emit_array_strict_eq_linux_x86_64` on x86_64.
pub fn emit_array_strict_eq(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_strict_eq_linux_x86_64(emitter);
        return;
    }

    emit_array_strict_eq_aarch64(emitter);
    emit_array_iter_next_aarch64(emitter);
}

/// Emits the AArch64 `__rt_array_strict_eq` deep-equality driver.
fn emit_array_strict_eq_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_strict_eq ---");
    emitter.label_global("__rt_array_strict_eq");

    // -- frame: 128 bytes; slots for both Mixed value cells, the left key/cursor spills, and the
    //    callee-saved loop state preserved across the iterator/compare calls --
    emitter.instruction("sub sp, sp, #128");                                    // allocate the deep-compare stack frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish the helper stack frame
    emitter.instruction("stp x19, x20, [sp, #80]");                             // preserve callee-saved registers for the array pointers
    emitter.instruction("stp x21, x22, [sp, #96]");                             // preserve callee-saved registers for the two cursors

    emitter.instruction("mov x19, x0");                                         // x19 = left array/hash pointer
    emitter.instruction("mov x20, x1");                                         // x20 = right array/hash pointer
    emitter.instruction("cmp x0, x1");                                          // identical heap pointers are trivially strictly equal
    emitter.instruction("b.eq __rt_array_strict_eq_true");                      // short-circuit copy-on-write shared arrays
    emitter.instruction("ldr x9, [x0]");                                        // x9 = left element count (header word 0)
    emitter.instruction("ldr x10, [x1]");                                       // x10 = right element count (header word 0)
    emitter.instruction("cmp x9, x10");                                         // arrays of different length are never strictly equal
    emitter.instruction("b.ne __rt_array_strict_eq_false");                     // reject on count mismatch
    emitter.instruction("cbz x9, __rt_array_strict_eq_true");                   // two empty arrays are strictly equal
    emitter.instruction("mov x21, #0");                                         // x21 = left cursor (fresh walk)
    emitter.instruction("mov x22, #0");                                         // x22 = right cursor (fresh walk)

    // -- lock-step walk of both operands --
    emitter.label("__rt_array_strict_eq_loop");
    emitter.instruction("mov x0, x19");                                         // iterate the left operand
    emitter.instruction("mov x1, x21");                                         // from the left cursor
    emitter.instruction("bl __rt_array_iter_next");                             // x0=next, x1=key_lo, x2=key_hi, x3=tag, x4=lo, x5=hi
    emitter.instruction("str x0, [sp, #64]");                                   // spill the left next cursor across the right iteration
    emitter.instruction("stp x1, x2, [sp, #48]");                               // spill the left key (lo, hi) for the key comparison
    emitter.instruction("str x3, [sp, #0]");                                    // build the left Mixed cell: value tag
    emitter.instruction("stp x4, x5, [sp, #8]");                                // build the left Mixed cell: value low/high words
    emitter.instruction("mov x0, x20");                                         // iterate the right operand
    emitter.instruction("mov x1, x22");                                         // from the right cursor
    emitter.instruction("bl __rt_array_iter_next");                             // x0=next, x1=key_lo, x2=key_hi, x3=tag, x4=lo, x5=hi
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the left next cursor
    emitter.instruction("cmn x9, #1");                                          // is the left operand exhausted (next == -1)?
    emitter.instruction("b.eq __rt_array_strict_eq_left_done");                 // handle simultaneous termination
    emitter.instruction("cmn x0, #1");                                          // the right operand ended before the left one?
    emitter.instruction("b.eq __rt_array_strict_eq_false");                     // unequal length despite the count pre-check
    emitter.instruction("str x0, [sp, #72]");                                   // spill the right next cursor
    emitter.instruction("str x3, [sp, #24]");                                   // build the right Mixed cell: value tag
    emitter.instruction("stp x4, x5, [sp, #32]");                               // build the right Mixed cell: value low/high words
    emitter.instruction("mov x3, x1");                                          // right key low word into the key-eq third argument
    emitter.instruction("mov x4, x2");                                          // right key high word into the key-eq fourth argument
    emitter.instruction("ldp x1, x2, [sp, #48]");                               // reload the left key (lo, hi) into the first two arguments
    emitter.instruction("bl __rt_hash_key_eq");                                 // strict key comparison (int keys never equal string keys)
    emitter.instruction("cbz x0, __rt_array_strict_eq_false");                  // reject on differing keys
    emitter.instruction("add x0, sp, #0");                                      // left Mixed cell address
    emitter.instruction("add x1, sp, #24");                                     // right Mixed cell address
    emitter.instruction("bl __rt_mixed_strict_eq");                             // deep value comparison (recurses here for nested arrays)
    emitter.instruction("cbz x0, __rt_array_strict_eq_false");                  // reject on differing values
    emitter.instruction("ldr x21, [sp, #64]");                                  // advance the left cursor
    emitter.instruction("ldr x22, [sp, #72]");                                  // advance the right cursor
    emitter.instruction("b __rt_array_strict_eq_loop");                         // continue the lock-step walk

    emitter.label("__rt_array_strict_eq_left_done");
    emitter.instruction("cmn x0, #1");                                          // the left ended; the right must end simultaneously
    emitter.instruction("b.eq __rt_array_strict_eq_true");                      // both exhausted together -> strictly equal
    emitter.instruction("b __rt_array_strict_eq_false");                        // right still has entries -> unequal

    emitter.label("__rt_array_strict_eq_true");
    emitter.instruction("mov x0, #1");                                          // report strict structural equality
    emitter.instruction("b __rt_array_strict_eq_done");                         // fall through to the shared epilogue

    emitter.label("__rt_array_strict_eq_false");
    emitter.instruction("mov x0, #0");                                          // report that the arrays are not strictly equal

    emitter.label("__rt_array_strict_eq_done");
    emitter.instruction("ldp x21, x22, [sp, #96]");                             // restore the cursor callee-saved registers
    emitter.instruction("ldp x19, x20, [sp, #80]");                             // restore the array-pointer callee-saved registers
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the deep-compare stack frame
    emitter.instruction("ret");                                                 // return the strict-equality boolean in x0
}

/// Emits the AArch64 `__rt_array_iter_next` uniform logical iterator.
fn emit_array_iter_next_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_iter_next ---");
    emitter.label_global("__rt_array_iter_next");

    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the heap kind word
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low heap-kind byte (2 = packed, 3 = hash)
    emitter.instruction("cmp x9, #3");                                          // is the operand a hash (associative) container?
    emitter.instruction("b.eq __rt_array_iter_next_hash");                      // hashes delegate to the ordered hash iterator

    // -- packed indexed array: keys are the sequential indices 0..len-1 --
    emitter.instruction("ldr x10, [x0]");                                       // x10 = packed length (header word 0)
    emitter.instruction("cmp x1, x10");                                         // has the cursor walked past the last element?
    emitter.instruction("b.ge __rt_array_iter_next_done");                      // exhausted packed arrays report the done sentinel
    emitter.instruction("ldr x11, [x0, #-8]");                                  // reload the kind word for the value_type field
    emitter.instruction("lsr x11, x11, #8");                                    // shift the array-wide value_type into the low bits
    emitter.instruction("and x11, x11, #0x7f");                                 // isolate the value_type, dropping the copy-on-write bit
    emitter.instruction("add x12, x0, #24");                                    // x12 = base of the packed data region (skip 24-byte header)
    emitter.instruction("add x13, x1, #1");                                     // x13 = next cursor = index + 1
    emitter.instruction("cmp x11, #1");                                         // is the array a string array (16-byte {ptr,len} slots)?
    emitter.instruction("b.eq __rt_array_iter_next_packed_str");                // strings load a pointer/length pair
    emitter.instruction("cmp x11, #11");                                        // is the array a tagged-scalar array (per-slot tag)?
    emitter.instruction("b.eq __rt_array_iter_next_packed_tagged");             // tagged scalars carry a per-slot runtime tag

    // -- 8-byte-slot value types (int/float/bool inline, or array/object/mixed/callable pointer) --
    emitter.instruction("lsl x14, x1, #3");                                     // byte offset = index * 8
    emitter.instruction("ldr x4, [x12, x14]");                                  // value low word = data[index]
    emitter.instruction("mov x5, #0");                                          // value high word unused for 8-byte slots
    emitter.instruction("mov x3, x11");                                         // value tag = the array-wide value_type
    emitter.instruction("b __rt_array_iter_next_packed_ret");                   // return the synthesized packed entry

    emitter.label("__rt_array_iter_next_packed_str");
    emitter.instruction("lsl x14, x1, #4");                                     // byte offset = index * 16
    emitter.instruction("add x14, x12, x14");                                   // address of the string slot
    emitter.instruction("ldr x4, [x14]");                                       // value low word = string pointer
    emitter.instruction("ldr x5, [x14, #8]");                                   // value high word = string length
    emitter.instruction("mov x3, #1");                                          // value tag = string
    emitter.instruction("b __rt_array_iter_next_packed_ret");                   // return the synthesized packed string entry

    emitter.label("__rt_array_iter_next_packed_tagged");
    emitter.instruction("lsl x14, x1, #4");                                     // byte offset = index * 16
    emitter.instruction("add x14, x12, x14");                                   // address of the tagged-scalar slot
    emitter.instruction("ldr x4, [x14]");                                       // value low word = tagged-scalar payload
    emitter.instruction("ldr x3, [x14, #8]");                                   // value tag = the per-slot runtime tag
    emitter.instruction("mov x5, #0");                                          // value high word unused for tagged scalars
    emitter.instruction("b __rt_array_iter_next_packed_ret");                   // return the synthesized tagged-scalar entry

    emitter.label("__rt_array_iter_next_packed_ret");
    emitter.instruction("mov x0, x13");                                         // x0 = next cursor
    emitter.instruction("mov x2, #-1");                                         // key high word = -1 marks an integer key
    emitter.instruction("ret");                                                 // x1 already holds the integer key = the index

    emitter.label("__rt_array_iter_next_done");
    emitter.instruction("mov x0, #-1");                                         // report the done sentinel (no more entries)
    emitter.instruction("ret");                                                 // return to the lock-step driver

    // -- hash container: reuse the ordered insertion-order iterator and remap its registers --
    emitter.label("__rt_array_iter_next_hash");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller frame before the nested call
    emitter.instruction("mov x29, sp");                                         // establish a minimal frame for the hash iterator call
    emitter.instruction("bl __rt_hash_iter_next");                              // x0=next, x1=key_ptr, x2=key_len, x3=lo, x4=hi, x5=tag
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame after the hash iterator returns
    emitter.instruction("mov x9, x5");                                          // stash the hash value tag before remapping
    emitter.instruction("mov x10, x3");                                         // stash the hash value low word before remapping
    emitter.instruction("mov x11, x4");                                         // stash the hash value high word before remapping
    emitter.instruction("mov x3, x9");                                          // value tag into the uniform slot
    emitter.instruction("mov x4, x10");                                         // value low word into the uniform slot
    emitter.instruction("mov x5, x11");                                         // value high word into the uniform slot
    emitter.instruction("ret");                                                 // x0/x1/x2 already carry next cursor and key (lo, hi)
}

/// Emits the x86_64 Linux `__rt_array_strict_eq` and `__rt_array_iter_next` helpers.
///
/// Mirrors the AArch64 semantics using the System V AMD64 ABI: `rdi`/`rsi` carry the two array
/// pointers (or the array pointer and cursor), and the boolean / iterator results return in `rax`
/// (plus `rcx`/`rdx`/`r8`/`r9`/`r10` for the iterator's key and value words). Loop state is held in
/// callee-saved `r12`–`r15` across the iterator, key-eq, and value-eq calls.
fn emit_array_strict_eq_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_strict_eq ---");
    emitter.label_global("__rt_array_strict_eq");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("push r12");                                            // preserve callee-saved r12 (left array pointer)
    emitter.instruction("push r13");                                            // preserve callee-saved r13 (right array pointer)
    emitter.instruction("push r14");                                            // preserve callee-saved r14 (left cursor)
    emitter.instruction("push r15");                                            // preserve callee-saved r15 (right cursor)
    emitter.instruction("sub rsp, 96");                                         // reserve the Mixed value cells and key/cursor spill slots

    emitter.instruction("mov r12, rdi");                                        // r12 = left array/hash pointer
    emitter.instruction("mov r13, rsi");                                        // r13 = right array/hash pointer
    emitter.instruction("cmp rdi, rsi");                                        // identical heap pointers are trivially strictly equal
    emitter.instruction("je __rt_array_strict_eq_true");                        // short-circuit copy-on-write shared arrays
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // rax = left element count (header word 0)
    emitter.instruction("cmp rax, QWORD PTR [rsi]");                            // arrays of different length are never strictly equal
    emitter.instruction("jne __rt_array_strict_eq_false");                      // reject on count mismatch
    emitter.instruction("test rax, rax");                                       // are both operands empty?
    emitter.instruction("je __rt_array_strict_eq_true");                        // two empty arrays are strictly equal
    emitter.instruction("xor r14, r14");                                        // r14 = left cursor (fresh walk)
    emitter.instruction("xor r15, r15");                                        // r15 = right cursor (fresh walk)

    // Mixed-cell / spill layout relative to rsp: left cell [rsp+0..23], right cell [rsp+24..47],
    // left key [rsp+48..63], left next cursor [rsp+64], right next cursor [rsp+72].
    emitter.label("__rt_array_strict_eq_loop");
    emitter.instruction("mov rdi, r12");                                        // iterate the left operand
    emitter.instruction("mov rsi, r14");                                        // from the left cursor
    emitter.instruction("call __rt_array_iter_next");                           // rax=next, rcx=key_lo, rdx=key_hi, r8=tag, r9=lo, r10=hi
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // spill the left next cursor
    emitter.instruction("mov QWORD PTR [rsp + 48], rcx");                       // spill the left key low word
    emitter.instruction("mov QWORD PTR [rsp + 56], rdx");                       // spill the left key high word
    emitter.instruction("mov QWORD PTR [rsp + 0], r8");                         // build the left Mixed cell: value tag
    emitter.instruction("mov QWORD PTR [rsp + 8], r9");                         // build the left Mixed cell: value low word
    emitter.instruction("mov QWORD PTR [rsp + 16], r10");                       // build the left Mixed cell: value high word
    emitter.instruction("mov rdi, r13");                                        // iterate the right operand
    emitter.instruction("mov rsi, r15");                                        // from the right cursor
    emitter.instruction("call __rt_array_iter_next");                           // rax=next, rcx=key_lo, rdx=key_hi, r8=tag, r9=lo, r10=hi
    emitter.instruction("mov r11, QWORD PTR [rsp + 64]");                       // reload the left next cursor
    emitter.instruction("cmp r11, -1");                                         // is the left operand exhausted?
    emitter.instruction("je __rt_array_strict_eq_left_done");                   // handle simultaneous termination
    emitter.instruction("cmp rax, -1");                                         // the right operand ended before the left one?
    emitter.instruction("je __rt_array_strict_eq_false");                       // unequal length despite the count pre-check
    emitter.instruction("mov QWORD PTR [rsp + 72], rax");                       // spill the right next cursor
    emitter.instruction("mov QWORD PTR [rsp + 24], r8");                        // build the right Mixed cell: value tag
    emitter.instruction("mov QWORD PTR [rsp + 32], r9");                        // build the right Mixed cell: value low word
    emitter.instruction("mov QWORD PTR [rsp + 40], r10");                       // build the right Mixed cell: value high word
    emitter.instruction("xchg rcx, rdx");                                       // swap so rdx = right key low, rcx = right key high
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // key-eq first argument = left key low word
    emitter.instruction("mov rsi, QWORD PTR [rsp + 56]");                       // key-eq second argument = left key high word
    emitter.instruction("call __rt_hash_key_eq");                               // strict key comparison (int keys never equal string keys)
    emitter.instruction("test rax, rax");                                       // did the keys differ?
    emitter.instruction("je __rt_array_strict_eq_false");                       // reject on differing keys
    emitter.instruction("lea rdi, [rsp + 0]");                                  // left Mixed cell address
    emitter.instruction("lea rsi, [rsp + 24]");                                 // right Mixed cell address
    emitter.instruction("call __rt_mixed_strict_eq");                           // deep value comparison (recurses here for nested arrays)
    emitter.instruction("test rax, rax");                                       // did the values differ?
    emitter.instruction("je __rt_array_strict_eq_false");                       // reject on differing values
    emitter.instruction("mov r14, QWORD PTR [rsp + 64]");                       // advance the left cursor
    emitter.instruction("mov r15, QWORD PTR [rsp + 72]");                       // advance the right cursor
    emitter.instruction("jmp __rt_array_strict_eq_loop");                       // continue the lock-step walk

    emitter.label("__rt_array_strict_eq_left_done");
    emitter.instruction("cmp rax, -1");                                         // the left ended; the right must end simultaneously
    emitter.instruction("je __rt_array_strict_eq_true");                        // both exhausted together -> strictly equal
    emitter.instruction("jmp __rt_array_strict_eq_false");                      // right still has entries -> unequal

    emitter.label("__rt_array_strict_eq_true");
    emitter.instruction("mov eax, 1");                                          // report strict structural equality
    emitter.instruction("jmp __rt_array_strict_eq_done");                       // fall through to the shared epilogue

    emitter.label("__rt_array_strict_eq_false");
    emitter.instruction("xor eax, eax");                                        // report that the arrays are not strictly equal

    emitter.label("__rt_array_strict_eq_done");
    emitter.instruction("add rsp, 96");                                         // release the Mixed value cells and spill slots
    emitter.instruction("pop r15");                                             // restore callee-saved r15
    emitter.instruction("pop r14");                                             // restore callee-saved r14
    emitter.instruction("pop r13");                                             // restore callee-saved r13
    emitter.instruction("pop r12");                                             // restore callee-saved r12
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the strict-equality boolean in rax

    // -- uniform logical iterator --
    emitter.blank();
    emitter.comment("--- runtime: array_iter_next ---");
    emitter.label_global("__rt_array_iter_next");

    emitter.instruction("mov rax, QWORD PTR [rdi - 8]");                        // load the heap kind word
    emitter.instruction("and rax, 0xff");                                       // isolate the low heap-kind byte (2 = packed, 3 = hash)
    emitter.instruction("cmp rax, 3");                                          // is the operand a hash (associative) container?
    emitter.instruction("je __rt_array_iter_next_hash");                        // hashes delegate to the ordered hash iterator

    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // rax = packed length (header word 0)
    emitter.instruction("cmp rsi, rax");                                        // has the cursor walked past the last element?
    emitter.instruction("jge __rt_array_iter_next_done");                       // exhausted packed arrays report the done sentinel
    emitter.instruction("mov r11, QWORD PTR [rdi - 8]");                        // reload the kind word for the value_type field
    emitter.instruction("shr r11, 8");                                          // shift the array-wide value_type into the low bits
    emitter.instruction("and r11, 0x7f");                                       // isolate the value_type, dropping the copy-on-write bit
    emitter.instruction("mov rcx, rsi");                                        // key low word = the integer index
    emitter.instruction("mov rdx, -1");                                         // key high word = -1 marks an integer key
    emitter.instruction("cmp r11, 1");                                          // is the array a string array (16-byte {ptr,len} slots)?
    emitter.instruction("je __rt_array_iter_next_packed_str");                  // strings load a pointer/length pair
    emitter.instruction("cmp r11, 11");                                         // is the array a tagged-scalar array (per-slot tag)?
    emitter.instruction("je __rt_array_iter_next_packed_tagged");               // tagged scalars carry a per-slot runtime tag

    emitter.instruction("mov r8, r11");                                         // value tag = the array-wide value_type
    emitter.instruction("mov r11, rsi");                                        // scratch index for the byte-offset computation
    emitter.instruction("shl r11, 3");                                          // byte offset = index * 8
    emitter.instruction("add r11, rdi");                                        // address of the 8-byte slot within the data region
    emitter.instruction("mov r9, QWORD PTR [r11 + 24]");                        // value low word = data[index] (past the 24-byte header)
    emitter.instruction("xor r10d, r10d");                                      // value high word unused for 8-byte slots
    emitter.instruction("jmp __rt_array_iter_next_packed_ret");                 // return the synthesized packed entry

    emitter.label("__rt_array_iter_next_packed_str");
    emitter.instruction("mov r11, rsi");                                        // scratch index for the byte-offset computation
    emitter.instruction("shl r11, 4");                                          // byte offset = index * 16
    emitter.instruction("add r11, rdi");                                        // address of the string slot base
    emitter.instruction("mov r9, QWORD PTR [r11 + 24]");                        // value low word = string pointer
    emitter.instruction("mov r10, QWORD PTR [r11 + 32]");                       // value high word = string length
    emitter.instruction("mov r8, 1");                                           // value tag = string
    emitter.instruction("jmp __rt_array_iter_next_packed_ret");                 // return the synthesized packed string entry

    emitter.label("__rt_array_iter_next_packed_tagged");
    emitter.instruction("mov r11, rsi");                                        // scratch index for the byte-offset computation
    emitter.instruction("shl r11, 4");                                          // byte offset = index * 16
    emitter.instruction("add r11, rdi");                                        // address of the tagged-scalar slot base
    emitter.instruction("mov r9, QWORD PTR [r11 + 24]");                        // value low word = tagged-scalar payload
    emitter.instruction("mov r8, QWORD PTR [r11 + 32]");                        // value tag = the per-slot runtime tag
    emitter.instruction("xor r10d, r10d");                                      // value high word unused for tagged scalars
    emitter.instruction("jmp __rt_array_iter_next_packed_ret");                 // return the synthesized tagged-scalar entry

    emitter.label("__rt_array_iter_next_packed_ret");
    emitter.instruction("lea rax, [rsi + 1]");                                  // next cursor = index + 1
    emitter.instruction("ret");                                                 // rcx/rdx carry the integer key; r8/r9/r10 the value

    emitter.label("__rt_array_iter_next_done");
    emitter.instruction("mov rax, -1");                                         // report the done sentinel (no more entries)
    emitter.instruction("ret");                                                 // return to the lock-step driver

    emitter.label("__rt_array_iter_next_hash");
    emitter.instruction("sub rsp, 8");                                          // align the stack to 16 bytes for the SysV call
    emitter.instruction("call __rt_hash_iter_next");                            // rdi=hash, rsi=cursor -> rax=next, rdi=key_ptr, rdx=key_len, rcx=lo, r8=hi, r9=tag
    emitter.instruction("add rsp, 8");                                          // restore the stack pointer after the call
    emitter.instruction("mov r10, r8");                                         // value high word from the hash entry
    emitter.instruction("mov r8, r9");                                          // value tag from the hash entry
    emitter.instruction("mov r9, rcx");                                         // value low word from the hash entry
    emitter.instruction("mov rcx, rdi");                                        // key low word = the entry key pointer / int value
    emitter.instruction("ret");                                                 // rax=next cursor, rdx already holds the key high word
}
