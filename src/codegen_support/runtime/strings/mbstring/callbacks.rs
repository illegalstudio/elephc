//! Purpose:
//! Supplies native value-copying and float-formatting callbacks to shared mbstring invocation.
//!
//! Called from:
//! - The runtime's MbInvokeHostV1 callback table.
//!
//! Key details:
//! - Value copying resolves invoker references and preserves resource-cell identity.
//! - Float bytes borrow runtime scratch only until Rust immediately copies them.
//! - Protected diagnostic/release callbacks return PHP exceptions as statuses.

use super::*;
use crate::codegen_support::runtime::exceptions::emit_protected;
mod diagnostic;
mod pin;

/// Emits all additional callbacks required by the shared coordinator's complete host table.
pub(super) fn emit(emitter: &mut Emitter) {
    pin::emit(emitter);
    if emitter.target.arch == Arch::AArch64 { clone_aarch64(emitter); float_aarch64(emitter); }
    else { clone_x86_64(emitter); float_x86_64(emitter); }
    emit_protected(emitter, "__rt_mbstring_release", release_body);
    emit_protected(emitter, "__rt_mbstring_diagnostic", diagnostic::body);
}

/// Releases one native owner inside the protected callback's handler record.
fn release_body(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("ldr x0, [sp, #232]");                              // pass the retained owner to the native GC convention
        emitter.instruction("bl __rt_decref_any");                              // consume native string or boxed-value ownership
    } else {
        emitter.instruction("mov rax, QWORD PTR [rsp + 232]");                  // pass the retained owner to the native GC convention
        emitter.instruction("call __rt_decref_any");                            // consume native string or boxed-value ownership
    }
}

/// Copies AArch64 boxed values while dereferencing invoker markers before scalar copying.
fn clone_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_clone");
    emitter.instruction("cbz x2, __rt_mbstring_clone_invalid");                 // require writable owner storage before reading an argument
    emitter.instruction("str xzr, [x2]");                                       // failures initially transfer no owner
    emitter.instruction("sub sp, sp, #32");                                     // reserve owner output and caller linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the C caller across native value copying
    emitter.instruction("add x29, sp, #16");                                    // establish an aligned callback frame
    emitter.instruction("str x2, [sp]");                                        // retain the output pointer across allocation helpers
    emitter.label("__rt_mbstring_clone_unwrap");
    emitter.instruction("cbz x1, __rt_mbstring_clone_value");                   // the ordinary clone helper boxes a null input as PHP null
    emitter.instruction("ldr x9, [x1]");                                        // inspect the current boxed tag without converting its payload
    emitter.instruction("cmp x9, #7");                                          // nested Mixed cells must be detached at their concrete value
    emitter.instruction("b.eq __rt_mbstring_clone_nested");                     // unwrap another borrowed Mixed layer
    emitter.instruction("cmp x9, #11");                                         // invoker markers describe referenced caller storage
    emitter.instruction("b.eq __rt_mbstring_clone_reference");                  // normalize a reference before taking the by-value argument copy
    emitter.label("__rt_mbstring_clone_value");
    emitter.instruction("mov x0, x1");                                          // pass the borrowed concrete box to the resource-aware clone helper
    emitter.instruction("bl __rt_mixed_clone");                                 // detach ordinary values while retaining shared resource-cell identity
    emitter.instruction("b __rt_mbstring_clone_publish");                       // publish the sole acquired argument owner
    emitter.label("__rt_mbstring_clone_nested");
    emitter.instruction("ldr x1, [x1, #8]");                                    // follow the boxed Mixed payload to its borrowed child
    emitter.instruction("b __rt_mbstring_clone_unwrap");                        // resolve nested references before copying anything
    emitter.label("__rt_mbstring_clone_reference");
    emitter.instruction("ldp x10, x0, [x1, #8]");                               // recover caller storage and its authoritative source value tag
    emitter.instruction("cbz x10, __rt_mbstring_clone_bad_reference");          // reject a missing caller-storage address before dereferencing it
    emitter.instruction("cmp x0, #11");                                         // source tags describe values, never another raw invoker marker
    emitter.instruction("b.hs __rt_mbstring_clone_bad_reference");              // reject unsupported source-tag metadata
    emitter.instruction("ldr x1, [x10]");                                       // read the referenced scalar or pointer value exactly once
    emitter.instruction("cmp x0, #7");                                          // Mixed caller storage contains another boxed value pointer
    emitter.instruction("b.eq __rt_mbstring_clone_unwrap");                     // retain resource semantics when dereferencing boxed storage
    emitter.instruction("mov x2, #0");                                          // one-word values have no high payload word
    emitter.instruction("cmp x0, #1");                                          // strings also store their byte length beside the pointer
    emitter.instruction("b.ne __rt_mbstring_clone_box");                        // scalar and container payloads are ready for boxing
    emitter.instruction("ldr x2, [x10, #8]");                                   // read the referenced string's complete binary byte length
    emitter.label("__rt_mbstring_clone_box");
    emitter.instruction("bl __rt_mixed_from_value");                            // persist strings or retain containers in one independent argument cell
    emitter.label("__rt_mbstring_clone_publish");
    emitter.instruction("ldr x9, [sp]");                                        // recover the coordinator's preallocated ownership slot
    emitter.instruction("str x0, [x9]");                                        // transfer the copied argument before returning success
    emitter.instruction("mov x0, #0");                                          // report successful by-value copying
    emitter.instruction("b __rt_mbstring_clone_done");                          // share balanced callback teardown
    emitter.label("__rt_mbstring_clone_bad_reference");
    emitter.instruction("mov x0, #1");                                          // malformed reference metadata fails without acquiring an owner
    emitter.label("__rt_mbstring_clone_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore caller linkage on success and malformed input
    emitter.instruction("add sp, sp, #32");                                     // release the argument-copy frame
    emitter.instruction("ret");                                                 // return the C callback status
    emitter.label("__rt_mbstring_clone_invalid");
    emitter.instruction("mov x0, #1");                                          // missing output storage is a fatal callback contract error
    emitter.instruction("ret");                                                 // return without reading the borrowed argument
}

/// Copies SysV boxed values with the same reference normalization and resource ownership rules.
fn clone_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_clone");
    emitter.instruction("test rdx, rdx");                                       // require writable argument-owner storage
    emitter.instruction("jz __rt_mbstring_clone_invalid");                      // reject missing output before reading host values
    emitter.instruction("mov QWORD PTR [rdx], 0");                              // failures initially transfer no owner
    emitter.instruction("push rbp");                                            // preserve linkage and align native clone calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable callback frame
    emitter.instruction("sub rsp, 16");                                         // reserve the output pointer across allocations
    emitter.instruction("mov QWORD PTR [rsp], rdx");                            // retain the coordinator's ownership slot
    emitter.label("__rt_mbstring_clone_unwrap");
    emitter.instruction("test rsi, rsi");                                       // a null borrowed pointer represents PHP null
    emitter.instruction("jz __rt_mbstring_clone_value");                        // use ordinary null cloning for absent cells
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // inspect the current boxed runtime tag
    emitter.instruction("cmp r10, 7");                                          // nested Mixed payloads contain another borrowed box
    emitter.instruction("je __rt_mbstring_clone_nested");                       // unwrap before copying the concrete PHP value
    emitter.instruction("cmp r10, 11");                                         // invoker markers refer to caller storage
    emitter.instruction("je __rt_mbstring_clone_reference");                    // resolve the reference before taking a by-value copy
    emitter.label("__rt_mbstring_clone_value");
    emitter.instruction("mov rax, rsi");                                        // pass the concrete borrowed box through the native convention
    emitter.instruction("call __rt_mixed_clone");                               // detach values while retaining the existing resource cell
    emitter.instruction("jmp __rt_mbstring_clone_publish");                     // publish the copied argument owner
    emitter.label("__rt_mbstring_clone_nested");
    emitter.instruction("mov rsi, QWORD PTR [rsi + 8]");                        // follow the next borrowed Mixed child
    emitter.instruction("jmp __rt_mbstring_clone_unwrap");                      // normalize nested reference markers before cloning
    emitter.label("__rt_mbstring_clone_reference");
    emitter.instruction("mov r10, QWORD PTR [rsi + 8]");                        // recover the referenced caller-storage address
    emitter.instruction("mov rax, QWORD PTR [rsi + 16]");                       // recover its authoritative source runtime tag
    emitter.instruction("test r10, r10");                                       // reject missing caller storage before reading it
    emitter.instruction("jz __rt_mbstring_clone_bad_reference");                // return failure with no acquired owner
    emitter.instruction("cmp rax, 11");                                         // raw source tags must describe concrete values or Mixed storage
    emitter.instruction("jae __rt_mbstring_clone_bad_reference");               // reject unsupported marker source metadata
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // read the referenced low payload exactly once
    emitter.instruction("mov rsi, rdi");                                        // prepare a nested Mixed pointer for the unwrap loop
    emitter.instruction("cmp rax, 7");                                          // source tag seven denotes boxed caller storage
    emitter.instruction("je __rt_mbstring_clone_unwrap");                       // preserve nested resource identity through the ordinary clone helper
    emitter.instruction("xor esi, esi");                                        // ordinary scalar/container values have no high payload word
    emitter.instruction("cmp rax, 1");                                          // source strings carry a second word containing their byte length
    emitter.instruction("jne __rt_mbstring_clone_box");                         // other source payloads are ready for independent boxing
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // retain the referenced string's complete binary byte length
    emitter.label("__rt_mbstring_clone_box");
    emitter.instruction("call __rt_mixed_from_value");                          // persist strings or retain containers into an independent argument cell
    emitter.label("__rt_mbstring_clone_publish");
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // recover the coordinator's preallocated ownership slot
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish exactly one copied argument owner
    emitter.instruction("xor eax, eax");                                        // report successful by-value copying
    emitter.instruction("jmp __rt_mbstring_clone_done");                        // share balanced callback teardown
    emitter.label("__rt_mbstring_clone_bad_reference");
    emitter.instruction("mov eax, 1");                                          // malformed reference metadata fails before ownership transfer
    emitter.label("__rt_mbstring_clone_done");
    emitter.instruction("leave");                                               // release the owner spill and restore caller linkage
    emitter.instruction("ret");                                                 // return the C callback status
    emitter.label("__rt_mbstring_clone_invalid");
    emitter.instruction("mov eax, 1");                                          // missing output storage is a fatal callback contract error
    emitter.instruction("ret");                                                 // return without touching the borrowed input
}

/// Formats AArch64 float bits through the existing PHP formatter, returning borrowed scratch bytes.
fn float_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_float");
    emitter.instruction("cbz x2, __rt_mbstring_float_invalid");                 // require writable host-string result storage
    emitter.instruction("sub sp, sp, #32");                                     // retain output storage across the float formatter
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the C caller across formatting helpers
    emitter.instruction("add x29, sp, #16");                                    // establish an aligned float callback frame
    emitter.instruction("str x2, [sp]");                                        // retain the output descriptor pointer
    emitter.instruction("fmov d0, x1");                                         // preserve the supplied IEEE-754 bits in the formatter's FP register
    emitter.instruction("bl __rt_ftoa");                                        // format using the runtime's current default precision without extra warnings
    emitter.instruction("ldr x9, [sp]");                                        // recover the host-string output descriptor
    emitter.instruction("stp x1, x2, [x9]");                                    // publish borrowed scratch bytes and exact length
    emitter.instruction("str xzr, [x9, #16]");                                  // borrowed scratch carries no native owner
    emitter.instruction("mov x0, #0");                                          // report success before Rust immediately copies these bytes
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller's frame and return address
    emitter.instruction("add sp, sp, #32");                                     // release float callback storage
    emitter.instruction("ret");                                                 // return without retaining a scratch pointer beyond the C action
    emitter.label("__rt_mbstring_float_invalid");
    emitter.instruction("mov x0, #1");                                          // missing output storage is an invalid callback contract
    emitter.instruction("ret");                                                 // reject the call without running the formatter
}

/// Formats SysV float bits with the same borrowed scratch ownership convention.
fn float_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_float");
    emitter.instruction("test rdx, rdx");                                       // require writable host-string result storage
    emitter.instruction("jz __rt_mbstring_float_invalid");                      // reject missing output before formatting
    emitter.instruction("push rbp");                                            // align the formatter call and preserve caller linkage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable float callback frame
    emitter.instruction("sub rsp, 16");                                         // reserve the output descriptor pointer
    emitter.instruction("mov QWORD PTR [rsp], rdx");                            // retain caller output storage across formatting
    emitter.instruction("movq xmm0, rsi");                                      // preserve the supplied float bits in the formatter's FP register
    emitter.instruction("call __rt_ftoa");                                      // use the shared default PHP float layout without duplicate diagnostics
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // recover the native string result descriptor
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish borrowed binary scratch bytes
    emitter.instruction("mov QWORD PTR [r10 + 8], rdx");                        // publish their exact byte length
    emitter.instruction("mov QWORD PTR [r10 + 16], 0");                         // borrowed scratch has no independently releasable owner
    emitter.instruction("xor eax, eax");                                        // report success before the coordinator copies the scratch range
    emitter.instruction("leave");                                               // release the output spill and restore caller linkage
    emitter.instruction("ret");                                                 // return the non-unwinding C callback status
    emitter.label("__rt_mbstring_float_invalid");
    emitter.instruction("mov eax, 1");                                          // reject missing output as a fatal callback contract error
    emitter.instruction("ret");                                                 // return without running the formatter
}
