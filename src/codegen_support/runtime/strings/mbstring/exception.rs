//! Purpose:
//! Materializes complete shared mbstring error chains into native PHP Throwables.
//!
//! Called from:
//! - Wire-result materialization after the shared operation engine returns.
//!
//! Key details:
//! - Borrowed records are validated in Rust and copied before bridge ownership is released.
//! - Each new Throwable consumes the preceding chain; no PHP unwinding crosses Rust.
//! - Both native architectures publish only after every exception is constructed.

use super::*;
use elephc_builtin_contract::mbstring_abi::coercion::PREPARED_ARGUMENT_COUNT_ERROR;

/// Emits a borrowing C result adapter preserving complete native exception chains.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_exception_chain");
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter); } else { x86_64(emitter); }
}

/// Maps validated wire classes to their program-specific Throwable identities.
fn classes() -> [(u64, &'static str); 4] {
    [(RESULT_VALUE_ERROR, "_spl_value_error_class_id"), (RESULT_TYPE_ERROR, "_spl_type_error_class_id"),
        (RESULT_ERROR, "_spl_error_class_id"), (PREPARED_ARGUMENT_COUNT_ERROR, "_spl_argument_count_error_class_id")]
}

/// Builds AArch64 Throwables oldest first while retaining all temporary ownership in the frame.
fn aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #96");                                     // reserve borrowed input, record view, partial chain, message, and linkage
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the Rust or native caller across allocation
    emitter.instruction("add x29, sp, #80");                                    // establish an aligned exception materialization frame
    emitter.instruction("stp x0, xzr, [sp]");                                   // retain the wire result and initialize the record index
    emitter.instruction("str xzr, [sp, #16]");                                  // start without an owned previous Throwable
    emitter.label("__rt_mbstring_exception_next");
    emitter.instruction("ldp x0, x1, [sp]");                                    // borrow the original result and select the next oldest-first record
    emitter.instruction("add x2, sp, #24");                                     // provide a separate class/message/length record descriptor
    emitter.bl_c("elephc_mbstring_exception_at_v1");
    emitter.instruction("cbz w0, __rt_mbstring_exception_publish");             // publish only at the validated exact end of the chain
    emitter.instruction("cmp w0, #1");                                          // require one complete borrowed record
    emitter.instruction("b.ne __rt_mbstring_exception_invalid");                // reject malformed chains without publishing partial objects
    emitter.instruction("ldr x10, [sp, #24]");                                  // inspect the record's validated PHP exception class
    for (kind, _) in classes() {
        emitter.instruction(&format!("cmp x10, #{kind}"));                      // select a per-program class identity from the wire kind
        emitter.instruction(&format!("b.eq __rt_mbstring_exception_class_{kind}")); // share construction after resolving this class
    }
    emitter.instruction("b __rt_mbstring_exception_invalid");                   // fail closed if the decoder and native class inventory disagree
    for (kind, symbol) in classes() {
        emitter.label(&format!("__rt_mbstring_exception_class_{kind}"));
        abi::emit_load_symbol_to_reg(emitter, "x9", symbol, 0);
        emitter.instruction("str x9, [sp, #48]");                               // retain the resolved class across string and object allocation
        emitter.instruction("b __rt_mbstring_exception_create");                // share message copying and ownership transfer
    }
    emitter.label("__rt_mbstring_exception_create");
    emitter.instruction("ldp x1, x2, [sp, #32]");                               // borrow the complete binary exception message
    emitter.instruction("bl __rt_str_persist");                                 // copy message bytes into native runtime ownership
    emitter.instruction("stp x1, x2, [sp, #56]");                               // retain the owned string while allocating its Throwable
    emitter.instruction("mov x0, #56");                                         // request the canonical compact Throwable payload
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate one object that will own the message and previous chain
    emitter.instruction("mov x9, #6");                                          // select the compact native object heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the allocation before registering its PHP handle
    emitter.instruction("bl __rt_object_handle_acquire");                       // assign the Throwable its native object identity
    emitter.instruction("ldr x9, [sp, #48]");                                   // restore the selected exception class
    emitter.instruction("str x9, [x0]");                                        // initialize its class identity before exposing the object
    emitter.instruction("ldp x9, x10, [sp, #56]");                              // recover the native-owned binary message
    emitter.instruction("stp x9, x10, [x0, #8]");                               // transfer message pointer and length into the Throwable
    emitter.instruction("str xzr, [x0, #24]");                                  // initialize the exception code to zero
    crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, "x0");
    emitter.instruction("ldr x9, [sp, #16]");                                   // consume the previously completed chain
    emitter.instruction("str x9, [x0, #40]");                                   // make the earlier error the new Throwable's previous owner
    emitter.instruction("str x0, [sp, #16]");                                   // retain the new complete chain until validated publication
    emitter.instruction("ldr x9, [sp, #8]");                                    // recover the completed record index
    emitter.instruction("add x9, x9, #1");                                      // advance after ownership has transferred successfully
    emitter.instruction("str x9, [sp, #8]");                                    // retain the next index across the Rust decoder call
    emitter.instruction("b __rt_mbstring_exception_next");                      // construct subsequent errors in oldest-first order
    emitter.label("__rt_mbstring_exception_publish");
    emitter.instruction("ldr x0, [sp, #16]");                                   // load the completed chain's latest Throwable
    emitter.instruction("cbz x0, __rt_mbstring_exception_invalid");             // an empty error result cannot publish a fabricated exception
    abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);
    emitter.instruction("mov x0, #2");                                          // report PendingThrowable without native unwinding
    emitter.instruction("b __rt_mbstring_exception_done");                      // return after transferring chain ownership to the runtime
    emitter.label("__rt_mbstring_exception_invalid");
    emitter.instruction("ldr x0, [sp, #16]");                                   // recover any partial native chain for balanced failure cleanup
    emitter.instruction("bl __rt_decref_any");                                  // release all compact error objects and their copied messages
    emitter.instruction("mov x0, #1");                                          // malformed transport reports a runtime fatal status
    emitter.label("__rt_mbstring_exception_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller after every native and Rust frame has returned
    emitter.instruction("add sp, sp, #96");                                     // release borrowed views and temporary ownership storage
    emitter.instruction("ret");                                                 // return only the C status without transferring bridge buffers
}

/// Builds identical SysV Throwable chains with aligned calls and the native string convention.
fn x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // preserve caller linkage and align allocation calls
    emitter.instruction("mov rbp, rsp");                                        // establish stable temporary ownership storage
    emitter.instruction("sub rsp, 80");                                         // reserve input, index, partial chain, record view, and owned message
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // retain the borrowed wire result across helper calls
    emitter.instruction("mov QWORD PTR [rsp + 8], 0");                          // initialize the oldest-first record index
    emitter.instruction("mov QWORD PTR [rsp + 16], 0");                         // start with no owned previous Throwable
    emitter.label("__rt_mbstring_exception_next");
    emitter.instruction("mov rdi, QWORD PTR [rsp]");                            // borrow the original complete wire result
    emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                        // select the next oldest-first exception record
    emitter.instruction("lea rdx, [rsp + 24]");                                 // provide independent borrowed class/message/length storage
    emitter.bl_c("elephc_mbstring_exception_at_v1");
    emitter.instruction("test eax, eax");                                       // recognize the validated exact end of the chain
    emitter.instruction("jz __rt_mbstring_exception_publish");                  // publish only after every record was constructed
    emitter.instruction("cmp eax, 1");                                          // require a complete borrowed exception record
    emitter.instruction("jne __rt_mbstring_exception_invalid");                 // reject malformed framing before publishing partial objects
    emitter.instruction("mov r11, QWORD PTR [rsp + 24]");                       // inspect the validated PHP exception kind
    for (kind, _) in classes() {
        emitter.instruction(&format!("cmp r11, {kind}"));                       // resolve the record's per-program exception class
        emitter.instruction(&format!("je __rt_mbstring_exception_class_{kind}")); // share construction after selecting this class
    }
    emitter.instruction("jmp __rt_mbstring_exception_invalid");                 // fail closed on a mismatched native class inventory
    for (kind, symbol) in classes() {
        emitter.label(&format!("__rt_mbstring_exception_class_{kind}"));
        abi::emit_load_symbol_to_reg(emitter, "r10", symbol, 0);
        emitter.instruction("mov QWORD PTR [rsp + 48], r10");                   // retain the class across native allocations
        emitter.instruction("jmp __rt_mbstring_exception_create");              // share binary copying and Throwable initialization
    }
    emitter.label("__rt_mbstring_exception_create");
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // borrow the complete binary message pointer
    emitter.instruction("mov rdx, QWORD PTR [rsp + 40]");                       // borrow its exact byte length
    emitter.instruction("call __rt_str_persist");                               // copy bytes into native runtime ownership
    emitter.instruction("mov QWORD PTR [rsp + 56], rax");                       // retain the native-owned message across object allocation
    emitter.instruction("mov QWORD PTR [rsp + 64], rdx");                       // retain its complete byte length
    emitter.instruction("mov rax, 56");                                         // request the canonical compact Throwable payload
    emitter.instruction("call __rt_heap_alloc");                                // allocate the owner of this message and previous chain
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(6))); // select the canonical compact object heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocation before binding its PHP identity
    emitter.instruction("call __rt_object_handle_acquire");                     // assign the Throwable a native object handle
    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");                       // restore the chosen per-program exception class
    emitter.instruction("mov QWORD PTR [rax], r10");                            // initialize class identity before publishing the object
    emitter.instruction("mov r10, QWORD PTR [rsp + 56]");                       // recover the native-owned binary message
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // transfer its pointer into the Throwable
    emitter.instruction("mov r10, QWORD PTR [rsp + 64]");                       // recover the exact message byte length
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // transfer complete binary message metadata
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // initialize the exception code to zero
    crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, "rax");
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // consume the earlier completed chain's sole owner
    emitter.instruction("mov QWORD PTR [rax + 40], r10");                       // retain every earlier exception as previous
    emitter.instruction("mov QWORD PTR [rsp + 16], rax");                       // retain this complete chain until final publication
    emitter.instruction("add QWORD PTR [rsp + 8], 1");                          // advance only after message and previous ownership transfer
    emitter.instruction("jmp __rt_mbstring_exception_next");                    // construct each later error in observable order
    emitter.label("__rt_mbstring_exception_publish");
    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                       // select the latest Throwable owning all earlier errors
    emitter.instruction("test rax, rax");                                       // reject an empty error result
    emitter.instruction("jz __rt_mbstring_exception_invalid");                  // avoid publishing a null pending Throwable
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
    emitter.instruction("mov eax, 2");                                          // report PendingThrowable without native unwinding
    emitter.instruction("jmp __rt_mbstring_exception_done");                    // return after native ownership publication
    emitter.label("__rt_mbstring_exception_invalid");
    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                       // recover any partial chain after a transport failure
    emitter.instruction("call __rt_decref_any");                                // balance all copied messages and compact Throwable owners
    emitter.instruction("mov eax, 1");                                          // report malformed transport as a runtime fatal status
    emitter.label("__rt_mbstring_exception_done");
    emitter.instruction("leave");                                               // release temporary views and restore the original caller frame
    emitter.instruction("ret");                                                 // return only the C status while the caller retains wire ownership
}
