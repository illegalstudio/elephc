//! Purpose:
//! Reads owned eval array values by insertion-order position without normalizing their keys.
//!
//! Called from:
//! - RuntimeValueOps::array_iter_value for exact associative iteration and JSON serialization.
//!
//! Key details:
//! - Integer key 1 and string key "1" remain distinct during traversal.
//! - Returned cells own their payload and never expose mutable source element boxes.

use super::*;
use crate::codegen_support::sentinels::emit_branch_if_null_container;

/// Emits positional value reads using the selected target's native iterator and boxing conventions.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter); } else { x86_64(emitter); }
}
/// Reads one owned aarch64 element using exact iteration position and concrete runtime tags.
fn aarch64(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_array_iter_value");
    emitter.instruction("sub sp, sp, #48");                                     // allocate a wrapper frame for insertion-order value iteration
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable iterator-value frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed array receiver while walking the container
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested zero-based foreach position
    emitter.instruction("bl __rt_mixed_deref");                                 // iterate the current array inside a reference wrapper
    emitter.instruction("str x0, [sp, #0]");                                    // save the concrete receiver before inspecting its tag
    emitter.instruction("cbz x0, __elephc_eval_value_array_iter_value_null");   // null handles produce a null value
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed Mixed runtime tag
    emitter.instruction("cmp x9, #4");                                          // tag 4 = indexed array
    emitter.instruction("b.eq __elephc_eval_value_array_iter_value_indexed");   // indexed arrays expose integer positions as foreach values
    emitter.instruction("cmp x9, #5");                                          // tag 5 = associative array
    emitter.instruction("b.eq __elephc_eval_value_array_iter_value_assoc");     // associative arrays expose insertion-order hash values
    emitter.instruction("b __elephc_eval_value_array_iter_value_null");         // scalar values have no foreach-visible value
    emitter.label("__elephc_eval_value_array_iter_value_indexed");
    emitter.instruction("ldr x0, [sp]");                                        // borrow the original indexed array box
    emitter.instruction("ldr x1, [sp, #8]");                                    // use the iteration position as the integer offset
    emitter.instruction("mov x2, #-1");                                         // select the normalized integer-key convention
    emitter.instruction("bl __rt_mixed_array_get");                             // acquire an independent indexed element value
    emitter.instruction("b __elephc_eval_value_array_iter_value_done");         // restore linkage with the owned element
    emitter.label("__elephc_eval_value_array_iter_value_assoc");
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the hash payload pointer from the Mixed cell
    emit_branch_if_null_container(
        emitter,
        "x9",
        "x10",
        "__elephc_eval_value_array_iter_value_null",
    );
    emitter.instruction("str x9, [sp, #16]");                                   // save the hash pointer for repeated iterator helper calls
    emitter.instruction("str xzr, [sp, #24]");                                  // start the insertion-order position counter at zero
    emitter.instruction("mov x1, xzr");                                         // cursor 0 starts at the hash head entry
    emitter.label("__elephc_eval_value_array_iter_value_assoc_loop");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the hash pointer before advancing the hash iterator
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next insertion-order hash value
    emitter.instruction("cmn x0, #1");                                          // did the iterator report the done sentinel?
    emitter.instruction("b.eq __elephc_eval_value_array_iter_value_null");      // out-of-range positions produce a null value
    emitter.instruction("ldr x10, [sp, #24]");                                  // load the current insertion-order position
    emitter.instruction("ldr x11, [sp, #8]");                                   // load the requested foreach position
    emitter.instruction("cmp x10, x11");                                        // is this the requested hash entry?
    emitter.instruction("b.eq __elephc_eval_value_array_iter_value_assoc_box"); // box the current hash value when the position matches
    emitter.instruction("add x10, x10, #1");                                    // advance the insertion-order position counter
    emitter.instruction("str x10, [sp, #24]");                                  // persist the updated position counter for the next probe
    emitter.instruction("mov x1, x0");                                          // use the returned cursor for the next hash iterator call
    emitter.instruction("b __elephc_eval_value_array_iter_value_assoc_loop");   // continue walking until the requested position is reached
    emitter.label("__elephc_eval_value_array_iter_value_assoc_box");
    emitter.instruction("cmp x5, #7");                                          // recognize an already boxed Mixed entry
    emitter.instruction("b.eq __elephc_eval_value_array_iter_value_clone");     // detach existing boxes before exposing their values
    emitter.instruction("mov x0, x5");                                          // select the concrete per-entry tag
    emitter.instruction("mov x1, x3");                                          // borrow the iterator value low payload
    emitter.instruction("mov x2, x4");                                          // borrow the iterator value high payload
    emitter.instruction("bl __rt_mixed_from_value");                            // copy strings or retain nested arrays independently
    emitter.instruction("b __elephc_eval_value_array_iter_value_done");         // return the acquired concrete value
    emitter.label("__elephc_eval_value_array_iter_value_clone");
    emitter.instruction("mov x0, x3");                                          // borrow the entry's boxed Mixed payload
    emitter.instruction("bl __rt_mixed_clone");                                 // create an independent value while retaining resource identity
    emitter.instruction("b __elephc_eval_value_array_iter_value_done");         // return the detached element owner
    emitter.label("__elephc_eval_value_array_iter_value_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, xzr");                                         // null values do not use a low payload word
    emitter.instruction("mov x2, xzr");                                         // null values do not use a high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box null for invalid foreach-value requests
    emitter.label("__elephc_eval_value_array_iter_value_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the iterator-value wrapper frame
    emitter.instruction("ret");                                                 // return the boxed foreach value to Rust

}

/// Reads one owned x86_64 element using exact iteration position and concrete runtime tags.
fn x86_64(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_array_iter_value");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable iterator-value wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve slots for receiver, target position, hash pointer, and counter
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the boxed array receiver while walking the container
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested zero-based foreach position
    emitter.instruction("mov rax, rdi");                                        // pass the possibly referenced receiver
    emitter.instruction("call __rt_mixed_deref");                               // iterate the current array inside a reference wrapper
    emitter.instruction("mov rdi, rax");                                        // restore the concrete receiver argument
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the concrete receiver for positional lookup
    emitter.instruction("test rdi, rdi");                                       // null handles produce a null value
    emitter.instruction("jz __elephc_eval_value_array_iter_value_null");        // branch to boxed null for null runtime cells
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed Mixed runtime tag
    emitter.instruction("cmp r10, 4");                                          // tag 4 = indexed array
    emitter.instruction("je __elephc_eval_value_array_iter_value_indexed");     // indexed arrays expose integer positions as foreach values
    emitter.instruction("cmp r10, 5");                                          // tag 5 = associative array
    emitter.instruction("je __elephc_eval_value_array_iter_value_assoc");       // associative arrays expose insertion-order hash values
    emitter.instruction("jmp __elephc_eval_value_array_iter_value_null");       // scalar values have no foreach-visible value
    emitter.label("__elephc_eval_value_array_iter_value_indexed");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // borrow the original indexed array box
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // use the iteration position as the integer offset
    emitter.instruction("mov rdx, -1");                                         // select the normalized integer-key convention
    emitter.instruction("call __rt_mixed_array_get");                           // acquire an independent indexed element value
    emitter.instruction("jmp __elephc_eval_value_array_iter_value_done");       // restore linkage with the owned element
    emitter.label("__elephc_eval_value_array_iter_value_assoc");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the hash payload pointer from the Mixed cell
    emit_branch_if_null_container(
        emitter,
        "r10",
        "r11",
        "__elephc_eval_value_array_iter_value_null",
    );
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the hash pointer for repeated iterator helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the insertion-order position counter at zero
    emitter.instruction("xor esi, esi");                                        // cursor 0 starts at the hash head entry
    emitter.label("__elephc_eval_value_array_iter_value_assoc_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the hash pointer before advancing the hash iterator
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next insertion-order hash value
    emitter.instruction("cmp rax, -1");                                         // did the iterator report the done sentinel?
    emitter.instruction("je __elephc_eval_value_array_iter_value_null");        // out-of-range positions produce a null value
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // load the current insertion-order position
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // load the requested foreach position
    emitter.instruction("cmp r10, r11");                                        // is this the requested hash entry?
    emitter.instruction("je __elephc_eval_value_array_iter_value_assoc_box");   // box the current hash value when the position matches
    emitter.instruction("add r10, 1");                                          // advance the insertion-order position counter
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the updated position counter for the next probe
    emitter.instruction("mov rsi, rax");                                        // use the returned cursor for the next hash iterator call
    emitter.instruction("jmp __elephc_eval_value_array_iter_value_assoc_loop"); // continue walking until the requested position is reached
    emitter.label("__elephc_eval_value_array_iter_value_assoc_box");
    emitter.instruction("cmp r9, 7");                                           // recognize an already boxed Mixed entry
    emitter.instruction("je __elephc_eval_value_array_iter_value_clone");       // detach existing boxes before exposing their values
    emitter.instruction("mov rax, r9");                                         // select the concrete per-entry tag
    emitter.instruction("mov rdi, rcx");                                        // borrow the iterator value low payload
    emitter.instruction("mov rsi, r8");                                         // borrow the iterator value high payload
    emitter.instruction("call __rt_mixed_from_value");                          // copy strings or retain nested arrays independently
    emitter.instruction("jmp __elephc_eval_value_array_iter_value_done");       // return the acquired concrete value
    emitter.label("__elephc_eval_value_array_iter_value_clone");
    emitter.instruction("mov rax, rcx");                                        // borrow the entry's boxed Mixed payload
    emitter.instruction("call __rt_mixed_clone");                               // create an independent value while retaining resource identity
    emitter.instruction("jmp __elephc_eval_value_array_iter_value_done");       // return the detached element owner
    emitter.label("__elephc_eval_value_array_iter_value_null");
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null values do not use a low payload word
    emitter.instruction("xor esi, esi");                                        // null values do not use a high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box null for invalid foreach-value requests
    emitter.label("__elephc_eval_value_array_iter_value_done");
    emitter.instruction("add rsp, 32");                                         // release the iterator-value wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed foreach value to Rust

}
