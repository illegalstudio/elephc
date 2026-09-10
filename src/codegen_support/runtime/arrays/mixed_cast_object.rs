//! Purpose:
//! Emits the runtime dispatcher for PHP `(object)` casts over boxed Mixed values.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime()`.
//! - `crate::codegen::lower_inst::conversions::lower_mixed_cast_object()`.
//!
//! Key details:
//! - Object payloads retain and return the original Mixed cell, preserving concrete class identity.
//! - Arrays clone before boxed-hash projection; the raw projection helper borrows that clone, so this helper releases it explicitly.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_mixed_cast_object(Mixed*) -> Mixed*` for the active target.
pub fn emit_mixed_cast_object(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_cast_object_x86_64(emitter);
        return;
    }
    emit_mixed_cast_object_aarch64(emitter);
}

/// Emits the AArch64 dispatcher for boxed dynamic object casts.
fn emit_mixed_cast_object_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_object ---");
    emitter.label_global("__rt_mixed_cast_object");
    emitter.instruction("sub sp, sp, #48");                                     // reserve source, tag, payload, object, and frame slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame across allocation helpers
    emitter.instruction("add x29, sp, #32");                                    // establish the object-cast helper frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the source Mixed cell across the tag dispatch
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose tag in x0 and payload in x1/x2
    emitter.instruction("str x0, [sp, #8]");                                    // save the runtime tag for branch selection
    emitter.instruction("str x1, [sp, #16]");                                   // save the low payload word across nested calls
    emitter.instruction("cmp x0, #6");                                          // does the source already hold an object?
    emitter.instruction("b.eq __rt_mixed_cast_object_existing");                // preserve identity and concrete class
    emitter.instruction("cmp x0, #4");                                          // does the source hold an indexed array?
    emitter.instruction("b.eq __rt_mixed_cast_object_array");                   // project a private indexed-array copy to properties
    emitter.instruction("cmp x0, #5");                                          // does the source hold an associative array?
    emitter.instruction("b.eq __rt_mixed_cast_object_hash");                    // project a private hash copy to properties
    emitter.instruction("cmp x0, #8");                                          // does the source hold PHP null?
    emitter.instruction("b.eq __rt_mixed_cast_object_null");                    // null produces an empty stdClass
    emitter.instruction("b __rt_mixed_cast_object_scalar");                     // every remaining tag becomes stdClass::$scalar

    emitter.label("__rt_mixed_cast_object_existing");
    emitter.instruction("ldr x0, [sp, #0]");                                    // recover the source cell for a retained identity result
    emitter.instruction("bl __rt_incref");                                      // give the cast expression its own Mixed-cell reference
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the original boxed object unchanged
    emitter.instruction("b __rt_mixed_cast_object_done");                       // skip stdClass result boxing

    emitter.label("__rt_mixed_cast_object_array");
    emitter.instruction("ldr x0, [sp, #16]");                                   // load the borrowed indexed-array payload
    emitter.instruction("bl __rt_array_clone_shallow");                         // create an owned COW-isolated source clone
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the clone because the raw helper only borrows it
    emitter.instruction("bl __rt_array_to_hash");                               // project integer keys into an owned hash without consuming the clone
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the projected hash while releasing the raw clone owner
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the temporary indexed-array clone
    emitter.instruction("bl __rt_decref_array");                                // release the clone after its raw-helper projection completes
    emitter.instruction("ldr x0, [sp, #24]");                                   // restore the projected hash for Mixed entry boxing
    emitter.instruction("bl __rt_hash_to_mixed");                               // box hash entries for stdClass dynamic properties
    emitter.instruction("bl __rt_stdclass_from_hash");                          // transfer the owned property hash into stdClass
    emitter.instruction("b __rt_mixed_cast_object_box");                        // box the new stdClass result

    emitter.label("__rt_mixed_cast_object_hash");
    emitter.instruction("ldr x0, [sp, #16]");                                   // load the borrowed associative-array payload
    emitter.instruction("bl __rt_hash_clone_shallow");                          // create an owned COW-isolated property hash
    emitter.instruction("bl __rt_hash_to_mixed");                               // box hash entries for stdClass dynamic properties
    emitter.instruction("bl __rt_stdclass_from_hash");                          // transfer the owned property hash into stdClass
    emitter.instruction("b __rt_mixed_cast_object_box");                        // box the new stdClass result

    emitter.label("__rt_mixed_cast_object_null");
    emitter.instruction("bl __rt_stdclass_new");                                // allocate the empty stdClass required by `(object) null`
    emitter.instruction("b __rt_mixed_cast_object_box");                        // box the new stdClass result

    emitter.label("__rt_mixed_cast_object_scalar");
    emitter.instruction("ldr x0, [sp, #0]");                                    // load the source scalar Mixed cell
    emitter.instruction("bl __rt_incref");                                      // transfer an independent cell reference into the property hash
    emitter.instruction("bl __rt_stdclass_new");                                // allocate the scalar wrapper object
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the new object while preparing the property call
    abi::emit_symbol_address(emitter, "x1", "_object_cast_scalar_key");
    emitter.instruction("mov x2, #6");                                          // pass the six-byte `scalar` property name
    emitter.instruction("ldr x3, [sp, #0]");                                    // pass the retained Mixed cell as the property value
    emitter.instruction("ldr x0, [sp, #24]");                                   // restore the stdClass receiver
    emitter.instruction("bl __rt_stdclass_set");                                // publish the scalar cell under the public property name
    emitter.instruction("ldr x0, [sp, #24]");                                   // recover the newly populated stdClass for boxing

    emitter.label("__rt_mixed_cast_object_box");
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the raw stdClass owner while boxing retains it
    emitter.instruction("mov x1, x0");                                          // move the object pointer into the Mixed payload lane
    emitter.instruction("mov x0, #6");                                          // runtime tag 6 denotes an object payload
    emitter.instruction("mov x2, xzr");                                         // object Mixed values have no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate the owning boxed result cell
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the result box while releasing the raw object owner
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the temporary raw stdClass owner
    emitter.instruction("bl __rt_decref_any");                                  // balance boxing's retain so only the result box owns the object
    emitter.instruction("ldr x0, [sp, #16]");                                   // restore the boxed object-cast result

    emitter.label("__rt_mixed_cast_object_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the object-cast helper frame
    emitter.instruction("ret");                                                 // return the owned boxed cast result in x0
}

/// Emits the x86_64 SysV dispatcher for boxed dynamic object casts.
fn emit_mixed_cast_object_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_object ---");
    emitter.label_global("__rt_mixed_cast_object");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve source, tag, payload, and object spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source Mixed cell across the tag dispatch
    emitter.instruction("call __rt_mixed_unbox");                               // expose tag in rax and payload in rdi/rdx
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the runtime tag for branch selection
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the low payload word across nested calls
    emitter.instruction("cmp rax, 6");                                          // does the source already hold an object?
    emitter.instruction("je __rt_mixed_cast_object_existing_x86");              // preserve identity and concrete class
    emitter.instruction("cmp rax, 4");                                          // does the source hold an indexed array?
    emitter.instruction("je __rt_mixed_cast_object_array_x86");                 // project a private indexed-array copy to properties
    emitter.instruction("cmp rax, 5");                                          // does the source hold an associative array?
    emitter.instruction("je __rt_mixed_cast_object_hash_x86");                  // project a private hash copy to properties
    emitter.instruction("cmp rax, 8");                                          // does the source hold PHP null?
    emitter.instruction("je __rt_mixed_cast_object_null_x86");                  // null produces an empty stdClass
    emitter.instruction("jmp __rt_mixed_cast_object_scalar_x86");               // every remaining tag becomes stdClass::$scalar

    emitter.label("__rt_mixed_cast_object_existing_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // recover the source cell for a retained identity result
    emitter.instruction("call __rt_incref");                                    // give the cast expression its own Mixed-cell reference
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the original boxed object unchanged
    emitter.instruction("jmp __rt_mixed_cast_object_done_x86");                 // skip stdClass result boxing

    emitter.label("__rt_mixed_cast_object_array_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // load the borrowed indexed-array payload
    emitter.instruction("call __rt_array_clone_shallow");                       // create an owned COW-isolated source clone
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the clone because the raw helper only borrows it
    emitter.instruction("mov rdi, rax");                                        // pass the cloned indexed array to the projection helper
    emitter.instruction("call __rt_array_to_hash");                             // project integer keys into an owned hash without consuming the clone
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the projected hash while releasing the raw clone owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the temporary indexed-array clone
    emitter.instruction("call __rt_decref_array");                              // release the clone after its raw-helper projection completes
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // restore the projected hash for Mixed entry boxing
    emitter.instruction("call __rt_hash_to_mixed");                             // box hash entries for stdClass dynamic properties
    emitter.instruction("mov rdi, rax");                                        // pass the owned property hash to stdClass construction
    emitter.instruction("call __rt_stdclass_from_hash");                        // transfer the owned property hash into stdClass
    emitter.instruction("jmp __rt_mixed_cast_object_box_x86");                  // box the new stdClass result

    emitter.label("__rt_mixed_cast_object_hash_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // load the borrowed associative-array payload
    emitter.instruction("call __rt_hash_clone_shallow");                        // create an owned COW-isolated property hash
    emitter.instruction("mov rdi, rax");                                        // pass the owned hash to entry boxing
    emitter.instruction("call __rt_hash_to_mixed");                             // box hash entries for stdClass dynamic properties
    emitter.instruction("mov rdi, rax");                                        // pass the owned property hash to stdClass construction
    emitter.instruction("call __rt_stdclass_from_hash");                        // transfer the owned property hash into stdClass
    emitter.instruction("jmp __rt_mixed_cast_object_box_x86");                  // box the new stdClass result

    emitter.label("__rt_mixed_cast_object_null_x86");
    emitter.instruction("call __rt_stdclass_new");                              // allocate the empty stdClass required by `(object) null`
    emitter.instruction("jmp __rt_mixed_cast_object_box_x86");                  // box the new stdClass result

    emitter.label("__rt_mixed_cast_object_scalar_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // load the source scalar Mixed cell
    emitter.instruction("call __rt_incref");                                    // transfer an independent cell reference into the property hash
    emitter.instruction("call __rt_stdclass_new");                              // allocate the scalar wrapper object
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the new object while preparing the property call
    emitter.instruction("mov rdi, rax");                                        // pass the stdClass receiver in the first SysV slot
    abi::emit_symbol_address(emitter, "rsi", "_object_cast_scalar_key");
    emitter.instruction("mov rdx, 6");                                          // pass the six-byte `scalar` property name
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // pass the retained Mixed cell as the property value
    emitter.instruction("call __rt_stdclass_set");                              // publish the scalar cell under the public property name
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // recover the newly populated stdClass for boxing

    emitter.label("__rt_mixed_cast_object_box_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the raw stdClass owner while boxing retains it
    emitter.instruction("mov rdi, rax");                                        // move the object pointer into the Mixed payload lane
    emitter.instruction("xor esi, esi");                                        // object Mixed values have no high payload word
    emitter.instruction("mov eax, 6");                                          // runtime tag 6 denotes an object payload
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the owning boxed result cell
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the result box while releasing the raw object owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the temporary raw stdClass owner
    emitter.instruction("call __rt_decref_any");                                // balance boxing's retain so only the result box owns the object
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // restore the boxed object-cast result

    emitter.label("__rt_mixed_cast_object_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // restore the stack pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owned boxed cast result in rax
}
