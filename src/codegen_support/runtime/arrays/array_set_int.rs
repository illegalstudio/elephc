//! Purpose:
//! Emits the `__rt_array_set_int` runtime helper for indexed-array scalar writes.
//! Keeps direct slot assignment target-aware while sharing COW, grow, and length extension.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The helper stores one 8-byte payload slot and returns the possibly reallocated array pointer.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the scalar indexed-array set helper for the current target.
pub fn emit_array_set_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_set_int_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_set_int ---");
    emitter.label_global("__rt_array_set_int");

    emitter.instruction("cmp x1, #0");                                          // reject negative offsets before mutating indexed-array storage
    emitter.instruction("b.lt __rt_array_set_int_return");                      // leave the array unchanged for unsupported negative indexed writes
    emitter.instruction("sub sp, sp, #48");                                     // reserve spill space for payload, index, array pointer, and saved frame state
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a frame pointer for nested runtime helper calls
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the target index across copy-on-write splitting
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the scalar payload across copy-on-write splitting
    emitter.instruction("bl __rt_array_ensure_unique");                         // split shared indexed arrays before mutating the payload storage
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the unique indexed-array pointer across shape and growth work

    emitter.instruction("ldr x9, [x0]");                                        // load the current logical length before first-write shape normalization
    emitter.instruction("cbnz x9, __rt_array_set_int_shape_ready");             // non-empty indexed arrays already have a stable element layout
    emitter.instruction("mov x10, #8");                                         // scalar indexed arrays use pointer-sized payload slots
    emitter.instruction("str x10, [x0, #16]");                                  // publish the scalar slot width before any later growth copies payload bytes
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load packed indexed-array metadata from the heap header
    emitter.instruction("mov x11, #0x80ff");                                    // keep only indexed-array kind and persistent copy-on-write bits
    emitter.instruction("and x10, x10, x11");                                   // clear stale string value_type metadata on first scalar write
    emitter.instruction("str x10, [x0, #-8]");                                  // persist scalar-oriented indexed-array metadata
    emitter.label("__rt_array_set_int_shape_ready");

    emitter.label("__rt_array_set_int_grow_check");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the current indexed-array pointer before checking capacity
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the target index after helper calls may have clobbered registers
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the current indexed-array capacity
    emitter.instruction("cmp x1, x10");                                         // does the target offset fit in the current allocation?
    emitter.instruction("b.lo __rt_array_set_int_store");                       // write directly once the slot is addressable
    emitter.instruction("bl __rt_array_grow");                                  // grow the indexed array so the target slot can be materialized
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the possibly reallocated indexed-array pointer
    emitter.instruction("b __rt_array_set_int_grow_check");                     // keep growing until the target offset fits within capacity

    emitter.label("__rt_array_set_int_store");
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the scalar payload that should be stored
    emitter.instruction("add x10, x0, #24");                                    // compute the base address of the scalar payload region
    emitter.instruction("str x2, [x10, x1, lsl #3]");                           // write the scalar payload into the addressed indexed-array slot
    emitter.instruction("ldr x9, [x0]");                                        // reload logical length to decide whether this write extends the array
    emitter.instruction("cmp x1, x9");                                          // does the target index overwrite an existing slot?
    emitter.instruction("b.lo __rt_array_set_int_done");                        // keep the current logical length for in-bounds overwrites
    emitter.instruction("mov x11, x9");                                         // start zero-filling gaps at the previous logical length
    emitter.label("__rt_array_set_int_fill_loop");
    emitter.instruction("cmp x11, x1");                                         // have all slots before the target index been initialized?
    emitter.instruction("b.ge __rt_array_set_int_store_len");                   // stop gap filling before touching the target slot
    emitter.instruction("str xzr, [x10, x11, lsl #3]");                         // initialize the scalar gap slot to zero/null
    emitter.instruction("add x11, x11, #1");                                    // advance to the next gap slot
    emitter.instruction("b __rt_array_set_int_fill_loop");                      // continue zero-filling until the target slot is reached
    emitter.label("__rt_array_set_int_store_len");
    emitter.instruction("add x11, x1, #1");                                     // compute the new logical length as target index plus one
    emitter.instruction("str x11, [x0]");                                       // publish the extended indexed-array length
    emitter.label("__rt_array_set_int_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release scalar set helper spill space
    emitter.instruction("ret");                                                 // return with x0 holding the current indexed-array pointer

    emitter.label("__rt_array_set_int_return");
    emitter.instruction("ret");                                                 // return the original indexed-array pointer for ignored negative writes
}

/// Emits the Linux x86_64 scalar indexed-array set helper.
fn emit_array_set_int_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_set_int ---");
    emitter.label_global("__rt_array_set_int");

    emitter.instruction("mov rax, rdi");                                        // default the return value to the incoming indexed-array pointer
    emitter.instruction("cmp rsi, 0");                                          // reject negative offsets before mutating indexed-array storage
    emitter.instruction("jl __rt_array_set_int_return");                        // leave the array unchanged for unsupported negative indexed writes
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving helper spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for array, index, and payload spills
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for array pointer, index, and scalar payload
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the target index across copy-on-write and growth helpers
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the scalar payload across copy-on-write and growth helpers
    emitter.instruction("call __rt_array_ensure_unique");                       // split shared indexed arrays before mutating payload storage
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the unique indexed-array pointer across shape and growth work

    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load logical length before first-write shape normalization
    emitter.instruction("test r10, r10");                                       // is this the first write into the indexed array?
    emitter.instruction("jnz __rt_array_set_int_shape_ready");                  // non-empty indexed arrays already have a stable element layout
    emitter.instruction("mov QWORD PTR [rax + 16], 8");                         // scalar indexed arrays use pointer-sized payload slots
    emitter.instruction("mov r11, QWORD PTR [rax - 8]");                        // load packed indexed-array metadata from the heap header
    emitter.instruction("mov r8, 0xffffffff000080ff");                          // preserve heap marker, indexed-array kind, and copy-on-write metadata
    emitter.instruction("and r11, r8");                                         // clear stale string value_type metadata on first scalar write
    emitter.instruction("mov QWORD PTR [rax - 8], r11");                        // persist scalar-oriented indexed-array metadata
    emitter.label("__rt_array_set_int_shape_ready");

    emitter.label("__rt_array_set_int_grow_check");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the current indexed-array pointer before checking capacity
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the target index after helper calls may have clobbered registers
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the current indexed-array capacity
    emitter.instruction("cmp rsi, r10");                                        // does the target offset fit in the current allocation?
    emitter.instruction("jb __rt_array_set_int_store");                         // write directly once the slot is addressable
    emitter.instruction("mov rdi, rax");                                        // pass the current indexed-array pointer to the growth helper
    emitter.instruction("call __rt_array_grow");                                // grow the indexed array so the target slot can be materialized
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the possibly reallocated indexed-array pointer
    emitter.instruction("jmp __rt_array_set_int_grow_check");                   // keep growing until the target offset fits within capacity

    emitter.label("__rt_array_set_int_store");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the scalar payload that should be stored
    emitter.instruction("mov QWORD PTR [rax + 24 + rsi * 8], rdx");             // write the scalar payload into the addressed indexed-array slot
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload logical length to decide whether this write extends the array
    emitter.instruction("cmp rsi, r9");                                         // does the target index overwrite an existing slot?
    emitter.instruction("jb __rt_array_set_int_done");                          // keep the current logical length for in-bounds overwrites
    emitter.instruction("mov r11, r9");                                         // start zero-filling gaps at the previous logical length
    emitter.label("__rt_array_set_int_fill_loop");
    emitter.instruction("cmp r11, rsi");                                        // have all slots before the target index been initialized?
    emitter.instruction("jae __rt_array_set_int_store_len");                    // stop gap filling before touching the target slot
    emitter.instruction("mov QWORD PTR [rax + 24 + r11 * 8], 0");               // initialize the scalar gap slot to zero/null
    emitter.instruction("add r11, 1");                                          // advance to the next gap slot
    emitter.instruction("jmp __rt_array_set_int_fill_loop");                    // continue zero-filling until the target slot is reached
    emitter.label("__rt_array_set_int_store_len");
    emitter.instruction("lea r11, [rsi + 1]");                                  // compute the new logical length as target index plus one
    emitter.instruction("mov QWORD PTR [rax], r11");                            // publish the extended indexed-array length
    emitter.label("__rt_array_set_int_done");
    emitter.instruction("add rsp, 32");                                         // release scalar set helper spill space
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.label("__rt_array_set_int_return");
    emitter.instruction("ret");                                                 // return with rax holding the current indexed-array pointer
}
