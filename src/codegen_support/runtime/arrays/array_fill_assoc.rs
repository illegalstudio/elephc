//! Purpose:
//! Emits the `__rt_array_fill_assoc` runtime helper that builds a keyed (hash) array for
//! `array_fill(start, count, value)` when the result needs explicit integer keys.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Used when the start index is non-zero (keys `start..start+count-1`, which a 0-based
//!   indexed array cannot represent) or when the fill value is a string (the scalar indexed
//!   path cannot store a pointer+length). Each slot is boxed independently via
//!   `__rt_mixed_from_value` (persists strings, increfs refcounted children), so per-slot
//!   PHP copy/share semantics are correct; the hash stores Mixed-tagged values.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_array_fill_assoc`: build a Mixed-valued hash with keys `start..start+count-1`
/// all set to `value`.
///
/// Input  (ARM64):  x0 = start index, x1 = count, x2 = value_lo, x3 = value_hi, x4 = value_tag
/// Input  (x86_64): rdi = start index, rsi = count, rdx = value_lo, rcx = value_hi, r8 = value_tag
/// Output: pointer to the new hash table (x0 / rax).
pub fn emit_array_fill_assoc(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_fill_assoc_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_fill_assoc ---");
    emitter.label_global("__rt_array_fill_assoc");

    // -- stack frame: [sp,#0]=start [8]=count [16]=value_lo [24]=value_hi [32]=value_tag [40]=hash [48]=i --
    emitter.instruction("sub sp, sp, #80");                                     // allocate the helper frame for the saved fill arguments and loop bookkeeping
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address across nested helper calls
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the start index
    emitter.instruction("str x1, [sp, #8]");                                    // save the element count
    emitter.instruction("str x2, [sp, #16]");                                   // save the fill value low word
    emitter.instruction("str x3, [sp, #24]");                                   // save the fill value high word
    emitter.instruction("str x4, [sp, #32]");                                   // save the fill value runtime tag

    // -- create a Mixed-valued hash sized to the element count --
    emitter.instruction("mov x0, x1");                                          // hash capacity = element count
    emitter.instruction("mov x1, #7");                                          // value_type_tag 7 = Mixed (each slot stores a boxed value)
    emitter.instruction("bl __rt_hash_new");                                    // allocate the destination hash table
    emitter.instruction("str x0, [sp, #40]");                                   // save the hash table pointer
    emitter.instruction("mov x9, #0");                                          // loop index i = 0
    emitter.instruction("str x9, [sp, #48]");                                   // save the loop index

    emitter.label("__rt_array_fill_assoc_loop");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the loop index
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the element count
    emitter.instruction("cmp x9, x10");                                         // have all elements been inserted?
    emitter.instruction("b.ge __rt_array_fill_assoc_done");                     // stop once every key has been filled

    // -- box the value for this slot (persists strings, increfs refcounted children) --
    emitter.instruction("ldr x0, [sp, #32]");                                   // value runtime tag
    emitter.instruction("ldr x1, [sp, #16]");                                   // value low word
    emitter.instruction("ldr x2, [sp, #24]");                                   // value high word
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = freshly owned boxed Mixed cell for this slot

    // -- insert key = start + i with the boxed Mixed value --
    emitter.instruction("mov x3, x0");                                          // value_lo = boxed Mixed pointer
    emitter.instruction("ldr x0, [sp, #40]");                                   // hash table pointer
    emitter.instruction("ldr x11, [sp, #0]");                                   // start index
    emitter.instruction("ldr x9, [sp, #48]");                                   // loop index
    emitter.instruction("add x1, x11, x9");                                     // key_lo = start + i
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("mov x4, #0");                                          // value_hi = 0 (the boxed pointer uses only the low word)
    emitter.instruction("mov x5, #7");                                          // value_tag 7 = Mixed
    emitter.instruction("bl __rt_hash_set");                                    // insert; x0 = hash table pointer (may have been reallocated)
    emitter.instruction("str x0, [sp, #40]");                                   // save the possibly-reallocated hash table pointer

    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the loop index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next key
    emitter.instruction("str x9, [sp, #48]");                                   // save the advanced loop index
    emitter.instruction("b __rt_array_fill_assoc_loop");                        // continue filling

    emitter.label("__rt_array_fill_assoc_done");
    emitter.instruction("ldr x0, [sp, #40]");                                   // return the filled hash table pointer
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return with x0 = filled hash table
}

/// x86_64 Linux variant of `emit_array_fill_assoc` using System V ABI register conventions.
fn emit_array_fill_assoc_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_fill_assoc ---");
    emitter.label_global("__rt_array_fill_assoc");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 64");                                         // reserve aligned slots for the saved arguments and loop bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the start index
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the element count
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the fill value low word
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the fill value high word
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the fill value runtime tag

    emitter.instruction("mov rdi, rsi");                                        // hash capacity = element count
    emitter.instruction("mov rsi, 7");                                          // value_type_tag 7 = Mixed (each slot stores a boxed value)
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination hash table
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the hash table pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // loop index i = 0

    emitter.label("__rt_array_fill_assoc_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the loop index
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // have all elements been inserted?
    emitter.instruction("jge __rt_array_fill_assoc_done_x86");                  // stop once every key has been filled

    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // value runtime tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // value low word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // value high word
    emitter.instruction("call __rt_mixed_from_value");                          // rax = freshly owned boxed Mixed cell for this slot

    emitter.instruction("mov rcx, rax");                                        // value_lo = boxed Mixed pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // hash table pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // start index
    emitter.instruction("add rsi, QWORD PTR [rbp - 56]");                       // key_lo = start + i
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("mov r8, 0");                                           // value_hi = 0 (the boxed pointer uses only the low word)
    emitter.instruction("mov r9, 7");                                           // value_tag 7 = Mixed
    emitter.instruction("call __rt_hash_set");                                  // insert; rax = hash table pointer (may have been reallocated)
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the possibly-reallocated hash table pointer

    emitter.instruction("inc QWORD PTR [rbp - 56]");                            // advance to the next key
    emitter.instruction("jmp __rt_array_fill_assoc_loop_x86");                  // continue filling

    emitter.label("__rt_array_fill_assoc_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return the filled hash table pointer
    emitter.instruction("add rsp, 64");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with rax = filled hash table
}
