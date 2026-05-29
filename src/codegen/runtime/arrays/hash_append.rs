//! Purpose:
//! Emits the `__rt_hash_append` runtime helper assembly for associative array append.
//! Computes PHP's next automatic integer key for hash-backed arrays before insertion.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Hash append must preserve insertion order and handle negative integer keys using PHP 8.3+ semantics.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_hash_append` for the active target.
///
/// The helper scans occupied hash entries, finds the largest integer key, and appends
/// at `largest + 1`. If the hash has no integer keys, it appends at key `0`.
/// It delegates the actual insert, growth, COW split, and ownership transfer to
/// `__rt_hash_set`.
pub fn emit_hash_append(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_append_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_append ---");
    emitter.label_global("__rt_hash_append");

    // -- set up stack frame and preserve append payload --
    // Stack layout:
    //   [sp, #0]  = hash_table_ptr
    //   [sp, #8]  = value_lo
    //   [sp, #16] = value_hi
    //   [sp, #24] = value_tag
    //   [sp, #64] = saved x29
    //   [sp, #72] = saved x30
    emitter.instruction("sub sp, sp, #80");                                     // reserve spill space for the table and payload tuple
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller frame while scanning entries
    emitter.instruction("add x29, sp, #64");                                    // establish a stable frame pointer for this helper
    emitter.instruction("str x0, [sp, #0]");                                    // save the hash table pointer for the final insertion call
    emitter.instruction("str x1, [sp, #8]");                                    // save the low payload word across the key scan
    emitter.instruction("str x2, [sp, #16]");                                   // save the high payload word across the key scan
    emitter.instruction("str x3, [sp, #24]");                                   // save the runtime value tag across the key scan

    // -- scan occupied entries for the largest integer key --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload the hash table pointer as the scan base
    emitter.instruction("ldr x6, [x5, #8]");                                    // load the hash table capacity for the scan bound
    emitter.instruction("mov x7, #0");                                          // initialize the entry slot cursor
    emitter.instruction("mov x8, #0");                                          // initialize the maximum integer key placeholder
    emitter.instruction("mov x9, #0");                                          // track whether any integer key has been observed

    emitter.label("__rt_hash_append_scan");
    emitter.instruction("cmp x7, x6");                                          // has every hash slot been inspected?
    emitter.instruction("b.ge __rt_hash_append_key_ready");                     // finish scanning once the cursor reaches capacity
    emitter.instruction("mov x10, #64");                                        // x10 = hash entry size in bytes
    emitter.instruction("mul x11, x7, x10");                                    // convert the slot cursor into a byte offset
    emitter.instruction("add x11, x5, x11");                                    // advance from the hash base to the selected slot
    emitter.instruction("add x11, x11, #40");                                   // skip the fixed hash header to reach the entry fields
    emitter.instruction("ldr x12, [x11]");                                      // load the occupied marker for this slot
    emitter.instruction("cmp x12, #1");                                         // is this slot a live entry?
    emitter.instruction("b.ne __rt_hash_append_next");                          // ignore empty or tombstone slots while deriving the next key
    emitter.instruction("ldr x12, [x11, #16]");                                 // load the normalized key length or integer sentinel
    emitter.instruction("cmn x12, #1");                                         // check whether the key length is the integer-key sentinel
    emitter.instruction("b.ne __rt_hash_append_next");                          // string keys do not affect PHP's next automatic integer key
    emitter.instruction("ldr x13, [x11, #8]");                                  // load the stored integer key payload
    emitter.instruction("cbz x9, __rt_hash_append_take_key");                   // the first integer key seeds the maximum key tracker
    emitter.instruction("cmp x13, x8");                                         // compare the candidate key against the current maximum
    emitter.instruction("b.le __rt_hash_append_next");                          // keep scanning if the candidate key is not larger

    emitter.label("__rt_hash_append_take_key");
    emitter.instruction("mov x8, x13");                                         // record the largest integer key seen so far
    emitter.instruction("mov x9, #1");                                          // remember that at least one integer key exists

    emitter.label("__rt_hash_append_next");
    emitter.instruction("add x7, x7, #1");                                      // advance to the next hash slot
    emitter.instruction("b __rt_hash_append_scan");                             // continue scanning for integer keys

    // -- materialize the append key and delegate insertion --
    emitter.label("__rt_hash_append_key_ready");
    emitter.instruction("cbz x9, __rt_hash_append_no_int_keys");                // hashes with no integer keys append at key zero
    emitter.instruction("add x1, x8, #1");                                      // append after the largest observed integer key
    emitter.instruction("b __rt_hash_append_call_set");                         // use the computed key for insertion

    emitter.label("__rt_hash_append_no_int_keys");
    emitter.instruction("mov x1, #0");                                          // first automatic integer key is zero

    emitter.label("__rt_hash_append_call_set");
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the hash table pointer for hash_set
    emitter.instruction("mov x2, #-1");                                         // mark the synthesized key as an integer key
    emitter.instruction("ldr x3, [sp, #8]");                                    // restore the low payload word for hash_set
    emitter.instruction("ldr x4, [sp, #16]");                                   // restore the high payload word for hash_set
    emitter.instruction("ldr x5, [sp, #24]");                                   // restore the runtime value tag for hash_set
    emitter.instruction("bl __rt_hash_set");                                    // insert the appended element and return the possibly-grown table

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame after hash_set returns
    emitter.instruction("add sp, sp, #80");                                     // release the helper spill frame
    emitter.instruction("ret");                                                 // return the updated hash table pointer
}

/// Emits the x86_64 Linux implementation of `__rt_hash_append`.
///
/// Uses the System V ABI: `rdi` is the hash table pointer, `rsi`/`rdx` are the
/// payload words, and `rcx` is the runtime value tag. Returns the possibly-grown
/// hash table pointer in `rax`.
fn emit_hash_append_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_append ---");
    emitter.label_global("__rt_hash_append");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving append spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved table and payload tuple
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill space while keeping nested calls ABI-aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hash table pointer for the final insertion call
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the low payload word across the key scan
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the high payload word across the key scan
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the runtime value tag across the key scan
    emitter.instruction("mov r10, rdi");                                        // use r10 as the immutable hash table scan base
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load the hash table capacity as the scan bound
    emitter.instruction("xor r8d, r8d");                                        // initialize the entry slot cursor
    emitter.instruction("xor r9d, r9d");                                        // track whether any integer key has been observed
    emitter.instruction("xor eax, eax");                                        // initialize the maximum integer key placeholder

    emitter.label("__rt_hash_append_scan");
    emitter.instruction("cmp r8, r11");                                         // has every hash slot been inspected?
    emitter.instruction("jge __rt_hash_append_key_ready");                      // finish scanning once the cursor reaches capacity
    emitter.instruction("mov rcx, r8");                                         // copy the slot cursor before scaling it into a byte offset
    emitter.instruction("shl rcx, 6");                                          // convert the slot cursor into a 64-byte hash-entry offset
    emitter.instruction("add rcx, r10");                                        // advance from the hash base to the selected entry block
    emitter.instruction("add rcx, 40");                                         // skip the fixed hash header to reach the entry fields
    emitter.instruction("cmp QWORD PTR [rcx], 1");                              // is this slot a live entry?
    emitter.instruction("jne __rt_hash_append_next");                           // ignore empty or tombstone slots while deriving the next key
    emitter.instruction("cmp QWORD PTR [rcx + 16], -1");                        // is the normalized key an integer key?
    emitter.instruction("jne __rt_hash_append_next");                           // string keys do not affect PHP's next automatic integer key
    emitter.instruction("mov rdx, QWORD PTR [rcx + 8]");                        // load the stored integer key payload
    emitter.instruction("test r9, r9");                                         // has any integer key seeded the maximum tracker?
    emitter.instruction("je __rt_hash_append_take_key");                        // the first integer key becomes the current maximum
    emitter.instruction("cmp rdx, rax");                                        // compare the candidate key against the current maximum
    emitter.instruction("jle __rt_hash_append_next");                           // keep scanning if the candidate key is not larger

    emitter.label("__rt_hash_append_take_key");
    emitter.instruction("mov rax, rdx");                                        // record the largest integer key seen so far
    emitter.instruction("mov r9, 1");                                           // remember that at least one integer key exists

    emitter.label("__rt_hash_append_next");
    emitter.instruction("add r8, 1");                                           // advance to the next hash slot
    emitter.instruction("jmp __rt_hash_append_scan");                           // continue scanning for integer keys

    emitter.label("__rt_hash_append_key_ready");
    emitter.instruction("test r9, r9");                                         // did the scan observe any integer keys?
    emitter.instruction("je __rt_hash_append_no_int_keys");                     // hashes with no integer keys append at key zero
    emitter.instruction("add rax, 1");                                          // append after the largest observed integer key
    emitter.instruction("jmp __rt_hash_append_call_set");                       // use the computed key for insertion

    emitter.label("__rt_hash_append_no_int_keys");
    emitter.instruction("xor eax, eax");                                        // first automatic integer key is zero

    emitter.label("__rt_hash_append_call_set");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the hash table pointer for hash_set
    emitter.instruction("mov rsi, rax");                                        // pass the synthesized integer key as hash_set key payload
    emitter.instruction("mov rdx, -1");                                         // mark the synthesized key as an integer key
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // restore the low payload word for hash_set
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // restore the high payload word for hash_set
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // restore the runtime value tag for hash_set
    emitter.instruction("call __rt_hash_set");                                  // insert the appended element and return the possibly-grown table
    emitter.instruction("add rsp, 64");                                         // release the helper spill frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the updated hash table pointer
}
