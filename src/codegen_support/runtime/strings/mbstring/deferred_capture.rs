//! Purpose:
//! Keeps values overwritten by reentrant capture initialization alive through the PHP request.
//!
//! Called from:
//! - Native capture invocation after coordinator cleanup and mbstring request teardown.
//!
//! Key details:
//! - Adoption transfers one existing boxed owner without copying its PHP value.
//! - Raw queue nodes are not PHP graph edges, so retained values remain external GC roots.
//! - Draining detaches each node before PHP callbacks and includes newly adopted values.
//! - Queue order is adoption order; PHP object-store shutdown ordering is a separate concern.

use super::*;
use crate::codegen_support::runtime::exceptions::deep_cleanup::Scope;

const HEAD: &str = "_mbstring_deferred_capture_head";
const TAIL: &str = "_mbstring_deferred_capture_tail";
const ACTIVE: &str = "_mbstring_deferred_capture_draining";
const ADOPT: &str = "__rt_mbstring_defer_capture";
const DRAIN: &str = "__rt_mbstring_release_deferred_captures";
const CLEANUP: Scope = Scope { arm: 16, x86: 32 };

/// Emits native unary ownership adoption and a resumable request-end drain returning a pending flag.
pub(super) fn emit(emitter: &mut Emitter) {
    adopt(emitter);
    drain(emitter);
}

/// Appends a nonnull boxed owner without changing the value's refcount or invoking PHP code.
fn adopt(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let result = if arm { "x0" } else { "rax" };
    let scratch = if arm { "x9" } else { "r10" };
    emitter.label_global(ADOPT);
    if arm {
        emitter.instruction(&format!("cbz x0, {ADOPT}_done"));                  // null transfers no owner and needs no queue node
        emitter.instruction("stp x29, x30, [sp, #-32]!");                       // preserve linkage and one incoming owner slot
        emitter.instruction("mov x29, sp");                                     // keep an aligned allocation frame
        emitter.instruction("str x0, [sp, #16]");                               // retain the transferred boxed pointer across allocation
        emitter.instruction("mov x0, #16");                                     // allocate a raw next-pointer and boxed-owner pair
    } else {
        emitter.instruction("test rax, rax");                                   // distinguish absence from a transferred boxed value
        emitter.instruction(&format!("jz {ADOPT}_done"));                       // omit empty queue entries
        emitter.instruction("push rbp");                                        // preserve linkage and align allocator calls
        emitter.instruction("mov rbp, rsp");                                    // establish the ownership-adoption frame
        emitter.instruction("sub rsp, 16");                                     // reserve the incoming pointer and alignment padding
        emitter.instruction("mov QWORD PTR [rsp], rax");                        // retain the owner without increasing its PHP refcount
        emitter.instruction("mov eax, 16");                                     // allocate two raw pointer words
    }
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    if arm {
        emitter.instruction("str xzr, [x0]");                                   // terminate the newly allocated queue node
        emitter.instruction("ldr x9, [sp, #16]");                               // recover the transferred boxed owner
        emitter.instruction("str x9, [x0, #8]");                                // keep one external owner until request teardown
    } else {
        emitter.instruction("mov QWORD PTR [rax], 0");                          // terminate the new raw queue node
        emitter.instruction("mov r10, QWORD PTR [rsp]");                        // recover the incoming owner after allocation
        emitter.instruction("mov QWORD PTR [rax + 8], r10");                    // retain the boxed pointer as an external GC root
    }
    abi::emit_load_symbol_to_reg(emitter, scratch, TAIL, 0);
    if arm {
        emitter.instruction(&format!("cbz x9, {ADOPT}_first"));                 // initialize the head when no earlier node remains
        emitter.instruction("str x0, [x9]");                                    // append after every previously adopted owner
        emitter.instruction(&format!("b {ADOPT}_tail"));                        // publish the new tail after linking its predecessor
    } else {
        emitter.instruction("test r10, r10");                                   // inspect the current queue tail
        emitter.instruction(&format!("jz {ADOPT}_first"));                      // initialize an empty queue
        emitter.instruction("mov QWORD PTR [r10], rax");                        // append without inspecting the contained PHP value
        emitter.instruction(&format!("jmp {ADOPT}_tail"));                      // retain the existing queue head
    }
    emitter.label(&format!("{ADOPT}_first"));
    abi::emit_store_reg_to_symbol(emitter, result, HEAD, 0);
    emitter.label(&format!("{ADOPT}_tail"));
    abi::emit_store_reg_to_symbol(emitter, result, TAIL, 0);
    if arm {
        emitter.instruction("ldp x29, x30, [sp], #32");                         // restore linkage after ownership publication
    } else {
        emitter.instruction("leave");                                           // restore the caller's stack and frame
    }
    emitter.label(&format!("{ADOPT}_done"));
    emitter.instruction("ret");                                                 // return without releasing or cloning the adopted value
}

/// Releases all adopted roots while keeping PHP exceptions contained until the queue is empty.
fn drain(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let scratch = if arm { "x9" } else { "r10" };
    emitter.label_global(DRAIN);
    abi::emit_load_symbol_to_reg(emitter, scratch, ACTIVE, 0);
    if arm {
        emitter.instruction(&format!("cbnz x9, {DRAIN}_nested"));               // let the enclosing drain consume owners adopted by reentrant callbacks
        emitter.instruction("sub sp, sp, #64");                                 // reserve detached ownership, cleanup state, and linkage
        emitter.instruction("stp x29, x30, [sp, #48]");                         // preserve linkage across destructor callbacks
        emitter.instruction("add x29, sp, #48");                                // expose the aligned cleanup frame
        emitter.instruction("mov x9, #1");                                      // publish a single active drain per request
    } else {
        emitter.instruction("test r10, r10");                                   // detect request-reset reentry from a destructor
        emitter.instruction(&format!("jnz {DRAIN}_nested"));                    // retain traversal in the already active frame
        emitter.instruction("push rbp");                                        // preserve linkage and align callback calls
        emitter.instruction("mov rbp, rsp");                                    // establish the cleanup frame
        emitter.instruction("sub rsp, 48");                                     // reserve detached ownership and the protected cleanup record
        emitter.instruction("mov r10, 1");                                      // mark the outermost drain active
    }
    abi::emit_store_reg_to_symbol(emitter, scratch, ACTIVE, 0);
    CLEANUP.begin(emitter);
    emitter.label(&format!("{DRAIN}_loop"));
    abi::emit_load_symbol_to_reg(emitter, scratch, HEAD, 0);
    if arm {
        emitter.instruction(&format!("cbz x9, {DRAIN}_finish"));                // finish only after callback-created entries have also been consumed
        emitter.instruction("ldr x10, [x9, #8]");                               // retain this node's boxed owner outside the queue
        emitter.instruction("str x10, [sp]");                                   // preserve the owner while reclaiming the raw node
        emitter.instruction("mov x0, x9");                                      // pass the detached node to the native allocator
        emitter.instruction("ldr x9, [x9]");                                    // recover the next node before freeing the current allocation
    } else {
        emitter.instruction("test r10, r10");                                   // inspect whether any queued owner remains
        emitter.instruction(&format!("jz {DRAIN}_finish"));                     // leave only after draining reentrant additions
        emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                    // take the node's boxed owner without decrementing it
        emitter.instruction("mov QWORD PTR [rsp], r11");                        // preserve its sole request owner across raw-node reclamation
        emitter.instruction("mov rax, r10");                                    // pass the queue allocation to the native free helper
        emitter.instruction("mov r10, QWORD PTR [r10]");                        // retain the next node before returning this block to the allocator
    }
    abi::emit_store_reg_to_symbol(emitter, scratch, HEAD, 0);
    if arm {
        emitter.instruction(&format!("cbnz x9, {DRAIN}_release"));              // preserve the tail of a nonempty queue
    } else {
        emitter.instruction("test r10, r10");                                   // test the published successor
        emitter.instruction(&format!("jnz {DRAIN}_release"));                   // retain an existing tail when more nodes remain
    }
    abi::emit_store_zero_to_symbol(emitter, TAIL, 0);
    emitter.label(&format!("{DRAIN}_release"));
    abi::emit_call_label(emitter, "__rt_heap_free");
    emitter.instruction(if arm { "ldr x0, [sp]" } else { "mov rax, QWORD PTR [rsp]" }); // recover the detached owner after freeing its queue node
    CLEANUP.call(emitter, "__rt_decref_any", false);
    emitter.instruction(&format!("{} {DRAIN}_loop", if arm { "b" } else { "jmp" })); // continue after contained destructor exceptions
    emitter.label(&format!("{DRAIN}_finish"));
    abi::emit_store_zero_to_symbol(emitter, ACTIVE, 0);
    CLEANUP.finish(emitter);
    if arm {
        emitter.instruction("ldp x29, x30, [sp, #48]");                         // restore linkage without changing the pending flag
        emitter.instruction("add sp, sp, #64");                                 // retire the cleanup record after every queued owner is released
    } else {
        emitter.instruction("leave");                                           // restore caller linkage while preserving the pending flag
    }
    emitter.instruction("ret");                                                 // let the lifecycle caller propagate failure after its remaining cleanup
    emitter.label(&format!("{DRAIN}_nested"));
    emitter.instruction(if arm { "mov x0, #0" } else { "xor eax, eax" });       // nested drains leave completion and exceptions to the active owner
    emitter.instruction("ret");                                                 // return without disturbing active traversal or GC suppression
}
