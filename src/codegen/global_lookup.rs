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

    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64_helper(emitter);
        emit_x86_64_materialize(emitter);
    } else {
        emit_aarch64_helper(emitter);
        emit_aarch64_materialize(emitter);
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
    emitter.instruction("mov x0, #0");                                          // unknown names have no program storage
    emitter.instruction("ret");                                                 // return to the caller
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
    emitter.instruction("xor eax, eax");                                        // unknown names have no program storage
    emitter.instruction("ret");                                                 // return to the caller
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
    emitter.instruction("b.hs __rt_globals_materialize_done");                  // all initialized globals are in the hash
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
    emitter.instruction("jae __rt_globals_materialize_done_x86");               // all initialized globals are in the hash
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
    emitter.label("__rt_globals_materialize_done_x86");
    emitter.instruction("mov rax, r12");                                        // return the materialized hash
    emitter.instruction("pop r15");                                             // restore the table cursor
    emitter.instruction("pop r14");                                             // restore the table base
    emitter.instruction("pop r13");                                             // restore the table count
    emitter.instruction("pop r12");                                             // restore the result hash
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the materialized globals hash
}
