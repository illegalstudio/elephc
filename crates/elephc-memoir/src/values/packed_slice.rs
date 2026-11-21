//! Purpose:
//! Allocates and copies a normalized contiguous window into fresh packed array storage.
//!
//! Called from:
//! - The compiler slice helpers after PHP offset and length normalization.
//!
//! Key details:
//! - The caller preserves the source at [sp] or [rbp - 8] before normalizing bounds.
//! - Offset and count enter in x1/x2 on AArch64 and rsi/rdx on x86_64.
//! - String and refcounted slots acquire independent owners through the existing append helpers.
//! - Each kernel leaves the new array in the target result register.

use crate::{Arch, Emitter};

/// Allocates 8-byte slots and copies scalar words from the normalized window.
pub fn emit_copy_scalar(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("str x1, [sp, #16]");                           // save computed offset
            emitter.instruction("str x2, [sp, #24]");                           // save computed slice length

            // -- create new array --
            emitter.instruction("mov x0, x2");                                  // x0 = capacity = slice length
            emitter.instruction("mov x1, #8");                                  // x1 = elem_size = 8 (integers)
            emitter.instruction("bl __rt_array_new");                           // allocate new array
            emitter.instruction("str x0, [sp, #32]");                           // save new array pointer

            // -- copy slice elements --
            emitter.instruction("ldr x1, [sp, #0]");                            // x1 = source array pointer
            emitter.instruction("add x2, x1, #24");                             // x2 = source data base
            emitter.instruction("ldr x3, [sp, #16]");                           // x3 = offset
            emitter.instruction("ldr x4, [sp, #24]");                           // x4 = slice length
            emitter.instruction("add x5, x0, #24");                             // x5 = dest data base
            emitter.instruction("mov x6, #0");                                  // x6 = i = 0

            emitter.label("__rt_array_slice_copy");
            emitter.instruction("cmp x6, x4");                                  // compare i with slice length
            emitter.instruction("b.ge __rt_array_slice_done");                  // if done, finish up
            emitter.instruction("add x7, x3, x6");                              // x7 = offset + i (source index)
            emitter.instruction("ldr x8, [x2, x7, lsl #3]");                    // x8 = source[offset + i]
            emitter.instruction("str x8, [x5, x6, lsl #3]");                    // dest[i] = source[offset + i]
            emitter.instruction("add x6, x6, #1");                              // i += 1
            emitter.instruction("b __rt_array_slice_copy");                     // continue loop

            // -- publish the initialized length --
            emitter.label("__rt_array_slice_done");
            emitter.instruction("ldr x0, [sp, #32]");                           // x0 = new array pointer
            emitter.instruction("ldr x9, [sp, #24]");                           // x9 = slice length
            emitter.instruction("str x9, [x0]");                                // set new array length
        }
        Arch::X86_64 => {
            emitter.instruction("mov QWORD PTR [rbp - 16], rsi");               // preserve the normalized slice offset across the destination indexed-array constructor call
            emitter.instruction("mov QWORD PTR [rbp - 24], rdx");               // preserve the clamped slice length across the destination indexed-array constructor call
            emitter.instruction("mov rdi, rdx");                                // pass the clamped slice length as the destination indexed-array capacity to the shared constructor
            emitter.instruction("mov rsi, 8");                                  // request 8-byte scalar payload slots for the destination indexed array
            emitter.instruction("call __rt_array_new");                         // allocate the destination indexed array through the shared x86_64 constructor
            emitter.instruction("mov QWORD PTR [rbp - 32], rax");               // preserve the destination indexed-array pointer across the scalar slice copy loop
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                // reload the source indexed-array pointer after the constructor clobbered caller-saved registers
            emitter.instruction("lea r10, [r10 + 24]");                         // compute the first scalar payload slot address in the source indexed array
            emitter.instruction("mov r11, QWORD PTR [rbp - 32]");               // reload the destination indexed-array pointer before seeding the slice-copy loop
            emitter.instruction("lea r11, [r11 + 24]");                         // compute the first scalar payload slot address in the destination indexed array
            emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                // reload the normalized slice offset before seeding the source index for the copy loop
            emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                // reload the clamped slice length before testing whether there is any scalar payload to copy
            emitter.instruction("xor ecx, ecx");                                // initialize the destination slice index to the first scalar payload slot in the destination indexed array

            emitter.label("__rt_array_slice_copy_x86");
            emitter.instruction("cmp rcx, r9");                                 // compare the destination slice index against the clamped slice length
            emitter.instruction("jge __rt_array_slice_done_x86");               // finish once every requested scalar payload has been copied into the destination indexed array
            emitter.instruction("mov rax, QWORD PTR [r10 + r8 * 8]");           // load the current scalar payload from the normalized source slice position
            emitter.instruction("mov QWORD PTR [r11 + rcx * 8], rax");          // store that scalar payload into the next destination indexed-array slot
            emitter.instruction("add r8, 1");                                   // advance the normalized source slice index to the next scalar payload slot
            emitter.instruction("add rcx, 1");                                  // advance the destination slice index after copying one scalar payload
            emitter.instruction("jmp __rt_array_slice_copy_x86");               // continue copying until the destination indexed array holds the full scalar slice

            emitter.label("__rt_array_slice_done_x86");
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // reload the destination indexed-array pointer before publishing its logical length
            emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                // reload the clamped slice length so the destination indexed-array header can report the copied payload count
            emitter.instruction("mov QWORD PTR [rax], r9");                     // store the clamped slice length as the destination indexed-array logical length
        }
    }
}

/// Allocates 16-byte slots and persists each borrowed string from the normalized window.
pub fn emit_copy_strings(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("str x1, [sp, #16]");                           // save normalized offset
            emitter.instruction("str x2, [sp, #24]");                           // save normalized slice length

            // -- create destination array with 16-byte string slots --
            emitter.instruction("mov x0, x2");                                  // move slice length into destination capacity
            emitter.instruction("mov x1, #16");                                 // request 16-byte slots for {pointer, length} pairs
            emitter.instruction("bl __rt_array_new");                           // allocate destination array
            emitter.instruction("str x0, [sp, #32]");                           // save destination array pointer

            // -- copy the requested range, duplicating each string --
            //
            // The index is SPILLED rather than parked in a register: `__rt_array_push_str` persists the
            // payload and may grow the destination, so it is a full call and every caller-saved register
            // is fair game across it.
            emitter.instruction("mov x6, #0");                                  // initialize loop index
            emitter.instruction("str x6, [sp, #40]");                           // seed the spilled loop index
            emitter.label("__rt_array_slice_str_loop");
            emitter.instruction("ldr x6, [sp, #40]");                           // reload the loop index for this iteration
            emitter.instruction("ldr x4, [sp, #24]");                           // reload slice length
            emitter.instruction("cmp x6, x4");                                  // compare loop index with slice length
            emitter.instruction("b.ge __rt_array_slice_str_done");              // finish after copying every requested element
            emitter.instruction("ldr x1, [sp, #0]");                            // reload source array pointer
            emitter.instruction("add x2, x1, #24");                             // compute source data base past the 24-byte header
            emitter.instruction("ldr x3, [sp, #16]");                           // reload normalized offset
            emitter.instruction("add x7, x3, x6");                              // compute source index = offset + loop index
            emitter.instruction("lsl x7, x7, #4");                              // scale the source index by the 16-byte string slot
            emitter.instruction("add x7, x2, x7");                              // compute the address of the source {pointer, length} pair
            emitter.instruction("ldp x1, x2, [x7]");                            // load the borrowed source pointer and length together
            emitter.instruction("ldr x0, [sp, #32]");                           // reload destination array pointer
            emitter.instruction("bl __rt_array_push_str");                      // append a PERSISTED copy of the pair into the destination
            emitter.instruction("str x0, [sp, #32]");                           // persist destination pointer after possible growth
            emitter.instruction("ldr x6, [sp, #40]");                           // reload loop index across the append helper call
            emitter.instruction("add x6, x6, #1");                              // increment loop index
            emitter.instruction("str x6, [sp, #40]");                           // save the updated loop index for the next iteration
            emitter.instruction("b __rt_array_slice_str_loop");                 // continue copying

            emitter.label("__rt_array_slice_str_done");
            emitter.instruction("ldr x0, [sp, #32]");                           // reload destination array pointer
        }
        Arch::X86_64 => {
            emitter.instruction("mov QWORD PTR [rbp - 16], rsi");               // preserve the normalized slice offset across the destination constructor call
            emitter.instruction("mov QWORD PTR [rbp - 24], rdx");               // preserve the clamped slice length across the destination constructor call
            emitter.instruction("mov rdi, rdx");                                // pass the clamped slice length as the destination capacity
            emitter.instruction("mov rsi, 16");                                 // request 16-byte payload slots for the destination string array
            emitter.instruction("call __rt_array_new");                         // allocate the destination indexed array
            emitter.instruction("mov QWORD PTR [rbp - 32], rax");               // preserve the destination pointer across the string append helper calls
            emitter.instruction("mov QWORD PTR [rbp - 40], 0");                 // initialize the slice-copy loop index
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload the normalized offset before seeding the source cursor
            emitter.instruction("mov QWORD PTR [rbp - 48], r10");               // preserve the current source slice cursor across append helper calls

            emitter.label("__rt_array_slice_str_copy_x86");
            emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");               // reload the destination slice index before the bound test
            emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");               // compare the destination index against the clamped slice length
            emitter.instruction("jge __rt_array_slice_str_done_x86");           // finish once every requested string pair has been copied
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                // reload the source indexed-array pointer
            emitter.instruction("lea r10, [r10 + 24]");                         // compute the payload base past the 24-byte header
            emitter.instruction("mov r11, QWORD PTR [rbp - 48]");               // reload the current source slice cursor
            emitter.instruction("shl r11, 4");                                  // scale the source cursor by the 16-byte string slot
            emitter.instruction("add r10, r11");                                // compute the address of the source {pointer, length} pair
            emitter.instruction("mov rsi, QWORD PTR [r10]");                    // load the borrowed source string pointer
            emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                // load the borrowed source string length
            emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");               // reload the destination indexed-array pointer
            emitter.instruction("call __rt_array_push_str");                    // append a PERSISTED copy of the pair into the destination
            emitter.instruction("mov QWORD PTR [rbp - 32], rax");               // persist the possibly-grown destination pointer
            emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");               // reload the destination index after the call clobbered caller-saved registers
            emitter.instruction("add rcx, 1");                                  // advance the destination index
            emitter.instruction("mov QWORD PTR [rbp - 40], rcx");               // persist the updated destination index
            emitter.instruction("mov r11, QWORD PTR [rbp - 48]");               // reload the source cursor after the call
            emitter.instruction("add r11, 1");                                  // advance the source cursor to the next string pair
            emitter.instruction("mov QWORD PTR [rbp - 48], r11");               // persist the updated source cursor
            emitter.instruction("jmp __rt_array_slice_str_copy_x86");           // continue copying until the window is exhausted

            emitter.label("__rt_array_slice_str_done_x86");
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // return the destination pointer in the standard result register
        }
    }
}

/// Allocates 8-byte slots and retains refcounted children from the normalized window.
pub fn emit_copy_refcounted(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("str x1, [sp, #16]");                           // save normalized offset
            emitter.instruction("str x2, [sp, #24]");                           // save normalized slice length

            // -- create destination array --
            emitter.instruction("mov x0, x2");                                  // move slice length into destination capacity
            emitter.instruction("mov x1, #8");                                  // use 8-byte slots for heap pointers
            emitter.instruction("bl __rt_array_new");                           // allocate destination array
            emitter.instruction("str x0, [sp, #32]");                           // save destination array pointer

            // -- copy the requested range with retains --
            emitter.instruction("mov x6, #0");                                  // initialize loop index
            emitter.label("__rt_array_slice_ref_loop");
            emitter.instruction("ldr x4, [sp, #24]");                           // reload slice length
            emitter.instruction("cmp x6, x4");                                  // compare loop index with slice length
            emitter.instruction("b.ge __rt_array_slice_ref_done");              // finish after copying every requested element
            emitter.instruction("ldr x1, [sp, #0]");                            // reload source array pointer
            emitter.instruction("add x2, x1, #24");                             // compute source data base
            emitter.instruction("ldr x3, [sp, #16]");                           // reload normalized offset
            emitter.instruction("add x7, x3, x6");                              // compute source index = offset + loop index
            emitter.instruction("ldr x1, [x2, x7, lsl #3]");                    // load borrowed source payload
            emitter.instruction("ldr x0, [sp, #32]");                           // reload destination array pointer
            emitter.instruction("bl __rt_array_push_refcounted");               // append retained payload into destination array
            emitter.instruction("str x0, [sp, #32]");                           // persist destination pointer after possible growth
            emitter.instruction("add x6, x6, #1");                              // increment loop index
            emitter.instruction("b __rt_array_slice_ref_loop");                 // continue copying

            emitter.label("__rt_array_slice_ref_done");
            emitter.instruction("ldr x0, [sp, #32]");                           // reload destination array pointer
        }
        Arch::X86_64 => {
            emitter.instruction("mov QWORD PTR [rbp - 16], rsi");               // preserve the normalized slice offset across the destination indexed-array constructor call
            emitter.instruction("mov QWORD PTR [rbp - 24], rdx");               // preserve the clamped slice length across the destination indexed-array constructor call
            emitter.instruction("mov rdi, rdx");                                // pass the clamped slice length as the destination indexed-array capacity to the shared constructor
            emitter.instruction("mov rsi, 8");                                  // request 8-byte payload slots for the destination refcounted indexed array
            emitter.instruction("call __rt_array_new");                         // allocate the destination indexed array through the shared x86_64 constructor
            emitter.instruction("mov QWORD PTR [rbp - 32], rax");               // preserve the destination indexed-array pointer across the refcounted append helper calls
            emitter.instruction("mov QWORD PTR [rbp - 40], 0");                 // initialize the slice-copy loop index to the first destination payload slot
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload the normalized source slice offset before seeding the source payload cursor
            emitter.instruction("mov QWORD PTR [rbp - 48], r10");               // preserve the current source slice cursor across refcounted append helper calls

            emitter.label("__rt_array_slice_ref_copy_x86");
            emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");               // reload the destination slice index before testing whether every requested payload has been copied
            emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");               // compare the destination slice index against the clamped slice length
            emitter.instruction("jge __rt_array_slice_ref_done_x86");           // finish once every requested refcounted payload has been copied into the destination indexed array
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                // reload the source indexed-array pointer before reading the next refcounted slice payload
            emitter.instruction("lea r10, [r10 + 24]");                         // compute the payload base address for the source indexed array
            emitter.instruction("mov r11, QWORD PTR [rbp - 48]");               // reload the current source slice cursor before reading the next refcounted payload
            emitter.instruction("mov rsi, QWORD PTR [r10 + r11 * 8]");          // load the next borrowed refcounted payload from the source slice window
            emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");               // reload the destination indexed-array pointer before appending the retained slice payload
            emitter.instruction("call __rt_array_push_refcounted");             // append the retained slice payload into the destination indexed array
            emitter.instruction("mov QWORD PTR [rbp - 32], rax");               // persist the possibly-grown destination indexed-array pointer after the append helper returns
            emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");               // reload the destination slice index after helper calls clobbered caller-saved registers
            emitter.instruction("add rcx, 1");                                  // advance the destination slice index after copying one refcounted payload
            emitter.instruction("mov QWORD PTR [rbp - 40], rcx");               // persist the updated destination slice index across the next append helper call
            emitter.instruction("mov r11, QWORD PTR [rbp - 48]");               // reload the source slice cursor after helper calls clobbered caller-saved registers
            emitter.instruction("add r11, 1");                                  // advance the source slice cursor to the next refcounted payload in the requested slice window
            emitter.instruction("mov QWORD PTR [rbp - 48], r11");               // persist the updated source slice cursor across the next append helper call
            emitter.instruction("jmp __rt_array_slice_ref_copy_x86");           // continue copying refcounted slice payloads until the requested slice window is exhausted

            emitter.label("__rt_array_slice_ref_done_x86");
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // return the destination indexed-array pointer in the standard x86_64 integer result register
        }
    }
}
