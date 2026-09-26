//! Purpose:
//! Builds owned array wire arguments and releases them after shared mbstring calls.
//!
//! Called from:
//! - AOT/eval argument staging and the non-unwinding mbstring status adapter.
//!
//! Key details:
//! - Snapshotting borrows original arrays; only the copied wire string crosses the call boundary.
//! - Argument ownership transfers to the status adapter, which clears released byte pointers.
//! - Rust-owned snapshot results are always released before returning to PHP argument staging.

use super::*;
use elephc_builtin_contract::mbstring_abi::{ARG_ARRAY, RESULT_ARRAY};

/// Emits packing and consuming helpers for the selected target architecture.
pub(super) fn emit(emitter: &mut Emitter) {
    match emitter.target.arch { Arch::AArch64 => aarch64(emitter), Arch::X86_64 => x86_64(emitter) }
}
/// Copies native AArch64 graphs into owned argument bytes and consumes their owners after calls.
fn aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_array_argument");
    emitter.instruction("sub sp, sp, #112");                                    // reserve the C result, root descriptor, owned bytes, and linkage
    emitter.instruction("stp x29, x30, [sp, #96]");                             // preserve the PHP caller frame and return address
    emitter.instruction("add x29, sp, #96");                                    // establish an aligned snapshot argument frame
    emitter.instruction("stp x0, x1, [sp, #48]");                               // stage the concrete array tag and opaque payload identity
    emitter.instruction("str xzr, [sp, #64]");                                  // clear the unused root high word
    emitter.instruction("stp xzr, xzr, [sp, #80]");                             // default failed snapshot output to absent wire bytes
    emitter.instruction("add x0, sp, #48");                                     // pass the borrowed root descriptor
    abi::emit_symbol_address(emitter, "x1", "__rt_mbstring_array_next");
    emitter.instruction("mov x2, xzr");                                         // the native reader needs no external context
    emitter.instruction("mov x3, sp");                                          // pass owned bridge result storage
    emitter.bl_c("elephc_mbstring_snapshot_v1");
    emitter.instruction("ldr x9, [sp]");                                        // inspect snapshot success before borrowing its packed data
    emitter.instruction(&format!("cmp x9, #{}", RESULT_ARRAY));                 // require a complete validated graph result
    emitter.instruction("b.ne __rt_mbstring_array_argument_release");           // retain absent bytes on explicit snapshot failure
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // borrow the complete packed graph byte range
    emitter.instruction("bl __rt_str_persist");                                 // copy the graph into one runtime-owned wire string
    emitter.instruction("stp x1, x2, [sp, #80]");                               // save the transferred argument storage across Rust release
    emitter.label("__rt_mbstring_array_argument_release");
    emitter.instruction("mov x0, sp");                                          // pass the snapshot result to its owning allocator
    emitter.bl_c("elephc_mbstring_release_v1");
    emitter.instruction("ldp x1, x2, [sp, #80]");                               // return owned wire bytes or an invalid empty array payload
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore the PHP caller frame and return address
    emitter.instruction("add sp, sp, #112");                                    // release staging after all bridge buffers have been reclaimed
    emitter.instruction("ret");                                                 // transfer the packed argument owner in the string result pair
    emitter.label_global("__rt_mbstring_release_array_arguments");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the mutable slot cursor, count, and caller linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the status adapter linkage across releases
    emitter.instruction("add x29, sp, #16");                                    // establish the argument-consumer frame
    emitter.instruction("stp x0, x1, [sp]");                                    // save the slot cursor and remaining argument count
    emitter.label("__rt_mbstring_release_array_arguments_loop");
    emitter.instruction("ldp x9, x10, [sp]");                                   // load the next borrowed slot and remaining count
    emitter.instruction("cbz x10, __rt_mbstring_release_array_arguments_done"); // finish after every supplied argument has been inspected
    emitter.instruction("ldr x11, [x9]");                                       // read the wire argument kind
    emitter.instruction(&format!("cmp x11, #{}", ARG_ARRAY));                   // only array wire strings transfer ownership to this adapter
    emitter.instruction("b.ne __rt_mbstring_release_array_arguments_next");     // leave ordinary borrowed string arguments with their caller
    emitter.instruction("ldr x0, [x9, #16]");                                   // load the owned packed graph string
    emitter.instruction("stp xzr, xzr, [x9, #16]");                             // clear consumed storage before an eval caller performs general cleanup
    emitter.instruction("bl __rt_decref_any");                                  // release the sole temporary wire owner after the engine returned
    emitter.label("__rt_mbstring_release_array_arguments_next");
    emitter.instruction("ldp x9, x10, [sp]");                                   // restore loop state after optional release
    emitter.instruction("add x9, x9, #32");                                     // advance to the next wire slot
    emitter.instruction("sub x10, x10, #1");                                    // consume one supplied argument
    emitter.instruction("stp x9, x10, [sp]");                                   // persist the remaining cleanup work
    emitter.instruction("b __rt_mbstring_release_array_arguments_loop");        // inspect the remaining argument owners
    emitter.label("__rt_mbstring_release_array_arguments_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore status-adapter linkage
    emitter.instruction("add sp, sp, #32");                                     // release the argument-consumer frame
    emitter.instruction("ret");                                                 // return after all transferred wire owners are released
}

/// Copies x86_64 native graphs into owned argument bytes and consumes their owners after calls.
fn x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_array_argument");
    emitter.instruction("push rbp");                                            // preserve the PHP caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned snapshot argument frame
    emitter.instruction("sub rsp, 96");                                         // reserve the C result, root descriptor, and owned wire bytes
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // stage the concrete array tag
    emitter.instruction("mov QWORD PTR [rsp + 56], rdi");                       // stage the opaque native array identity
    emitter.instruction("mov QWORD PTR [rsp + 64], 0");                         // clear the unused root high word
    emitter.instruction("mov QWORD PTR [rsp + 80], 0");                         // default failed snapshot output to absent wire storage
    emitter.instruction("mov QWORD PTR [rsp + 88], 0");                         // default failed snapshot output to zero bytes
    emitter.instruction("lea rdi, [rsp + 48]");                                 // pass the borrowed root descriptor
    abi::emit_symbol_address(emitter, "rsi", "__rt_mbstring_array_next");
    emitter.instruction("xor edx, edx");                                        // the native reader needs no external context
    emitter.instruction("mov rcx, rsp");                                        // pass owned bridge result storage
    emitter.bl_c("elephc_mbstring_snapshot_v1");
    emitter.instruction(&format!("cmp QWORD PTR [rsp], {}", RESULT_ARRAY));     // require a complete validated graph result
    emitter.instruction("jne __rt_mbstring_array_argument_release");            // retain absent bytes on explicit snapshot failure
    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                       // borrow the packed graph bytes
    emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");                       // borrow the complete packed graph length
    emitter.instruction("call __rt_str_persist");                               // copy the graph into one runtime-owned wire string
    emitter.instruction("mov QWORD PTR [rsp + 80], rax");                       // save the transferred argument pointer across Rust release
    emitter.instruction("mov QWORD PTR [rsp + 88], rdx");                       // save its complete byte length
    emitter.label("__rt_mbstring_array_argument_release");
    emitter.instruction("mov rdi, rsp");                                        // pass the snapshot result to its owning allocator
    emitter.bl_c("elephc_mbstring_release_v1");
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // return owned wire bytes or an invalid absent payload
    emitter.instruction("mov rdx, QWORD PTR [rsp + 88]");                       // return the packed byte length
    emitter.instruction("mov rsp, rbp");                                        // release staging after all bridge buffers have been reclaimed
    emitter.instruction("pop rbp");                                             // restore the PHP caller frame pointer
    emitter.instruction("ret");                                                 // transfer the packed argument owner in the string result pair
    emitter.label_global("__rt_mbstring_release_array_arguments");
    emitter.instruction("push rbp");                                            // preserve the status adapter frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned argument-consumer frame
    emitter.instruction("sub rsp, 16");                                         // reserve the mutable slot cursor and remaining count
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // save the first supplied wire slot
    emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                        // save the supplied argument count
    emitter.label("__rt_mbstring_release_array_arguments_loop");
    emitter.instruction("cmp QWORD PTR [rsp + 8], 0");                          // test whether all supplied arguments have been inspected
    emitter.instruction("je __rt_mbstring_release_array_arguments_done");       // finish after consuming every transferred owner
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // load the next wire slot
    emitter.instruction(&format!("cmp QWORD PTR [r10], {}", ARG_ARRAY));        // only array wire strings transfer ownership to this adapter
    emitter.instruction("jne __rt_mbstring_release_array_arguments_next");      // leave ordinary borrowed string arguments with their caller
    emitter.instruction("mov rax, QWORD PTR [r10 + 16]");                       // load the owned packed graph string
    emitter.instruction("mov QWORD PTR [r10 + 16], 0");                         // clear consumed storage before general eval argument cleanup
    emitter.instruction("mov QWORD PTR [r10 + 24], 0");                         // clear the consumed byte length
    emitter.instruction("call __rt_decref_any");                                // release the sole temporary wire owner after the engine returned
    emitter.label("__rt_mbstring_release_array_arguments_next");
    emitter.instruction("add QWORD PTR [rsp], 32");                             // advance to the next wire slot
    emitter.instruction("sub QWORD PTR [rsp + 8], 1");                          // consume one supplied argument
    emitter.instruction("jmp __rt_mbstring_release_array_arguments_loop");      // inspect the remaining argument owners
    emitter.label("__rt_mbstring_release_array_arguments_done");
    emitter.instruction("mov rsp, rbp");                                        // release argument-consumer state
    emitter.instruction("pop rbp");                                             // restore status-adapter linkage
    emitter.instruction("ret");                                                 // return after all transferred wire owners are released
}
