//! Purpose:
//! Emits the runtime lookup used by `method_exists()` and `property_exists()` when the member
//! name is not known during compilation.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::member_exists` after resolving a target class name.
//!
//! Key details:
//! - The 64-byte class row contains separate class-string method, object method, and property lists.
//! - Class and method names are ASCII case-insensitive; property names are byte-case-sensitive.
//! - Inputs are class pointer/length, member pointer/length, and list kind; output is a boolean.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits the target-specific `__rt_member_exists` registry lookup.
pub fn emit_rt_member_exists(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 member-existence registry lookup.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: member_exists ---");
    emitter.label_global("__rt_member_exists");

    emitter.instruction("stp x29, x30, [sp, #-80]!");                           // save frame pointer/return address and reserve spill slots
    emitter.instruction("mov x29, sp");                                         // establish the fixed runtime-helper frame
    emitter.instruction("stp x0, x1, [sp, #16]");                               // save class-name pointer/length
    emitter.instruction("stp x2, x3, [sp, #32]");                               // save member-name pointer/length
    emitter.instruction("str x4, [sp, #48]");                                   // save the requested member-list kind

    emitter.instruction("mov x1, x0");                                          // class-name pointer -> strtolower pointer input
    emitter.instruction("ldr x2, [sp, #24]");                                   // class-name length -> strtolower length input
    emitter.instruction("bl __rt_strtolower");                                  // lowercase the class-name query for PHP lookup
    emitter.instruction("str x1, [sp, #72]");                                   // keep the owned lowered class copy for release after the search
    emitter.instruction("mov x0, x1");                                          // lowered class-name pointer -> sorted-search arg0
    emitter.instruction("mov x1, x2");                                          // class-name length -> sorted-search arg1
    abi::emit_symbol_address(emitter, "x2", "_member_exists_table");
    abi::emit_load_symbol_to_reg(emitter, "x3", "_member_exists_table_count", 0);
    emitter.instruction("mov x4, #64");                                         // member-registry rows are 64 bytes wide
    emitter.instruction("bl __rt_sorted_name_search");                          // resolve the class row, or return null
    // -- strtolower returns an OWNED copy; the class copy is done once the row is found --
    emitter.instruction("str x0, [sp, #56]");                                   // preserve the row pointer across the copy release
    emitter.instruction("ldr x0, [sp, #72]");                                   // reload the owned lowered class copy
    emitter.instruction("bl __rt_decref_any");                                  // release the lowered class copy
    emitter.instruction("str xzr, [sp, #72]");                                  // no owned lowered copy is outstanding now
    emitter.instruction("ldr x0, [sp, #56]");                                   // restore the row pointer
    emitter.instruction("cbz x0, __rt_member_exists_aarch64_miss");             // an unknown class cannot expose the member

    emitter.instruction("ldr x4, [sp, #48]");                                   // reload the list-kind selector
    emitter.instruction("cmp x4, #1");                                          // does the caller need object-visible methods?
    emitter.instruction("b.eq __rt_member_exists_aarch64_object_methods");
    emitter.instruction("cmp x4, #2");                                          // does the caller need properties?
    emitter.instruction("b.eq __rt_member_exists_aarch64_properties");
    emitter.instruction("ldr x5, [x0, #16]");                                   // class-string method list pointer
    emitter.instruction("ldr x6, [x0, #24]");                                   // class-string method count
    emitter.instruction("b __rt_member_exists_aarch64_list_ready");

    emitter.label("__rt_member_exists_aarch64_object_methods");
    emitter.instruction("ldr x5, [x0, #32]");                                   // object method list pointer
    emitter.instruction("ldr x6, [x0, #40]");                                   // object method count
    emitter.instruction("b __rt_member_exists_aarch64_list_ready");

    emitter.label("__rt_member_exists_aarch64_properties");
    emitter.instruction("ldr x5, [x0, #48]");                                   // property list pointer
    emitter.instruction("ldr x6, [x0, #56]");                                   // property list count

    emitter.label("__rt_member_exists_aarch64_list_ready");
    emitter.instruction("str x5, [sp, #56]");                                   // persist the list cursor across string calls
    emitter.instruction("str x6, [sp, #64]");                                   // persist the remaining entry count
    emitter.instruction("cbz x6, __rt_member_exists_aarch64_miss");             // an empty member list cannot match
    emitter.instruction("ldr x4, [sp, #48]");                                   // reload the member-list kind
    emitter.instruction("cmp x4, #2");                                          // properties preserve byte-case-sensitive names
    emitter.instruction("b.eq __rt_member_exists_aarch64_property_query");
    emitter.instruction("ldr x1, [sp, #32]");                                   // method-name pointer -> strtolower input
    emitter.instruction("ldr x2, [sp, #40]");                                   // method-name length -> strtolower input
    emitter.instruction("bl __rt_strtolower");                                  // normalize the case-insensitive method query
    emitter.instruction("str x1, [sp, #72]");                                   // remember the owned lowered method copy for release at exit
    emitter.instruction("str x1, [sp, #16]");                                   // save normalized query pointer
    emitter.instruction("str x2, [sp, #24]");                                   // save normalized query length
    emitter.instruction("b __rt_member_exists_aarch64_scan");

    emitter.label("__rt_member_exists_aarch64_property_query");
    emitter.instruction("ldr x7, [sp, #32]");                                   // reload the original property-name pointer
    emitter.instruction("ldr x8, [sp, #40]");                                   // reload the original property-name length
    emitter.instruction("str x7, [sp, #16]");                                   // save exact property query pointer
    emitter.instruction("str x8, [sp, #24]");                                   // save exact property query length

    emitter.label("__rt_member_exists_aarch64_scan");
    emitter.instruction("ldr x5, [sp, #56]");                                   // reload the current member-list entry
    emitter.instruction("ldr x3, [x5]");                                        // candidate member pointer -> strcmp arg2
    emitter.instruction("ldr x4, [x5, #8]");                                    // candidate member length -> strcmp arg3
    emitter.instruction("add x5, x5, #16");                                     // advance to the next member-list entry
    emitter.instruction("str x5, [sp, #56]");                                   // save the advanced cursor across strcmp
    emitter.instruction("ldr x1, [sp, #16]");                                   // query member pointer -> strcmp arg0
    emitter.instruction("ldr x2, [sp, #24]");                                   // query member length -> strcmp arg1
    emitter.instruction("bl __rt_strcmp");                                      // compare the explicit-length member names
    emitter.instruction("cbz x0, __rt_member_exists_aarch64_found");            // an exact byte match means the member exists
    emitter.instruction("ldr x6, [sp, #64]");                                   // reload remaining entry count
    emitter.instruction("sub x6, x6, #1");                                      // account for the candidate just tested
    emitter.instruction("str x6, [sp, #64]");                                   // save the decremented count
    emitter.instruction("cbnz x6, __rt_member_exists_aarch64_scan");            // scan the next member while entries remain

    emitter.label("__rt_member_exists_aarch64_miss");
    emitter.instruction("mov x0, #0");                                          // return false for an unknown class or absent member
    emitter.instruction("b __rt_member_exists_aarch64_done");

    emitter.label("__rt_member_exists_aarch64_found");
    emitter.instruction("mov x0, #1");                                          // return true for the matched member

    emitter.label("__rt_member_exists_aarch64_done");
    emitter.instruction("str x0, [sp, #56]");                                   // preserve the answer across the copy release
    emitter.instruction("ldr x0, [sp, #72]");                                   // reload any owned lowered method copy (null when none)
    emitter.instruction("bl __rt_decref_any");                                  // release it; a null pointer is ignored
    emitter.instruction("ldr x0, [sp, #56]");                                   // restore the answer
    emitter.instruction("ldp x29, x30, [sp], #80");                             // restore the caller frame and release spills
    emitter.instruction("ret");                                                 // return the boolean in x0
}

/// Emits the x86_64 member-existence registry lookup.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: member_exists ---");
    emitter.label_global("__rt_member_exists");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable aligned frame
    emitter.instruction("sub rsp, 80");                                         // reserve spill slots while retaining call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save class-name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save member-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save member-name length
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the requested member-list kind

    emitter.instruction("mov rax, rdi");                                        // class-name pointer -> strtolower pointer input
    emitter.instruction("mov rdx, rsi");                                        // class-name length -> strtolower length input
    emitter.instruction("call __rt_strtolower");                                // lowercase the class-name query for PHP lookup
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // keep the owned lowered class copy for release after the search
    emitter.instruction("mov rdi, rax");                                        // lowered class-name pointer -> sorted-search arg0
    emitter.instruction("mov rsi, rdx");                                        // class-name length -> sorted-search arg1
    abi::emit_symbol_address(emitter, "rdx", "_member_exists_table");
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_member_exists_table_count", 0);
    emitter.instruction("mov r8, 64");                                          // member-registry rows are 64 bytes wide
    emitter.instruction("call __rt_sorted_name_search");                        // resolve the class row, or return null
    // -- strtolower returns an OWNED copy; the class copy is done once the row is found --
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the row pointer across the copy release
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // reload the owned lowered class copy
    emitter.instruction("call __rt_decref_any");                                // release the lowered class copy
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // no owned lowered copy is outstanding now
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // restore the row pointer
    emitter.instruction("test rax, rax");                                       // did the class lookup find a row?
    emitter.instruction("jz __rt_member_exists_x86_64_miss");                   // an unknown class cannot expose the member

    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the list-kind selector
    emitter.instruction("cmp r8, 1");                                           // does the caller need object-visible methods?
    emitter.instruction("je __rt_member_exists_x86_64_object_methods");
    emitter.instruction("cmp r8, 2");                                           // does the caller need properties?
    emitter.instruction("je __rt_member_exists_x86_64_properties");
    emitter.instruction("mov r9, QWORD PTR [rax + 16]");                        // class-string method list pointer
    emitter.instruction("mov r10, QWORD PTR [rax + 24]");                       // class-string method count
    emitter.instruction("jmp __rt_member_exists_x86_64_list_ready");

    emitter.label("__rt_member_exists_x86_64_object_methods");
    emitter.instruction("mov r9, QWORD PTR [rax + 32]");                        // object method list pointer
    emitter.instruction("mov r10, QWORD PTR [rax + 40]");                       // object method count
    emitter.instruction("jmp __rt_member_exists_x86_64_list_ready");

    emitter.label("__rt_member_exists_x86_64_properties");
    emitter.instruction("mov r9, QWORD PTR [rax + 48]");                        // property list pointer
    emitter.instruction("mov r10, QWORD PTR [rax + 56]");                       // property list count

    emitter.label("__rt_member_exists_x86_64_list_ready");
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // persist the list cursor across string calls
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // persist the remaining entry count
    emitter.instruction("test r10, r10");                                      // does the selected class have any candidate members?
    emitter.instruction("jz __rt_member_exists_x86_64_miss");                   // an empty member list cannot match
    emitter.instruction("cmp QWORD PTR [rbp - 40], 2");                        // properties preserve byte-case-sensitive names
    emitter.instruction("je __rt_member_exists_x86_64_property_query");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // method-name pointer -> strtolower input
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // method-name length -> strtolower input
    emitter.instruction("call __rt_strtolower");                                // normalize the case-insensitive method query
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // remember the owned lowered method copy for release at exit
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save normalized query pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // save normalized query length
    emitter.instruction("jmp __rt_member_exists_x86_64_scan");

    emitter.label("__rt_member_exists_x86_64_property_query");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the original property-name pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the original property-name length
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // save exact property query pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // save exact property query length

    emitter.label("__rt_member_exists_x86_64_scan");
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the current member-list entry
    emitter.instruction("mov rdx, QWORD PTR [r9]");                             // candidate member pointer -> strcmp arg2
    emitter.instruction("mov rcx, QWORD PTR [r9 + 8]");                         // candidate member length -> strcmp arg3
    emitter.instruction("add r9, 16");                                          // advance to the next member-list entry
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the advanced cursor across strcmp
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // query member pointer -> strcmp arg0
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // query member length -> strcmp arg1
    emitter.instruction("call __rt_strcmp");                                    // compare the explicit-length member names
    emitter.instruction("test rax, rax");                                       // was the candidate an exact match?
    emitter.instruction("jz __rt_member_exists_x86_64_found");                  // an exact byte match means the member exists
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload remaining entry count
    emitter.instruction("sub r10, 1");                                          // account for the candidate just tested
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // save the decremented count
    emitter.instruction("jnz __rt_member_exists_x86_64_scan");                  // scan the next member while entries remain

    emitter.label("__rt_member_exists_x86_64_miss");
    emitter.instruction("xor eax, eax");                                        // return false for an unknown class or absent member
    emitter.instruction("jmp __rt_member_exists_x86_64_done");

    emitter.label("__rt_member_exists_x86_64_found");
    emitter.instruction("mov rax, 1");                                          // return true for the matched member

    emitter.label("__rt_member_exists_x86_64_done");
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the answer across the copy release
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // reload any owned lowered method copy (null when none)
    emitter.instruction("call __rt_decref_any");                                // release it; a null pointer is ignored
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // restore the answer
    emitter.instruction("mov rsp, rbp");                                        // discard spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boolean in rax
}

