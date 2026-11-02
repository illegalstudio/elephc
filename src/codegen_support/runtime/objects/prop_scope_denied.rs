//! Purpose:
//! Emits `__rt_prop_scope_denied`, the runtime visibility test behind a closure's RUNTIME scope.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::closure_scope_gate`, before a property access inside a
//!   closure that carries the hidden scope capture.
//! - `crate::codegen_support::runtime::emitters::managed`.
//!
//! Key details:
//! - Contract: `x0`/`rdi` = the receiver's runtime class id, `x1`/`rsi` + `x2`/`rdx` = the plain
//!   property name, `x3`/`rcx` = the scope class id (negative = global scope). Result in
//!   `x0`/`rax`: `0` = the access is allowed (or the name is not a declared member of that class,
//!   which php resolves dynamically), `1` = a private property of the receiver's own class the
//!   scope may not reach, `2` = a protected property the scope is unrelated to.
//! - It walks the same per-class serialize descriptor `__rt_object_to_hash` filters by lexical
//!   scope: rows of 32 bytes (`key ptr`, `key len`, `offset`, `tag`) whose key is the plain name
//!   for a public property and `\0*\0name` / `\0Owner\0name` for a protected / private one, plus
//!   the parallel declaring-class id table. The plain name is whatever follows the key's second NUL.
//! - A private property declared by a STRICT ancestor of the receiver is not refused: php resolves
//!   that name to a dynamic property outside its declaring class, so this helper stays silent and
//!   the caller's permissive slot resolution wins. That is the one place this approximates php.
//! - Protected visibility is php's ancestor-or-descendant test between the scope and the declaring
//!   class, walked through `_class_parent_ids`.
//! - Leaf routine: it makes no calls and touches only caller-saved registers on AArch64; the
//!   x86_64 form saves the callee-saved registers it borrows.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the target-specific property-scope visibility helper.
pub(crate) fn emit_prop_scope_denied(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 helper. Registers: x4 row cursor, x5 row count, x6 declaring-id table,
/// x7 row index, x8/x9 key pointer/length, x12/x13 plain-name pointer/length, x15 denied code.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: property scope denied ---");
    emitter.label_global("__rt_prop_scope_denied");
    emitter.instruction("mov x15, #0");                                         // denied code: nothing refused yet
    abi::emit_load_symbol_to_reg(emitter, "x9", "_class_gc_desc_count", 0);
    emitter.instruction("cmp x0, x9");                                          // unsigned: a negative or out-of-range class id has no descriptor
    emitter.instruction("b.hs __rt_psd_done");                                  // nothing to check
    abi::emit_symbol_address(emitter, "x9", "_class_serprop_ptrs");
    emitter.instruction("ldr x4, [x9, x0, lsl #3]");                            // x4 = the receiver class's property descriptor
    emitter.instruction("cbz x4, __rt_psd_done");                               // no descriptor, no declared members
    emitter.instruction("ldr x5, [x4]");                                        // x5 = declared property row count
    emitter.instruction("cbz x5, __rt_psd_done");                               // no declared members
    abi::emit_symbol_address(emitter, "x9", "_class_serprop_declaring_ptrs");
    emitter.instruction("ldr x6, [x9, x0, lsl #3]");                            // x6 = per-row declaring class id table
    emitter.instruction("cbz x6, __rt_psd_done");                               // no declaring table, cannot judge
    emitter.instruction("add x4, x4, #8");                                      // x4 = first 32-byte row
    emitter.instruction("mov x7, #0");                                          // row index = 0
    emitter.label("__rt_psd_loop");
    emitter.instruction("cmp x7, x5");                                          // visited every row?
    emitter.instruction("b.hs __rt_psd_done");                                  // yes, report what was refused
    emitter.instruction("ldr x8, [x4]");                                        // x8 = row key pointer
    emitter.instruction("ldr x9, [x4, #8]");                                    // x9 = row key length
    emitter.instruction("ldrb w10, [x8]");                                      // first key byte: NUL marks a mangled (non-public) key
    emitter.instruction("cbnz w10, __rt_psd_public_key");                       // public keys are the plain name
    emitter.instruction("mov x11, #1");                                         // scan for the second NUL starting after the first
    emitter.label("__rt_psd_scan");
    emitter.instruction("cmp x11, x9");                                         // ran off the key?
    emitter.instruction("b.hs __rt_psd_next");                                  // malformed key, skip the row
    emitter.instruction("ldrb w10, [x8, x11]");                                 // next key byte
    emitter.instruction("cbz w10, __rt_psd_found_nul");                         // second NUL ends the owner marker
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("b __rt_psd_scan");                                     // keep scanning
    emitter.label("__rt_psd_found_nul");
    emitter.instruction("add x12, x8, x11");                                    // plain name starts after the second NUL
    emitter.instruction("add x12, x12, #1");                                    // skip the NUL itself
    emitter.instruction("sub x13, x9, x11");                                    // plain length = key length - NUL position
    emitter.instruction("sub x13, x13, #1");                                    // minus the NUL itself
    emitter.instruction("b __rt_psd_compare");                                  // compare it with the requested name
    emitter.label("__rt_psd_public_key");
    emitter.instruction("mov x12, x8");                                         // the whole key is the plain name
    emitter.instruction("mov x13, x9");                                         // its length is the key length
    emitter.label("__rt_psd_compare");
    emitter.instruction("cmp x13, x2");                                         // different lengths cannot be the same name
    emitter.instruction("b.ne __rt_psd_next");                                  // next row
    emitter.instruction("mov x14, #0");                                         // byte index
    emitter.label("__rt_psd_cmp_loop");
    emitter.instruction("cmp x14, x2");                                         // compared every byte?
    emitter.instruction("b.hs __rt_psd_matched");                               // yes: same name
    emitter.instruction("ldrb w10, [x12, x14]");                                // row name byte
    emitter.instruction("ldrb w11, [x1, x14]");                                 // requested name byte
    emitter.instruction("cmp w10, w11");                                        // equal?
    emitter.instruction("b.ne __rt_psd_next");                                  // different name
    emitter.instruction("add x14, x14, #1");                                    // advance
    emitter.instruction("b __rt_psd_cmp_loop");                                 // keep comparing
    emitter.label("__rt_psd_matched");
    emitter.instruction("ldrb w10, [x8]");                                      // key marker again
    emitter.instruction("cbz w10, __rt_psd_mangled");                           // non-public member
    emitter.instruction("b __rt_psd_visible");                                  // a public member is always visible
    emitter.label("__rt_psd_mangled");
    emitter.instruction("ldrb w10, [x8, #1]");                                  // '*' marks protected, anything else a private owner name
    emitter.instruction("ldr x16, [x6, x7, lsl #3]");                           // x16 = this row's declaring class id
    emitter.instruction("cmp w10, #42");                                        // ASCII '*'
    emitter.instruction("b.eq __rt_psd_protected");                             // protected rules
    emitter.instruction("cmp x16, x3");                                         // private: only the declaring class's own scope reaches it
    emitter.instruction("b.eq __rt_psd_visible");                               // the scope IS the declaring class
    emitter.instruction("cmp x16, x0");                                         // declared by the receiver's own class?
    emitter.instruction("b.ne __rt_psd_next");                                  // a strict ancestor's private name is dynamic here, never refused
    emitter.instruction("cbnz x15, __rt_psd_next");                             // keep the first refusal
    emitter.instruction("mov x15, #1");                                         // private refusal
    emitter.instruction("b __rt_psd_next");                                     // keep looking for a visible declaration
    emitter.label("__rt_psd_protected");
    emitter.instruction("cmp x3, #0");                                          // global scope sees no protected member
    emitter.instruction("b.lt __rt_psd_prot_denied");                           // refuse
    emitter.instruction("mov x14, x16");                                        // walk up from the declaring class looking for the scope
    emitter.label("__rt_psd_prot_up");
    emitter.instruction("cmp x14, x3");                                         // is the scope an ancestor-or-self of the declaring class?
    emitter.instruction("b.eq __rt_psd_visible");                               // related
    emitter.instruction("cmp x14, #0");                                         // reached the root sentinel?
    emitter.instruction("b.lt __rt_psd_prot_down");                             // try the other direction
    abi::emit_symbol_address(emitter, "x17", "_class_parent_ids");
    emitter.instruction("ldr x14, [x17, x14, lsl #3]");                         // parent class id
    emitter.instruction("b __rt_psd_prot_up");                                  // continue upward
    emitter.label("__rt_psd_prot_down");
    emitter.instruction("mov x14, x3");                                         // walk up from the scope looking for the declaring class
    emitter.label("__rt_psd_prot_down_loop");
    emitter.instruction("cmp x14, x16");                                        // is the declaring class an ancestor-or-self of the scope?
    emitter.instruction("b.eq __rt_psd_visible");                               // related
    emitter.instruction("cmp x14, #0");                                         // reached the root sentinel?
    emitter.instruction("b.lt __rt_psd_prot_denied");                           // unrelated
    abi::emit_symbol_address(emitter, "x17", "_class_parent_ids");
    emitter.instruction("ldr x14, [x17, x14, lsl #3]");                         // parent class id
    emitter.instruction("b __rt_psd_prot_down_loop");                           // continue upward
    emitter.label("__rt_psd_prot_denied");
    emitter.instruction("cbnz x15, __rt_psd_next");                             // keep the first refusal
    emitter.instruction("mov x15, #2");                                         // protected refusal
    emitter.instruction("b __rt_psd_next");                                     // keep looking for a visible declaration
    emitter.label("__rt_psd_visible");
    emitter.instruction("mov x15, #0");                                         // a visible declaration clears any earlier refusal
    emitter.instruction("b __rt_psd_done");                                     // allowed
    emitter.label("__rt_psd_next");
    emitter.instruction("add x4, x4, #32");                                     // next 32-byte row
    emitter.instruction("add x7, x7, #1");                                      // next row index
    emitter.instruction("b __rt_psd_loop");                                     // continue
    emitter.label("__rt_psd_done");
    emitter.instruction("mov x0, x15");                                         // result: 0 allowed, 1 private refused, 2 protected refused
    emitter.instruction("ret");                                                 // return to the gate
}

/// Emits the x86_64 helper. Registers: r8 row cursor, r9 row index, r10 declaring-id table,
/// r11 row count, r12/r13 key pointer/length, r14/r15 plain-name pointer/length (then declaring id /
/// byte index), rbx denied code.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: property scope denied (x86_64) ---");
    emitter.label_global("__rt_prop_scope_denied");
    emitter.instruction("push rbx");                                            // borrow callee-saved registers for the row walk
    emitter.instruction("push r12");                                            // saved
    emitter.instruction("push r13");                                            // saved
    emitter.instruction("push r14");                                            // saved
    emitter.instruction("push r15");                                            // saved
    emitter.instruction("xor ebx, ebx");                                        // denied code: nothing refused yet
    abi::emit_load_symbol_to_reg(emitter, "rax", "_class_gc_desc_count", 0);
    emitter.instruction("cmp rdi, rax");                                        // unsigned: a negative or out-of-range class id has no descriptor
    emitter.instruction("jae __rt_psd_done");                                   // nothing to check
    abi::emit_symbol_address(emitter, "rax", "_class_serprop_ptrs");
    emitter.instruction("mov r8, QWORD PTR [rax + rdi*8]");                     // r8 = the receiver class's property descriptor
    emitter.instruction("test r8, r8");                                         // no descriptor, no declared members
    emitter.instruction("jz __rt_psd_done");                                    // nothing to check
    emitter.instruction("mov r11, QWORD PTR [r8]");                             // r11 = declared property row count
    emitter.instruction("test r11, r11");                                       // no declared members
    emitter.instruction("jz __rt_psd_done");                                    // nothing to check
    abi::emit_symbol_address(emitter, "rax", "_class_serprop_declaring_ptrs");
    emitter.instruction("mov r10, QWORD PTR [rax + rdi*8]");                    // r10 = per-row declaring class id table
    emitter.instruction("test r10, r10");                                       // no declaring table, cannot judge
    emitter.instruction("jz __rt_psd_done");                                    // nothing to check
    emitter.instruction("add r8, 8");                                           // r8 = first 32-byte row
    emitter.instruction("xor r9d, r9d");                                        // row index = 0
    emitter.label("__rt_psd_loop");
    emitter.instruction("cmp r9, r11");                                         // visited every row?
    emitter.instruction("jae __rt_psd_done");                                   // yes, report what was refused
    emitter.instruction("mov r12, QWORD PTR [r8]");                             // r12 = row key pointer
    emitter.instruction("mov r13, QWORD PTR [r8 + 8]");                         // r13 = row key length
    emitter.instruction("movzx eax, BYTE PTR [r12]");                           // first key byte: NUL marks a mangled (non-public) key
    emitter.instruction("test eax, eax");                                       // public key?
    emitter.instruction("jnz __rt_psd_public_key");                             // public keys are the plain name
    emitter.instruction("mov r15, 1");                                          // scan for the second NUL starting after the first
    emitter.label("__rt_psd_scan");
    emitter.instruction("cmp r15, r13");                                        // ran off the key?
    emitter.instruction("jae __rt_psd_next");                                   // malformed key, skip the row
    emitter.instruction("movzx eax, BYTE PTR [r12 + r15]");                     // next key byte
    emitter.instruction("test eax, eax");                                       // second NUL?
    emitter.instruction("jz __rt_psd_found_nul");                               // it ends the owner marker
    emitter.instruction("add r15, 1");                                          // advance
    emitter.instruction("jmp __rt_psd_scan");                                   // keep scanning
    emitter.label("__rt_psd_found_nul");
    emitter.instruction("lea r14, [r12 + r15 + 1]");                            // plain name starts after the second NUL
    emitter.instruction("mov rax, r13");                                        // key length
    emitter.instruction("sub rax, r15");                                        // minus the NUL position
    emitter.instruction("sub rax, 1");                                          // minus the NUL itself
    emitter.instruction("mov r15, rax");                                        // r15 = plain length
    emitter.instruction("jmp __rt_psd_compare");                                // compare it with the requested name
    emitter.label("__rt_psd_public_key");
    emitter.instruction("mov r14, r12");                                        // the whole key is the plain name
    emitter.instruction("mov r15, r13");                                        // its length is the key length
    emitter.label("__rt_psd_compare");
    emitter.instruction("cmp r15, rdx");                                        // different lengths cannot be the same name
    emitter.instruction("jne __rt_psd_next");                                   // next row
    emitter.instruction("xor r15d, r15d");                                      // r15 = byte index from here on
    emitter.label("__rt_psd_cmp_loop");
    emitter.instruction("cmp r15, rdx");                                        // compared every byte?
    emitter.instruction("jae __rt_psd_matched");                                // yes: same name
    emitter.instruction("movzx eax, BYTE PTR [r14 + r15]");                     // row name byte
    emitter.instruction("movzx r13d, BYTE PTR [rsi + r15]");                    // requested name byte
    emitter.instruction("cmp eax, r13d");                                       // equal?
    emitter.instruction("jne __rt_psd_next");                                   // different name
    emitter.instruction("add r15, 1");                                          // advance
    emitter.instruction("jmp __rt_psd_cmp_loop");                               // keep comparing
    emitter.label("__rt_psd_matched");
    emitter.instruction("movzx eax, BYTE PTR [r12]");                           // key marker again
    emitter.instruction("test eax, eax");                                       // public member?
    emitter.instruction("jnz __rt_psd_visible");                                // a public member is always visible
    emitter.instruction("movzx eax, BYTE PTR [r12 + 1]");                       // '*' marks protected, anything else a private owner name
    emitter.instruction("mov r14, QWORD PTR [r10 + r9*8]");                     // r14 = this row's declaring class id
    emitter.instruction("cmp eax, 42");                                         // ASCII '*'
    emitter.instruction("je __rt_psd_protected");                               // protected rules
    emitter.instruction("cmp r14, rcx");                                        // private: only the declaring class's own scope reaches it
    emitter.instruction("je __rt_psd_visible");                                 // the scope IS the declaring class
    emitter.instruction("cmp r14, rdi");                                        // declared by the receiver's own class?
    emitter.instruction("jne __rt_psd_next");                                   // a strict ancestor's private name is dynamic here, never refused
    emitter.instruction("test rbx, rbx");                                       // keep the first refusal
    emitter.instruction("jnz __rt_psd_next");                                   // already refused
    emitter.instruction("mov ebx, 1");                                          // private refusal
    emitter.instruction("jmp __rt_psd_next");                                   // keep looking for a visible declaration
    emitter.label("__rt_psd_protected");
    emitter.instruction("test rcx, rcx");                                       // global scope sees no protected member
    emitter.instruction("js __rt_psd_prot_denied");                             // refuse
    emitter.instruction("mov r15, r14");                                        // walk up from the declaring class looking for the scope
    emitter.label("__rt_psd_prot_up");
    emitter.instruction("cmp r15, rcx");                                        // is the scope an ancestor-or-self of the declaring class?
    emitter.instruction("je __rt_psd_visible");                                 // related
    emitter.instruction("test r15, r15");                                       // reached the root sentinel?
    emitter.instruction("js __rt_psd_prot_down");                               // try the other direction
    abi::emit_symbol_address(emitter, "rax", "_class_parent_ids");
    emitter.instruction("mov r15, QWORD PTR [rax + r15*8]");                    // parent class id
    emitter.instruction("jmp __rt_psd_prot_up");                                // continue upward
    emitter.label("__rt_psd_prot_down");
    emitter.instruction("mov r15, rcx");                                        // walk up from the scope looking for the declaring class
    emitter.label("__rt_psd_prot_down_loop");
    emitter.instruction("cmp r15, r14");                                        // is the declaring class an ancestor-or-self of the scope?
    emitter.instruction("je __rt_psd_visible");                                 // related
    emitter.instruction("test r15, r15");                                       // reached the root sentinel?
    emitter.instruction("js __rt_psd_prot_denied");                             // unrelated
    abi::emit_symbol_address(emitter, "rax", "_class_parent_ids");
    emitter.instruction("mov r15, QWORD PTR [rax + r15*8]");                    // parent class id
    emitter.instruction("jmp __rt_psd_prot_down_loop");                         // continue upward
    emitter.label("__rt_psd_prot_denied");
    emitter.instruction("test rbx, rbx");                                       // keep the first refusal
    emitter.instruction("jnz __rt_psd_next");                                   // already refused
    emitter.instruction("mov ebx, 2");                                          // protected refusal
    emitter.instruction("jmp __rt_psd_next");                                   // keep looking for a visible declaration
    emitter.label("__rt_psd_visible");
    emitter.instruction("xor ebx, ebx");                                        // a visible declaration clears any earlier refusal
    emitter.instruction("jmp __rt_psd_done");                                   // allowed
    emitter.label("__rt_psd_next");
    emitter.instruction("add r8, 32");                                          // next 32-byte row
    emitter.instruction("add r9, 1");                                           // next row index
    emitter.instruction("jmp __rt_psd_loop");                                   // continue
    emitter.label("__rt_psd_done");
    emitter.instruction("mov rax, rbx");                                        // result: 0 allowed, 1 private refused, 2 protected refused
    emitter.instruction("pop r15");                                             // restore callee-saved registers
    emitter.instruction("pop r14");                                             // restored
    emitter.instruction("pop r13");                                             // restored
    emitter.instruction("pop r12");                                             // restored
    emitter.instruction("pop rbx");                                             // restored
    emitter.instruction("ret");                                                 // return to the gate
}
