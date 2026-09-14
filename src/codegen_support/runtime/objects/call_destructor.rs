//! Purpose:
//! Emits the `__rt_call_object_destructor` runtime helper: given an object
//! pointer, it looks up the class's PHP `__destruct` method in the
//! `_class_destruct_ptrs` table (indexed by the object's runtime class_id) and
//! invokes it before the object's storage is released.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` (helper definition).
//! - `__rt_object_free_deep` calls `__rt_call_object_destructor` at the top of the
//!   deep-free path, so a destructor runs exactly once when refcount hits zero,
//!   before any property payloads are released.
//!
//! Key details:
//! - `$this` is passed in the first integer argument register and is borrowed by
//!   the callee (no incref/decref around the call), matching normal method ABI, so
//!   the call cannot double-free the receiver.
//! - An optional eval callback can claim runtime-generic objects that actually
//!   belong to eval-declared classes; when no callback is installed, the helper
//!   follows the original static destructor table path.
//! - Re-entrancy guard: before calling the destructor, bit 31 of the 32-bit
//!   refcount is set. A balanced `$tmp = $this;`/scope-exit inside the body then
//!   decrements from `0x8000_0001` back to `0x8000_0000` instead of reaching zero,
//!   so it cannot re-enter the free path and double-free the object. Resurrecting
//!   `$this` leaves ordinary owners after the protected boundary clears bit 31.
//! - Object kind-word bit 14 persists the destructor-called or failed-construction
//!   suppression state across GC; the deep-free caller preserves a receiver with remaining owners.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::abi;

/// Emits `__rt_call_object_destructor` for the active target.
/// Input: x0/rdi = object pointer (heap-backed, non-null, an object instance).
/// Output: none. Clobbers scratch registers; preserves the object pointer's
/// memory so the caller can continue the deep-free after the destructor returns.
pub(crate) fn emit_call_object_destructor(emitter: &mut Emitter) {
    emit_eval_throwable(emitter);
    emit_destructor_lifetime_boundary(emitter);
    if emitter.target.arch == Arch::X86_64 {
        emit_call_object_destructor_x86_64(emitter);
        return;
    }
    emit_call_object_destructor_aarch64(emitter);
}



/// Marks destructor invocation once; failed construction uses the same bit to suppress invocation.
fn emit_destructor_lifetime_boundary(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_call_object_destructor");
    if arm {
        emitter.instruction("cbz x0, __rt_object_destructor_boundary_ret");     // reject a missing receiver before probing its header
        emitter.instruction("ldr x9, [x0, #-8]");                               // inspect persistent object destructor state
        emitter.instruction("tbnz x9, #14, __rt_object_destructor_boundary_ret");// skip a completed destructor or failed construction permanently
        emitter.instruction("orr x9, x9, #0x4000");                             // record invocation before any user callback or throwable
        emitter.instruction("str x9, [x0, #-8]");                               // preserve called state in the low sixteen kind-word bits across GC
        emitter.instruction("b __rt_call_object_destructor_body");              // let the protected caller finish temporary release state
    } else {
        emitter.instruction("test rdi, rdi");                                   // reject a missing receiver before reading object metadata
        emitter.instruction("jz __rt_object_destructor_boundary_ret");          // return immediately for a null receiver
        emitter.instruction("test QWORD PTR [rdi - 8], 0x4000");                // inspect persistent destructor-called or suppression state
        emitter.instruction("jnz __rt_object_destructor_boundary_ret");         // completed and failed construction must not invoke PHP destruction
        emitter.instruction("or QWORD PTR [rdi - 8], 0x4000");                  // record invocation before a callback can escape
        emitter.instruction("jmp __rt_call_object_destructor_body");            // reuse the existing protected caller and native/eval dispatch
    }
    emitter.label("__rt_object_destructor_boundary_ret");
    emitter.instruction("ret");                                                 // leave reclamation to current owners and the collector's fresh root scan
}

/// Emits the ARM64 `__rt_call_object_destructor` helper.
fn emit_call_object_destructor_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: call_object_destructor ---");
    emitter.label_global("__rt_call_object_destructor_body");

    emitter.instruction("cbz x0, __rt_call_object_destructor_ret");             // null receiver → nothing to destruct
    emitter.instruction("ldr w9, [x0, #-12]");                                  // w9 = object refcount (header offset -12)
    emitter.instruction("tbnz w9, #31, __rt_call_object_destructor_ret");       // destruction already in progress → never run twice
    abi::emit_symbol_address(emitter, "x10", "_elephc_eval_dynamic_object_destruct_fn");
    emitter.instruction("ldr x10, [x10]");                                      // x10 = optional eval dynamic destructor callback
    emitter.instruction("cbz x10, __rt_call_object_destructor_static");         // no eval callback installed → use static class table
    emitter.instruction("movz w12, #0x8000, lsl #16");                          // w12 = 0x80000000, the destruction-in-progress flag bit
    emitter.instruction("orr w9, w9, w12");                                     // mark destruction in progress before boxing borrowed $this
    emitter.instruction("str w9, [x0, #-12]");                                  // persist the guard flag in the refcount field
    emitter.instruction("sub sp, sp, #32");                                     // allocate an aligned frame for the eval callback
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address before the Rust call
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer across the callback
    emitter.instruction("add x1, sp, #8");                                      // provide owned Throwable output storage to the eval callback
    emitter.instruction("blr x10");                                             // ask eval whether it owns and destructed this object
    emitter.instruction("mov x12, x0");                                         // preserve the eval callback handled flag
    emitter.instruction("ldr x13, [sp, #8]");                                   // retain any escaping boxed Throwable before dropping the callback frame
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the object pointer after the callback
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the eval callback frame
    emitter.instruction("cmp x12, #2");                                         // distinguish a returned exception owner from successful destruction
    emitter.instruction("b.eq __rt_call_object_destructor_eval_throw");         // unwind only after the Rust destructor callback has returned
    emitter.instruction("cbnz x12, __rt_call_object_destructor_ret");           // eval handled the dynamic object → skip static lookup
    emitter.instruction("ldr w9, [x0, #-12]");                                  // reload the refcount after an eval miss
    emitter.instruction("movz w12, #0x8000, lsl #16");                          // w12 = destruction-in-progress flag bit
    emitter.instruction("bic w9, w9, w12");                                     // clear the temporary eval guard before static lookup
    emitter.instruction("str w9, [x0, #-12]");                                  // persist the restored refcount guard state
    emitter.instruction("b __rt_call_object_destructor_static");                // continue ordinary lookup after restoring a missed eval guard
    emitter.label("__rt_call_object_destructor_eval_throw");
    emitter.instruction("mov x0, x13");                                         // transfer the callback-owned boxed Throwable to native publication
    emitter.instruction("b __rt_destructor_throw_mixed");                       // consume the box and enter the enclosing native cleanup handler
    emitter.label("__rt_call_object_destructor_static");
    emitter.instruction("ldr x11, [x0]");                                       // x11 = runtime class_id (object payload offset 0)
    // emit_load_symbol_to_reg uses x9 as scratch, so class_id is kept in x11.
    crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "x10", "_class_destruct_count", 0);
    emitter.instruction("cmp x11, x10");                                        // is class_id within the destructor table?
    emitter.instruction("b.hs __rt_call_object_destructor_ret");                // out-of-range class ids have no destructor
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_class_destruct_ptrs");
    emitter.instruction("ldr x10, [x10, x11, lsl #3]");                         // x10 = destructor symbol for this class (or 0)
    emitter.instruction("cbz x10, __rt_call_object_destructor_ret");            // class defines no __destruct → done
    emitter.instruction("ldr w9, [x0, #-12]");                                  // w9 = object refcount (header offset -12)
    emitter.instruction("movz w12, #0x8000, lsl #16");                          // w12 = 0x80000000, the destruction-in-progress flag bit
    emitter.instruction("orr w9, w9, w12");                                     // mark destruction in progress so a balanced self-ref cannot re-enter the free path
    emitter.instruction("str w9, [x0, #-12]");                                  // persist the guard flag in the refcount field
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address before the user call
    emitter.instruction("mov x29, sp");                                         // establish the helper frame
    emitter.instruction("blr x10");                                             // invoke <class>::__destruct with x0 = $this (borrowed)
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address

    emitter.label("__rt_call_object_destructor_ret");
    emitter.instruction("ret");                                                 // return to __rt_object_free_deep to release the storage
}

/// Emits the x86_64 `__rt_call_object_destructor` helper.
fn emit_call_object_destructor_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: call_object_destructor ---");
    emitter.label_global("__rt_call_object_destructor_body");

    emitter.instruction("test rdi, rdi");                                       // null receiver → nothing to destruct
    emitter.instruction("jz __rt_call_object_destructor_ret");                  // skip the lookup for a null object
    emitter.instruction("mov eax, DWORD PTR [rdi - 12]");                       // eax = object refcount (header offset -12)
    emitter.instruction("test eax, 0x80000000");                                // is destruction already in progress?
    emitter.instruction("jnz __rt_call_object_destructor_ret");                 // never run a destructor twice
    abi::emit_symbol_address(emitter, "r10", "_elephc_eval_dynamic_object_destruct_fn");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // r10 = optional eval dynamic destructor callback
    emitter.instruction("test r10, r10");                                       // is the eval callback installed?
    emitter.instruction("jz __rt_call_object_destructor_static_x86");           // no eval callback installed → use static class table
    emitter.instruction("or eax, 0x80000000");                                  // mark destruction in progress before boxing borrowed $this
    emitter.instruction("mov DWORD PTR [rdi - 12], eax");                       // persist the guard flag in the refcount field
    emitter.instruction("push rbp");                                            // align the stack and save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the eval callback frame
    emitter.instruction("sub rsp, 16");                                         // reserve a spill slot for the object pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the object pointer across the callback
    emitter.instruction("lea rsi, [rbp - 16]");                                 // provide a boxed Throwable output slot for the Rust callback
    emitter.instruction("call r10");                                            // ask eval whether it owns and destructed this object
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // preserve any returned Throwable owner across callback frame teardown
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the object pointer after the callback
    emitter.instruction("add rsp, 16");                                         // release the eval callback spill slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("cmp rax, 2");                                          // detect a PHP exception transferred by the completed Rust callback
    emitter.instruction("je __rt_call_object_destructor_eval_throw");           // publish and propagate only after returning to native code
    emitter.instruction("test rax, rax");                                       // did eval handle this dynamic object?
    emitter.instruction("jnz __rt_call_object_destructor_ret");                 // eval handled the dynamic object → skip static lookup
    emitter.instruction("mov eax, DWORD PTR [rdi - 12]");                       // reload the refcount after an eval miss
    emitter.instruction("and eax, 0x7fffffff");                                 // clear the temporary eval guard before static lookup
    emitter.instruction("mov DWORD PTR [rdi - 12], eax");                       // persist the restored refcount guard state
    emitter.instruction("jmp __rt_call_object_destructor_static_x86");          // continue static lookup after restoring a missed eval guard
    emitter.label("__rt_call_object_destructor_eval_throw");
    emitter.instruction("mov rax, r11");                                        // transfer the owned boxed Throwable through the native unary convention
    emitter.instruction("jmp __rt_destructor_throw_mixed");                     // consume the returned owner inside the nearest native exception boundary
    emitter.label("__rt_call_object_destructor_static_x86");
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // rax = runtime class_id (object payload offset 0)
    abi::emit_cmp_reg_to_symbol(emitter, "rax", "_class_destruct_count");       // is class_id within the destructor table?
    emitter.instruction("jae __rt_call_object_destructor_ret");                 // out-of-range class ids have no destructor
    abi::emit_symbol_address(emitter, "r10", "_class_destruct_ptrs");           // r10 = base of the per-class destructor symbol table
    emitter.instruction("mov r10, QWORD PTR [r10 + rax * 8]");                  // r10 = destructor symbol for this class (or 0)
    emitter.instruction("test r10, r10");                                       // class defines no __destruct?
    emitter.instruction("jz __rt_call_object_destructor_ret");                  // nothing to call → done
    emitter.instruction("mov eax, DWORD PTR [rdi - 12]");                       // eax = object refcount (header offset -12)
    emitter.instruction("or eax, 0x80000000");                                  // mark destruction in progress so a balanced self-ref cannot re-enter the free path
    emitter.instruction("mov DWORD PTR [rdi - 12], eax");                       // persist the guard flag in the refcount field
    emitter.instruction("push rbp");                                            // align the stack and save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("call r10");                                            // invoke <class>::__destruct with rdi = $this (borrowed)
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer

    emitter.label("__rt_call_object_destructor_ret");
    emitter.instruction("ret");                                                 // return to __rt_object_free_deep to release the storage
}

/// Publishes an owned eval Throwable box before entering the native exception unwinder.
fn emit_eval_throwable(emitter: &mut Emitter) {
    emitter.label_global("__rt_destructor_throw_mixed");
    if emitter.target.arch == Arch::AArch64 {
    emitter.instruction("sub sp, sp, #32");                                     // reserve boxed ownership and native linkage for exception publication
    emitter.instruction("stp x29, x30, [sp, #16]");                             // retain the native cleanup caller while transferring Throwable ownership
    emitter.instruction("add x29, sp, #16");                                    // establish the native publication frame
    emitter.instruction("str x0, [sp]");                                        // preserve the sole boxed Throwable owner
    emitter.instruction("bl __rt_mixed_unbox");                                 // resolve the concrete object payload before acquiring its pending owner
    emitter.instruction("mov x0, x1");                                          // adapt the raw Throwable payload to the retain convention
    emitter.instruction("bl __rt_incref");                                      // retain the object independently from its boxed callback owner
        abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);
    emitter.instruction("ldr x0, [sp]");                                        // recover the callback box after publishing raw Throwable ownership
    emitter.instruction("bl __rt_decref_mixed");                                // consume the box while its object remains owned by the pending slot
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the native caller before propagating the exception
    emitter.instruction("add sp, sp, #32");                                     // release publication storage after ownership transfer
    emitter.instruction("b __rt_throw_current");                                // enter the cleanup handler without any remaining Rust frame
    } else {
    emitter.instruction("push rbp");                                            // preserve native linkage and align the publication calls
    emitter.instruction("mov rbp, rsp");                                        // establish the Throwable publication frame
    emitter.instruction("sub rsp, 16");                                         // reserve the callback box across native unboxing and retention
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // retain the owned box while its object gains independent ownership
    emitter.instruction("call __rt_mixed_unbox");                               // resolve the concrete Throwable object payload
    emitter.instruction("mov rax, rdi");                                        // pass the unboxed object to the native retain helper
    emitter.instruction("call __rt_incref");                                    // acquire the object owner transferred to pending exception storage
        abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // recover the boxed callback owner after publishing its object
    emitter.instruction("call __rt_decref_mixed");                              // consume the box without releasing the still-pending Throwable
    emitter.instruction("leave");                                               // restore native linkage after the ownership transfer is complete
    emitter.instruction("jmp __rt_throw_current");                              // propagate through the enclosing protected cleanup handler
    }
}
