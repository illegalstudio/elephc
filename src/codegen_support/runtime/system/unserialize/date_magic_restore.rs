//! Purpose:
//! Restores user-declared DateTime-subclass properties after native hydration hooks.
//!
//! Called from:
//! - The unserialize runtime orchestrator.
//!
//! Key details:
//! - Uses generated filtered descriptors and balances ownership on both supported ABIs.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the target-specific date-magic property restoration helper.
pub(super) fn emit(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}
/// Emits the AArch64 restoration helper.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.comment("--- runtime: date_magic_restore_props ---");
    emitter.label_global("__rt_date_magic_restore_props");
    emitter.instruction("sub sp, sp, #112");                                    // allocate the date-property restore frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the restore frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the concrete object pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the parsed magic data hash
    emitter.instruction("ldr x9, [x0]");                                        // load the concrete runtime class id
    crate::codegen_support::abi::emit_symbol_address(
        emitter,
        "x10",
        "_class_date_unserialize_prop_ptrs",
    );
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // filtered user-property descriptor
    emitter.instruction("str x10, [sp, #16]");                                  // save the descriptor pointer
    emitter.instruction("ldr x11, [x10]");                                      // load the custom property count
    emitter.instruction("str x11, [sp, #24]");                                  // save the custom property count
    emitter.instruction("str xzr, [sp, #32]");                                  // property cursor = 0
    emitter.label("__rt_date_magic_restore_props_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the property cursor
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the property count
    emitter.instruction("cmp x9, x10");                                         // restored every custom property?
    emitter.instruction("b.ge __rt_date_magic_restore_props_done");             // finish when every row was visited
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the descriptor base
    emitter.instruction("add x11, x10, #8");                                    // skip the descriptor count
    emitter.instruction("add x11, x11, x9, lsl #5");                            // address the current 32-byte row
    emitter.instruction("str x11, [sp, #40]");                                  // save the current row pointer
    emitter.instruction("ldr x0, [sp, #8]");                                    // hash argument for the custom-key lookup
    emitter.instruction("ldr x1, [x11]");                                       // PHP-mangled custom-property key pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // PHP-mangled custom-property key length
    emitter.instruction("bl __rt_hash_get");                                    // look up the parsed boxed value
    emitter.instruction("cbz x0, __rt_date_magic_restore_props_next");          // missing optional custom property stays at its default
    emitter.instruction("str x1, [sp, #48]");                                   // save the parsed Mixed cell pointer
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload the descriptor row
    emitter.instruction("ldr x12, [x11, #16]");                                 // load the target property byte offset
    emitter.instruction("ldr x13, [x11, #24]");                                 // load the target property runtime tag
    emitter.instruction("str x13, [sp, #64]");                                  // save the target tag across ownership calls
    emitter.instruction("ldr x14, [sp, #0]");                                   // reload the concrete object pointer
    emitter.instruction("add x14, x14, x12");                                   // address the target property slot
    emitter.instruction("str x14, [sp, #56]");                                  // save the target slot address
    emitter.instruction("cmp x13, #7");                                         // is the target a boxed Mixed slot?
    emitter.instruction("b.eq __rt_date_magic_restore_props_mixed");            // replace the owned Mixed cell
    emitter.instruction("cmp x13, #1");                                         // is the target a string slot?
    emitter.instruction("b.eq __rt_date_magic_restore_props_string");           // retain and restore pointer plus length
    emitter.instruction("cmp x13, #4");                                         // is the target an indexed-array slot?
    emitter.instruction("b.eq __rt_date_magic_restore_props_array");            // rebuild indexed storage from the parsed hash
    emitter.instruction("cmp x13, #5");                                         // is the target an associative-array slot?
    emitter.instruction("b.eq __rt_date_magic_restore_props_heap");             // retain and restore the heap pointer
    emitter.instruction("cmp x13, #6");                                         // is the target an object slot?
    emitter.instruction("b.eq __rt_date_magic_restore_props_heap");             // retain and restore the object pointer
    emitter.instruction("cmp x13, #9");                                         // is the target a resource slot?
    emitter.instruction("b.eq __rt_date_magic_restore_props_heap");             // retain and restore both resource words
    emitter.instruction("cmp x13, #11");                                        // is the target an inline TaggedScalar slot?
    emitter.instruction("b.eq __rt_date_magic_restore_props_tagged_scalar");    // restore payload plus the parsed runtime tag
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the parsed Mixed cell
    emitter.instruction("ldr x10, [x9, #8]");                                   // unbox the scalar low payload word
    emitter.instruction("ldr x11, [x9, #16]");                                  // unbox the scalar high payload word
    emitter.instruction("ldr x14, [sp, #56]");                                  // reload the target slot address
    emitter.instruction("stp x10, x11, [x14]");                                 // restore the scalar property payload
    emitter.instruction("b __rt_date_magic_restore_props_next");                // continue with the next descriptor row
    emitter.label("__rt_date_magic_restore_props_tagged_scalar");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the parsed Mixed cell
    emitter.instruction("ldr x10, [x9, #8]");                                  // load the scalar payload word
    emitter.instruction("ldr x11, [x9]");                                      // load the parsed int/null runtime tag
    emitter.instruction("ldr x14, [sp, #56]");                                 // reload the target slot address
    emitter.instruction("stp x10, x11, [x14]");                                 // restore the inline TaggedScalar words
    emitter.instruction("b __rt_date_magic_restore_props_next");                // continue with the next descriptor row
    emitter.label("__rt_date_magic_restore_props_mixed");
    emitter.instruction("ldr x14, [sp, #56]");                                  // reload the target Mixed slot
    emitter.instruction("ldr x0, [x14]");                                       // load the previously owned Mixed cell
    emitter.instruction("bl __rt_decref_mixed");                                // release the default or previous property value
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the parsed Mixed cell
    emitter.instruction("bl __rt_incref");                                      // retain it for the object property owner
    emitter.instruction("ldr x14, [sp, #56]");                                  // reload the target Mixed slot
    emitter.instruction("str x0, [x14]");                                       // install the retained Mixed cell
    emitter.instruction("str xzr, [x14, #8]");                                  // clear the unused high slot word
    emitter.instruction("b __rt_date_magic_restore_props_next");                // continue with the next descriptor row
    emitter.label("__rt_date_magic_restore_props_string");
    emitter.instruction("ldr x14, [sp, #56]");                                  // reload the target string slot
    emitter.instruction("ldr x0, [x14]");                                       // load the previously owned string pointer
    emitter.instruction("bl __rt_decref_any");                                  // release the previous string storage when managed
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the parsed Mixed cell
    emitter.instruction("ldr x0, [x9, #8]");                                    // load the parsed string pointer
    emitter.instruction("bl __rt_incref");                                      // retain the parsed string for the property owner
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the parsed Mixed cell after the retain
    emitter.instruction("ldr x10, [x9, #16]");                                  // load the parsed string length
    emitter.instruction("ldr x14, [sp, #56]");                                  // reload the target string slot
    emitter.instruction("stp x0, x10, [x14]");                                  // restore the owned string pointer and length
    emitter.instruction("b __rt_date_magic_restore_props_next");                // continue with the next descriptor row
    emitter.label("__rt_date_magic_restore_props_array");
    emitter.instruction("ldr x14, [sp, #56]");                                  // reload the target array slot
    emitter.instruction("ldr x0, [x14]");                                       // load the previously owned indexed array
    emitter.instruction("bl __rt_decref_any");                                  // release the previous array storage
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the parsed Mixed cell
    emitter.instruction("ldr x0, [x9, #8]");                                    // load the parsed associative array
    emitter.instruction("bl __rt_hash_to_indexed_array");                       // rebuild native indexed-array storage
    emitter.instruction("ldr x14, [sp, #56]");                                  // reload the target array slot
    emitter.instruction("str x0, [x14]");                                       // install the rebuilt indexed array
    emitter.instruction("str xzr, [x14, #8]");                                  // clear the unused high slot word
    emitter.instruction("b __rt_date_magic_restore_props_next");                // continue with the next descriptor row
    emitter.label("__rt_date_magic_restore_props_heap");
    emitter.instruction("ldr x14, [sp, #56]");                                  // reload the target heap-backed slot
    emitter.instruction("ldr x0, [x14]");                                       // load the previously owned heap pointer
    emitter.instruction("bl __rt_decref_any");                                  // release the previous property storage
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the parsed Mixed cell
    emitter.instruction("ldr x0, [x9, #8]");                                    // load the parsed heap payload pointer
    emitter.instruction("bl __rt_incref");                                      // retain the payload for the property owner
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the parsed Mixed cell after the retain
    emitter.instruction("ldr x10, [x9, #16]");                                  // load the parsed high payload word
    emitter.instruction("ldr x14, [sp, #56]");                                  // reload the target heap-backed slot
    emitter.instruction("stp x0, x10, [x14]");                                  // restore the retained heap payload
    emitter.label("__rt_date_magic_restore_props_next");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the property cursor
    emitter.instruction("add x9, x9, #1");                                      // advance to the next custom property
    emitter.instruction("str x9, [sp, #32]");                                   // persist the advanced cursor
    emitter.instruction("b __rt_date_magic_restore_props_loop");                // continue restoring properties
    emitter.label("__rt_date_magic_restore_props_done");
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // deallocate the restore frame
    emitter.instruction("ret");                                                 // return after restoring custom properties
}

/// Emits the x86_64 restoration helper.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.comment("--- runtime: date_magic_restore_props ---");
    emitter.label_global("__rt_date_magic_restore_props");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the property-restore frame
    emitter.instruction("sub rsp, 96");                                         // reserve descriptor and ownership spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the concrete object pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the parsed magic data hash
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the concrete runtime class id
    crate::codegen_support::abi::emit_symbol_address(
        emitter,
        "r10",
        "_class_date_unserialize_prop_ptrs",
    );
    emitter.instruction("mov r10, QWORD PTR [r10 + rax*8]");                    // filtered user-property descriptor
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the descriptor pointer
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the custom property count
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the custom property count
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // property cursor = 0
    emitter.label("__rt_date_magic_restore_props_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the property cursor
    emitter.instruction("cmp rax, QWORD PTR [rbp - 32]");                       // restored every custom property?
    emitter.instruction("jge __rt_date_magic_restore_props_done");              // finish when every row was visited
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the descriptor base
    emitter.instruction("add r10, 8");                                          // skip the descriptor count
    emitter.instruction("shl rax, 5");                                          // scale the cursor by the 32-byte row size
    emitter.instruction("add r10, rax");                                        // address the current descriptor row
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save the current row pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // hash argument for the custom-key lookup
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // PHP-mangled custom-property key pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // PHP-mangled custom-property key length
    emitter.instruction("call __rt_hash_get");                                  // look up the parsed boxed value
    emitter.instruction("test rax, rax");                                       // did the serialized hash contain this property?
    emitter.instruction("jz __rt_date_magic_restore_props_next");               // missing optional custom property stays at its default
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // save the parsed Mixed cell pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the descriptor row
    emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                       // load the target property byte offset
    emitter.instruction("mov r12, QWORD PTR [r10 + 24]");                       // load the target property runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 72], r12");                       // save the target tag across ownership calls
    emitter.instruction("mov r13, QWORD PTR [rbp - 8]");                        // reload the concrete object pointer
    emitter.instruction("add r13, r11");                                        // address the target property slot
    emitter.instruction("mov QWORD PTR [rbp - 64], r13");                       // save the target slot address
    emitter.instruction("cmp r12, 7");                                          // is the target a boxed Mixed slot?
    emitter.instruction("je __rt_date_magic_restore_props_mixed");              // replace the owned Mixed cell
    emitter.instruction("cmp r12, 1");                                          // is the target a string slot?
    emitter.instruction("je __rt_date_magic_restore_props_string");             // retain and restore pointer plus length
    emitter.instruction("cmp r12, 4");                                          // is the target an indexed-array slot?
    emitter.instruction("je __rt_date_magic_restore_props_array");              // rebuild indexed storage from the parsed hash
    emitter.instruction("cmp r12, 5");                                          // is the target an associative-array slot?
    emitter.instruction("je __rt_date_magic_restore_props_heap");               // retain and restore the heap pointer
    emitter.instruction("cmp r12, 6");                                          // is the target an object slot?
    emitter.instruction("je __rt_date_magic_restore_props_heap");               // retain and restore the object pointer
    emitter.instruction("cmp r12, 9");                                          // is the target a resource slot?
    emitter.instruction("je __rt_date_magic_restore_props_heap");               // retain and restore both resource words
    emitter.instruction("cmp r12, 11");                                         // is the target an inline TaggedScalar slot?
    emitter.instruction("je __rt_date_magic_restore_props_tagged_scalar");      // restore payload plus the parsed runtime tag
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the parsed Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // unbox the scalar low payload word
    emitter.instruction("mov r11, QWORD PTR [rax + 16]");                       // unbox the scalar high payload word
    emitter.instruction("mov r13, QWORD PTR [rbp - 64]");                       // reload the target slot address
    emitter.instruction("mov QWORD PTR [r13], r10");                            // restore the scalar low payload
    emitter.instruction("mov QWORD PTR [r13 + 8], r11");                        // restore the scalar high payload
    emitter.instruction("jmp __rt_date_magic_restore_props_next");              // continue with the next descriptor row
    emitter.label("__rt_date_magic_restore_props_tagged_scalar");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the parsed Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the scalar payload word
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // load the parsed int/null runtime tag
    emitter.instruction("mov r13, QWORD PTR [rbp - 64]");                       // reload the target slot address
    emitter.instruction("mov QWORD PTR [r13], r10");                            // restore the inline scalar payload
    emitter.instruction("mov QWORD PTR [r13 + 8], r11");                        // restore the inline runtime tag
    emitter.instruction("jmp __rt_date_magic_restore_props_next");              // continue with the next descriptor row
    emitter.label("__rt_date_magic_restore_props_mixed");
    emitter.instruction("mov r13, QWORD PTR [rbp - 64]");                       // reload the target Mixed slot
    emitter.instruction("mov rdi, QWORD PTR [r13]");                            // load the previously owned Mixed cell
    emitter.instruction("call __rt_decref_mixed");                              // release the default or previous property value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the parsed Mixed cell
    emitter.instruction("call __rt_incref");                                    // retain it for the object property owner
    emitter.instruction("mov r13, QWORD PTR [rbp - 64]");                       // reload the target Mixed slot
    emitter.instruction("mov QWORD PTR [r13], rax");                            // install the retained Mixed cell
    emitter.instruction("mov QWORD PTR [r13 + 8], 0");                          // clear the unused high slot word
    emitter.instruction("jmp __rt_date_magic_restore_props_next");              // continue with the next descriptor row
    emitter.label("__rt_date_magic_restore_props_string");
    emitter.instruction("mov r13, QWORD PTR [rbp - 64]");                       // reload the target string slot
    emitter.instruction("mov rdi, QWORD PTR [r13]");                            // load the previously owned string pointer
    emitter.instruction("call __rt_decref_any");                                // release the previous string storage when managed
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the parsed Mixed cell
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the parsed string pointer
    emitter.instruction("call __rt_incref");                                    // retain the parsed string for the property owner
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the parsed Mixed cell after the retain
    emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                       // load the parsed string length
    emitter.instruction("mov r13, QWORD PTR [rbp - 64]");                       // reload the target string slot
    emitter.instruction("mov QWORD PTR [r13], rax");                            // restore the owned string pointer
    emitter.instruction("mov QWORD PTR [r13 + 8], r11");                        // restore the string length
    emitter.instruction("jmp __rt_date_magic_restore_props_next");              // continue with the next descriptor row
    emitter.label("__rt_date_magic_restore_props_array");
    emitter.instruction("mov r13, QWORD PTR [rbp - 64]");                       // reload the target array slot
    emitter.instruction("mov rdi, QWORD PTR [r13]");                            // load the previously owned indexed array
    emitter.instruction("call __rt_decref_any");                                // release the previous array storage
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the parsed Mixed cell
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the parsed associative array
    emitter.instruction("call __rt_hash_to_indexed_array");                     // rebuild native indexed-array storage
    emitter.instruction("mov r13, QWORD PTR [rbp - 64]");                       // reload the target array slot
    emitter.instruction("mov QWORD PTR [r13], rax");                            // install the rebuilt indexed array
    emitter.instruction("mov QWORD PTR [r13 + 8], 0");                          // clear the unused high slot word
    emitter.instruction("jmp __rt_date_magic_restore_props_next");              // continue with the next descriptor row
    emitter.label("__rt_date_magic_restore_props_heap");
    emitter.instruction("mov r13, QWORD PTR [rbp - 64]");                       // reload the target heap-backed slot
    emitter.instruction("mov rdi, QWORD PTR [r13]");                            // load the previously owned heap pointer
    emitter.instruction("call __rt_decref_any");                                // release the previous property storage
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the parsed Mixed cell
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the parsed heap payload pointer
    emitter.instruction("call __rt_incref");                                    // retain the payload for the property owner
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the parsed Mixed cell after the retain
    emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                       // load the parsed high payload word
    emitter.instruction("mov r13, QWORD PTR [rbp - 64]");                       // reload the target heap-backed slot
    emitter.instruction("mov QWORD PTR [r13], rax");                            // restore the retained heap payload
    emitter.instruction("mov QWORD PTR [r13 + 8], r11");                        // restore the high payload word
    emitter.label("__rt_date_magic_restore_props_next");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the property cursor
    emitter.instruction("add rax, 1");                                          // advance to the next custom property
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // persist the advanced cursor
    emitter.instruction("jmp __rt_date_magic_restore_props_loop");              // continue restoring properties
    emitter.label("__rt_date_magic_restore_props_done");
    emitter.instruction("add rsp, 96");                                         // deallocate the restore frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return after restoring custom properties
}
