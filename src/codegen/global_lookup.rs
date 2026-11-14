//! Purpose:
//! Emits the per-program runtime table and lookup helper behind computed `$GLOBALS[$name]`.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` before the user data section is serialized.
//!
//! Key details:
//! - The table maps each program-global name to the address of its Mixed storage word.
//! - The helper is emitted only when lowered code contains a dynamic global load or store, so
//!   programs using literal `$GLOBALS['name']` keep direct symbol access and pay nothing.
//! - Entries use a 24-byte stride: name pointer, name byte length, global storage pointer.

use crate::codegen_support::abi;
use crate::codegen_support::data_section::{DataSection, DataWord};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::ir::{Module, Op};

/// Returns whether any lowered function reads or writes a runtime-named program global.
pub(crate) fn module_uses_dynamic_globals(module: &Module) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
        .any(|function| {
            function
                .instructions
                .iter()
                .any(|inst| matches!(
                    inst.op,
                    Op::LoadGlobalDynamic | Op::StoreGlobalDynamic | Op::LoadGlobalsArray
                ))
        })
}

/// Emits the named lookup table into the user data section and the lookup helper into the
/// user text section.
pub(crate) fn emit_dynamic_global_lookup(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let mut entries = Vec::new();
    for name in &module.program_global_names {
        if module.extern_globals.contains_key(name) {
            continue;
        }
        // Web superglobals use a native container representation, not the ordinary Mixed word.
        if module.web && crate::superglobals::is_superglobal(name) {
            continue;
        }
        let symbol = crate::names::ir_global_symbol(name);
        data.add_comm(symbol.clone(), 8);
        let (label, length) = data.add_string(name.as_bytes());
        entries.push(DataWord::Symbol(label));
        entries.push(DataWord::U64(length as u64));
        entries.push(DataWord::Symbol(symbol));
    }
    data.add_words_named(
        "_eir_global_lookup_count",
        vec![DataWord::U64((entries.len() / 3) as u64)],
    );
    data.add_words_named("_eir_global_lookup_entries", entries);
    // Runtime-only global names live in a linked list of stable cells headed here. The static
    // table cannot grow, so a computed write misses it and creates one of these entries.
    data.add_comm("_eir_dynamic_globals".to_string(), 8);

    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64_helper(emitter);
        emit_x86_64_materialize(emitter);
        emit_x86_64_dynamic_helpers(emitter);
    } else {
        emit_aarch64_helper(emitter);
        emit_aarch64_materialize(emitter);
        emit_aarch64_dynamic_helpers(emitter);
    }
}

/// Emits the ARM64 dynamic-global lookup helper.
fn emit_aarch64_helper(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: dynamic global lookup ---");
    emitter.label_global("__rt_global_lookup_dynamic");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_eir_global_lookup_count");
    emitter.instruction("ldr x3, [x9]");                                        // load the number of named program globals
    crate::codegen_support::abi::emit_symbol_address(emitter, "x4", "_eir_global_lookup_entries");
    emitter.instruction("mov x5, #0");                                          // start at the first lookup entry
    emitter.label("__rt_global_lookup_loop");
    emitter.instruction("cmp x5, x3");                                          // visited every entry?
    emitter.instruction("b.hs __rt_global_lookup_miss");                        // no match: report an unknown global
    emitter.instruction("mov x9, #24");                                         // each entry is a 24-byte (ptr, len, cell) record
    emitter.instruction("madd x6, x5, x9, x4");                                 // address the current entry
    emitter.instruction("ldr x7, [x6, #8]");                                    // entry name length
    emitter.instruction("cmp x7, x2");                                          // does the length match the requested name?
    emitter.instruction("b.ne __rt_global_lookup_next");                        // length mismatch: try the next entry
    emitter.instruction("ldr x8, [x6]");                                        // entry name pointer
    emitter.instruction("mov x9, #0");                                          // byte comparison index
    emitter.label("__rt_global_lookup_bytes");
    emitter.instruction("cmp x9, x2");                                          // compared every requested byte?
    emitter.instruction("b.hs __rt_global_lookup_hit");                         // full match: return this global's storage
    emitter.instruction("ldrb w10, [x8, x9]");                                  // entry name byte
    emitter.instruction("ldrb w11, [x1, x9]");                                  // requested name byte
    emitter.instruction("cmp w10, w11");                                        // do the bytes match?
    emitter.instruction("b.ne __rt_global_lookup_next");                        // mismatch: try the next entry
    emitter.instruction("add x9, x9, #1");                                      // advance to the next byte
    emitter.instruction("b __rt_global_lookup_bytes");                          // continue the byte comparison
    emitter.label("__rt_global_lookup_hit");
    emitter.instruction("ldr x0, [x6, #16]");                                   // return the address of the global's Mixed word
    emitter.instruction("ret");                                                 // return to the caller
    emitter.label("__rt_global_lookup_next");
    emitter.instruction("add x5, x5, #1");                                      // advance to the next table entry
    emitter.instruction("b __rt_global_lookup_loop");                           // keep searching
    emitter.label("__rt_global_lookup_miss");
    emitter.instruction("mov x0, x1");                                          // pass the requested name pointer to the runtime table
    emitter.instruction("mov x1, x2");                                          // pass the requested name length to the runtime table
    emitter.instruction("b __rt_dynamic_global_lookup");                        // unknown static names may still exist at runtime
}

/// Emits the x86_64 dynamic-global lookup helper.
fn emit_x86_64_helper(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: dynamic global lookup ---");
    emitter.label_global("__rt_global_lookup_dynamic");
    crate::codegen_support::abi::emit_symbol_address(emitter, "rax", "_eir_global_lookup_count");
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // load the number of named program globals
    crate::codegen_support::abi::emit_symbol_address(emitter, "rcx", "_eir_global_lookup_entries");
    emitter.instruction("xor edx, edx");                                        // start at the first lookup entry
    emitter.label("__rt_global_lookup_loop_x86");
    emitter.instruction("cmp rdx, rax");                                        // visited every entry?
    emitter.instruction("jae __rt_global_lookup_miss_x86");                     // no match: report an unknown global
    emitter.instruction("imul r8, rdx, 24");                                    // each entry is a 24-byte (ptr, len, cell) record
    emitter.instruction("add r8, rcx");                                         // address the current entry
    emitter.instruction("cmp QWORD PTR [r8 + 8], rsi");                         // does the length match the requested name?
    emitter.instruction("jne __rt_global_lookup_next_x86");                     // length mismatch: try the next entry
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // entry name pointer
    emitter.instruction("xor r10d, r10d");                                      // byte comparison index
    emitter.label("__rt_global_lookup_bytes_x86");
    emitter.instruction("cmp r10, rsi");                                        // compared every requested byte?
    emitter.instruction("jae __rt_global_lookup_hit_x86");                      // full match: return this global's storage
    emitter.instruction("mov r11b, BYTE PTR [r9 + r10]");                       // entry name byte
    emitter.instruction("cmp r11b, BYTE PTR [rdi + r10]");                      // do the bytes match?
    emitter.instruction("jne __rt_global_lookup_next_x86");                     // mismatch: try the next entry
    emitter.instruction("inc r10");                                             // advance to the next byte
    emitter.instruction("jmp __rt_global_lookup_bytes_x86");                    // continue the byte comparison
    emitter.label("__rt_global_lookup_hit_x86");
    emitter.instruction("mov rax, QWORD PTR [r8 + 16]");                        // return the address of the global's Mixed word
    emitter.instruction("ret");                                                 // return to the caller
    emitter.label("__rt_global_lookup_next_x86");
    emitter.instruction("inc rdx");                                             // advance to the next table entry
    emitter.instruction("jmp __rt_global_lookup_loop_x86");                     // keep searching
    emitter.label("__rt_global_lookup_miss_x86");
    emitter.instruction("jmp __rt_dynamic_global_lookup");                      // unknown static names may still exist in the runtime table
}


/// Emits the ARM64 bare-`$GLOBALS` hash builder.
///
/// The result is a fresh hash whose string keys are the program-global names and whose values
/// are the boxed Mixed cells currently stored in each initialized global slot. Uninitialized
/// slots (a zero cell or a zero boxed pointer) are omitted, matching PHP's `$GLOBALS`.
fn emit_aarch64_materialize(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: globals materialize ---");
    emitter.label_global("__rt_globals_materialize");
    emitter.instruction("sub sp, sp, #48");                                     // reserve saved callee registers
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and continuation
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("stp x19, x20, [sp, #0]");                              // preserve the hash and table count
    emitter.instruction("stp x21, x22, [sp, #16]");                             // preserve the table base and cursor
    emitter.instruction("mov x0, #8");                                          // seed the result hash with usable capacity
    emitter.instruction("mov x1, #7");                                          // result entries own boxed Mixed cells
    abi::emit_call_label(emitter, "__rt_hash_new");
    emitter.instruction("mov x19, x0");                                         // x19 = result hash
    abi::emit_symbol_address(emitter, "x9", "_eir_global_lookup_count");
    emitter.instruction("ldr x20, [x9]");                                       // x20 = number of program-global names
    abi::emit_symbol_address(emitter, "x21", "_eir_global_lookup_entries");
    emitter.instruction("mov x22, #0");                                         // start at the first table entry
    emitter.label("__rt_globals_materialize_loop");
    emitter.instruction("cmp x22, x20");                                        // visited every program-global name?
    emitter.instruction("b.hs __rt_globals_materialize_dynamic");               // static globals are in; now merge runtime entries
    emitter.instruction("mov x9, #24");                                         // each entry is a 24-byte (name, length, cell) record
    emitter.instruction("madd x10, x22, x9, x21");                              // address the current table entry
    emitter.instruction("ldr x11, [x10, #16]");                                 // load the global storage cell address
    emitter.instruction("cbz x11, __rt_globals_materialize_next");              // skip an uninitialized slot
    emitter.instruction("ldr x12, [x11]");                                      // load the global's boxed Mixed pointer
    emitter.instruction("cbz x12, __rt_globals_materialize_next");              // skip a global that was never stored
    emitter.instruction("mov x0, x12");                                         // retain the global's boxed value for the hash
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("mov x9, #24");                                         // reload the entry after the retain helper
    emitter.instruction("madd x10, x22, x9, x21");                              // address the current table entry again
    emitter.instruction("ldr x11, [x10, #16]");                                 // reload the global storage cell address
    emitter.instruction("ldr x12, [x11]");                                      // reload the boxed Mixed pointer
    emitter.instruction("mov x0, x19");                                         // pass the result hash
    emitter.instruction("ldr x1, [x10]");                                       // pass the name pointer
    emitter.instruction("ldr x2, [x10, #8]");                                   // pass the name byte length
    emitter.instruction("mov x3, x12");                                         // pass the owned boxed value
    emitter.instruction("mov x4, #0");                                          // boxed Mixed values use one payload word
    emitter.instruction("mov x5, #7");                                          // the hash entry owns a boxed Mixed cell
    abi::emit_call_label(emitter, "__rt_hash_set");
    emitter.instruction("mov x19, x0");                                         // preserve a possibly relocated hash
    emitter.label("__rt_globals_materialize_next");
    emitter.instruction("add x22, x22, #1");                                    // advance to the next program-global name
    emitter.instruction("b __rt_globals_materialize_loop");                     // continue until every entry is visited
    emitter.label("__rt_globals_materialize_dynamic");
    abi::emit_symbol_address(emitter, "x9", "_eir_dynamic_globals");
    emitter.instruction("ldr x22, [x9]");                                       // start at the runtime-created global list
    emitter.label("__rt_globals_materialize_dynamic_loop");
    emitter.instruction("cbz x22, __rt_globals_materialize_done");              // no more runtime-created globals
    emitter.instruction("ldr x0, [x22, #24]");                                  // load the runtime global's boxed Mixed pointer
    emitter.instruction("cbz x0, __rt_globals_materialize_dynamic_next");       // skip an unset runtime global
    emitter.instruction("str x22, [sp, #-16]!");                                // preserve the entry across the retain helper
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("ldr x22, [sp], #16");                                  // restore the runtime global entry
    emitter.instruction("ldr x12, [x22, #24]");                                 // reload the boxed Mixed pointer
    emitter.instruction("mov x0, x19");                                         // pass the materialized hash
    emitter.instruction("ldr x1, [x22, #8]");                                   // pass the runtime global's name pointer
    emitter.instruction("ldr x2, [x22, #16]");                                  // pass its name byte length
    emitter.instruction("mov x3, x12");                                         // pass the owned boxed value
    emitter.instruction("mov x4, #0");                                          // boxed Mixed values use one payload word
    emitter.instruction("mov x5, #7");                                          // the hash entry owns a boxed Mixed cell
    abi::emit_call_label(emitter, "__rt_hash_set");
    emitter.instruction("mov x19, x0");                                         // preserve a possibly relocated hash
    emitter.label("__rt_globals_materialize_dynamic_next");
    emitter.instruction("ldr x22, [x22, #0]");                                  // advance to the next runtime global
    emitter.instruction("b __rt_globals_materialize_dynamic_loop");             // continue until the runtime list ends
    emitter.label("__rt_globals_materialize_done");
    emitter.instruction("mov x0, x19");                                         // return the materialized hash
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore the hash and table count
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore the table base and cursor
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and continuation
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the materialized globals hash
}

/// Emits the x86_64 bare-`$GLOBALS` hash builder using the System V calling convention.
fn emit_x86_64_materialize(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: globals materialize ---");
    emitter.label_global("__rt_globals_materialize");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("push r12");                                            // preserve the result hash
    emitter.instruction("push r13");                                            // preserve the table count
    emitter.instruction("push r14");                                            // preserve the table base
    emitter.instruction("push r15");                                            // preserve the table cursor
    emitter.instruction("mov edi, 8");                                          // seed the result hash with usable capacity
    emitter.instruction("mov esi, 7");                                          // result entries own boxed Mixed cells
    abi::emit_call_label(emitter, "__rt_hash_new");
    emitter.instruction("mov r12, rax");                                        // r12 = result hash
    abi::emit_symbol_address(emitter, "r13", "_eir_global_lookup_count");
    emitter.instruction("mov r13, QWORD PTR [r13]");                            // r13 = number of program-global names
    abi::emit_symbol_address(emitter, "r14", "_eir_global_lookup_entries");
    emitter.instruction("xor r15d, r15d");                                      // start at the first table entry
    emitter.label("__rt_globals_materialize_loop_x86");
    emitter.instruction("cmp r15, r13");                                        // visited every program-global name?
    emitter.instruction("jae __rt_globals_materialize_dynamic_x86");            // static globals are in; now merge runtime entries
    emitter.instruction("mov r10, r15");                                        // copy the table cursor before scaling
    emitter.instruction("imul r10, 24");                                        // each entry is a 24-byte record
    emitter.instruction("add r10, r14");                                        // address the current table entry
    emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                       // load the global storage cell address
    emitter.instruction("test r11, r11");                                       // is the slot initialized?
    emitter.instruction("jz __rt_globals_materialize_next_x86");                // skip an uninitialized slot
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // load the global's boxed Mixed pointer
    emitter.instruction("test rax, rax");                                       // was the global ever stored?
    emitter.instruction("jz __rt_globals_materialize_next_x86");                // skip a global that was never stored
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("mov r10, r15");                                        // reload the table cursor after the retain helper
    emitter.instruction("imul r10, 24");                                        // each entry is a 24-byte record
    emitter.instruction("add r10, r14");                                        // address the current table entry again
    emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                       // reload the global storage cell address
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // reload the boxed Mixed pointer
    emitter.instruction("mov rdi, r12");                                        // pass the result hash
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // pass the name pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // pass the name byte length
    emitter.instruction("mov rcx, rax");                                        // pass the owned boxed value
    emitter.instruction("xor r8d, r8d");                                        // boxed Mixed values use one payload word
    emitter.instruction("mov r9d, 7");                                          // the hash entry owns a boxed Mixed cell
    abi::emit_call_label(emitter, "__rt_hash_set");
    emitter.instruction("mov r12, rax");                                        // preserve a possibly relocated hash
    emitter.label("__rt_globals_materialize_next_x86");
    emitter.instruction("inc r15");                                             // advance to the next program-global name
    emitter.instruction("jmp __rt_globals_materialize_loop_x86");               // continue until every entry is visited
    emitter.label("__rt_globals_materialize_dynamic_x86");
    abi::emit_symbol_address(emitter, "r10", "_eir_dynamic_globals");
    emitter.instruction("mov r15, QWORD PTR [r10]");                            // start at the runtime-created global list
    emitter.label("__rt_globals_materialize_dynamic_loop_x86");
    emitter.instruction("test r15, r15");                                       // no more runtime-created globals?
    emitter.instruction("jz __rt_globals_materialize_done_x86");                // finish once the runtime list ends
    emitter.instruction("mov rax, QWORD PTR [r15 + 24]");                       // load the runtime global's boxed Mixed pointer
    emitter.instruction("test rax, rax");                                       // was the runtime global ever stored?
    emitter.instruction("jz __rt_globals_materialize_dynamic_next_x86");        // skip an unset runtime global
    emitter.instruction("sub rsp, 16");                                         // preserve the entry across the retain helper
    emitter.instruction("mov QWORD PTR [rsp], r15");                            // stash the current runtime global entry
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("mov r15, QWORD PTR [rsp]");                            // restore the current runtime global entry
    emitter.instruction("add rsp, 16");                                         // release the temporary entry slot
    emitter.instruction("mov rax, QWORD PTR [r15 + 24]");                       // reload the boxed Mixed pointer
    emitter.instruction("mov rdi, r12");                                        // pass the materialized hash
    emitter.instruction("mov rsi, QWORD PTR [r15 + 8]");                        // pass the runtime global's name pointer
    emitter.instruction("mov rdx, QWORD PTR [r15 + 16]");                       // pass its name byte length
    emitter.instruction("mov rcx, rax");                                        // pass the owned boxed value
    emitter.instruction("xor r8d, r8d");                                        // boxed Mixed values use one payload word
    emitter.instruction("mov r9d, 7");                                          // the hash entry owns a boxed Mixed cell
    abi::emit_call_label(emitter, "__rt_hash_set");
    emitter.instruction("mov r12, rax");                                        // preserve a possibly relocated hash
    emitter.label("__rt_globals_materialize_dynamic_next_x86");
    emitter.instruction("mov r15, QWORD PTR [r15]");                            // advance to the next runtime global
    emitter.instruction("jmp __rt_globals_materialize_dynamic_loop_x86");       // continue until the runtime list ends
    emitter.label("__rt_globals_materialize_done_x86");
    emitter.instruction("mov rax, r12");                                        // return the materialized hash
    emitter.instruction("pop r15");                                             // restore the table cursor
    emitter.instruction("pop r14");                                             // restore the table base
    emitter.instruction("pop r13");                                             // restore the table count
    emitter.instruction("pop r12");                                             // restore the result hash
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the materialized globals hash
}

/// Emits the ARM64 runtime-global lookup, creation, unset, and shutdown helpers.
fn emit_aarch64_dynamic_helpers(emitter: &mut Emitter) {
    // -- lookup: x0 = name pointer, x1 = name length, returns the stable cell or zero --
    emitter.blank();
    emitter.comment("--- runtime: dynamic global lookup ---");
    emitter.label_global("__rt_dynamic_global_lookup");
    abi::emit_symbol_address(emitter, "x9", "_eir_dynamic_globals");
    emitter.instruction("ldr x10, [x9]");                                       // start at the runtime-global list head
    emitter.label("__rt_dynamic_global_lookup_loop");
    emitter.instruction("cbz x10, __rt_dynamic_global_lookup_miss");            // end of the runtime list
    emitter.instruction("ldr x11, [x10, #16]");                                 // entry name byte length
    emitter.instruction("cmp x11, x1");                                         // does the length match?
    emitter.instruction("b.ne __rt_dynamic_global_lookup_next");                // try the next runtime entry
    emitter.instruction("ldr x12, [x10, #8]");                                  // entry name pointer
    emitter.instruction("mov x13, #0");                                         // byte comparison index
    emitter.label("__rt_dynamic_global_lookup_bytes");
    emitter.instruction("cmp x13, x1");                                         // compared every requested byte?
    emitter.instruction("b.hs __rt_dynamic_global_lookup_hit");                 // full match: return this cell
    emitter.instruction("ldrb w14, [x12, x13]");                                // entry name byte
    emitter.instruction("ldrb w15, [x0, x13]");                                 // requested name byte
    emitter.instruction("cmp w14, w15");                                        // do the bytes match?
    emitter.instruction("b.ne __rt_dynamic_global_lookup_next");                // mismatch: try the next entry
    emitter.instruction("add x13, x13, #1");                                    // advance to the next byte
    emitter.instruction("b __rt_dynamic_global_lookup_bytes");                  // continue the byte comparison
    emitter.label("__rt_dynamic_global_lookup_hit");
    emitter.instruction("add x0, x10, #24");                                    // return the entry's stable cell address
    emitter.instruction("ret");                                                 // return to the caller
    emitter.label("__rt_dynamic_global_lookup_next");
    emitter.instruction("ldr x10, [x10, #0]");                                  // advance to the next runtime entry
    emitter.instruction("b __rt_dynamic_global_lookup_loop");                   // keep searching
    emitter.label("__rt_dynamic_global_lookup_miss");
    emitter.instruction("mov x0, #0");                                          // no runtime cell for this name
    emitter.instruction("ret");                                                 // return to the caller

    // -- lookup-or-create: x0 = name pointer, x1 = name length, returns the stable cell --
    emitter.blank();
    emitter.comment("--- runtime: dynamic global lookup or create ---");
    emitter.label_global("__rt_global_lookup_or_create_dynamic");
    emitter.instruction("sub sp, sp, #48");                                     // reserve the saved callee registers
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and continuation
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("stp x19, x20, [sp, #0]");                              // preserve the name pointer and length
    emitter.instruction("stp x21, x22, [sp, #16]");                             // preserve the entry and copied-name pointer
    emitter.instruction("mov x19, x0");                                         // keep the name pointer
    emitter.instruction("mov x20, x1");                                         // keep the name length
    emitter.instruction("mov x1, x19");                                         // static lookup expects the name pointer in x1
    emitter.instruction("mov x2, x20");                                         // and the name length in x2
    abi::emit_call_label(emitter, "__rt_global_lookup_dynamic");
    emitter.instruction("cbnz x0, __rt_global_lookup_or_create_dynamic_done");  // static or existing runtime entry
    emitter.instruction("mov x0, #32");                                         // an entry is next/key pointer/key length/cell
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction("mov x21, x0");                                         // x21 = new runtime entry
    emitter.instruction("str xzr, [x21, #0]");                                  // clear the next link
    emitter.instruction("str xzr, [x21, #8]");                                  // clear the name pointer
    emitter.instruction("str xzr, [x21, #16]");                                 // clear the name length
    emitter.instruction("str xzr, [x21, #24]");                                 // clear the stable cell
    emitter.instruction("mov x22, #0");                                         // copied-name pointer defaults to null
    emitter.instruction("cbz x20, __rt_global_lookup_or_create_dynamic_name");  // an empty name has no bytes to copy
    emitter.instruction("mov x0, x20");                                         // allocate the name byte block
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction("mov x22, x0");                                         // x22 = copied-name buffer
    emitter.instruction("mov x9, #0");                                          // byte copy index
    emitter.label("__rt_global_lookup_or_create_dynamic_copy");
    emitter.instruction("cmp x9, x20");                                         // copied every name byte?
    emitter.instruction("b.hs __rt_global_lookup_or_create_dynamic_name");      // name bytes are ready
    emitter.instruction("ldrb w10, [x19, x9]");                                 // load the source name byte
    emitter.instruction("strb w10, [x22, x9]");                                 // store it in the persistent buffer
    emitter.instruction("add x9, x9, #1");                                      // advance to the next byte
    emitter.instruction("b __rt_global_lookup_or_create_dynamic_copy");         // continue copying
    emitter.label("__rt_global_lookup_or_create_dynamic_name");
    emitter.instruction("str x22, [x21, #8]");                                  // publish the copied name pointer
    emitter.instruction("str x20, [x21, #16]");                                 // publish the name byte length
    abi::emit_symbol_address(emitter, "x9", "_eir_dynamic_globals");
    emitter.instruction("ldr x10, [x9]");                                       // load the current list head
    emitter.instruction("str x10, [x21, #0]");                                  // link the new entry before the old head
    emitter.instruction("str x21, [x9]");                                       // publish the new list head
    emitter.instruction("add x0, x21, #24");                                    // return the stable cell address
    emitter.label("__rt_global_lookup_or_create_dynamic_done");
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore the name pointer and length
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore the entry and name buffer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and continuation
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the stable cell

    // -- unset: x0 = name pointer, x1 = name length; clears a static or runtime cell --
    emitter.blank();
    emitter.comment("--- runtime: dynamic global unset ---");
    emitter.label_global("__rt_global_unset_dynamic");
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller continuation
    emitter.instruction("mov x2, x1");                                          // static lookup expects the name length in x2
    emitter.instruction("mov x1, x0");                                          // and the name pointer in x1
    abi::emit_call_label(emitter, "__rt_global_lookup_dynamic");
    emitter.instruction("cbz x0, __rt_global_unset_dynamic_done");              // unknown name: nothing to remove
    emitter.instruction("ldr x9, [x0]");                                        // capture the previous Mixed owner
    emitter.instruction("str xzr, [x0]");                                       // clear the cell so the global reads as unset
    emitter.instruction("cbz x9, __rt_global_unset_dynamic_done");              // nothing to release when already unset
    emitter.instruction("mov x0, x9");                                          // pass the retired Mixed cell
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    emitter.label("__rt_global_unset_dynamic_done");
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller continuation
    emitter.instruction("ret");                                                 // return to the caller

    // -- shutdown cleanup: release every runtime-created global and its name --
    emitter.blank();
    emitter.comment("--- runtime: dynamic globals cleanup ---");
    emitter.label_global("__rt_dynamic_globals_cleanup");
    emitter.instruction("sub sp, sp, #48");                                     // reserve the cleanup frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and continuation
    emitter.instruction("add x29, sp, #32");                                    // establish the cleanup frame pointer
    emitter.instruction("stp x19, x20, [sp, #0]");                              // preserve the current entry and next link
    abi::emit_symbol_address(emitter, "x9", "_eir_dynamic_globals");
    emitter.instruction("ldr x19, [x9]");                                       // start at the runtime-global list head
    emitter.instruction("str xzr, [x9]");                                       // clear the list head before freeing entries
    emitter.label("__rt_dynamic_globals_cleanup_loop");
    emitter.instruction("cbz x19, __rt_dynamic_globals_cleanup_done");          // no more runtime globals
    emitter.instruction("ldr x20, [x19, #0]");                                  // preserve the next entry across frees
    emitter.instruction("ldr x9, [x19, #24]");                                  // load the cell's current Mixed owner
    emitter.instruction("cbz x9, __rt_dynamic_globals_cleanup_name");           // unset runtime globals have no value
    emitter.instruction("str xzr, [x19, #24]");                                 // clear the cell before releasing it
    emitter.instruction("mov x0, x9");                                          // pass the retired Mixed cell
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    emitter.label("__rt_dynamic_globals_cleanup_name");
    emitter.instruction("ldr x9, [x19, #8]");                                   // load the copied name buffer
    emitter.instruction("cbz x9, __rt_dynamic_globals_cleanup_entry");          // an empty name has no buffer
    emitter.instruction("mov x0, x9");                                          // pass the name buffer to the heap
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    emitter.label("__rt_dynamic_globals_cleanup_entry");
    emitter.instruction("mov x0, x19");                                         // pass the runtime entry block to the heap
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    emitter.instruction("mov x19, x20");                                        // advance to the preserved next entry
    emitter.instruction("b __rt_dynamic_globals_cleanup_loop");                 // continue until the list is empty
    emitter.label("__rt_dynamic_globals_cleanup_done");
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore the cleanup temporaries
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and continuation
    emitter.instruction("add sp, sp, #48");                                     // release the cleanup frame
    emitter.instruction("ret");                                                 // return to the shutdown path
}

/// Emits the x86_64 runtime-global lookup, creation, unset, and shutdown helpers.
fn emit_x86_64_dynamic_helpers(emitter: &mut Emitter) {
    // -- lookup: rdi = name pointer, rsi = name length, returns the stable cell or zero --
    emitter.blank();
    emitter.comment("--- runtime: dynamic global lookup ---");
    emitter.label_global("__rt_dynamic_global_lookup");
    abi::emit_symbol_address(emitter, "r10", "_eir_dynamic_globals");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // start at the runtime-global list head
    emitter.label("__rt_dynamic_global_lookup_loop_x86");
    emitter.instruction("test r10, r10");                                       // end of the runtime list?
    emitter.instruction("jz __rt_dynamic_global_lookup_miss_x86");              // no matching runtime global
    emitter.instruction("cmp QWORD PTR [r10 + 16], rsi");                       // does the name length match?
    emitter.instruction("jne __rt_dynamic_global_lookup_next_x86");             // try the next runtime entry
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // entry name pointer
    emitter.instruction("xor rcx, rcx");                                        // byte comparison index
    emitter.label("__rt_dynamic_global_lookup_bytes_x86");
    emitter.instruction("cmp rcx, rsi");                                        // compared every requested byte?
    emitter.instruction("jae __rt_dynamic_global_lookup_hit_x86");              // full match: return this cell
    emitter.instruction("mov al, BYTE PTR [r11 + rcx]");                        // entry name byte
    emitter.instruction("cmp al, BYTE PTR [rdi + rcx]");                        // do the bytes match?
    emitter.instruction("jne __rt_dynamic_global_lookup_next_x86");             // mismatch: try the next entry
    emitter.instruction("inc rcx");                                             // advance to the next byte
    emitter.instruction("jmp __rt_dynamic_global_lookup_bytes_x86");            // continue the byte comparison
    emitter.label("__rt_dynamic_global_lookup_hit_x86");
    emitter.instruction("lea rax, [r10 + 24]");                                 // return the entry's stable cell address
    emitter.instruction("ret");                                                 // return to the caller
    emitter.label("__rt_dynamic_global_lookup_next_x86");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // advance to the next runtime entry
    emitter.instruction("jmp __rt_dynamic_global_lookup_loop_x86");             // keep searching
    emitter.label("__rt_dynamic_global_lookup_miss_x86");
    emitter.instruction("xor eax, eax");                                        // no runtime cell for this name
    emitter.instruction("ret");                                                 // return to the caller

    // -- lookup-or-create: rdi = name pointer, rsi = name length, returns the stable cell --
    emitter.blank();
    emitter.comment("--- runtime: dynamic global lookup or create ---");
    emitter.label_global("__rt_global_lookup_or_create_dynamic");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame
    emitter.instruction("push r12");                                            // preserve the name pointer
    emitter.instruction("push r13");                                            // preserve the name length
    emitter.instruction("push r14");                                            // preserve the new entry
    emitter.instruction("push r15");                                            // preserve the copied-name buffer
    emitter.instruction("mov r12, rdi");                                        // keep the name pointer
    emitter.instruction("mov r13, rsi");                                        // keep the name length
    emitter.instruction("call __rt_global_lookup_dynamic");                     // reuse the static and runtime lookup
    emitter.instruction("test rax, rax");                                       // does the global already exist?
    emitter.instruction("jnz __rt_global_lookup_or_create_dynamic_done_x86");   // return the existing cell
    emitter.instruction("mov rax, 32");                                         // an entry is next/key pointer/key length/cell
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction("mov r14, rax");                                        // r14 = new runtime entry
    emitter.instruction("xor ecx, ecx");                                        // clear the entry's link and name fields
    emitter.instruction("mov QWORD PTR [r14], rcx");                            // clear the next link
    emitter.instruction("mov QWORD PTR [r14 + 8], rcx");                        // clear the name pointer
    emitter.instruction("mov QWORD PTR [r14 + 16], rcx");                       // clear the name length
    emitter.instruction("mov QWORD PTR [r14 + 24], rcx");                       // clear the stable cell
    emitter.instruction("xor r15d, r15d");                                      // copied-name pointer defaults to null
    emitter.instruction("test r13, r13");                                       // an empty name has no bytes to copy
    emitter.instruction("jz __rt_global_lookup_or_create_dynamic_name_x86");    // publish the empty name directly
    emitter.instruction("mov rax, r13");                                        // allocate the name byte block
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction("mov r15, rax");                                        // r15 = copied-name buffer
    emitter.instruction("xor rcx, rcx");                                        // byte copy index
    emitter.label("__rt_global_lookup_or_create_dynamic_copy_x86");
    emitter.instruction("cmp rcx, r13");                                        // copied every name byte?
    emitter.instruction("jae __rt_global_lookup_or_create_dynamic_name_x86");   // name bytes are ready
    emitter.instruction("mov al, BYTE PTR [r12 + rcx]");                        // load the source name byte
    emitter.instruction("mov BYTE PTR [r15 + rcx], al");                        // store it in the persistent buffer
    emitter.instruction("inc rcx");                                             // advance to the next byte
    emitter.instruction("jmp __rt_global_lookup_or_create_dynamic_copy_x86");   // continue copying
    emitter.label("__rt_global_lookup_or_create_dynamic_name_x86");
    emitter.instruction("mov QWORD PTR [r14 + 8], r15");                        // publish the copied name pointer
    emitter.instruction("mov QWORD PTR [r14 + 16], r13");                       // publish the name byte length
    abi::emit_symbol_address(emitter, "r10", "_eir_dynamic_globals");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current list head
    emitter.instruction("mov QWORD PTR [r14], r11");                            // link the new entry before the old head
    emitter.instruction("mov QWORD PTR [r10], r14");                            // publish the new list head
    emitter.instruction("lea rax, [r14 + 24]");                                 // return the stable cell address
    emitter.label("__rt_global_lookup_or_create_dynamic_done_x86");
    emitter.instruction("pop r15");                                             // restore the copied-name buffer
    emitter.instruction("pop r14");                                             // restore the new entry
    emitter.instruction("pop r13");                                             // restore the name length
    emitter.instruction("pop r12");                                             // restore the name pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the stable cell

    // -- unset: rdi = name pointer, rsi = name length; clears a static or runtime cell --
    emitter.blank();
    emitter.comment("--- runtime: dynamic global unset ---");
    emitter.label_global("__rt_global_unset_dynamic");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame
    abi::emit_call_label(emitter, "__rt_global_lookup_dynamic");
    emitter.instruction("test rax, rax");                                       // unknown name: nothing to remove
    emitter.instruction("jz __rt_global_unset_dynamic_done_x86");               // return without touching the heap
    emitter.instruction("mov r10, rax");                                        // keep the target cell address
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // capture the previous Mixed owner
    emitter.instruction("mov QWORD PTR [r10], 0");                              // clear the cell so the global reads as unset
    emitter.instruction("test rax, rax");                                       // nothing to release when already unset
    emitter.instruction("jz __rt_global_unset_dynamic_done_x86");               // return without touching the heap
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    emitter.label("__rt_global_unset_dynamic_done_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller

    // -- shutdown cleanup: release every runtime-created global and its name --
    emitter.blank();
    emitter.comment("--- runtime: dynamic globals cleanup ---");
    emitter.label_global("__rt_dynamic_globals_cleanup");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame
    emitter.instruction("push r12");                                            // preserve the current entry
    emitter.instruction("push r13");                                            // preserve the next link
    abi::emit_symbol_address(emitter, "r10", "_eir_dynamic_globals");
    emitter.instruction("mov r12, QWORD PTR [r10]");                            // start at the runtime-global list head
    emitter.instruction("mov QWORD PTR [r10], 0");                              // clear the list head before freeing entries
    emitter.label("__rt_dynamic_globals_cleanup_loop_x86");
    emitter.instruction("test r12, r12");                                       // no more runtime globals?
    emitter.instruction("jz __rt_dynamic_globals_cleanup_done_x86");            // finish the cleanup
    emitter.instruction("mov r13, QWORD PTR [r12]");                            // preserve the next entry across frees
    emitter.instruction("mov rax, QWORD PTR [r12 + 24]");                       // load the cell's current Mixed owner
    emitter.instruction("test rax, rax");                                       // unset runtime globals have no value
    emitter.instruction("jz __rt_dynamic_globals_cleanup_name_x86");            // skip the value release
    emitter.instruction("mov QWORD PTR [r12 + 24], 0");                         // clear the cell before releasing it
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    emitter.label("__rt_dynamic_globals_cleanup_name_x86");
    emitter.instruction("mov rax, QWORD PTR [r12 + 8]");                        // load the copied name buffer
    emitter.instruction("test rax, rax");                                       // an empty name has no buffer
    emitter.instruction("jz __rt_dynamic_globals_cleanup_entry_x86");           // skip the name release
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    emitter.label("__rt_dynamic_globals_cleanup_entry_x86");
    emitter.instruction("mov rax, r12");                                        // pass the runtime entry block to the heap
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    emitter.instruction("mov r12, r13");                                        // advance to the preserved next entry
    emitter.instruction("jmp __rt_dynamic_globals_cleanup_loop_x86");           // continue until the list is empty
    emitter.label("__rt_dynamic_globals_cleanup_done_x86");
    emitter.instruction("pop r13");                                             // restore the next-link temporary
    emitter.instruction("pop r12");                                             // restore the current-entry temporary
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the shutdown path
}
