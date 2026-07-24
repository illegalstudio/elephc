//! Purpose:
//! Emits the `__rt_array_set_refcounted` runtime helper for indexed-array writes
//! of heap-backed pointer payloads such as arrays, hashes, objects, and Mixed cells.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The helper preserves array COW, retains the incoming payload for the array owner,
//!   releases overwritten heap-backed slots, and keeps indexed-array metadata stamped.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::emit_branch_if_null_container;

/// Emits the refcounted indexed-array set helper for the current target.
pub fn emit_array_set_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_set_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_set_refcounted ---");
    emitter.label_global("__rt_array_set_refcounted");

    emitter.instruction("cmp x1, #0");                                          // reject negative offsets before retaining or mutating indexed-array storage
    emitter.instruction("b.lt __rt_array_set_refcounted_return");               // leave the array unchanged for unsupported negative indexed writes
    emitter.instruction("sub sp, sp, #64");                                     // reserve spill space for array, index, payload, and saved frame state
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a frame pointer for nested runtime helper calls
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the target index across copy-on-write and growth helpers
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the borrowed heap payload across helper calls
    emitter.instruction("bl __rt_array_ensure_unique");                         // split shared indexed arrays before mutating payload storage
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the unique indexed-array pointer across metadata and growth work

    emitter.instruction("ldr x9, [x0]");                                        // load logical length before first-write shape normalization
    emitter.instruction("cbnz x9, __rt_array_set_refcounted_shape_ready");      // non-empty indexed arrays already have a stable element layout
    emitter.instruction("mov x10, #8");                                         // refcounted indexed arrays use pointer-sized payload slots
    emitter.instruction("str x10, [x0, #16]");                                  // publish the pointer slot width before any later growth copies payload bytes
    emitter.label("__rt_array_set_refcounted_shape_ready");

    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the current packed indexed-array metadata
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the incoming heap payload before deriving the runtime value_type
    emit_branch_if_null_container(
        emitter,
        "x10",
        "x11",
        "__rt_array_set_refcounted_retain",
    );
    emitter.instruction("ldr x11, [x10, #-8]");                                 // load the incoming payload heap kind word
    emitter.instruction("and x11, x11, #0xff");                                 // isolate the low-byte heap kind tag
    emitter.instruction("cmp x11, #2");                                         // is the incoming payload an indexed array?
    emitter.instruction("b.eq __rt_array_set_refcounted_kind_array");           // encode value_type 4 for nested indexed arrays
    emitter.instruction("cmp x11, #3");                                         // is the incoming payload an associative array?
    emitter.instruction("b.eq __rt_array_set_refcounted_kind_hash");            // encode value_type 5 for nested hashes
    emitter.instruction("cmp x11, #4");                                         // is the incoming payload an object instance?
    emitter.instruction("b.eq __rt_array_set_refcounted_kind_object");          // encode value_type 6 for nested objects
    emitter.instruction("cmp x11, #5");                                         // is the incoming payload a boxed Mixed cell?
    emitter.instruction("b.ne __rt_array_set_refcounted_retain");               // unexpected heap kinds keep existing array metadata unchanged
    emitter.instruction("mov x10, #7");                                         // encode value_type 7 for boxed Mixed payloads
    emitter.instruction("b __rt_array_set_refcounted_kind_store");              // store the derived value_type tag in the array metadata
    emitter.label("__rt_array_set_refcounted_kind_object");
    emitter.instruction("mov x10, #6");                                         // encode value_type 6 for object payloads
    emitter.instruction("b __rt_array_set_refcounted_kind_store");              // store the derived value_type tag in the array metadata
    emitter.label("__rt_array_set_refcounted_kind_array");
    emitter.instruction("mov x10, #4");                                         // encode value_type 4 for indexed-array payloads
    emitter.instruction("b __rt_array_set_refcounted_kind_store");              // store the derived value_type tag in the array metadata
    emitter.label("__rt_array_set_refcounted_kind_hash");
    emitter.instruction("mov x10, #5");                                         // encode value_type 5 for associative-array payloads
    emitter.label("__rt_array_set_refcounted_kind_store");
    emitter.instruction("mov x14, #0x80ff");                                    // preserve the indexed-array kind and persistent copy-on-write bits
    emitter.instruction("and x9, x9, x14");                                     // clear stale runtime value_type metadata before stamping the new payload kind
    emitter.instruction("lsl x10, x10, #8");                                    // move the value_type tag into the packed kind-word byte lane
    emitter.instruction("orr x9, x9, x10");                                     // combine stable indexed-array metadata with the payload value_type tag
    emitter.instruction("str x9, [x0, #-8]");                                   // persist the updated indexed-array metadata

    emitter.label("__rt_array_set_refcounted_retain");
    emitter.instruction("ldr x0, [sp, #16]");                                   // move the borrowed payload into the retain helper input register
    emitter.instruction("bl __rt_incref");                                      // retain the payload so the array slot owns its stored reference

    emitter.label("__rt_array_set_refcounted_grow_check");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the current indexed-array pointer before checking capacity
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the target index after helper calls may have clobbered registers
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the current indexed-array capacity
    emitter.instruction("cmp x1, x10");                                         // does the target offset fit in the current allocation?
    emitter.instruction("b.lo __rt_array_set_refcounted_store");                // write directly once the slot is addressable
    emitter.instruction("bl __rt_array_grow");                                  // grow the indexed array so the target slot can be materialized
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the possibly reallocated indexed-array pointer
    emitter.instruction("b __rt_array_set_refcounted_grow_check");              // keep growing until the target offset fits within capacity

    emitter.label("__rt_array_set_refcounted_store");
    emitter.instruction("ldr x9, [x0]");                                        // reload logical length before deciding whether an old slot must be released
    emitter.instruction("cmp x1, x9");                                          // does the target index overwrite an existing heap-backed slot?
    emitter.instruction("b.hs __rt_array_set_refcounted_skip_release");         // writes beyond current length do not own an old slot
    emitter.instruction("add x10, x0, #24");                                    // compute the base address of the pointer payload region
    emitter.instruction("ldr x0, [x10, x1, lsl #3]");                           // load the previous heap-backed payload before overwriting the slot
    emitter.instruction("bl __rt_decref_any");                                  // release the overwritten payload through the uniform dispatcher
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the indexed-array pointer after old-slot release
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore the target index after old-slot release
    emitter.label("__rt_array_set_refcounted_skip_release");
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the retained payload that should be stored
    emitter.instruction("add x10, x0, #24");                                    // compute the base address of the pointer payload region
    emitter.instruction("str x2, [x10, x1, lsl #3]");                           // store the retained heap payload into the addressed indexed-array slot
    emitter.instruction("ldr x9, [x0]");                                        // reload logical length to decide whether this write extends the array
    emitter.instruction("cmp x1, x9");                                          // does the target index overwrite an existing slot?
    emitter.instruction("b.lo __rt_array_set_refcounted_done");                 // keep the current logical length for in-bounds overwrites
    emitter.instruction("mov x11, x9");                                         // start zero-filling gaps at the previous logical length
    emitter.label("__rt_array_set_refcounted_fill_loop");
    emitter.instruction("cmp x11, x1");                                         // have all slots before the target index been initialized?
    emitter.instruction("b.ge __rt_array_set_refcounted_store_len");            // stop gap filling before touching the target slot
    emitter.instruction("str xzr, [x10, x11, lsl #3]");                         // initialize the gap pointer slot to null
    emitter.instruction("add x11, x11, #1");                                    // advance to the next gap slot
    emitter.instruction("b __rt_array_set_refcounted_fill_loop");               // continue zero-filling until the target slot is reached
    emitter.label("__rt_array_set_refcounted_store_len");
    emitter.instruction("add x11, x1, #1");                                     // compute the new logical length as target index plus one
    emitter.instruction("str x11, [x0]");                                       // publish the extended indexed-array length
    emitter.label("__rt_array_set_refcounted_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release refcounted set helper spill space
    emitter.instruction("ret");                                                 // return with x0 holding the current indexed-array pointer

    emitter.label("__rt_array_set_refcounted_return");
    emitter.instruction("ret");                                                 // return the original indexed-array pointer for ignored negative writes
}

/// Emits the Linux x86_64 refcounted indexed-array set helper.
fn emit_array_set_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_set_refcounted ---");
    emitter.label_global("__rt_array_set_refcounted");

    emitter.instruction("mov rax, rdi");                                        // default the return value to the incoming indexed-array pointer
    emitter.instruction("cmp rsi, 0");                                          // reject negative offsets before retaining or mutating indexed-array storage
    emitter.instruction("jl __rt_array_set_refcounted_return");                 // leave the array unchanged for unsupported negative indexed writes
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving helper spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for array, index, and payload spills
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for array pointer, index, and payload
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the target index across copy-on-write and growth helpers
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the borrowed heap payload across helper calls
    emitter.instruction("call __rt_array_ensure_unique");                       // split shared indexed arrays before mutating payload storage
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the unique indexed-array pointer across metadata and growth work

    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load logical length before first-write shape normalization
    emitter.instruction("test r10, r10");                                       // is this the first write into the indexed array?
    emitter.instruction("jnz __rt_array_set_refcounted_shape_ready");           // non-empty indexed arrays already have a stable element layout
    emitter.instruction("mov QWORD PTR [rax + 16], 8");                         // refcounted indexed arrays use pointer-sized payload slots
    emitter.label("__rt_array_set_refcounted_shape_ready");

    emitter.instruction("mov r11, QWORD PTR [rax - 8]");                        // load the current packed indexed-array metadata
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the incoming heap payload before deriving the runtime value_type
    emit_branch_if_null_container(
        emitter,
        "r8",
        "r9",
        "__rt_array_set_refcounted_retain",
    );
    emitter.instruction("mov r9, QWORD PTR [r8 - 8]");                          // load the incoming payload heap kind word
    emitter.instruction("and r9, 0xff");                                        // isolate the low-byte heap kind tag
    emitter.instruction("cmp r9, 2");                                           // is the incoming payload an indexed array?
    emitter.instruction("je __rt_array_set_refcounted_kind_array");             // encode value_type 4 for nested indexed arrays
    emitter.instruction("cmp r9, 3");                                           // is the incoming payload an associative array?
    emitter.instruction("je __rt_array_set_refcounted_kind_hash");              // encode value_type 5 for nested hashes
    emitter.instruction("cmp r9, 4");                                           // is the incoming payload an object instance?
    emitter.instruction("je __rt_array_set_refcounted_kind_object");            // encode value_type 6 for nested objects
    emitter.instruction("cmp r9, 5");                                           // is the incoming payload a boxed Mixed cell?
    emitter.instruction("jne __rt_array_set_refcounted_retain");                // unexpected heap kinds keep existing array metadata unchanged
    emitter.instruction("mov r9, 7");                                           // encode value_type 7 for boxed Mixed payloads
    emitter.instruction("jmp __rt_array_set_refcounted_kind_store");            // store the derived value_type tag in the array metadata
    emitter.label("__rt_array_set_refcounted_kind_object");
    emitter.instruction("mov r9, 6");                                           // encode value_type 6 for object payloads
    emitter.instruction("jmp __rt_array_set_refcounted_kind_store");            // store the derived value_type tag in the array metadata
    emitter.label("__rt_array_set_refcounted_kind_array");
    emitter.instruction("mov r9, 4");                                           // encode value_type 4 for indexed-array payloads
    emitter.instruction("jmp __rt_array_set_refcounted_kind_store");            // store the derived value_type tag in the array metadata
    emitter.label("__rt_array_set_refcounted_kind_hash");
    emitter.instruction("mov r9, 5");                                           // encode value_type 5 for associative-array payloads
    emitter.label("__rt_array_set_refcounted_kind_store");
    emitter.instruction("mov r8, 0xffffffff000080ff");                          // preserve heap marker, indexed-array kind, and persistent COW metadata
    emitter.instruction("and r11, r8");                                         // clear stale runtime value_type metadata before stamping the new payload kind
    emitter.instruction("shl r9, 8");                                           // move the value_type tag into the packed kind-word byte lane
    emitter.instruction("or r11, r9");                                          // combine stable indexed-array metadata with the payload value_type tag
    emitter.instruction("mov QWORD PTR [rax - 8], r11");                        // persist the updated indexed-array metadata

    emitter.label("__rt_array_set_refcounted_retain");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // move the borrowed payload into the retain helper input register
    emitter.instruction("call __rt_incref");                                    // retain the payload so the array slot owns its stored reference

    emitter.label("__rt_array_set_refcounted_grow_check");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the current indexed-array pointer before checking capacity
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the target index after helper calls may have clobbered registers
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the current indexed-array capacity
    emitter.instruction("cmp rsi, r10");                                        // does the target offset fit in the current allocation?
    emitter.instruction("jb __rt_array_set_refcounted_store");                  // write directly once the slot is addressable
    emitter.instruction("mov rdi, rax");                                        // pass the current indexed-array pointer to the growth helper
    emitter.instruction("call __rt_array_grow");                                // grow the indexed array so the target slot can be materialized
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the possibly reallocated indexed-array pointer
    emitter.instruction("jmp __rt_array_set_refcounted_grow_check");            // keep growing until the target offset fits within capacity

    emitter.label("__rt_array_set_refcounted_store");
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload logical length before deciding whether an old slot must be released
    emitter.instruction("cmp rsi, r9");                                         // does the target index overwrite an existing heap-backed slot?
    emitter.instruction("jae __rt_array_set_refcounted_skip_release");          // writes beyond current length do not own an old slot
    emitter.instruction("mov rax, QWORD PTR [rax + 24 + rsi * 8]");             // load the previous heap-backed payload before overwriting the slot
    emitter.instruction("call __rt_decref_any");                                // release the overwritten payload through the uniform dispatcher
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // restore the indexed-array pointer after old-slot release
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // restore the target index after old-slot release
    emitter.label("__rt_array_set_refcounted_skip_release");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the retained payload that should be stored
    emitter.instruction("mov QWORD PTR [rax + 24 + rsi * 8], rdx");             // store the retained heap payload into the addressed indexed-array slot
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload logical length to decide whether this write extends the array
    emitter.instruction("cmp rsi, r9");                                         // does the target index overwrite an existing slot?
    emitter.instruction("jb __rt_array_set_refcounted_done");                   // keep the current logical length for in-bounds overwrites
    emitter.instruction("mov r11, r9");                                         // start zero-filling gaps at the previous logical length
    emitter.label("__rt_array_set_refcounted_fill_loop");
    emitter.instruction("cmp r11, rsi");                                        // have all slots before the target index been initialized?
    emitter.instruction("jae __rt_array_set_refcounted_store_len");             // stop gap filling before touching the target slot
    emitter.instruction("mov QWORD PTR [rax + 24 + r11 * 8], 0");               // initialize the gap pointer slot to null
    emitter.instruction("add r11, 1");                                          // advance to the next gap slot
    emitter.instruction("jmp __rt_array_set_refcounted_fill_loop");             // continue zero-filling until the target slot is reached
    emitter.label("__rt_array_set_refcounted_store_len");
    emitter.instruction("lea r11, [rsi + 1]");                                  // compute the new logical length as target index plus one
    emitter.instruction("mov QWORD PTR [rax], r11");                            // publish the extended indexed-array length
    emitter.label("__rt_array_set_refcounted_done");
    emitter.instruction("add rsp, 32");                                         // release refcounted set helper spill space
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.label("__rt_array_set_refcounted_return");
    emitter.instruction("ret");                                                 // return with rax holding the current indexed-array pointer
}
