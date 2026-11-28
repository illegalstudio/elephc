//! Purpose:
//! Emits the runtime tag dispatch for casting boxed `Mixed` values to PHP arrays.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Existing arrays retain their COW payload; objects project to hashes; null
//!   becomes empty; every other concrete tag becomes a one-element Mixed array.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_mixed_cast_array` for the active target.
pub fn emit_mixed_cast_array(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_cast_array_x86_64(emitter);
    } else {
        emit_mixed_cast_array_aarch64(emitter);
    }
}

/// Emits the AArch64 boxed-Mixed array-cast dispatcher.
fn emit_mixed_cast_array_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_array ---");
    emitter.label_global("__rt_mixed_cast_array");
    emitter.instruction("sub sp, sp, #80");                                     // reserve source, payload, container, result, and frame spill slots
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller frame across allocation helpers
    emitter.instruction("add x29, sp, #64");                                    // establish the mixed-array-cast helper frame
    emitter.instruction("str x0, [sp, #0]");                                    // retain the source Mixed cell for scalar array insertion
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the concrete runtime tag and payload words
    emitter.instruction("str x0, [sp, #8]");                                    // save the concrete runtime tag across dispatch helpers
    emitter.instruction("str x1, [sp, #16]");                                   // save the concrete low payload word
    emitter.instruction("str x2, [sp, #24]");                                   // save the concrete high payload word
    emitter.instruction("cmp x0, #4");                                          // detect an already-indexed array payload
    emitter.instruction("b.eq __rt_mixed_cast_array_existing");                 // retain indexed-array COW storage in a fresh result box
    emitter.instruction("cmp x0, #5");                                          // detect an already-associative array payload
    emitter.instruction("b.eq __rt_mixed_cast_array_existing");                 // retain associative COW storage in a fresh result box
    emitter.instruction("cmp x0, #6");                                          // detect an object requiring property projection
    emitter.instruction("b.eq __rt_mixed_cast_array_object");                   // project object properties using cast visibility keys
    emitter.instruction("cmp x0, #8");                                          // detect canonical PHP null
    emitter.instruction("b.eq __rt_mixed_cast_array_null");                     // null casts to an empty indexed array

    emitter.instruction("mov x0, #1");                                          // allocate capacity for the scalar element at index zero
    emitter.instruction("mov x1, #8");                                          // boxed Mixed indexed arrays use pointer-sized slots
    emitter.instruction("bl __rt_array_new");                                   // allocate fresh COW indexed storage for the scalar cast
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the array pointer across scalar value cloning
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source Mixed cell that becomes element zero
    emitter.instruction("bl __rt_mixed_clone");                                 // snapshot the scalar payload independently from its reference wrapper
    emitter.instruction("mov x2, x0");                                          // transfer the owned scalar snapshot into element zero
    emitter.instruction("ldr x0, [sp, #32]");                                   // pass the fresh indexed array to its consuming writer
    emitter.instruction("mov x1, #0");                                          // scalar casts always use integer key zero
    emitter.instruction("bl __rt_array_set_mixed");                             // install the scalar cell while preserving COW metadata
    emitter.instruction("b __rt_mixed_cast_array_box_indexed");                 // box the resulting indexed array for dynamic consumers

    emitter.label("__rt_mixed_cast_array_null");
    emitter.instruction("mov x0, #4");                                          // allocate a small mutation-safe empty-array capacity
    emitter.instruction("mov x1, #8");                                          // empty dynamic arrays use Mixed-compatible pointer slots
    emitter.instruction("bl __rt_array_new");                                   // materialize PHP's empty result for a null cast
    emitter.label("__rt_mixed_cast_array_box_indexed");
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the owned raw indexed array across boxing
    emitter.instruction("mov x1, x0");                                          // pass the indexed-array pointer as the boxed low payload
    emitter.instruction("mov x2, xzr");                                         // indexed arrays have no high payload word
    emitter.instruction("mov x0, #4");                                          // runtime tag 4 identifies indexed-array payloads
    emitter.instruction("bl __rt_mixed_from_value");                            // create an owned dynamic result box and retain the array
    emitter.instruction("str x0, [sp, #40]");                                   // preserve the result box while dropping the raw owner
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the temporary raw indexed-array owner
    emitter.instruction("bl __rt_decref_any");                                  // balance boxing's retain so only the result box owns it
    emitter.instruction("ldr x0, [sp, #40]");                                   // restore the boxed array result
    emitter.instruction("b __rt_mixed_cast_array_done");                        // join the common helper epilogue

    emitter.label("__rt_mixed_cast_array_object");
    emitter.instruction("ldr x0, [sp, #16]");                                   // pass the unboxed object pointer to property projection
    emitter.instruction("mov x1, #1");                                          // non-zero mode requests visibility-mangled cast keys
    emitter.instruction("mov x2, #-1");                                         // array casts have no lexical class visibility scope
    emitter.instruction("bl __rt_object_to_hash");                              // allocate the associative object-property projection
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the owned raw hash across boxing
    emitter.instruction("mov x1, x0");                                          // pass the hash pointer as the boxed low payload
    emitter.instruction("mov x2, xzr");                                         // hashes have no high payload word
    emitter.instruction("mov x0, #5");                                          // runtime tag 5 identifies associative arrays
    emitter.instruction("bl __rt_mixed_from_value");                            // create an owned dynamic result box and retain the hash
    emitter.instruction("str x0, [sp, #40]");                                   // preserve the result box while dropping the raw owner
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the temporary raw hash owner
    emitter.instruction("bl __rt_decref_any");                                  // balance boxing's retain on the projected hash
    emitter.instruction("ldr x0, [sp, #40]");                                   // restore the boxed hash result
    emitter.instruction("b __rt_mixed_cast_array_done");                        // join the common helper epilogue

    emitter.label("__rt_mixed_cast_array_existing");
    emitter.instruction("ldr x0, [sp, #8]");                                    // restore the existing array or hash runtime tag
    emitter.instruction("ldr x1, [sp, #16]");                                   // restore its shared COW payload pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // restore its high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // retain the shared container for the fresh result box
    emitter.label("__rt_mixed_cast_array_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame after all nested helpers
    emitter.instruction("add sp, sp, #80");                                     // release mixed-array-cast spill storage
    emitter.instruction("ret");                                                 // return the boxed dynamic array result
}

/// Emits the x86_64 boxed-Mixed array-cast dispatcher.
fn emit_mixed_cast_array_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_array ---");
    emitter.label_global("__rt_mixed_cast_array");
    emitter.instruction("push rbp");                                            // preserve the caller frame before nested helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable mixed-array-cast frame
    emitter.instruction("sub rsp, 64");                                         // reserve source, payload, container, and result spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // retain the source Mixed cell for scalar array insertion
    emitter.instruction("call __rt_mixed_unbox");                               // expose the concrete runtime tag and payload words
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the concrete runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the concrete low payload word
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the concrete high payload word
    emitter.instruction("cmp rax, 4");                                          // detect an already-indexed array payload
    emitter.instruction("je __rt_mixed_cast_array_existing_x");                 // retain indexed-array COW storage in a fresh result box
    emitter.instruction("cmp rax, 5");                                          // detect an already-associative array payload
    emitter.instruction("je __rt_mixed_cast_array_existing_x");                 // retain associative COW storage in a fresh result box
    emitter.instruction("cmp rax, 6");                                          // detect an object requiring property projection
    emitter.instruction("je __rt_mixed_cast_array_object_x");                   // project object properties using cast visibility keys
    emitter.instruction("cmp rax, 8");                                          // detect canonical PHP null
    emitter.instruction("je __rt_mixed_cast_array_null_x");                     // null casts to an empty indexed array
    emitter.instruction("mov rdi, 1");                                          // allocate capacity for the scalar element at index zero
    emitter.instruction("mov rsi, 8");                                          // boxed Mixed indexed arrays use pointer-sized slots
    emitter.instruction("call __rt_array_new");                                 // allocate fresh COW indexed storage for the scalar cast
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the array pointer across scalar value cloning
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source Mixed cell that becomes element zero
    emitter.instruction("call __rt_mixed_clone");                               // snapshot the scalar payload independently from its reference wrapper
    emitter.instruction("mov rdx, rax");                                        // transfer the owned scalar snapshot into element zero
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // pass the fresh indexed array to its consuming writer
    emitter.instruction("xor esi, esi");                                        // scalar casts always use integer key zero
    emitter.instruction("call __rt_array_set_mixed");                           // install the scalar cell while preserving COW metadata
    emitter.instruction("jmp __rt_mixed_cast_array_box_indexed_x");             // box the resulting indexed array for dynamic consumers
    emitter.label("__rt_mixed_cast_array_null_x");
    emitter.instruction("mov rdi, 4");                                          // allocate a small mutation-safe empty-array capacity
    emitter.instruction("mov rsi, 8");                                          // empty dynamic arrays use Mixed-compatible pointer slots
    emitter.instruction("call __rt_array_new");                                 // materialize PHP's empty result for a null cast
    emitter.label("__rt_mixed_cast_array_box_indexed_x");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the owned raw indexed array across boxing
    emitter.instruction("mov rdi, rax");                                        // pass the indexed-array pointer as the boxed low payload
    emitter.instruction("xor esi, esi");                                        // indexed arrays have no high payload word
    emitter.instruction("mov eax, 4");                                          // runtime tag 4 identifies indexed-array payloads
    emitter.instruction("call __rt_mixed_from_value");                          // create an owned dynamic result box and retain the array
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the result box while dropping the raw owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the temporary raw indexed-array owner
    emitter.instruction("call __rt_decref_any");                                // balance boxing's retain so only the result box owns it
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // restore the boxed array result
    emitter.instruction("jmp __rt_mixed_cast_array_done_x");                    // join the common helper epilogue
    emitter.label("__rt_mixed_cast_array_object_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // pass the unboxed object pointer to property projection
    emitter.instruction("mov rdx, 1");                                          // non-zero mode requests visibility-mangled cast keys
    emitter.instruction("mov rcx, -1");                                         // array casts have no lexical class visibility scope
    emitter.instruction("call __rt_object_to_hash");                            // allocate the associative object-property projection
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the owned raw hash across boxing
    emitter.instruction("mov rdi, rax");                                        // pass the hash pointer as the boxed low payload
    emitter.instruction("xor esi, esi");                                        // hashes have no high payload word
    emitter.instruction("mov eax, 5");                                          // runtime tag 5 identifies associative arrays
    emitter.instruction("call __rt_mixed_from_value");                          // create an owned dynamic result box and retain the hash
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the result box while dropping the raw owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the temporary raw hash owner
    emitter.instruction("call __rt_decref_any");                                // balance boxing's retain on the projected hash
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // restore the boxed hash result
    emitter.instruction("jmp __rt_mixed_cast_array_done_x");                    // join the common helper epilogue
    emitter.label("__rt_mixed_cast_array_existing_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // restore the existing array or hash runtime tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // restore its shared COW payload pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // restore its high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // retain the shared container for the fresh result box
    emitter.label("__rt_mixed_cast_array_done_x");
    emitter.instruction("mov rsp, rbp");                                        // discard mixed-array-cast spill storage
    emitter.instruction("pop rbp");                                             // restore the caller frame
    emitter.instruction("ret");                                                 // return the boxed dynamic array result
}
