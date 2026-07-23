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
//!   `$this` (storing it to outlive the destructor) is unsupported: the object is
//!   still freed.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::abi;

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
    emitter.instruction("blr x10");                                             // ask eval whether it owns and destructed this object
    emitter.instruction("mov x12, x0");                                         // preserve the eval callback handled flag
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the object pointer after the callback
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the eval callback frame
    emitter.instruction("cbnz x12, __rt_call_object_destructor_ret");           // eval handled the dynamic object → skip static lookup
    emitter.instruction("ldr w9, [x0, #-12]");                                  // reload the refcount after an eval miss
    emitter.instruction("movz w12, #0x8000, lsl #16");                          // w12 = destruction-in-progress flag bit
    emitter.instruction("bic w9, w9, w12");                                     // clear the temporary eval guard before static lookup
    emitter.instruction("str w9, [x0, #-12]");                                  // persist the restored refcount guard state
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
    emitter.label_global("__rt_call_object_destructor");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer for both callback paths
    emitter.instruction("mov rbp, rsp");                                        // establish one unwindable frame for the whole helper
    emitter.instruction("sub rsp, 16");                                         // reserve the eval callback object spill slot

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
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the object pointer across the callback
    emitter.emit_native_bridge_call("r10", 1);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the object pointer after the callback
    emitter.instruction("test rax, rax");                                       // did eval handle this dynamic object?
    emitter.instruction("jnz __rt_call_object_destructor_ret");                 // eval handled the dynamic object → skip static lookup
    emitter.instruction("mov eax, DWORD PTR [rdi - 12]");                       // reload the refcount after an eval miss
    emitter.instruction("and eax, 0x7fffffff");                                 // clear the temporary eval guard before static lookup
    emitter.instruction("mov DWORD PTR [rdi - 12], eax");                       // persist the restored refcount guard state
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
    emitter.emit_platform_callback_call("r10", 1);

    emitter.label("__rt_call_object_destructor_ret");
    emitter.instruction("leave");                                               // release the shared callback frame and restore rbp
    emitter.instruction("ret");                                                 // return to __rt_object_free_deep to release the storage
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies the windows-x86_64 `__rt_call_object_destructor` call site emits
    /// the reverse-ABI SysV->MSx64 remap immediately before the indirect
    /// `call r10` into the generated `__destruct` method (finding F1,
    /// reverse-ABI): without it, the generated destructor would read `$this`
    /// from the wrong register on windows-x86_64.
    #[test]
    fn test_windows_x86_64_call_object_destructor_remaps_before_indirect_call() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_call_object_destructor(&mut emitter);
        let asm = emitter.output();

        let remap_idx = asm.find("mov rcx, rdi").expect("expected SysV->MSx64 remap");
        let shadow_idx = asm
            .find("sub rsp, 32")
            .expect("expected MSx64 shadow space before the destructor call");
        let call_idx = asm.find("call r11").expect("expected relocated indirect call r11");
        assert!(
            shadow_idx < remap_idx && remap_idx < call_idx,
            "shadow reservation and remap must precede the indirect __destruct call"
        );
        assert!(asm[call_idx..].contains("add rsp, 32"));
    }

    /// Verifies linux-x86_64 emission stays byte-identical to before the
    /// reverse-ABI remap was introduced: the remap is windows-x86_64-only, so a
    /// linux-x86_64 build must never see a `mov rcx, rdi` instruction.
    #[test]
    fn test_linux_x86_64_call_object_destructor_has_no_reverse_abi_remap() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_call_object_destructor(&mut emitter);
        let asm = emitter.output();

        assert!(!asm.contains("mov rcx, rdi"));
        assert!(!asm.contains("sub rsp, 32"));
    }
}
