//! Purpose:
//! Emits `__rt_interface_exists`, the runtime helper backing a non-literal
//! `interface_exists($name)` call by searching the closed-world interface registry.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` (gated by the
//!   `class_introspection` feature) and the EIR `interface_exists` lowering.
//!
//! Key details:
//! - Input:  x0/rdi=name_ptr, x1/rsi=name_len. The `$autoload` argument is
//!   irrelevant in a closed-world build and is dropped before this helper.
//! - Output: x0/rax = 1 when a matching interface is active, 0 otherwise. No throw.
//! - PHP interface names are case-insensitive. Activation-overlay entries use
//!   request cells; names absent from that overlay use immutable builtin/root metadata.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits the target-specific `__rt_interface_exists` interface-existence helper.
pub fn emit_rt_interface_exists(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 implementation of `__rt_interface_exists`.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: interface_exists ---");
    emitter.label_global("__rt_interface_exists");

    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // save frame pointer and reserve query storage
    emitter.instruction("mov x29, sp");                                         // establish the new frame pointer

    // -- lowercase the query name so the lookup matches PHP's case-insensitive interface names --
    emitter.instruction("mov x2, x1");                                          // stash the query length before strtolower's args overwrite x1
    emitter.instruction("mov x1, x0");                                          // move the query pointer into strtolower's pointer argument
    emitter.instruction("bl __rt_strtolower");                                  // x1=lowercased query copy, x2=length unchanged

    emitter.instruction("str x1, [sp, #16]");                                   // retain lowered query pointer across overlay lookup
    emitter.instruction("str x2, [sp, #24]");                                   // retain lowered query length across overlay lookup

    // -- search request-active bindings first --
    emitter.instruction("mov x0, x1");                                          // lowercased query pointer -> overlay arg0
    emitter.instruction("mov x1, x2");                                          // query length -> overlay arg1
    abi::emit_symbol_address(emitter, "x2", "_interface_activation_table");
    abi::emit_load_symbol_to_reg(emitter, "x3", "_interface_activation_table_count", 0);
    emitter.instruction("mov x4, #24");                                         // overlay entries contain an active-cell pointer
    emitter.instruction("bl __rt_sorted_name_search");                          // find the declaration binding cell
    let immutable = "L_rt_interface_exists_immutable";
    emitter.instruction(&format!("cbz x0, {}", immutable));                      // names without an event use immutable metadata
    emitter.instruction("ldr x0, [x0, #16]");                                   // load request-active cell address
    emitter.instruction("ldr x0, [x0]");                                        // read activation state
    emitter.instruction("b L_rt_interface_exists_release");                     // give the lowered copy back before returning

    // -- search the interface registry --
    emitter.label(immutable);
    emitter.instruction("ldr x0, [sp, #16]");                                   // restore lowercased query pointer for metadata lookup
    emitter.instruction("ldr x1, [sp, #24]");                                   // restore lowercased query length for metadata lookup
    abi::emit_symbol_address(emitter, "x2", "_interface_table");
    abi::emit_load_symbol_to_reg(emitter, "x3", "_interface_table_count", 0);
    emitter.instruction("mov x4, #16");                                         // interface entries are 16 bytes wide
    emitter.instruction("bl __rt_sorted_name_search");                          // search for the interface name
    emitter.instruction("cmp x0, #0");                                          // did the search return a matching entry?
    emitter.instruction("cset x0, ne");                                         // map a non-null entry pointer to true
    // -- strtolower returns an OWNED copy; release it on every exit --
    emitter.label("L_rt_interface_exists_release");
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the answer across the copy release (length no longer needed)
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the owned lowered query copy
    emitter.instruction("bl __rt_decref_any");                                  // give the lowered copy back
    emitter.instruction("ldr x0, [sp, #24]");                                   // restore the answer
    emitter.instruction("ldp x29, x30, [sp], #32");                             // release query storage and restore frame
    emitter.instruction("ret");                                                 // return the boolean in x0
}

/// Emits the x86_64 implementation of `__rt_interface_exists`.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: interface_exists ---");
    emitter.label_global("__rt_interface_exists");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 16");                                         // reserve aligned lowered-query storage

    // -- lowercase the query name so the lookup matches PHP's case-insensitive interface names --
    emitter.instruction("mov rdx, rsi");                                        // stash the query length before strtolower's args overwrite it
    emitter.instruction("mov rax, rdi");                                        // move the query pointer into strtolower's pointer argument
    emitter.instruction("call __rt_strtolower");                                // rax=lowercased query copy, rdx=length unchanged

    emitter.instruction("mov [rsp], rax");                                      // retain lowered query pointer across overlay lookup
    emitter.instruction("mov [rsp + 8], rdx");                                  // retain lowered query length across overlay lookup

    // -- search request-active bindings first --
    emitter.instruction("mov rdi, rax");                                        // lowercased query pointer -> overlay arg0
    emitter.instruction("mov rsi, rdx");                                        // query length -> overlay arg1
    abi::emit_symbol_address(emitter, "rdx", "_interface_activation_table");
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_interface_activation_table_count", 0);
    emitter.instruction("mov r8, 24");                                          // overlay entries contain an active-cell pointer
    emitter.instruction("call __rt_sorted_name_search");                        // find the declaration binding cell
    let immutable = "L_rt_interface_exists_immutable";
    emitter.instruction("test rax, rax");                                       // did this name receive an activation event?
    emitter.instruction(&format!("je {}", immutable));                           // names without an event use immutable metadata
    emitter.instruction("mov rax, [rax + 16]");                                 // load request-active cell address
    emitter.instruction("mov rax, [rax]");                                      // read activation state
    emitter.instruction("jmp L_rt_interface_exists_release_x");                 // give the lowered copy back before returning

    // -- search the interface registry --
    emitter.label(immutable);
    emitter.instruction("mov rdi, [rsp]");                                      // restore lowercased query pointer for metadata lookup
    emitter.instruction("mov rsi, [rsp + 8]");                                  // restore lowercased query length for metadata lookup
    abi::emit_symbol_address(emitter, "rdx", "_interface_table");
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_interface_table_count", 0);
    emitter.instruction("mov r8, 16");                                          // interface entries are 16 bytes wide
    emitter.instruction("call __rt_sorted_name_search");                        // search for the interface name
    emitter.instruction("test rax, rax");                                       // did the search return a matching entry?
    emitter.instruction("setne al");                                            // map a non-null entry pointer to true
    emitter.instruction("movzx eax, al");                                       // widen the boolean byte into the result register
    // -- strtolower returns an OWNED copy; release it on every exit --
    emitter.label("L_rt_interface_exists_release_x");
    emitter.instruction("mov [rsp + 8], rax");                                  // preserve the answer across the copy release (length no longer needed)
    emitter.instruction("mov rax, [rsp]");                                      // reload the owned lowered query copy
    emitter.instruction("call __rt_decref_any");                                // give the lowered copy back
    emitter.instruction("mov rax, [rsp + 8]");                                  // restore the answer
    emitter.instruction("leave");                                               // release query storage and restore frame
    emitter.instruction("ret");                                                 // return the boolean in rax
}
