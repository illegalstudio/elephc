//! Purpose:
//! Constructs independent native indexed and associative arrays from validated mbstring result descriptors.
//!
//! Called from:
//! - The shared graph restorer after engine dispatch has returned without a request-state borrow.
//!
//! Key details:
//! - Completed child arrays are retained, binary strings and keys are copied, and scalar bits survive.
//! - Every table remains uniquely owned during insertion; shared children are never mutated.
//! - Only scalar/array values reach these callbacks, so ownership release cannot execute PHP.
//! - Optional per-entry INI tokens bind copied strings before their wire leases are consumed.

use super::*;

#[cfg(test)]
mod tests;

/// Emits the graph entry and allocation/release callbacks for the current native architecture.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_restore_array");
    let arm = emitter.target.arch == Arch::AArch64;
    abi::emit_symbol_address(emitter, if arm { "x2" } else { "rdx" }, "__rt_mbstring_build_array");
    abi::emit_symbol_address(emitter, if arm { "x3" } else { "rcx" }, "__rt_mbstring_drop_array");
    abi::emit_load_int_immediate(emitter, if arm { "x4" } else { "r8" }, 0);
    let symbol = emitter.target.extern_symbol("elephc_mbstring_restore_v1");
    emitter.instruction(&format!("{} {symbol}", if arm { "b" } else { "jmp" })); // tail-call the allocator-neutral restorer through the target C ABI
    emitter.label_global("__rt_mbstring_restore_ini_array");
    abi::emit_symbol_address(emitter, if arm { "x3" } else { "rcx" }, "__rt_mbstring_build_ini_array");
    abi::emit_symbol_address(emitter, if arm { "x4" } else { "r8" }, "__rt_mbstring_drop_array");
    abi::emit_load_int_immediate(emitter, if arm { "x5" } else { "r9" }, 0);
    let symbol = emitter.target.extern_symbol("elephc_mbstring_restore_ini_v1");
    emitter.instruction(&format!("{} {symbol}", if arm { "b" } else { "jmp" })); // preserve graph length as the third argument of the identity-aware restorer
    emitter.label_global("__rt_mbstring_drop_array");
    emitter.instruction(if arm { "mov x0, x1" } else { "mov rax, rsi" });       // adapt the owned callback handle to the native release convention
    abi::emit_jump(emitter, "__rt_decref_any");
    if arm { aarch64(emitter); } else { x86_64(emitter); }
}

/// Builds one aarch64 native array from borrowed descriptors with independent child ownership.
fn aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_build_array");
    emitter.instruction("mov x5, #0");                                          // ordinary graphs have no per-entry INI identity vector
    emitter.label_global("__rt_mbstring_build_ini_array");
    emitter.instruction("sub sp, sp, #96");                                     // reserve construction state and aligned caller linkage
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the Rust caller across native allocations
    emitter.instruction("add x29, sp, #80");                                    // establish the constructor frame
    emitter.instruction("stp x1, x2, [sp]");                                    // retain count and borrowed key descriptors
    emitter.instruction("stp x3, xzr, [sp, #16]");                              // retain value descriptors and initialize the insertion index
    emitter.instruction("str x4, [sp, #64]");                                   // preserve the required indexed or associative layout
    emitter.instruction("str x5, [sp, #72]");                                   // borrow the optional identity vector through this construction callback
    emitter.instruction("mov x0, x1");                                          // request capacity for the validated entry count
    emitter.instruction("cbnz x4, __rt_mbstring_build_new_list");               // use indexed storage for PHP list keys
    emitter.instruction("mov x1, #7");                                          // mark the associative table as heterogeneous
    abi::emit_call_label(emitter, "__rt_hash_new");
    emitter.instruction("b __rt_mbstring_build_new_ready");                     // join fresh array ownership after associative allocation
    emitter.label("__rt_mbstring_build_new_list");
    emitter.instruction("mov x1, #8");                                          // store owned Mixed cell pointers in indexed elements
    abi::emit_call_label(emitter, "__rt_array_new");
    emitter.instruction("ldr x9, [x0, #-8]");                                   // preserve allocation and indexed heap metadata
    emitter.instruction("orr x9, x9, #0x700");                                  // mark boxed Mixed elements for cleanup and traversal
    emitter.instruction("str x9, [x0, #-8]");                                   // publish indexed ownership before any element allocation
    emitter.label("__rt_mbstring_build_new_ready");
    emitter.instruction("str x0, [sp, #32]");                                   // own the fresh unique table until returning it
    emitter.label("__rt_mbstring_build_loop");
    emitter.instruction("ldp x9, x10, [sp, #16]");                              // load value descriptors and current insertion index
    emitter.instruction("ldr x11, [sp]");                                       // load the expected entry count
    emitter.instruction("cmp x10, x11");                                        // check whether every entry is initialized
    emitter.instruction("b.eq __rt_mbstring_build_done");                       // transfer the completed table after the last entry
    emitter.instruction("mov x11, #24");                                        // select the concrete descriptor size
    emitter.instruction("madd x9, x10, x11, x9");                               // address the current borrowed value
    emitter.instruction("ldr x10, [x9]");                                       // load the concrete PHP value tag
    emitter.instruction("ldr x0, [x9, #8]");                                    // load the low payload without changing scalar bits
    emitter.instruction("ldr x2, [x9, #16]");                                   // load the binary string length or unused high payload
    emitter.instruction("str x10, [sp, #40]");                                  // preserve the value tag across ownership acquisition
    emitter.instruction("stp x0, x2, [sp, #48]");                               // preserve both payload words across native helpers
    emitter.instruction("ldr x9, [sp, #64]");                                   // inspect the selected array representation
    emitter.instruction("cbnz x9, __rt_mbstring_build_list_value");             // box borrowed descriptors directly for indexed arrays
    emitter.instruction("cmp x10, #1");                                         // recognize borrowed binary strings
    emitter.instruction("b.eq __rt_mbstring_build_string");                     // copy string bytes before the Rust buffer can be released
    emitter.instruction("cmp x10, #5");                                         // recognize already completed child arrays
    emitter.instruction("b.hi __rt_mbstring_build_value_ready");                // immediate values above the array tags carry no child ownership
    emitter.instruction("cmp x10, #4");                                         // accept completed indexed and associative children
    emitter.instruction("b.lo __rt_mbstring_build_value_ready");                // immediate scalars require no ownership acquisition
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("b __rt_mbstring_build_value_ready");                   // retain children independently from the restorer arena
    emitter.label("__rt_mbstring_build_string");
    emitter.instruction("mov x1, x0");                                          // adapt the borrowed byte pointer to the native string convention
    abi::emit_call_label(emitter, "__rt_str_persist");
    emitter.instruction("stp x1, x2, [sp, #48]");                               // transfer the persistent string owner into the pending entry
    bind_string(emitter, false);
    emitter.label("__rt_mbstring_build_value_ready");
    emitter.instruction("ldr x9, [sp, #8]");                                    // load borrowed key descriptors
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current insertion index
    emitter.instruction("mov x11, #24");                                        // select the key descriptor size
    emitter.instruction("madd x9, x10, x11, x9");                               // address the current normalized key
    emitter.instruction("ldp x10, x1, [x9]");                                   // load key kind and exact low payload
    emitter.instruction("ldr x2, [x9, #16]");                                   // load the string key length
    emitter.instruction("cmp x10, #0");                                         // distinguish integer keys from numeric string keys
    emitter.instruction("mov x11, #-1");                                        // prepare the native integer-key sentinel
    emitter.instruction("csel x2, x11, x2, eq");                                // retain exact integer or string key identity
    emitter.instruction("ldr x0, [sp, #32]");                                   // load the still uniquely owned destination table
    emitter.instruction("ldp x3, x4, [sp, #48]");                               // transfer the acquired value payload to insertion
    emitter.instruction("ldr x5, [sp, #40]");                                   // restore the concrete per-entry value tag
    abi::emit_call_label(emitter, "__rt_hash_set");
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the current table if insertion grew its storage
    emitter.label("__rt_mbstring_build_advance");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the current initialized entry count
    emitter.instruction("add x9, x9, #1");                                      // advance to the next validated entry
    emitter.instruction("str x9, [sp, #24]");                                   // save progress across the next allocation
    emitter.instruction("b __rt_mbstring_build_loop");                          // construct every key and value in insertion order
    emitter.label("__rt_mbstring_build_list_value");
    emitter.instruction("mov x1, x0");                                          // pass the borrowed scalar or child payload to boxing
    emitter.instruction("mov x0, x10");                                         // preserve the concrete descriptor tag
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    bind_string(emitter, true);
    emitter.instruction("ldr x9, [sp, #32]");                                   // recover the unique indexed destination
    emitter.instruction("ldr x10, [sp, #24]");                                  // recover the next initialized element index
    emitter.instruction("add x11, x9, #24");                                    // skip the indexed header
    emitter.instruction("str x0, [x11, x10, lsl #3]");                          // transfer the fresh boxed owner into its element slot
    emitter.instruction("add x10, x10, #1");                                    // include the newly initialized owner in cleanup
    emitter.instruction("str x10, [x9]");                                       // expose exactly the initialized elements
    emitter.instruction("b __rt_mbstring_build_advance");                       // share progress with associative construction
    emitter.label("__rt_mbstring_build_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // transfer the single completed table owner to Rust
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore caller linkage after every native helper returned
    emitter.instruction("add sp, sp, #96");                                     // release constructor-local storage
    emitter.instruction("ret");                                                 // return the owned associative-array handle
}

/// Builds one x86_64 native array from borrowed descriptors with independent child ownership.
fn x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_build_array");
    emitter.instruction("xor r9d, r9d");                                        // ordinary graphs have no per-entry INI identity vector
    emitter.label_global("__rt_mbstring_build_ini_array");
    emitter.instruction("push rbp");                                            // preserve caller linkage and align nested C calls
    emitter.instruction("mov rbp, rsp");                                        // establish the constructor frame
    emitter.instruction("sub rsp, 80");                                         // reserve descriptors, progress, and ownership storage
    emitter.instruction("mov QWORD PTR [rsp], rsi");                            // retain the validated entry count
    emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                        // retain borrowed key descriptors
    emitter.instruction("mov QWORD PTR [rsp + 16], rcx");                       // retain borrowed value descriptors
    emitter.instruction("mov QWORD PTR [rsp + 24], 0");                         // initialize the insertion index
    emitter.instruction("mov QWORD PTR [rsp + 64], r8");                        // preserve the required array representation
    emitter.instruction("mov QWORD PTR [rsp + 72], r9");                        // borrow the optional identity vector through this construction callback
    emitter.instruction("test r8, r8");                                         // distinguish list keys from associative keys
    emitter.instruction("mov rdi, rsi");                                        // request capacity for the validated entry count
    emitter.instruction("jnz __rt_mbstring_build_new_list");                    // choose indexed storage for consecutive integer keys
    emitter.instruction("mov esi, 7");                                          // mark the associative table as heterogeneous
    abi::emit_call_label(emitter, "__rt_hash_new");
    emitter.instruction("jmp __rt_mbstring_build_new_ready");                   // join fresh array ownership after associative allocation
    emitter.label("__rt_mbstring_build_new_list");
    emitter.instruction("mov esi, 8");                                          // store owned Mixed cell pointers in indexed elements
    abi::emit_call_label(emitter, "__rt_array_new");
    emitter.instruction("or QWORD PTR [rax - 8], 1792");                        // select boxed Mixed element cleanup metadata
    emitter.label("__rt_mbstring_build_new_ready");
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // own the fresh unique table until returning it
    emitter.label("__rt_mbstring_build_loop");
    emitter.instruction("mov r10, QWORD PTR [rsp + 24]");                       // load the current insertion index
    emitter.instruction("cmp r10, QWORD PTR [rsp]");                            // check whether every entry is initialized
    emitter.instruction("je __rt_mbstring_build_done");                         // transfer the completed table after the last entry
    emitter.instruction("imul r10, r10, 24");                                   // scale the value index by the concrete descriptor size
    emitter.instruction("add r10, QWORD PTR [rsp + 16]");                       // address the current borrowed value
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the concrete PHP value tag
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // load the low payload without changing scalar bits
    emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                       // load binary string length or unused high payload
    emitter.instruction("mov QWORD PTR [rsp + 40], r11");                       // preserve the value tag across ownership acquisition
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // preserve the low payload across native helpers
    emitter.instruction("mov QWORD PTR [rsp + 56], rdx");                       // preserve the high payload across native helpers
    emitter.instruction("cmp QWORD PTR [rsp + 64], 0");                         // inspect the selected array representation
    emitter.instruction("jne __rt_mbstring_build_list_value");                  // box borrowed descriptors directly for indexed arrays
    emitter.instruction("cmp r11, 1");                                          // recognize borrowed binary strings
    emitter.instruction("je __rt_mbstring_build_string");                       // copy string bytes before the Rust buffer can be released
    emitter.instruction("cmp r11, 5");                                          // recognize already completed child arrays
    emitter.instruction("ja __rt_mbstring_build_value_ready");                  // values above array tags carry no child ownership
    emitter.instruction("cmp r11, 4");                                          // accept completed indexed and associative children
    emitter.instruction("jb __rt_mbstring_build_value_ready");                  // immediate scalars require no ownership acquisition
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("jmp __rt_mbstring_build_value_ready");                 // retain children independently from the restorer arena
    emitter.label("__rt_mbstring_build_string");
    abi::emit_call_label(emitter, "__rt_str_persist");
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // transfer the persistent string pointer to the pending entry
    emitter.instruction("mov QWORD PTR [rsp + 56], rdx");                       // retain its exact binary byte length
    bind_string(emitter, false);
    emitter.label("__rt_mbstring_build_value_ready");
    emitter.instruction("mov r10, QWORD PTR [rsp + 24]");                       // reload the current insertion index
    emitter.instruction("imul r10, r10, 24");                                   // scale the key index by the concrete descriptor size
    emitter.instruction("add r10, QWORD PTR [rsp + 8]");                        // address the current normalized key
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // load the exact integer or binary string key payload
    emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                       // load the string key length
    emitter.instruction("mov r11, -1");                                         // prepare the native integer-key sentinel
    emitter.instruction("cmp QWORD PTR [r10], 0");                              // distinguish integer keys from numeric string keys
    emitter.instruction("cmove rdx, r11");                                      // retain exact integer or string key identity
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // load the still uniquely owned destination table
    emitter.instruction("mov rcx, QWORD PTR [rsp + 48]");                       // transfer the acquired low value payload to insertion
    emitter.instruction("mov r8, QWORD PTR [rsp + 56]");                        // transfer the high value payload to insertion
    emitter.instruction("mov r9, QWORD PTR [rsp + 40]");                        // restore the concrete per-entry value tag
    abi::emit_call_label(emitter, "__rt_hash_set");
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // preserve the current table if insertion grew its storage
    emitter.label("__rt_mbstring_build_advance");
    emitter.instruction("add QWORD PTR [rsp + 24], 1");                         // advance the initialized entry count
    emitter.instruction("jmp __rt_mbstring_build_loop");                        // construct every key and value in insertion order
    emitter.label("__rt_mbstring_build_list_value");
    emitter.instruction("mov rdi, rax");                                        // pass the borrowed low payload to boxing
    emitter.instruction("mov rsi, rdx");                                        // preserve the binary string length or unused high payload
    emitter.instruction("mov rax, r11");                                        // select the concrete runtime tag
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    bind_string(emitter, true);
    emitter.instruction("mov r10, QWORD PTR [rsp + 32]");                       // recover the unique indexed destination
    emitter.instruction("mov r11, QWORD PTR [rsp + 24]");                       // recover the next element index
    emitter.instruction("mov QWORD PTR [r10 + r11 * 8 + 24], rax");             // transfer the fresh boxed owner into its slot
    emitter.instruction("add r11, 1");                                          // include the initialized element in cleanup
    emitter.instruction("mov QWORD PTR [r10], r11");                            // expose exactly initialized owners
    emitter.instruction("jmp __rt_mbstring_build_advance");                     // share progress with associative construction
    emitter.label("__rt_mbstring_build_done");
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // transfer the single completed table owner to Rust
    emitter.instruction("leave");                                               // release constructor-local storage and restore linkage
    emitter.instruction("ret");                                                 // return the owned associative-array handle
}

/// Retains an INI identity on a copied hash string or indexed Mixed payload, preserving the box owner.
fn bind_string(emitter: &mut Emitter, boxed: bool) {
    let done = if boxed { "__rt_mbstring_build_list_identity_done" } else { "__rt_mbstring_build_hash_identity_done" };
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("ldr x9, [sp, #72]");                               // inspect optional identities without changing the acquired owner
        emitter.instruction(&format!("cbz x9, {done}"));                        // ordinary graphs retain their existing string ownership behavior
        emitter.instruction("ldr x10, [sp, #24]");                              // select the current entry's identity token
        emitter.instruction("ldr x2, [x9, x10, lsl #3]");                       // borrow the validated identity while the wire lease is still live
        emitter.instruction(&format!("cbz x2, {done}"));                        // non-string values have no identity to acquire
        if boxed {
            emitter.instruction("mov x12, x0");                                 // preserve the completed Mixed cell across identity argument setup
            emitter.instruction("ldp x0, x1, [x12, #8]");                       // bind the owned string payload and exact byte length inside the cell
        } else { emitter.instruction("ldp x0, x1, [sp, #48]"); }                // bind the acquired hash value before insertion transfers ownership
        abi::emit_call_label(emitter, "__rt_mbstring_ini_bind");
        if boxed { emitter.instruction("mov x0, x12"); }                        // restore the Mixed cell for transfer into indexed storage
    } else {
        emitter.instruction("mov rcx, QWORD PTR [rsp + 72]");                   // inspect optional identities without changing the acquired owner
        emitter.instruction("test rcx, rcx");                                   // distinguish ordinary graphs from identity-aware results
        emitter.instruction(&format!("jz {done}"));                             // retain ordinary graph construction when no identities were supplied
        emitter.instruction("mov r11, QWORD PTR [rsp + 24]");                   // select the current entry's identity token
        emitter.instruction("mov rcx, QWORD PTR [rcx + r11 * 8]");              // borrow the token before wire result release
        emitter.instruction("test rcx, rcx");                                   // non-string entries carry a zero token
        emitter.instruction(&format!("jz {done}"));                             // skip identity binding for scalars and completed children
        if boxed {
            emitter.instruction("mov r10, rax");                                // preserve the Mixed cell across native bind argument setup
            emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                // bind the copied string payload inside the completed cell
            emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");               // preserve its exact binary byte length
        } else {
            emitter.instruction("mov rax, QWORD PTR [rsp + 48]");               // load the acquired hash string before insertion transfers ownership
            emitter.instruction("mov rdx, QWORD PTR [rsp + 56]");               // bind only its complete logical byte range
        }
        abi::emit_call_label(emitter, "__rt_mbstring_ini_bind");
        if boxed { emitter.instruction("mov rax, r10"); }                       // return the original box owner to indexed insertion
    }
    emitter.label(done);
}
