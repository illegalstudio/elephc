//! Purpose:
//! Emits the `__rt_call_object_destructor` runtime helper: given an object
//! pointer, it looks up the class's PHP `__destruct` method in the
//! `_class_destruct_ptrs` table (indexed by the object's runtime class_id) and
//! invokes it before the object's storage is released.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` (helper definition).
//! - `__rt_object_free_deep` calls `__rt_call_object_destructor` at the top of the
//!   deep-free path, so a destructor runs exactly once when refcount hits zero,
//!   before any property payloads are released.
//!
//! Key details:
//! - `$this` is passed in the first integer argument register and is borrowed by
//!   the callee (no incref/decref around the call), matching normal method ABI, so
//!   the call cannot double-free the receiver.
//! - Re-entrancy guard: before calling the destructor, bit 31 of the 32-bit
//!   refcount is set. A balanced `$tmp = $this;`/scope-exit inside the body then
//!   decrements from `0x8000_0001` back to `0x8000_0000` instead of reaching zero,
//!   so it cannot re-enter the free path and double-free the object. Resurrecting
//!   `$this` (storing it to outlive the destructor) is unsupported: the object is
//!   still freed.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_call_object_destructor` for the active target.
/// Input: x0/rdi = object pointer (heap-backed, non-null, an object instance).
/// Output: none. Clobbers scratch registers; preserves the object pointer's
/// memory so the caller can continue the deep-free after the destructor returns.
pub(crate) fn emit_call_object_destructor(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_call_object_destructor_x86_64(emitter);
        return;
    }
    emit_call_object_destructor_aarch64(emitter);
}

/// Emits the ARM64 `__rt_call_object_destructor` helper.
fn emit_call_object_destructor_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: call_object_destructor ---");
    emitter.label_global("__rt_call_object_destructor");

    emitter.instruction("cbz x0, __rt_call_object_destructor_ret");             // null receiver → nothing to destruct
    emitter.instruction("ldr x11, [x0]");                                       // x11 = runtime class_id (object payload offset 0)
    // emit_load_symbol_to_reg uses x9 as scratch, so class_id is kept in x11.
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "x10", "_class_destruct_count", 0);
    emitter.instruction("cmp x11, x10");                                        // is class_id within the destructor table?
    emitter.instruction("b.hs __rt_call_object_destructor_ret");                // out-of-range class ids have no destructor
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_class_destruct_ptrs");
    emitter.instruction("ldr x10, [x10, x11, lsl #3]");                         // x10 = destructor symbol for this class (or 0)
    emitter.instruction("cbz x10, __rt_call_object_destructor_ret");            // class defines no __destruct → done
    emitter.instruction("ldr w9, [x0, #-12]");                                  // w9 = object refcount (header offset -12)
    emitter.instruction("tbnz w9, #31, __rt_call_object_destructor_ret");       // destruction already in progress → never run twice
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
    emitter.label_global("__rt_call_object_destructor");

    emitter.instruction("test rdi, rdi");                                       // null receiver → nothing to destruct
    emitter.instruction("jz __rt_call_object_destructor_ret");                  // skip the lookup for a null object
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // rax = runtime class_id (object payload offset 0)
    emitter.instruction("cmp rax, QWORD PTR [rip + _class_destruct_count]");    // is class_id within the destructor table?
    emitter.instruction("jae __rt_call_object_destructor_ret");                 // out-of-range class ids have no destructor
    emitter.instruction("lea r10, [rip + _class_destruct_ptrs]");               // r10 = base of the per-class destructor symbol table
    emitter.instruction("mov r10, QWORD PTR [r10 + rax * 8]");                  // r10 = destructor symbol for this class (or 0)
    emitter.instruction("test r10, r10");                                       // class defines no __destruct?
    emitter.instruction("jz __rt_call_object_destructor_ret");                  // nothing to call → done
    emitter.instruction("mov eax, DWORD PTR [rdi - 12]");                       // eax = object refcount (header offset -12)
    emitter.instruction("test eax, 0x80000000");                                // is destruction already in progress?
    emitter.instruction("jnz __rt_call_object_destructor_ret");                 // never run a destructor twice
    emitter.instruction("or eax, 0x80000000");                                  // mark destruction in progress so a balanced self-ref cannot re-enter the free path
    emitter.instruction("mov DWORD PTR [rdi - 12], eax");                       // persist the guard flag in the refcount field
    emitter.instruction("push rbp");                                            // align the stack and save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("call r10");                                            // invoke <class>::__destruct with rdi = $this (borrowed)
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer

    emitter.label("__rt_call_object_destructor_ret");
    emitter.instruction("ret");                                                 // return to __rt_object_free_deep to release the storage
}
