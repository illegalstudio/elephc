//! Purpose:
//! Emits the `__rt_zval_free` and `__rt_zval_free_children` runtime helpers that
//! release PHP-shaped storage owned by a `zval`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::zval`,
//!   and from `__rt_zval_free_array` for each bucket.
//!
//! Key details:
//! - `__rt_zval_free` releases the owned children and then the 16-byte zval block.
//! - `__rt_zval_free_children` releases only the owned children referenced by the
//!   zval value (a `zend_string` for `IS_STRING`, a `zend_array` for `IS_ARRAY`).
//!   The array free path calls it per bucket because bucket slots live inside the
//!   shared data block and must not be freed as standalone blocks.
//! - Object/resource payloads are borrowed elephc pointers and are not freed.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// zval_free: release a zval and its owned PHP-shaped children.
/// Input:  x0 / rax = zval pointer
/// Output: none
pub fn emit_zval_free(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_zval_free_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: zval_free ---");
    emitter.label_global("__rt_zval_free");

    // -- save the link register and spill the zval pointer across the children free --
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // preserve frame and link registers (32 keeps 16-byte alignment for the spill slot)
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the helper
    emitter.instruction("str x0, [sp, #16]");                                   // spill the zval pointer (caller-saved regs do not survive the nested heap_free inside free_children)
    emitter.instruction("bl __rt_zval_free_children");                          // release owned string/array children
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the zval pointer after the children free clobbered the scratch registers
    emitter.instruction("bl __rt_heap_free");                                   // release the zval storage block
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame and link registers
    emitter.instruction("ret");                                                 // return to the caller

    emit_zval_free_children_aarch64(emitter);
}

/// zval_free_children: release owned children referenced by a zval value.
/// Input:  x0 = zval pointer
/// Output: none (the zval block itself is not freed)
fn emit_zval_free_children_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_free_children ---");
    emitter.label_global("__rt_zval_free_children");

    // -- save the link register across child free calls --
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve frame and link registers
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the helper
    emitter.instruction("mov x10, x0");                                         // keep the zval pointer across child calls
    emitter.instruction("ldrb w9, [x10, #8]");                                  // load the zval type byte
    emitter.instruction("cmp w9, #6");                                          // IS_STRING owns a zend_string
    emitter.instruction("b.eq __rt_zval_free_children_string");                 // take the string-free path when the value is a zend_string
    emitter.instruction("cmp w9, #7");                                          // IS_ARRAY owns a zend_array
    emitter.instruction("b.eq __rt_zval_free_children_array");                  // take the array-free path when the value is a zend_array
    emitter.instruction("b __rt_zval_free_children_done");                      // scalars/objects/resources own nothing extra

    emitter.label("__rt_zval_free_children_string");
    emitter.instruction("ldr x0, [x10]");                                       // load the zend_string pointer from the zval value
    emitter.instruction("bl __rt_heap_free");                                   // release the zend_string storage
    emitter.instruction("b __rt_zval_free_children_done");                      // done once the zend_string is released

    emitter.label("__rt_zval_free_children_array");
    emitter.instruction("ldr x0, [x10]");                                       // load the zend_array pointer from the zval value
    emitter.instruction("bl __rt_zval_free_array");                             // release the zend_array and its element tree
    emitter.instruction("b __rt_zval_free_children_done");                      // done once the zend_array is released

    emitter.label("__rt_zval_free_children_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame and link registers
    emitter.instruction("ret");                                                 // return to the caller
}

/// x86_64 Linux implementation of `__rt_zval_free` and `__rt_zval_free_children`.
fn emit_zval_free_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_free ---");
    emitter.label_global("__rt_zval_free");

    // -- save the frame pointer and spill the zval pointer across the children free --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a frame base for the helper
    emitter.instruction("sub rsp, 16");                                         // reserve one aligned spill slot for the zval pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // spill the zval pointer (caller-saved regs do not survive the nested heap_free inside free_children)
    emitter.instruction("call __rt_zval_free_children");                        // release owned string/array children
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the zval pointer after the children free clobbered the scratch registers
    emitter.instruction("call __rt_heap_free");                                 // release the zval storage block
    emitter.instruction("add rsp, 16");                                         // release the spill slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller

    emitter.blank();
    emitter.comment("--- runtime: zval_free_children ---");
    emitter.label_global("__rt_zval_free_children");

    // -- save the frame pointer across child free calls --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a frame base for the helper
    emitter.instruction("mov r10, rax");                                        // keep the zval pointer across child calls
    emitter.instruction("movzx rcx, BYTE PTR [r10 + 8]");                       // load the zval type byte
    emitter.instruction("cmp rcx, 6");                                          // IS_STRING owns a zend_string
    emitter.instruction("je __rt_zval_free_children_string");                   // take the string-free path when the value is a zend_string
    emitter.instruction("cmp rcx, 7");                                          // IS_ARRAY owns a zend_array
    emitter.instruction("je __rt_zval_free_children_array");                    // take the array-free path when the value is a zend_array
    emitter.instruction("jmp __rt_zval_free_children_done");                    // scalars/objects/resources own nothing extra

    emitter.label("__rt_zval_free_children_string");
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the zend_string pointer from the zval value
    emitter.instruction("call __rt_heap_free");                                 // release the zend_string storage
    emitter.instruction("jmp __rt_zval_free_children_done");                    // done once the zend_string is released

    emitter.label("__rt_zval_free_children_array");
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the zend_array pointer from the zval value
    emitter.instruction("call __rt_zval_free_array");                           // release the zend_array and its element tree
    emitter.instruction("jmp __rt_zval_free_children_done");                    // done once the zend_array is released

    emitter.label("__rt_zval_free_children_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
