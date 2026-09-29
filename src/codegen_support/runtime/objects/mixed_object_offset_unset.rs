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
//! - The key arrives twice. Boxed with the value the program wrote, because PHP hands a user
//!   `offsetUnset()` the offset as given (`"12"` stays a string, `1.5` stays a float). And as
//!   the normalized array key (`key_hi == -1` marks an integer), which the runtime's SPL
//!   containers get boxed, as they always have: their own offset check does not accept a
//!   numeric string yet. The caller transfers ownership of the first box.
//! - A PHP `offsetUnset()` BORROWS the offset box, so this frame frees it after the call. The
//!   SPL helpers CONSUME theirs, as they do for `__rt_mixed_array_get`, and the unused original
//!   box is freed first. The Error path frees it too.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_mixed_object_offset_unset` for the current target.
///
/// Inputs: the unboxed object (`x0` / `rdi`), the owned boxed offset as written (`x1` / `rsi`),
/// and the normalized key's low and high words (`x2`/`x3` / `rdx`/`rcx`). Returns nothing; it
/// may throw.
pub fn emit_mixed_object_offset_unset(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_object_offset_unset_x86_64(emitter);
        return;
    }
    emit_mixed_object_offset_unset_aarch64(emitter);
}

/// Emits the ARM64 helper. Frame: `[sp]` receiver, `[sp, #8]` boxed offset, `[sp, #16]` /
/// `[sp, #24]` normalized key words, `[sp, #32]` saved fp/lr.
fn emit_mixed_object_offset_unset_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_object_offset_unset ---");
    emitter.label_global("__rt_mixed_object_offset_unset");
    emitter.instruction("sub sp, sp, #48");                                     // reserve receiver, offset, key words and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the unboxed receiver
    emitter.instruction("str x1, [sp, #8]");                                    // save the owned boxed offset
    emitter.instruction("stp x2, x3, [sp, #16]");                               // save the normalized key words for the SPL containers
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
        emitter.instruction(&format!("b.eq {target}_key"));                     // box the normalized key for the container's own helper
    }
    emitter.instruction("tbnz x11, #63, __rt_mixed_object_offset_unset_not_indexable"); // synthetic negative ids cannot index metadata
    abi::emit_load_symbol_to_reg(emitter, "x12", "_class_iface_method_count", 0);
    emitter.instruction("cmp x11, x12");                                        // is the id within the dense table?
    emitter.instruction("b.hs __rt_mixed_object_offset_unset_not_indexable");   // out-of-range ids have no entry
    abi::emit_symbol_address(emitter, "x12", "_class_offsetunset_ptrs");
    emitter.instruction("ldr x12, [x12, x11, lsl #3]");                         // resolve the concrete or inherited offsetUnset
    emitter.instruction("cbz x12, __rt_mixed_object_offset_unset_not_indexable"); // 0 means the class is not ArrayAccess
    emitter.instruction("blr x12");                                             // remove through PHP's ArrayAccess::offsetUnset (x0 receiver, x1 offset)
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed offset
    emitter.instruction("bl __rt_decref_mixed");                                // a PHP method BORROWED it, so this frame frees it
    emitter.instruction("b __rt_mixed_object_offset_unset_done");               // the offset is gone
    for (entry, helper, what) in [
        ("__rt_mixed_object_offset_unset_spl_fixed", "__rt_spl_fixed_offset_unset", "SplFixedArray"),
        ("__rt_mixed_object_offset_unset_spl_list", "__rt_spl_dll_offset_unset", "SPL list"),
    ] {
        emitter.label(&format!("{entry}_key"));
        emitter.instruction("ldr x0, [sp, #8]");                                // the offset as written, which this helper does not need
        emitter.instruction("bl __rt_decref_mixed");                            // free it; the container gets the normalized key
        emitter.instruction("ldp x1, x2, [sp, #16]");                           // normalized key low and high words
        emitter.instruction("cmn x2, #1");                                      // does key_hi carry the integer-key sentinel?
        emitter.instruction("mov x0, #1");                                      // tag = string for mixed_from_value
        emitter.instruction("csel x0, xzr, x0, eq");                            // tag = int when the key is an integer
        emitter.instruction("csel x2, xzr, x2, eq");                            // integer keys have no high payload
        emitter.instruction("bl __rt_mixed_from_value");                        // box the normalized key
        emitter.instruction("mov x1, x0");                                      // pass the boxed key as argument 1
        emitter.instruction("ldr x0, [sp, #0]");                                // pass the unboxed receiver as argument 0
        emitter.instruction(&format!("bl {helper}"));                           // remove through the container's offsetUnset, which consumes the box
        emitter.instruction("b __rt_mixed_object_offset_unset_done");           // nothing left to release
        emitter.comment(&format!("end of the {what} path"));
    }
    emitter.label("__rt_mixed_object_offset_unset_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return to the unset site

    emitter.label("__rt_mixed_object_offset_unset_not_indexable");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the owned boxed offset nobody will take
    emitter.instruction("bl __rt_decref_mixed");                                // free it before leaving through the Error
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass the receiver so the Error names its class
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame before the tail-call
    emitter.instruction("b __rt_throw_object_not_array");                       // never returns
}

/// Emits the x86_64 helper. Frame: `[rbp - 8]` receiver, `[rbp - 16]` boxed offset,
/// `[rbp - 24]` / `[rbp - 32]` normalized key words.
fn emit_mixed_object_offset_unset_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_object_offset_unset ---");
    emitter.label_global("__rt_mixed_object_offset_unset");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve receiver, offset and key-word slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the unboxed receiver
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the owned boxed offset
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the normalized key low word for the SPL containers
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the normalized key high word
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // load the receiver's class id
    for (symbol, target) in [
        ("_spl_fixed_array_class_id", "__rt_mixed_object_offset_unset_spl_fixed"),
        ("_spl_dll_class_id", "__rt_mixed_object_offset_unset_spl_list"),
        ("_spl_stack_class_id", "__rt_mixed_object_offset_unset_spl_list"),
        ("_spl_queue_class_id", "__rt_mixed_object_offset_unset_spl_list"),
    ] {
        abi::emit_load_symbol_to_reg(emitter, "r12", symbol, 0);
        emitter.instruction("cmp r11, r12");                                    // is the receiver this runtime container?
        emitter.instruction(&format!("je {target}_key"));                       // box the normalized key for the container's own helper
    }
    emitter.instruction("test r11, r11");                                       // reject negative synthetic class ids
    emitter.instruction("js __rt_mixed_object_offset_unset_not_indexable");     // synthetic ids cannot index metadata
    emitter.instruction("cmp r11, QWORD PTR [rip + _class_iface_method_count]"); // is the id within the dense table?
    emitter.instruction("jae __rt_mixed_object_offset_unset_not_indexable");    // out-of-range ids have no entry
    emitter.instruction("lea r12, [rip + _class_offsetunset_ptrs]");            // dense ArrayAccess::offsetUnset table
    emitter.instruction("mov r12, QWORD PTR [r12 + r11 * 8]");                  // resolve the concrete or inherited offsetUnset
    emitter.instruction("test r12, r12");                                       // 0 means the class is not ArrayAccess
    emitter.instruction("jz __rt_mixed_object_offset_unset_not_indexable");     // raise PHP's object-as-array Error
    emitter.instruction("call r12");                                            // remove through PHP's ArrayAccess::offsetUnset (rdi receiver, rsi offset)
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed offset
    emitter.instruction("call __rt_decref_mixed");                              // a PHP method BORROWED it, so this frame frees it
    emitter.instruction("jmp __rt_mixed_object_offset_unset_done");             // the offset is gone
    for (entry, helper) in [
        ("__rt_mixed_object_offset_unset_spl_fixed", "__rt_spl_fixed_offset_unset"),
        ("__rt_mixed_object_offset_unset_spl_list", "__rt_spl_dll_offset_unset"),
    ] {
        let int_key = format!("{entry}_int_key");
        let boxed = format!("{entry}_boxed");
        emitter.label(&format!("{entry}_key"));
        emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                   // the offset as written, which this helper does not need
        emitter.instruction("call __rt_decref_mixed");                          // free it; the container gets the normalized key
        emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                   // normalized key low word
        emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                   // normalized key high word
        emitter.instruction("cmp rsi, -1");                                     // does key_hi carry the integer-key sentinel?
        emitter.instruction(&format!("je {int_key}"));                          // integer keys box as Mixed int
        emitter.instruction("mov rax, 1");                                      // tag = string for mixed_from_value
        emitter.instruction(&format!("jmp {boxed}"));                           // share the boxing call
        emitter.label(&int_key);
        emitter.instruction("mov rax, 0");                                      // tag = int for mixed_from_value
        emitter.instruction("xor esi, esi");                                    // integer keys have no high payload
        emitter.label(&boxed);
        emitter.instruction("call __rt_mixed_from_value");                      // box the normalized key
        emitter.instruction("mov rsi, rax");                                    // pass the boxed key as argument 1
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // pass the unboxed receiver as argument 0
        emitter.instruction(&format!("call {helper}"));                         // remove through the container's offsetUnset, which consumes the box
        emitter.instruction("jmp __rt_mixed_object_offset_unset_done");         // nothing left to release
    }
    emitter.label("__rt_mixed_object_offset_unset_done");
    emitter.instruction("mov rsp, rbp");                                        // restore the stack pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the unset site

    emitter.label("__rt_mixed_object_offset_unset_not_indexable");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the owned boxed offset nobody will take
    emitter.instruction("call __rt_decref_mixed");                              // free it before leaving through the Error
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the receiver so the Error names its class
    emitter.instruction("mov rsp, rbp");                                        // restore the stack pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before the tail-call
    emitter.instruction("jmp __rt_throw_object_not_array");                     // never returns
}
