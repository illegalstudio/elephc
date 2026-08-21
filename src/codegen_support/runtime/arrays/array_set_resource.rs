//! Purpose:
//! Emits the `__rt_array_set_resource` runtime helper for indexed-array resource writes.
//! Keeps COW cloning and opaque registry-handle ownership balanced on every supported ABI.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via the array runtime section.
//!
//! Key details:
//! - The incoming handle is retained before an overwritten handle is released, so self-assignment is safe.
//! - Runtime value_type 9 identifies pointer-sized opaque resource handles during clone and deep free.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the resource indexed-array set helper for the current target.
pub fn emit_array_set_resource(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_set_resource_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_set_resource ---");
    emitter.label_global("__rt_array_set_resource");

    emitter.instruction("cmp x1, #0");                                          // reject negative offsets before acquiring resource ownership
    emitter.instruction("b.lt __rt_array_set_resource_return");                 // leave the indexed array unchanged for unsupported negative writes
    emitter.instruction("sub sp, sp, #64");                                     // reserve array, index, handle, and saved-frame spill storage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a frame pointer for nested runtime calls
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the target index across COW and registry helpers
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the borrowed incoming resource handle
    emitter.instruction("bl __rt_array_ensure_unique");                         // split shared indexed arrays before mutating a resource slot
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the unique indexed-array pointer across helper calls

    // -- normalize resource-array shape and ownership --
    emitter.instruction("mov x9, #8");                                          // resource arrays use one pointer-sized opaque handle per slot
    emitter.instruction("str x9, [x0, #16]");                                   // publish the resource slot width before growth copies payload bytes
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load packed indexed-array metadata from the heap header
    emitter.instruction("mov x10, #0x80ff");                                    // preserve the indexed-array kind and persistent COW flag
    emitter.instruction("and x9, x9, x10");                                     // clear any stale runtime value_type bits
    emitter.instruction("mov x10, #9");                                         // materialize runtime value_type 9 for opaque resource handles
    emitter.instruction("lsl x10, x10, #8");                                    // move the resource tag into the packed value_type byte
    emitter.instruction("orr x9, x9, x10");                                     // combine stable array metadata with the resource tag
    emitter.instruction("str x9, [x0, #-8]");                                   // persist the resource-array metadata before a possible clone or free
    emitter.instruction("ldr x0, [sp, #16]");                                   // pass the borrowed incoming handle to the registry retain helper
    emitter.instruction("bl __rt_resource_retain");                             // acquire the reference that will be owned by the destination slot

    // -- grow the unique array until the destination slot exists --
    emitter.label("__rt_array_set_resource_grow_check");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the current indexed-array pointer before checking capacity
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the target index after nested runtime calls
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the current indexed-array capacity
    emitter.instruction("cmp x1, x9");                                          // does the target index fit in the current allocation?
    emitter.instruction("b.lo __rt_array_set_resource_store");                  // proceed once the addressed resource slot is allocated
    emitter.instruction("bl __rt_array_grow");                                  // grow the indexed array while preserving its resource metadata
    emitter.instruction("str x0, [sp, #0]");                                    // retain the possibly reallocated indexed-array pointer
    emitter.instruction("b __rt_array_set_resource_grow_check");                // keep growing until the target index is addressable

    // -- release an overwritten owner, then install the retained handle --
    emitter.label("__rt_array_set_resource_store");
    emitter.instruction("ldr x9, [x0]");                                        // load logical length before testing for an existing slot owner
    emitter.instruction("cmp x1, x9");                                          // does this write replace a live resource handle?
    emitter.instruction("b.hs __rt_array_set_resource_skip_release");           // writes beyond logical length have no prior owner to release
    emitter.instruction("add x10, x0, #24");                                    // compute the base address of resource-handle payloads
    emitter.instruction("ldr x0, [x10, x1, lsl #3]");                           // load the overwritten opaque handle
    emitter.instruction("bl __rt_resource_release");                            // relinquish the replaced array-slot reference exactly once
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the indexed-array pointer after registry release
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore the target index after registry release
    emitter.label("__rt_array_set_resource_skip_release");
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the retained incoming opaque handle
    emitter.instruction("add x10, x0, #24");                                    // recompute the resource payload base
    emitter.instruction("str x2, [x10, x1, lsl #3]");                           // transfer the retained handle reference into the target slot
    emitter.instruction("ldr x9, [x0]");                                        // reload logical length to determine whether the write extends the array
    emitter.instruction("cmp x1, x9");                                          // is the target offset already within logical length?
    emitter.instruction("b.lo __rt_array_set_resource_done");                   // preserve logical length for an in-bounds overwrite
    emitter.instruction("mov x11, x9");                                         // start zero-filling resource gaps at the old logical end
    emitter.label("__rt_array_set_resource_fill_loop");
    emitter.instruction("cmp x11, x1");                                         // have all resource slots before the target been initialized?
    emitter.instruction("b.ge __rt_array_set_resource_store_len");              // stop gap filling immediately before the target slot
    emitter.instruction("str xzr, [x10, x11, lsl #3]");                         // initialize an unowned resource gap with the null handle
    emitter.instruction("add x11, x11, #1");                                    // advance to the next gap slot
    emitter.instruction("b __rt_array_set_resource_fill_loop");                 // continue until every intermediate slot is initialized
    emitter.label("__rt_array_set_resource_store_len");
    emitter.instruction("add x11, x1, #1");                                     // compute the extended logical length
    emitter.instruction("str x11, [x0]");                                       // publish the new logical length after initializing all slots
    emitter.label("__rt_array_set_resource_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release resource-set spill storage
    emitter.instruction("ret");                                                 // return the current indexed-array pointer in x0

    emitter.label("__rt_array_set_resource_return");
    emitter.instruction("ret");                                                 // return the original array pointer for ignored negative writes
}

/// Emits the Linux x86_64 resource indexed-array set helper.
fn emit_array_set_resource_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_set_resource ---");
    emitter.label_global("__rt_array_set_resource");

    emitter.instruction("mov rax, rdi");                                        // default the result to the incoming indexed-array pointer
    emitter.instruction("cmp rsi, 0");                                          // reject negative offsets before acquiring resource ownership
    emitter.instruction("jl __rt_array_set_resource_return");                   // leave the indexed array unchanged for unsupported negative writes
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for nested runtime calls
    emitter.instruction("sub rsp, 32");                                         // reserve aligned array, index, and handle spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the target index across COW and registry helpers
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the borrowed incoming resource handle
    emitter.instruction("call __rt_array_ensure_unique");                       // split shared indexed arrays before mutating a resource slot
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the unique indexed-array pointer across helper calls

    // -- normalize resource-array shape and ownership --
    emitter.instruction("mov QWORD PTR [rax + 16], 8");                         // resource arrays use one pointer-sized opaque handle per slot
    emitter.instruction("mov r9, QWORD PTR [rax - 8]");                         // load packed indexed-array metadata from the heap header
    emitter.instruction("mov r10, 0xffffffff000080ff");                         // preserve heap magic, indexed-array kind, and persistent COW metadata
    emitter.instruction("and r9, r10");                                         // clear any stale runtime value_type bits
    emitter.instruction("or r9, 0x900");                                        // stamp runtime value_type 9 for opaque resource handles
    emitter.instruction("mov QWORD PTR [rax - 8], r9");                         // persist resource metadata before later growth or deep free
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass the borrowed incoming handle to registry retain
    emitter.instruction("call __rt_resource_retain");                           // acquire the reference that the destination slot will own

    // -- grow the unique array until the destination slot exists --
    emitter.label("__rt_array_set_resource_grow_check");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the current indexed-array pointer before checking capacity
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the target index after nested runtime calls
    emitter.instruction("mov r9, QWORD PTR [rax + 8]");                         // load the current indexed-array capacity
    emitter.instruction("cmp rsi, r9");                                         // does the target index fit in the current allocation?
    emitter.instruction("jb __rt_array_set_resource_store");                    // proceed once the addressed resource slot is allocated
    emitter.instruction("mov rdi, rax");                                        // pass the current array pointer to the growth helper
    emitter.instruction("call __rt_array_grow");                                // grow the indexed array while preserving its resource metadata
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the possibly reallocated indexed-array pointer
    emitter.instruction("jmp __rt_array_set_resource_grow_check");              // keep growing until the target index is addressable

    // -- release an overwritten owner, then install the retained handle --
    emitter.label("__rt_array_set_resource_store");
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // load logical length before testing for an existing slot owner
    emitter.instruction("cmp rsi, r9");                                         // does this write replace a live resource handle?
    emitter.instruction("jae __rt_array_set_resource_skip_release");            // writes beyond logical length have no prior owner to release
    emitter.instruction("mov rdi, QWORD PTR [rax + 24 + rsi * 8]");             // load the overwritten opaque handle
    emitter.instruction("call __rt_resource_release");                          // relinquish the replaced array-slot reference exactly once
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // restore the indexed-array pointer after registry release
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // restore the target index after registry release
    emitter.label("__rt_array_set_resource_skip_release");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the retained incoming opaque handle
    emitter.instruction("mov QWORD PTR [rax + 24 + rsi * 8], rdx");             // transfer the retained handle reference into the target slot
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload logical length to determine whether the write extends the array
    emitter.instruction("cmp rsi, r9");                                         // is the target offset already within logical length?
    emitter.instruction("jb __rt_array_set_resource_done");                     // preserve logical length for an in-bounds overwrite
    emitter.instruction("mov r11, r9");                                         // start zero-filling resource gaps at the old logical end
    emitter.label("__rt_array_set_resource_fill_loop");
    emitter.instruction("cmp r11, rsi");                                        // have all resource slots before the target been initialized?
    emitter.instruction("jae __rt_array_set_resource_store_len");               // stop gap filling immediately before the target slot
    emitter.instruction("mov QWORD PTR [rax + 24 + r11 * 8], 0");               // initialize an unowned resource gap with the null handle
    emitter.instruction("add r11, 1");                                          // advance to the next gap slot
    emitter.instruction("jmp __rt_array_set_resource_fill_loop");               // continue until every intermediate slot is initialized
    emitter.label("__rt_array_set_resource_store_len");
    emitter.instruction("lea r11, [rsi + 1]");                                  // compute the extended logical length
    emitter.instruction("mov QWORD PTR [rax], r11");                            // publish the new logical length after initializing all slots
    emitter.label("__rt_array_set_resource_done");
    emitter.instruction("add rsp, 32");                                         // release resource-set spill storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the current indexed-array pointer in rax

    emitter.label("__rt_array_set_resource_return");
    emitter.instruction("ret");                                                 // return the original array pointer for ignored negative writes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies every supported ABI retains the incoming handle before releasing an overwritten one.
    #[test]
    fn resource_set_retain_precedes_overwrite_release() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_array_set_resource(&mut emitter);
            let asm = emitter.output();
            let retain = asm.find("__rt_resource_retain").expect("resource retain call");
            let release = asm.find("__rt_resource_release").expect("resource release call");

            assert!(retain < release, "{target:?} can invalidate resource self-assignment");
            assert!(asm.contains("__rt_array_ensure_unique"));
        }
    }

    /// Verifies every supported ABI stamps indexed resource arrays with runtime value_type 9.
    #[test]
    fn resource_set_stamps_resource_array_metadata() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_array_set_resource(&mut emitter);
            let asm = emitter.output();

            assert!(
                asm.contains("mov x10, #9") || asm.contains("or r9, 0x900"),
                "{target:?} omitted resource-array value_type metadata"
            );
        }
    }
}
