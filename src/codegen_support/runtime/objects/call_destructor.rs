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
//! - The collector calls it while graph snapshots keep cyclic peers alive, before sweeping.
//!
//! Key details:
//! - `$this` is passed in the first integer argument register and is borrowed by
//!   the callee (no incref/decref around the call), matching normal method ABI, so
//!   the call cannot double-free the receiver.
//! - An optional eval callback can claim runtime-generic objects that actually
//!   belong to eval-declared classes; when no callback is installed, the helper
//!   follows the original static destructor table path.
//! - Callback status two transfers an owned boxed Throwable after Rust returns.
//!   Native propagation then reaches the collector's bounded handler or PHP's catch.
//! - Re-entrancy guard: before calling the destructor, bit 31 of the 32-bit
//!   refcount is set. A balanced `$tmp = $this;`/scope-exit inside the body then
//!   decrements from `0x8000_0001` back to `0x8000_0000` instead of reaching zero,
//!   so it cannot re-enter the free path and double-free the object. The entry
//!   boundary checks that guard before marking persistent completion in kind bit 14.
//!   Kind bit 17 also marks current GC scan candidates, so it cannot suppress
//!   an ordinary destructor call by itself.

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
        emitter.instruction("ldr w10, [x0, #-12]");                             // inspect a destructor already running on this receiver
        emitter.instruction("tbnz w10, #31, __rt_object_destructor_boundary_ret");// leave a reentrant call unmarked for its active owner
        emitter.instruction("ldr x9, [x0, #-8]");                               // inspect persistent object destructor state
        emitter.instruction("tbnz x9, #14, __rt_object_destructor_boundary_ret");// skip a completed destructor or failed construction permanently
        emitter.instruction("orr x9, x9, #0x4000");                             // record invocation before any user callback or throwable
        emitter.instruction("str x9, [x0, #-8]");                               // preserve called state in the low sixteen kind-word bits across GC
        emitter.instruction("b __rt_call_object_destructor_body");              // let the protected caller finish temporary release state
    } else {
        emitter.instruction("test rdi, rdi");                                   // reject a missing receiver before reading object metadata
        emitter.instruction("jz __rt_object_destructor_boundary_ret");          // return immediately for a null receiver
        emitter.instruction("test DWORD PTR [rdi - 12], 0x80000000");           // inspect a destructor already running on this receiver
        emitter.instruction("jnz __rt_object_destructor_boundary_ret");         // leave a reentrant call unmarked for its active owner
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
    emitter.instruction("str xzr, [sp, #8]");                                   // initialize the owned Throwable output to null
    emitter.instruction("add x1, sp, #8");                                      // pass a writable output slot to the Rust destructor callback
    emitter.instruction("blr x10");                                             // ask eval whether it owns and destructed this object
    emitter.instruction("mov x12, x0");                                         // preserve the eval callback handled flag
    emitter.instruction("ldr x11, [sp, #8]");                                   // recover a transferred Throwable only after Rust has returned
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the object pointer after the callback
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the eval callback frame
    emitter.instruction("cmp x12, #2");                                         // status two transfers an escaping eval Throwable
    emitter.instruction("b.eq __rt_call_object_destructor_eval_throw");         // propagate only outside the Rust callback frame
    emitter.instruction("cbnz x12, __rt_call_object_destructor_ret");           // eval handled the dynamic object → skip static lookup
    emitter.instruction("ldr w9, [x0, #-12]");                                  // reload the refcount after an eval miss
    emitter.instruction("movz w12, #0x8000, lsl #16");                          // w12 = destruction-in-progress flag bit
    emitter.instruction("bic w9, w9, w12");                                     // clear the temporary eval guard before static lookup
    emitter.instruction("str w9, [x0, #-12]");                                  // persist the restored refcount guard state
    emitter.instruction("b __rt_call_object_destructor_static");                // continue ordinary lookup after restoring a missed eval guard
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
    emitter.label("__rt_call_object_destructor_eval_throw");
    emitter.instruction("mov x0, x11");                                         // transfer the owned boxed Throwable into native propagation
    emitter.instruction("b __rt_throw_boxed_destructor_exception");             // the collector's local handler contains the native throw
}

/// Emits the x86_64 `__rt_call_object_destructor` helper.
fn emit_call_object_destructor_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: call_object_destructor ---");
    emitter.label_global("__rt_call_object_destructor_body");

    emitter.instruction("push rbp");                                            // preserve the caller frame for every normal and throwing exit
    emitter.instruction("mov rbp, rsp");                                        // establish one shared frame before any branch reaches a callback
    emitter.instruction("sub rsp, 16");                                         // reserve the object and transferred-Throwable spill slots

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
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // initialize the owned Throwable output to null
    emitter.instruction("lea rsi, [rbp - 16]");                                 // pass its address as the Rust callback's second argument
    emitter.emit_native_bridge_call("r10", 2);                                  // ask eval through the target Rust ABI whether it owns and destructed this object
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // recover the transferred Throwable after Rust returns normally
    emitter.instruction("cmp rax, 2");                                          // status two transfers an escaping eval Throwable
    emitter.instruction("je __rt_call_object_destructor_eval_throw");           // leave the local frame only after the Rust callback returns
    emitter.instruction("test rax, rax");                                       // did eval handle this dynamic object?
    emitter.instruction("jnz __rt_call_object_destructor_ret");                 // eval handled the dynamic object → skip static lookup
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the object pointer for the static fallback
    emitter.instruction("mov eax, DWORD PTR [rdi - 12]");                       // reload the refcount after an eval miss
    emitter.instruction("and eax, 0x7fffffff");                                 // clear the temporary eval guard before static lookup
    emitter.instruction("mov DWORD PTR [rdi - 12], eax");                       // persist the restored refcount guard state
    emitter.instruction("jmp __rt_call_object_destructor_static_x86");          // continue static lookup after restoring a missed eval guard
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
    emitter.label("__rt_call_object_destructor_eval_throw");
    emitter.instruction("mov rax, r11");                                        // pass the owned box to native exception propagation
    emitter.instruction("leave");                                               // release the callback frame after the native bridge has returned
    emitter.instruction("jmp __rt_throw_boxed_destructor_exception");           // the protected collector callback catches the native throw
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Eval callbacks receive a Throwable output slot and return before native propagation begins.
    #[test]
    fn eval_destructor_throw_status_is_handled_after_the_callback_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_call_object_destructor(&mut emitter);
            let asm = emitter.output();
            let (output, callback, status) = match target.arch {
                Arch::AArch64 => ("add x1, sp, #8", "blr x10", "cmp x12, #2"),
                Arch::X86_64 => ("lea rsi, [rbp - 16]", "call r10", "cmp rax, 2"),
            };
            assert!(asm.find(output).unwrap() < asm.find(callback).unwrap(), "{name}");
            assert!(asm.find(callback).unwrap() < asm.find(status).unwrap(), "{name}");
            assert!(asm.find(status).unwrap() < asm.find("__rt_throw_boxed_destructor_exception").unwrap(), "{name}");
        }
    }

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

        let static_path = asm
            .split_once("__rt_call_object_destructor_static_x86:")
            .expect("expected static destructor path")
            .1;
        let remap_idx = static_path.find("mov rcx, rdi").expect("expected SysV->MSx64 remap");
        let shadow_idx = static_path
            .find("sub rsp, 32")
            .expect("expected MSx64 shadow space before the destructor call");
        let call_idx = static_path.find("call r11").expect("expected relocated indirect call r11");
        assert!(
            shadow_idx < remap_idx && remap_idx < call_idx,
            "shadow reservation and remap must precede the indirect __destruct call"
        );
        assert!(static_path[call_idx..].contains("add rsp, 32"));
    }

    /// The eval destructor hook is a Rust callback, so its two SysV-staged
    /// arguments must be remapped and given MSx64 shadow space on Windows.
    #[test]
    fn test_windows_x86_64_eval_destructor_uses_native_bridge_abi() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_call_object_destructor(&mut emitter);
        let asm = emitter.output();
        let eval_path = asm
            .split_once("_elephc_eval_dynamic_object_destruct_fn")
            .expect("expected eval destructor callback")
            .1
            .split_once("__rt_call_object_destructor_static_x86:")
            .expect("expected static fallback")
            .0;

        for instruction in ["mov r11, r10", "sub rsp, 32", "mov rdx, rsi", "mov rcx, rdi", "call r11", "add rsp, 32"] {
            assert!(eval_path.contains(instruction), "eval destructor callback needs MSx64 staging: {instruction}");
        }
    }

    /// Every x86 branch that reaches `leave` owns the shared frame and both eval spill slots.
    #[test]
    fn x86_destructor_body_establishes_one_frame_before_any_exit_or_spill() {
        for platform in [Platform::Linux, Platform::Windows] {
            let mut emitter = Emitter::new(Target::new(platform, Arch::X86_64));
            emit_call_object_destructor(&mut emitter);
            let asm = emitter.output();
            let body = asm
                .split_once("__rt_call_object_destructor_body:")
                .expect("destructor body")
                .1;
            let prologue = body.find("push rbp").expect("shared frame save");
            let frame = body.find("mov rbp, rsp").expect("shared frame pointer");
            let spills = body.find("sub rsp, 16").expect("shared spill allocation");
            let first_branch = body.find("test rdi, rdi").expect("first body branch");
            let first_spill = body.find("[rbp - 8]").expect("object spill");
            let first_leave = body.find("leave").expect("shared epilogue");
            assert!(prologue < frame && frame < spills && spills < first_branch, "{platform:?}");
            assert!(spills < first_spill && prologue < first_leave, "{platform:?}");
            assert_eq!(body.matches("push rbp").count(), 1, "{platform:?}");
            assert_eq!(body.matches("leave").count(), 2, "{platform:?}");
        }
    }

    /// Dynamic-eval destructor status must dispatch while the shared outer
    /// frame is still live: status zero falls through to the static path,
    /// handled status returns through the shared epilogue, and a transferred
    /// Throwable tears down that frame immediately before propagation.
    #[test]
    fn test_windows_x86_64_eval_destructor_keeps_its_frame_until_status_dispatch() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_call_object_destructor(&mut emitter);
        let asm = emitter.output();

        let eval_path = asm
            .split_once("_elephc_eval_dynamic_object_destruct_fn")
            .expect("expected eval destructor callback")
            .1
            .split_once("__rt_call_object_destructor_static_x86:")
            .expect("expected static fallback")
            .0;
        let status = eval_path.find("cmp rax, 2").expect("expected eval status dispatch");
        assert!(status < eval_path.len());
        assert!(!eval_path.contains("leave"), "the dynamic status dispatch must retain the outer frame");
        assert!(eval_path.contains("mov rdi, QWORD PTR [rbp - 8]"), "status zero must restore $this before static fallback");

        let return_path = asm
            .split_once("__rt_call_object_destructor_ret:")
            .expect("expected shared return epilogue")
            .1
            .split_once("__rt_call_object_destructor_eval_throw:")
            .expect("expected eval exception path")
            .0;
        assert!(return_path.contains("leave\n    ret"), "handled status must tear down the shared frame exactly once");
        let throw_path = asm
            .split_once("__rt_call_object_destructor_eval_throw:")
            .expect("expected eval exception path")
            .1;
        assert!(throw_path.contains("mov rax, r11\n    leave\n    jmp __rt_throw_boxed_destructor_exception"));
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
