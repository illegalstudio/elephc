//! Purpose:
//! Pins original boxed identities used by protected mbstring graph readers.
//!
//! Called from:
//! - V3 argument preparation and the native graph-entry callback.
//!
//! Key details:
//! - Heap ownership is acquired without copying or interpreting a mutable PHP value.
//! - Stack-backed descriptors need no owner because their caller frame spans the invocation.
//! - No PHP callback or destructor runs while acquiring the pin.

use super::*;

/// Emits the C callback that transfers an optional native identity owner into caller storage.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_pin");
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter); } else { x86_64(emitter); }
}

/// Pins one AArch64 heap box while keeping nonheap caller descriptors borrowed.
fn aarch64(emitter: &mut Emitter) {
    emitter.instruction("cbz x2, __rt_mbstring_pin_invalid");                   // reject missing output storage before reading input
    emitter.instruction("str xzr, [x2]");                                       // initialize optional ownership before metadata inspection
    emitter.instruction("sub sp, sp, #32");                                     // reserve original identity, output address, and caller linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the C caller across native heap helpers
    emitter.instruction("add x29, sp, #16");                                    // establish the aligned callback frame
    emitter.instruction("stp x1, x2, [sp]");                                    // retain both callback inputs across heap classification
    emitter.instruction("mov x0, x1");                                          // inspect the borrowed original boxed identity
    emitter.instruction("bl __rt_heap_kind");                                   // classify managed ownership without reading PHP payloads
    emitter.instruction("cbz x0, __rt_mbstring_pin_done");                      // caller-owned stack descriptors remain valid without an acquired owner
    emitter.instruction("ldr x0, [sp]");                                        // recover the actual heap pointer after classification
    emitter.instruction("bl __rt_incref");                                      // acquire one exact identity owner without cloning its mutable box
    emitter.instruction("ldp x9, x10, [sp]");                                   // recover original pointer and caller ownership destination
    emitter.instruction("str x9, [x10]");                                       // transfer the acquired pin to explicit invocation cleanup
    emitter.label("__rt_mbstring_pin_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the native caller on every successful path
    emitter.instruction("add sp, sp, #32");                                     // release callback-local storage
    emitter.instruction("mov x0, #0");                                          // report success with optional pinned ownership
    emitter.instruction("ret");                                                 // return through the C callback convention
    emitter.label("__rt_mbstring_pin_invalid");
    emitter.instruction("mov x0, #1");                                          // report a fatal callback contract violation
    emitter.instruction("ret");                                                 // return without reading or retaining input
}

/// Applies the same optional ownership protocol to boxed identities under SysV.
fn x86_64(emitter: &mut Emitter) {
    emitter.instruction("test rdx, rdx");                                       // require writable optional-owner storage
    emitter.instruction("jz __rt_mbstring_pin_invalid");                        // reject a missing destination before metadata reads
    emitter.instruction("mov QWORD PTR [rdx], 0");                              // initialize cleanup ownership before any helper call
    emitter.instruction("push rbp");                                            // preserve caller linkage and align native calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable callback frame
    emitter.instruction("sub rsp, 16");                                         // reserve original identity and owner output address
    emitter.instruction("mov QWORD PTR [rsp], rsi");                            // retain the original identity across heap classification
    emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                        // preserve the ownership destination
    emitter.instruction("mov rax, rsi");                                        // pass the borrowed identity to the internal heap convention
    emitter.instruction("call __rt_heap_kind");                                 // reject nonheap descriptors without interpreting their value
    emitter.instruction("test eax, eax");                                       // distinguish owned boxes from caller stack storage
    emitter.instruction("jz __rt_mbstring_pin_done");                           // nonheap identities need no separate release
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // recover the actual managed box pointer
    emitter.instruction("call __rt_incref");                                    // pin the exact original box rather than its copied value
    emitter.instruction("mov r10, QWORD PTR [rsp + 8]");                        // recover caller ownership storage
    emitter.instruction("mov r11, QWORD PTR [rsp]");                            // preserve the original metadata identity
    emitter.instruction("mov QWORD PTR [r10], r11");                            // transfer the acquired identity owner to the coordinator
    emitter.label("__rt_mbstring_pin_done");
    emitter.instruction("leave");                                               // release spills and restore caller linkage
    emitter.instruction("xor eax, eax");                                        // report success with zero or one acquired pin
    emitter.instruction("ret");                                                 // return the C callback status
    emitter.label("__rt_mbstring_pin_invalid");
    emitter.instruction("mov eax, 1");                                          // report fatal failure for an invalid output pointer
    emitter.instruction("ret");                                                 // return without dereferencing the borrowed identity
}
