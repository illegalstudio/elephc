//! Purpose:
//! Emits `__rt_stdclass_prop_ref_cell`: makes a `stdClass` property a PHP reference in place and
//! hands back its cell, so `$v = &$object->{$name}` (and a by-reference walk of the object) binds
//! the property itself.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed` (emission).
//! - `crate::codegen::lower_inst::objects::lower_dynamic_prop_ref_cell` (the `stdClass` arm).
//!
//! Key details:
//! - The properties live in the hash at `obj+8`; `__rt_elem_ref_cell` turns the entry into a
//!   reference marker (creating it as null first) and may hand back a relocated hash, which is
//!   stored back into the object.
//! - The cell is returned BORROWED, like a declared reference slot's cell: the marker in the
//!   hash keeps its share, and the extra share `__rt_elem_ref_cell` retains for its caller is
//!   dropped here.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the helper for the active target.
///
/// - ARM64: in `x0` = object, `x1`/`x2` = property name; out `x0` = the cell.
/// - x86_64: in `rdi` = object, `rsi`/`rdx` = property name; out `rax` = the cell.
pub(crate) fn emit_stdclass_prop_ref_cell(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stdclass_prop_ref_cell ---");
    emitter.label_global("__rt_stdclass_prop_ref_cell");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // frame: object, cell
            emitter.instruction("stp x29, x30, [sp, #0]");                      // save frame pointer and return address
            emitter.instruction("mov x29, sp");                                 // establish the helper frame
            emitter.instruction("str x0, [sp, #16]");                           // keep the object across the helper
            emitter.instruction("ldr x0, [x0, #8]");                            // the property hash
            emitter.instruction("bl __rt_elem_ref_cell");                       // x0 = hash to store back, x1 = retained cell
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the object
            emitter.instruction("str x0, [x9, #8]");                            // publish the (possibly relocated) hash
            emitter.instruction("str x1, [sp, #24]");                           // keep the cell across the release
            emitter.instruction("mov x0, x1");                                  // the caller's extra share
            emitter.instruction("bl __rt_decref_any");                          // the marker keeps the cell alive
            emitter.instruction("ldr x0, [sp, #24]");                           // return the borrowed cell
            emitter.instruction("ldp x29, x30, [sp, #0]");                      // restore frame pointer and return address
            emitter.instruction("add sp, sp, #32");                             // release the frame
            emitter.instruction("ret");                                         // cell in x0
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // save the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the helper frame
            emitter.instruction("sub rsp, 16");                                 // object, cell
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // keep the object across the helper
            emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                // the property hash
            emitter.instruction("call __rt_elem_ref_cell");                     // rax = hash to store back, rdx = retained cell
            emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                // reload the object
            emitter.instruction("mov QWORD PTR [rcx + 8], rax");                // publish the (possibly relocated) hash
            emitter.instruction("mov QWORD PTR [rbp - 16], rdx");               // keep the cell across the release
            emitter.instruction("mov rax, rdx");                                // the caller's extra share
            emitter.instruction("call __rt_decref_any");                        // the marker keeps the cell alive
            emitter.instruction("mov rax, QWORD PTR [rbp - 16]");               // return the borrowed cell
            emitter.instruction("add rsp, 16");                                 // release the frame
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // cell in rax
        }
    }
}

/// Emits `__rt_object_hash_prop_ref_cell`: the declared-class form of the helper above, for an
/// undeclared name on a class with per-instance dynamic-property storage
/// (`#[AllowDynamicProperties]`): `$r = &$object->$name`.
///
/// The hash lives at a per-class payload offset and is null until the first dynamic write, so it
/// is created empty (Mixed values) first. Same ownership as `__rt_stdclass_prop_ref_cell`: the
/// returned cell is borrowed, the marker in the hash keeps its share.
///
/// - ARM64: in `x0` = object, `x1`/`x2` = property name, `x3` = hash byte offset; out `x0`.
/// - x86_64: in `rdi` = object, `rsi`/`rdx` = property name, `rcx` = hash byte offset; out `rax`.
pub(crate) fn emit_object_hash_prop_ref_cell(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_hash_prop_ref_cell ---");
    emitter.label_global("__rt_object_hash_prop_ref_cell");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #64");                             // frame: object, cell, name, offset
            emitter.instruction("stp x29, x30, [sp, #0]");                      // save frame pointer and return address
            emitter.instruction("mov x29, sp");                                 // establish the helper frame
            emitter.instruction("str x0, [sp, #16]");                           // keep the object across the helpers
            emitter.instruction("stp x1, x2, [sp, #32]");                       // keep the property name across hash creation
            emitter.instruction("str x3, [sp, #48]");                           // keep the hash offset
            emitter.instruction("ldr x0, [x0, x3]");                            // the dynamic-property hash, or null
            emitter.instruction("cbnz x0, __rt_object_hash_prop_ref_cell_have"); // an existing hash takes the entry directly
            emitter.instruction("mov x0, #4");                                  // initial capacity for the first dynamic property
            emitter.instruction("mov x1, #7");                                  // values are Mixed boxes
            emitter.instruction("bl __rt_hash_new");                            // x0 = the new empty hash
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the object
            emitter.instruction("ldr x10, [sp, #48]");                          // reload the hash offset
            emitter.instruction("str x0, [x9, x10]");                           // publish the hash before the entry is made
            emitter.label("__rt_object_hash_prop_ref_cell_have");
            emitter.instruction("ldp x1, x2, [sp, #32]");                       // reload the property name as the key
            emitter.instruction("bl __rt_elem_ref_cell");                       // x0 = hash to store back, x1 = retained cell
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the object
            emitter.instruction("ldr x10, [sp, #48]");                          // reload the hash offset
            emitter.instruction("str x0, [x9, x10]");                           // publish the (possibly relocated) hash
            emitter.instruction("str x1, [sp, #24]");                           // keep the cell across the release
            emitter.instruction("mov x0, x1");                                  // the caller's extra share
            emitter.instruction("bl __rt_decref_any");                          // the marker keeps the cell alive
            emitter.instruction("ldr x0, [sp, #24]");                           // return the borrowed cell
            emitter.instruction("ldp x29, x30, [sp, #0]");                      // restore frame pointer and return address
            emitter.instruction("add sp, sp, #64");                             // release the frame
            emitter.instruction("ret");                                         // cell in x0
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // save the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the helper frame
            emitter.instruction("sub rsp, 48");                                 // object, name, offset, cell
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // keep the object across the helpers
            emitter.instruction("mov QWORD PTR [rbp - 16], rsi");               // keep the property name pointer
            emitter.instruction("mov QWORD PTR [rbp - 24], rdx");               // keep the property name length
            emitter.instruction("mov QWORD PTR [rbp - 32], rcx");               // keep the hash offset
            emitter.instruction("mov rax, QWORD PTR [rdi + rcx]");              // the dynamic-property hash, or null
            emitter.instruction("test rax, rax");                               // does the instance have its hash yet?
            emitter.instruction("jnz __rt_object_hash_prop_ref_cell_have_x");   // an existing hash takes the entry directly
            emitter.instruction("mov rdi, 4");                                  // initial capacity for the first dynamic property
            emitter.instruction("mov rsi, 7");                                  // values are Mixed boxes
            emitter.instruction("call __rt_hash_new");                          // rax = the new empty hash
            emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                // reload the object
            emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");               // reload the hash offset
            emitter.instruction("mov QWORD PTR [rcx + rdx], rax");              // publish the hash before the entry is made
            emitter.label("__rt_object_hash_prop_ref_cell_have_x");
            emitter.instruction("mov rdi, rax");                                // the hash is the container
            emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");               // the property name pointer is the key
            emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");               // the property name length
            emitter.instruction("call __rt_elem_ref_cell");                     // rax = hash to store back, rdx = retained cell
            emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                // reload the object
            emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");               // reload the hash offset
            emitter.instruction("mov QWORD PTR [rcx + rsi], rax");              // publish the (possibly relocated) hash
            emitter.instruction("mov QWORD PTR [rbp - 40], rdx");               // keep the cell across the release
            emitter.instruction("mov rax, rdx");                                // the caller's extra share
            emitter.instruction("call __rt_decref_any");                        // the marker keeps the cell alive
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // return the borrowed cell
            emitter.instruction("add rsp, 48");                                 // release the frame
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // cell in rax
        }
    }
}
