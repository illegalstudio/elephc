//! Purpose:
//! Transfers an eval-owned boxed Throwable into the native pending-exception slot.
//!
//! Called from:
//! - The eval array-reference adapter before protected intermediate-owner cleanup.
//!
//! Key details:
//! - The C action consumes its boxed input and returns without unwinding through Rust.
//! - Raw object ownership is retained before the boxed owner is released.
//! - Subsequent cleanup can preserve and chain the already-published exception.

use super::*;

#[cfg(all(test, unix))]
mod tests;

/// Emits the boxed-owner publication action used by the eval reference adapter.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_publish_throw");
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter); } else { x86_64(emitter); }
}

/// Publishes one AArch64 object owner and consumes its input through protected release.
fn aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #48");                                     // reserve boxed ownership, raw payload, status, and caller linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the C caller across native ownership operations
    emitter.instruction("add x29, sp, #32");                                    // establish an aligned publication frame
    emitter.instruction("str x1, [sp]");                                        // retain the input owner before inspecting its value
    emitter.instruction("str xzr, [sp, #16]");                                  // assume success until the runtime tag is validated
    emitter.instruction("mov x0, x1");                                          // pass the borrowed boxed Throwable through the native convention
    emitter.instruction("bl __rt_mixed_unbox");                                 // obtain the concrete tag and raw object payload
    emitter.instruction("cmp x0, #6");                                          // only PHP objects can occupy the pending Throwable slot
    emitter.instruction("b.ne __rt_mbstring_publish_throw_invalid");            // consume malformed input without publishing it
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the raw object while retaining its new owner
    emitter.instruction("mov x0, x1");                                          // retain object ownership separately from the soon-to-be-released box
    emitter.instruction("bl __rt_incref");                                      // acquire the owner transferred into the pending exception slot
    emitter.instruction("ldr x10, [sp, #8]");                                   // recover the retained pending object
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_value", 0);
    emitter.instruction("b __rt_mbstring_publish_throw_release");               // consume boxed ownership after raw publication
    emitter.label("__rt_mbstring_publish_throw_invalid");
    emitter.instruction("mov x9, #1");                                          // malformed Throwable metadata produces a fatal host status
    emitter.instruction("str x9, [sp, #16]");                                   // preserve failure across protected input release
    emitter.label("__rt_mbstring_publish_throw_release");
    emitter.instruction("mov x0, #0");                                          // native release does not require an eval context
    emitter.instruction("ldr x1, [sp]");                                        // transfer the boxed owner to the protected release action
    emitter.instruction("bl __rt_mbstring_release");                            // never unwind through the calling Rust adapter
    emitter.instruction("cbz x0, __rt_mbstring_publish_throw_status");          // successful cleanup restores the saved publication status
    emitter.instruction("ldr x9, [sp, #16]");                                   // determine whether an exception owner was already installed
    emitter.instruction("cbz x9, __rt_mbstring_publish_throw_pending");         // an installed exception outranks a later fatal release status
    emitter.instruction("cmp x0, #2");                                          // an invalid input can still encounter a throwing cleanup callback
    emitter.instruction("b.eq __rt_mbstring_publish_throw_pending");            // retain that native pending exception
    emitter.label("__rt_mbstring_publish_throw_status");
    emitter.instruction("ldr x0, [sp, #16]");                                   // restore success or validation failure after consuming the input
    emitter.instruction("b __rt_mbstring_publish_throw_done");                  // finish with the selected non-unwinding status
    emitter.label("__rt_mbstring_publish_throw_pending");
    emitter.instruction("mov x0, #2");                                          // preserve published native exception ownership on cleanup failure
    emitter.label("__rt_mbstring_publish_throw_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the original C caller linkage
    emitter.instruction("add sp, sp, #48");                                     // release the publication frame after every owned path
    emitter.instruction("ret");                                                 // return a runtime status without throwing
}

/// Publishes the same independently retained object owner under the SysV ABI.
fn x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // preserve linkage and align subsequent native calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable publication frame
    emitter.instruction("sub rsp, 32");                                         // reserve input ownership, raw object payload, and completion status
    emitter.instruction("mov QWORD PTR [rsp], rsi");                            // retain the boxed owner supplied through the second C argument
    emitter.instruction("mov QWORD PTR [rsp + 16], 0");                         // initialize the successful publication status
    emitter.instruction("mov rax, rsi");                                        // inspect the borrowed box using the native value convention
    emitter.instruction("call __rt_mixed_unbox");                               // obtain the concrete object tag and payload
    emitter.instruction("cmp rax, 6");                                          // pending Throwable ownership requires a PHP object value
    emitter.instruction("jne __rt_mbstring_publish_throw_invalid");             // consume invalid input without installing a fabricated exception
    emitter.instruction("mov QWORD PTR [rsp + 8], rdi");                        // preserve the raw object across its retain operation
    emitter.instruction("mov rax, rdi");                                        // pass the raw object to native reference counting
    emitter.instruction("call __rt_incref");                                    // acquire the independent pending-exception owner
    emitter.instruction("mov r10, QWORD PTR [rsp + 8]");                        // recover the retained raw object
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_value", 0);
    emitter.instruction("jmp __rt_mbstring_publish_throw_release");             // release the input only after pending ownership is safe
    emitter.label("__rt_mbstring_publish_throw_invalid");
    emitter.instruction("mov QWORD PTR [rsp + 16], 1");                         // preserve validation failure through protected input cleanup
    emitter.label("__rt_mbstring_publish_throw_release");
    emitter.instruction("xor edi, edi");                                        // protected native release needs no eval context
    emitter.instruction("mov rsi, QWORD PTR [rsp]");                            // transfer exactly one boxed owner for release
    emitter.instruction("call __rt_mbstring_release");                          // contain any PHP exception within the native callback boundary
    emitter.instruction("test eax, eax");                                       // distinguish cleanup success from a later failure
    emitter.instruction("jz __rt_mbstring_publish_throw_status");               // successful cleanup preserves the original publication result
    emitter.instruction("cmp QWORD PTR [rsp + 16], 0");                         // a successful publication already owns a pending exception
    emitter.instruction("je __rt_mbstring_publish_throw_pending");              // preserve it even when cleanup reports a fatal status
    emitter.instruction("cmp eax, 2");                                          // invalid input may still trigger a pending cleanup exception
    emitter.instruction("je __rt_mbstring_publish_throw_pending");              // pending Throwable ownership has precedence
    emitter.label("__rt_mbstring_publish_throw_status");
    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                       // restore the completed publication or validation status
    emitter.instruction("jmp __rt_mbstring_publish_throw_done");                // share balanced teardown after selecting the result
    emitter.label("__rt_mbstring_publish_throw_pending");
    emitter.instruction("mov eax, 2");                                          // report the native pending owner after cleanup failure
    emitter.label("__rt_mbstring_publish_throw_done");
    emitter.instruction("leave");                                               // release publication storage and restore the C caller frame
    emitter.instruction("ret");                                                 // return without unwinding across Rust
}
