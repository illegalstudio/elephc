//! Purpose:
//! Emits the `__rt_array_ensure_unique`, `__rt_array_ensure_unique_done` runtime helper assembly for array ensure unique.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - COW helpers must clone shared storage before mutation while leaving unique arrays and hashes untouched.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Splits a shared array before mutation using copy-on-write (COW) semantics.
///
/// Null arrays (x0=0) are returned immediately as they are trivially unique.
/// For non-null arrays: if refcount > 1, clones the array and decrements the
/// original's refcount; if refcount <= 1, returns the array unchanged.
///
/// Dispatches to `emit_array_ensure_unique_linux_x86_64` on x86_64; emits
/// inline ARM64 code on ARM64 targets.
///
/// Input:  x0 = candidate array pointer (ARM64) / rdi = candidate array pointer (x86_64)
/// Output: x0 = unique array pointer (ARM64) / rax = unique array pointer (x86_64)
pub fn emit_array_ensure_unique(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_ensure_unique_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_ensure_unique ---");
    emitter.label_global("__rt_array_ensure_unique");

    // -- null arrays are already trivially unique --
    emitter.instruction("cbz x0, __rt_array_ensure_unique_done");               // null inputs do not need copy-on-write splitting

    // -- only shared arrays need to be cloned --
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load the current array refcount from the uniform header
    emitter.instruction("cmp w9, #1");                                          // is there more than one owner of this array?
    emitter.instruction("b.ls __rt_array_ensure_unique_done");                  // refcount <= 1 means the array can be mutated in place

    // -- clone the shared array and release this mutator's old owner slot --
    emitter.instruction("sub sp, sp, #32");                                     // allocate a small stack frame for the split path
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the shared source array pointer
    emitter.instruction("bl __rt_array_clone_shallow");                         // clone the shared array for this mutating owner
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the previous shared array pointer
    emitter.instruction("ldr w10, [x9, #-12]");                                 // reload the old shared refcount
    emitter.instruction("sub w10, w10, #1");                                    // drop this mutator's old owner slot from the shared source array
    emitter.instruction("str w10, [x9, #-12]");                                 // persist the decremented refcount on the old shared array
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the split-path stack frame

    emitter.label("__rt_array_ensure_unique_done");
    emitter.instruction("ret");                                                 // return with x0 = a unique array pointer
}

/// Emits the x86_64 Linux implementation of `__rt_array_ensure_unique`.
///
/// Mirrors the ARM64 logic: null inputs return immediately; shared arrays
/// (refcount > 1) are cloned via `__rt_array_clone_shallow` and the original's
/// refcount is decremented; unique arrays (refcount <= 1) are returned unchanged.
///
/// Input:  rdi = candidate indexed-array pointer
/// Output: rax = unique indexed-array pointer
fn emit_array_ensure_unique_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_ensure_unique ---");
    emitter.label_global("__rt_array_ensure_unique");

    emitter.instruction("mov rax, rdi");                                        // default to returning the original indexed-array pointer when no copy-on-write split is needed
    emitter.instruction("test rdi, rdi");                                       // null indexed-array pointers are already trivially unique
    emitter.instruction("je __rt_array_ensure_unique_done");                    // return immediately for null inputs without touching heap metadata
    emitter.instruction("mov r10d, DWORD PTR [rdi - 12]");                      // load the current indexed-array refcount from the uniform heap header
    emitter.instruction("cmp r10d, 1");                                         // does the indexed array have more than one logical owner?
    emitter.instruction("jbe __rt_array_ensure_unique_done");                   // refcount <= 1 means the indexed array can be mutated in place
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving copy-on-write spill space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved shared indexed-array pointer
    emitter.instruction("sub rsp, 16");                                         // reserve one aligned spill slot for the shared indexed-array pointer across the clone call
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the shared indexed-array pointer across the clone helper call
    emitter.instruction("call __rt_array_clone_shallow");                       // clone the shared indexed array so this mutator gets an isolated owner copy
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the previous shared indexed-array pointer after the clone helper returns
    emitter.instruction("mov r11d, DWORD PTR [r10 - 12]");                      // reload the shared indexed-array refcount before dropping this mutator's owner slot
    emitter.instruction("sub r11d, 1");                                         // remove this mutator from the shared indexed-array refcount after cloning
    emitter.instruction("mov DWORD PTR [r10 - 12], r11d");                      // persist the decremented refcount back into the previous shared indexed-array header
    emitter.instruction("add rsp, 16");                                         // release the aligned copy-on-write spill slot before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the copy-on-write split
    emitter.label("__rt_array_ensure_unique_done");
    emitter.instruction("ret");                                                 // return to the caller with rax holding the unique indexed-array pointer
}
