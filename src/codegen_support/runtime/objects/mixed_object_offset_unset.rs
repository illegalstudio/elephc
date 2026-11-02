//! Purpose:
//! Emits `__rt_mixed_object_offset_unset`, which removes an offset from an object held in a
//! boxed `mixed` cell: `unset($o->bag[$k])` where `bag` is untyped or `mixed` and holds an
//! `ArrayAccess` object.
//!
//! Called from:
//! - `crate::codegen::lower_inst::offset_unset`'s object branch.
//!
//! Key details:
//! - Dispatch mirrors `__rt_mixed_array_get`. The runtime's own SPL containers are recognised
//!   by class id: `SplFixedArray` goes to `__rt_spl_fixed_offset_unset`, and
//!   `SplDoublyLinkedList`, `SplStack` and `SplQueue` to `__rt_spl_dll_offset_unset`. Any other
//!   class reaches its `ArrayAccess::offsetUnset` through the dense `_class_offsetunset_ptrs`
//!   table.
//! - A class with no `offsetUnset` entry (a plain object, `stdClass`) raises PHP's
//!   `Error("Cannot use object of type C as array")` through `__rt_throw_object_not_array`.
//! - The caller transfers one box containing the original PHP offset. User methods borrow it;
//!   SPL helpers consume it and apply their own current offset conversion rules.
//! - A PHP `offsetUnset()` BORROWS the offset box, so this frame frees it after the call. The
//!   shared exceptional-owner guard also frees it when the method throws instead of returning.
//!   SPL helpers consume the same box. The non-indexable Error path frees it too.
//! - A separate receiver owner spans PHP callbacks and SPL diagnostics, including replacement
//!   of the property that supplied the object. It is released after the key on every path.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::exceptions::guards;

/// Emits `__rt_mixed_object_offset_unset` for the current target.
///
/// Inputs: the unboxed object (`x0` / `rdi`) and the owned original offset (`x1` / `rsi`).
/// Returns nothing and may throw.
pub fn emit_mixed_object_offset_unset(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_object_offset_unset_x86_64(emitter);
        return;
    }
    emit_mixed_object_offset_unset_aarch64(emitter);
}

/// Emits the ARM64 helper. Frame: `[sp]` receiver, `[sp, #8]` boxed offset,
/// `[sp, #24]` selected method, `[sp, #32..64]` key owner, `[sp, #64..96]` receiver owner,
/// `[sp, #96]` fp/lr.
fn emit_mixed_object_offset_unset_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_object_offset_unset ---");
    emitter.label_global("__rt_mixed_object_offset_unset");
    emitter.instruction("sub sp, sp, #112");                                    // reserve operands, two exceptional owners and saved linkage
    emitter.instruction("stp x29, x30, [sp, #96]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #96");                                    // establish a stable frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the unboxed receiver
    emitter.instruction("str x1, [sp, #8]");                                    // save the owned boxed offset
    emitter.instruction("bl __rt_incref");                                      // retain the receiver before any callback can replace its property
    emitter.instruction("ldr x0, [sp, #0]");                                    // publish the retained raw object as an exceptional owner
    guards::guard(emitter, 64, 96);
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the receiver after guard registration
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore the original offset before dispatch
    emitter.instruction("ldr x11, [x0]");                                       // load the receiver's class id
    for (symbol, target) in [
        ("_spl_fixed_array_class_id", "__rt_mixed_object_offset_unset_spl_fixed"),
        ("_spl_dll_class_id", "__rt_mixed_object_offset_unset_spl_list"),
        ("_spl_stack_class_id", "__rt_mixed_object_offset_unset_spl_list"),
        ("_spl_queue_class_id", "__rt_mixed_object_offset_unset_spl_list"),
    ] {
        abi::emit_symbol_address(emitter, "x12", symbol);
        emitter.instruction("ldr x12, [x12]");                                  // load the runtime container's class id
        emitter.instruction("cmp x11, x12");                                    // is the receiver this runtime container?
        emitter.instruction(&format!("b.eq {target}"));                         // delegate the original offset to its consuming helper
    }
    emitter.instruction("tbnz x11, #63, __rt_mixed_object_offset_unset_not_indexable"); // synthetic negative ids cannot index metadata
    abi::emit_load_symbol_to_reg(emitter, "x12", "_class_iface_method_count", 0);
    emitter.instruction("cmp x11, x12");                                        // is the id within the dense table?
    emitter.instruction("b.hs __rt_mixed_object_offset_unset_not_indexable");   // out-of-range ids have no entry
    abi::emit_symbol_address(emitter, "x12", "_class_offsetunset_ptrs");
    emitter.instruction("ldr x12, [x12, x11, lsl #3]");                         // resolve the concrete or inherited offsetUnset
    emitter.instruction("cbz x12, __rt_mixed_object_offset_unset_not_indexable"); // 0 means the class is not ArrayAccess
    emitter.instruction("str x12, [sp, #24]");                                  // preserve the selected method across guard registration
    emitter.instruction("mov x0, x1");                                          // register this frame's owned key before entering PHP
    guards::guard(emitter, 32, 64);
    emitter.instruction("ldr x12, [sp, #24]");                                  // restore the selected method after guard registration
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the borrowed PHP receiver
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore the original key box borrowed by the method
    emitter.instruction("blr x12");                                             // remove through PHP's ArrayAccess::offsetUnset (x0 receiver, x1 offset)
    guards::unguard(emitter, 32, 64);
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed offset
    emitter.instruction("bl __rt_decref_mixed");                                // a PHP method BORROWED it, so this frame frees it
    emitter.instruction("b __rt_mixed_object_offset_unset_done");               // the offset is gone
    for (entry, helper, what) in [
        ("__rt_mixed_object_offset_unset_spl_fixed", "__rt_spl_fixed_offset_unset", "SplFixedArray"),
        ("__rt_mixed_object_offset_unset_spl_list", "__rt_spl_dll_offset_unset", "SPL list"),
    ] {
        emitter.label(entry);
        emitter.instruction("ldr x1, [sp, #8]");                                // transfer the original offset box to SPL
        emitter.instruction("ldr x0, [sp, #0]");                                // restore the unboxed receiver
        emitter.instruction(&format!("bl {helper}"));                           // remove through the container's offsetUnset, which consumes the box
        emitter.instruction("b __rt_mixed_object_offset_unset_done");           // nothing left to release
        emitter.comment(&format!("end of the {what} path"));
    }
    emitter.label("__rt_mixed_object_offset_unset_done");
    guards::unguard(emitter, 64, 96);
    emitter.instruction("ldr x0, [sp, #0]");                                    // transfer the retained receiver to ordinary cleanup
    emitter.instruction("bl __rt_decref_object");                               // release the receiver after its callback and key have finished
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the local frame
    emitter.instruction("ret");                                                 // return to the unset site

    emitter.label("__rt_mixed_object_offset_unset_not_indexable");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the owned boxed offset nobody will take
    emitter.instruction("bl __rt_decref_mixed");                                // free it before leaving through the Error
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass the receiver so the Error names its class
    emitter.instruction("bl __rt_throw_object_not_array");                      // never returns
}

/// Emits the x86_64 helper. Frame: `[rbp - 8]` receiver, `[rbp - 16]` boxed offset,
/// `[rbp - 32]` selected method, `[rbp - 64..32]` key owner, `[rbp - 96..64]` receiver owner.
fn emit_mixed_object_offset_unset_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_object_offset_unset ---");
    emitter.label_global("__rt_mixed_object_offset_unset");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 96");                                         // reserve operands, selected method and two exceptional owners
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the unboxed receiver
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the owned boxed offset
    emitter.instruction("mov rax, rdi");                                        // retain the receiver through the native GC convention
    emitter.instruction("call __rt_incref");                                    // keep it alive when a callback replaces its property
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // publish the retained receiver as an exceptional owner
    guards::guard(emitter, 64, 96);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the receiver after guard registration
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // restore the original offset before dispatch
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // load the receiver's class id
    for (symbol, target) in [
        ("_spl_fixed_array_class_id", "__rt_mixed_object_offset_unset_spl_fixed"),
        ("_spl_dll_class_id", "__rt_mixed_object_offset_unset_spl_list"),
        ("_spl_stack_class_id", "__rt_mixed_object_offset_unset_spl_list"),
        ("_spl_queue_class_id", "__rt_mixed_object_offset_unset_spl_list"),
    ] {
        abi::emit_load_symbol_to_reg(emitter, "r12", symbol, 0);
        emitter.instruction("cmp r11, r12");                                    // is the receiver this runtime container?
        emitter.instruction(&format!("je {target}"));                           // delegate the original offset to its consuming helper
    }
    emitter.instruction("test r11, r11");                                       // reject negative synthetic class ids
    emitter.instruction("js __rt_mixed_object_offset_unset_not_indexable");     // synthetic ids cannot index metadata
    emitter.instruction("cmp r11, QWORD PTR [rip + _class_iface_method_count]"); // is the id within the dense table?
    emitter.instruction("jae __rt_mixed_object_offset_unset_not_indexable");    // out-of-range ids have no entry
    emitter.instruction("lea r12, [rip + _class_offsetunset_ptrs]");            // dense ArrayAccess::offsetUnset table
    emitter.instruction("mov r12, QWORD PTR [r12 + r11 * 8]");                  // resolve the concrete or inherited offsetUnset
    emitter.instruction("test r12, r12");                                       // 0 means the class is not ArrayAccess
    emitter.instruction("jz __rt_mixed_object_offset_unset_not_indexable");     // raise PHP's object-as-array Error
    emitter.instruction("mov QWORD PTR [rbp - 32], r12");                       // preserve the selected method across guard registration
    emitter.instruction("mov rax, rsi");                                        // register this frame's owned key before entering PHP
    guards::guard(emitter, 32, 64);
    emitter.instruction("mov r12, QWORD PTR [rbp - 32]");                       // restore the selected method after guard registration
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the borrowed PHP receiver
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // restore the original key box borrowed by the method
    emitter.instruction("call r12");                                            // remove through PHP's ArrayAccess::offsetUnset (rdi receiver, rsi offset)
    guards::unguard(emitter, 32, 64);
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed offset
    emitter.instruction("call __rt_decref_mixed");                              // a PHP method BORROWED it, so this frame frees it
    emitter.instruction("jmp __rt_mixed_object_offset_unset_done");             // the offset is gone
    for (entry, helper) in [
        ("__rt_mixed_object_offset_unset_spl_fixed", "__rt_spl_fixed_offset_unset"),
        ("__rt_mixed_object_offset_unset_spl_list", "__rt_spl_dll_offset_unset"),
    ] {
        emitter.label(entry);
        emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                   // transfer the original offset box to SPL
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // restore the unboxed receiver
        emitter.instruction(&format!("call {helper}"));                         // remove through the container's offsetUnset, which consumes the box
        emitter.instruction("jmp __rt_mixed_object_offset_unset_done");         // nothing left to release
    }
    emitter.label("__rt_mixed_object_offset_unset_done");
    guards::unguard(emitter, 64, 96);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // transfer the retained receiver to ordinary cleanup
    emitter.instruction("call __rt_decref_object");                             // release the receiver after its callback and key have finished
    emitter.instruction("mov rsp, rbp");                                        // restore the stack pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the unset site

    emitter.label("__rt_mixed_object_offset_unset_not_indexable");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the owned boxed offset nobody will take
    emitter.instruction("call __rt_decref_mixed");                              // free it before leaving through the Error
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the receiver so the Error names its class
    emitter.instruction("call __rt_throw_object_not_array");                    // never returns
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target guards the receiver across dispatch and the key around the borrowing PHP call.
    #[test]
    fn offset_unset_php_key_owner_is_guarded_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_mixed_object_offset_unset(&mut emitter);
            let assembly = emitter.output();
            let guard = assembly.find("__rt_exception_guard_owned").unwrap();
            let call = assembly.find(if target.arch == Arch::AArch64 { "blr x12" } else { "call r12" }).unwrap();
            let detach = assembly.find("__rt_exception_unguard_owned").unwrap();
            let release = assembly.find("__rt_decref_mixed").unwrap();
            assert!(guard < call && call < detach && detach < release, "{name}: {assembly}");
            assert_eq!(assembly.matches("__rt_exception_guard_owned").count(), 2, "{name}");
            assert_eq!(assembly.matches("__rt_exception_unguard_owned").count(), 2, "{name}");
        }
    }
}
