//! Purpose:
//! Emits the `__rt_array_to_mixed` runtime helper for indexed arrays that widen
//! from typed slots to boxed Mixed slots.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Conversion transfers existing slot ownership into Mixed boxes and stamps the
//!   indexed array with value_type tag 7.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// array_to_mixed: convert an indexed array payload to boxed Mixed cells.
/// Input:  x0=array pointer, x1=current value_type tag
/// Output: x0=unique array pointer with value_type tag 7 and 8-byte slots
pub fn emit_array_to_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_to_mixed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_to_mixed ---");
    emitter.label_global("__rt_array_to_mixed");

    emitter.instruction("sub sp, sp, #80");                                     // reserve conversion frame slots and saved return state
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish a stable conversion frame
    emitter.instruction("str x1, [sp, #0]");                                    // save the source indexed-array value_type tag
    emitter.instruction("bl __rt_array_ensure_unique");                         // split shared arrays before rewriting element slots
    emitter.instruction("str x0, [sp, #8]");                                    // save the unique indexed-array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load the logical array length before the conversion loop
    emitter.instruction("str x9, [sp, #16]");                                   // save the logical length across mixed-box allocations
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize the element index to zero
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the source value_type tag
    emitter.instruction("cmp x10, #7");                                         // is the source already boxed Mixed?
    emitter.instruction("b.eq __rt_array_to_mixed_stamp");                      // already-mixed arrays only need metadata normalization

    emitter.label("__rt_array_to_mixed_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the current element index
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the logical array length
    emitter.instruction("cmp x9, x10");                                         // have all live slots been converted?
    emitter.instruction("b.ge __rt_array_to_mixed_stamp");                      // stamp the array once every live slot is boxed
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the unique indexed-array pointer
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the source value_type tag
    emitter.instruction("cmp x12, #1");                                         // do source slots contain string pointer/length pairs?
    emitter.instruction("b.eq __rt_array_to_mixed_load_string");                // string slots need a 16-byte load
    emitter.instruction("add x13, x11, #24");                                   // compute the pointer-sized source payload base
    emitter.instruction("ldr x1, [x13, x9, lsl #3]");                           // load the low payload word from the source slot
    emitter.instruction("mov x2, xzr");                                         // pointer-sized and scalar slots do not use a high payload word
    emitter.instruction("b __rt_array_to_mixed_box");                           // allocate a Mixed box for the loaded payload

    emitter.label("__rt_array_to_mixed_load_string");
    emitter.instruction("lsl x13, x9, #4");                                     // scale the string slot index by 16 bytes
    emitter.instruction("add x13, x11, x13");                                   // advance to the source string slot
    emitter.instruction("add x13, x13, #24");                                   // skip the indexed-array header
    emitter.instruction("ldr x1, [x13]");                                       // load the owned string pointer from the source slot
    emitter.instruction("ldr x2, [x13, #8]");                                   // load the string length from the source slot

    emitter.label("__rt_array_to_mixed_box");
    emitter.instruction("mov x0, x12");                                         // pass the source runtime value tag to the owned-box helper
    emitter.instruction("bl __rt_array_to_mixed_box_owned");                    // allocate a Mixed cell without retaining payloads already owned by the array
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the converted element index
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the unique indexed-array pointer
    emitter.instruction("add x13, x11, #24");                                   // compute the destination Mixed-slot payload base
    emitter.instruction("str x0, [x13, x9, lsl #3]");                           // replace the original slot with the new Mixed cell pointer
    emitter.instruction("add x9, x9, #1");                                      // advance to the next live slot
    emitter.instruction("str x9, [sp, #24]");                                   // persist the next index across allocations
    emitter.instruction("b __rt_array_to_mixed_loop");                          // continue converting live slots

    emitter.label("__rt_array_to_mixed_stamp");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the unique indexed-array pointer for metadata stamping
    emitter.instruction("mov x9, #8");                                          // Mixed arrays use pointer-sized slots
    emitter.instruction("str x9, [x0, #16]");                                   // store the normalized 8-byte element size
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the packed indexed-array kind word
    emitter.instruction("mov x11, #0x80ff");                                    // preserve heap kind and persistent copy-on-write flag
    emitter.instruction("and x10, x10, x11");                                   // clear stale value_type bits
    emitter.instruction("mov x11, #7");                                         // runtime value_type 7 = boxed Mixed
    emitter.instruction("lsl x11, x11, #8");                                    // move the Mixed tag into the packed kind word
    emitter.instruction("orr x10, x10, x11");                                   // combine stable metadata with the Mixed value_type tag
    emitter.instruction("str x10, [x0, #-8]");                                  // persist the Mixed indexed-array metadata
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the conversion frame
    emitter.instruction("ret");                                                 // return the converted array pointer

    emitter.label("__rt_array_to_mixed_box_owned");
    emitter.instruction("sub sp, sp, #48");                                     // reserve a helper frame for tag and payload words
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save helper frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the runtime value tag
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the payload words that transfer into the Mixed box
    emitter.instruction("mov x0, #24");                                         // Mixed cells store tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the boxed Mixed cell
    emitter.instruction("mov x9, #5");                                          // low byte 5 = boxed Mixed heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the heap allocation as a Mixed cell
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the saved runtime value tag
    emitter.instruction("str x10, [x0]");                                       // store the runtime value tag in the Mixed cell
    emitter.instruction("ldp x11, x12, [sp, #8]");                              // reload the payload words
    emitter.instruction("stp x11, x12, [x0, #8]");                              // store the payload words in the Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore helper frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the Mixed cell pointer
}

fn emit_array_to_mixed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_to_mixed ---");
    emitter.label_global("__rt_array_to_mixed");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before converting slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable conversion frame
    emitter.instruction("sub rsp, 32");                                         // reserve slots for tag, array pointer, length, and index
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the source indexed-array value_type tag
    emitter.instruction("call __rt_array_ensure_unique");                       // split shared arrays before rewriting element slots
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the unique indexed-array pointer
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the logical array length before the conversion loop
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the logical length across mixed-box allocations
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the element index to zero
    emitter.instruction("cmp QWORD PTR [rbp - 8], 7");                          // is the source already boxed Mixed?
    emitter.instruction("je __rt_array_to_mixed_x86_stamp");                    // already-mixed arrays only need metadata normalization

    emitter.label("__rt_array_to_mixed_x86_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current element index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // have all live slots been converted?
    emitter.instruction("jae __rt_array_to_mixed_x86_stamp");                   // stamp the array once every live slot is boxed
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the unique indexed-array pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source value_type tag
    emitter.instruction("cmp rax, 1");                                          // do source slots contain string pointer/length pairs?
    emitter.instruction("je __rt_array_to_mixed_x86_load_string");              // string slots need a 16-byte load
    emitter.instruction("mov rdi, QWORD PTR [r11 + 24 + r10 * 8]");             // load the low payload word from the source slot
    emitter.instruction("xor rsi, rsi");                                        // pointer-sized and scalar slots do not use a high payload word
    emitter.instruction("jmp __rt_array_to_mixed_x86_box");                     // allocate a Mixed box for the loaded payload

    emitter.label("__rt_array_to_mixed_x86_load_string");
    emitter.instruction("mov r8, r10");                                         // copy the string slot index before scaling
    emitter.instruction("shl r8, 4");                                           // scale the string slot index by 16 bytes
    emitter.instruction("lea r8, [r11 + r8 + 24]");                             // address the source string slot
    emitter.instruction("mov rdi, QWORD PTR [r8]");                             // load the owned string pointer from the source slot
    emitter.instruction("mov rsi, QWORD PTR [r8 + 8]");                         // load the string length from the source slot

    emitter.label("__rt_array_to_mixed_x86_box");
    emitter.instruction("call __rt_array_to_mixed_x86_box_owned");              // allocate a Mixed cell without retaining payloads already owned by the array
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the converted element index
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the unique indexed-array pointer
    emitter.instruction("mov QWORD PTR [r11 + 24 + r10 * 8], rax");             // replace the original slot with the new Mixed cell pointer
    emitter.instruction("add r10, 1");                                          // advance to the next live slot
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the next index across allocations
    emitter.instruction("jmp __rt_array_to_mixed_x86_loop");                    // continue converting live slots

    emitter.label("__rt_array_to_mixed_x86_stamp");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the unique indexed-array pointer for metadata stamping
    emitter.instruction("mov QWORD PTR [rax + 16], 8");                         // Mixed arrays use pointer-sized slots
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the packed indexed-array kind word
    emitter.instruction("mov r11, r10");                                        // copy the kind word so the heap marker can be preserved
    emitter.instruction("and r10, 0x80ff");                                     // preserve heap kind and persistent copy-on-write flag
    emitter.instruction("and r11, -65536");                                     // preserve the x86_64 heap marker bits
    emitter.instruction("or r10, r11");                                         // combine stable low metadata with the heap marker
    emitter.instruction("or r10, 0x700");                                       // stamp runtime value_type 7 = boxed Mixed
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // persist the Mixed indexed-array metadata
    emitter.instruction("add rsp, 32");                                         // release the conversion frame slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the converted array pointer

    emitter.label("__rt_array_to_mixed_x86_box_owned");
    emitter.instruction("push rbp");                                            // preserve the conversion frame before allocating a Mixed box
    emitter.instruction("mov rbp, rsp");                                        // establish a helper frame for tag and payload words
    emitter.instruction("sub rsp, 32");                                         // reserve helper slots for tag, payload, and alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the runtime value tag
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the low payload word
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the high payload word
    emitter.instruction("mov rax, 24");                                         // Mixed cells store tag plus two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate the boxed Mixed cell
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 5)); // materialize the x86_64 Mixed heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the heap allocation as a Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the saved runtime value tag
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the runtime value tag in the Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the low payload word
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the low payload word in the Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the high payload word
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store the high payload word in the Mixed cell
    emitter.instruction("add rsp, 32");                                         // release the helper frame slots
    emitter.instruction("pop rbp");                                             // restore the conversion frame pointer
    emitter.instruction("ret");                                                 // return the Mixed cell pointer
}
