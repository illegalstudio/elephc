//! Purpose:
//! Preserves native string identity metadata across persistence, final release, and request reset.
//!
//! Called from:
//! - INI materialization, str_persist, heap_free, and the mbstring request cleanup path.
//!
//! Key details:
//! - Value creation records lazy origins before any INI call; cleanup skips an untouched map.
//! - Every integer and vector register survives Rust callbacks, including leaf allocator callers.
//! - SysV stack alignment is repaired locally because existing runtime leaf call depths vary.

use super::*;

#[cfg(test)]
mod tests;

/// Emits identity lease hooks for every supported target without adding a second native owner system.
pub(super) fn emit(emitter: &mut Emitter) {
    for operation in ["bind", "copy", "fresh", "literal", "persist", "forget", "reset"] {
        if emitter.target.arch == Arch::AArch64 { aarch64(emitter, operation); }
        else { x86_64(emitter, operation); }
    }
}

/// Adapts x0/x1/x2 to the C contract while preserving all integer and SIMD registers.
fn aarch64(emitter: &mut Emitter, operation: &str) {
    let label = format!("__rt_mbstring_ini_{operation}");
    emitter.label_global(&label);
    if dormant_gate(operation) {
        emitter.instruction("stp x9, x30, [sp, #-16]!");                        // preserve the gate scratch register and leaf return address
        abi::emit_load_symbol_to_reg(emitter, "x9", "_mbstring_ini_native_active", 0);
        emitter.instruction(&format!("cbz x9, {label}_inactive"));              // skip bridge access until native INI metadata exists
        emitter.instruction("ldp x9, x30, [sp], #16");                          // restore the original register values before the complete save
    }
    emitter.instruction("sub sp, sp, #768");                                    // reserve aligned storage for every general and vector register
    for register in (0..30).step_by(2) {
        emitter.instruction(&format!("stp x{register}, x{}, [sp, #{}]", register + 1, register * 8));// preserve the native caller's integer register pair
    }
    emitter.instruction("stp x30, xzr, [sp, #240]");                            // preserve linkage and the integer-save padding
    for register in (0..32).step_by(2) {
        emitter.instruction(&format!("stp q{register}, q{}, [sp, #{}]", register + 1, 256 + register * 16));// preserve the full vector register pair across Rust
    }
    if matches!(operation, "fresh" | "literal") {
        emitter.instruction("mov x0, x1");                                      // pass the native string result pointer after preserving all live registers
        emitter.instruction("mov x1, x2");                                      // pass its exact logical byte length to the origin registry
    }
    emitter.bl_c(&format!("elephc_mbstring_native_string_{operation}_v1"));
    emitter.instruction(&format!("cbz x0, {label}_success"));                   // propagate internal metadata failures as fatal process failures
    emitter.instruction("mov x0, #1");                                          // preserve a nonzero status for a broken identity contract
    emitter.bl_c("exit");
    emitter.label(&format!("{label}_success"));
    if !dormant_gate(operation) { abi::emit_store_imm_to_symbol(emitter, "_mbstring_ini_native_active", 0, 1); }
    if operation == "reset" { abi::emit_store_zero_to_symbol(emitter, "_mbstring_ini_native_active", 0); }
    for register in (0..32).step_by(2) {
        emitter.instruction(&format!("ldp q{register}, q{}, [sp, #{}]", register + 1, 256 + register * 16));// restore native floating-point and vector values
    }
    for register in (0..30).step_by(2) {
        emitter.instruction(&format!("ldp x{register}, x{}, [sp, #{}]", register + 1, register * 8));// restore the native caller's original integer pair
    }
    emitter.instruction("ldr x30, [sp, #240]");                                 // recover the leaf caller's original return address
    emitter.instruction("add sp, sp, #768");                                    // release the complete aligned register-save frame
    emitter.instruction("ret");                                                 // return without changing the native input or result registers
    if dormant_gate(operation) {
        emitter.label(&format!("{label}_inactive"));
        emitter.instruction("ldp x9, x30, [sp], #16");                          // remove the dormant gate's temporary save
        emitter.instruction("ret");                                             // return without calling Rust or touching metadata ownership
    }
}

/// Adapts rax/rdx/rcx while preserving all GPR/XMM values and accepting either native leaf stack phase.
fn x86_64(emitter: &mut Emitter, operation: &str) {
    let label = format!("__rt_mbstring_ini_{operation}");
    emitter.label_global(&label);
    if dormant_gate(operation) {
        emitter.instruction("push r11");                                        // preserve the sole dormant-gate scratch register
        abi::emit_load_symbol_to_reg(emitter, "r11", "_mbstring_ini_native_active", 0);
        emitter.instruction("test r11, r11");                                   // test whether this request owns native INI metadata
        emitter.instruction("pop r11");                                         // restore the caller's scratch value without changing the gate flags
        emitter.instruction(&format!("jz {label}_inactive"));                   // skip all Rust work when the identity map has never been activated
    }
    emitter.instruction("push rbp");                                            // preserve frame linkage independently of the incoming stack phase
    emitter.instruction("mov rbp, rsp");                                        // retain the exact stack position for lossless teardown
    let registers = ["rax", "rbx", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"];
    for register in registers { emitter.instruction(&format!("push {register}")); }// preserve every non-frame general register across Rust callbacks
    emitter.instruction("sub rsp, 256");                                        // reserve sixteen full XMM register slots before alignment
    emitter.instruction("and rsp, -16");                                        // repair either allocator leaf stack phase for a SysV C call
    for register in 0..16 {
        emitter.instruction(&format!("movdqu XMMWORD PTR [rsp + {}], xmm{register}", register * 16));// preserve each complete vector value before calling Rust
    }
    if operation != "reset" {
        emitter.instruction("mov rdi, rax");                                    // pass the native owner or destination allocation to C
        if operation != "forget" {
            emitter.instruction("mov rsi, rdx");                                // pass the source pointer or bound byte length
            if matches!(operation, "bind" | "copy" | "persist") {
                emitter.instruction("mov rdx, rcx");                            // pass the copy length or retained identity token
            }
        }
    }
    emitter.bl_c(&format!("elephc_mbstring_native_string_{operation}_v1"));
    emitter.instruction("test eax, eax");                                       // distinguish successful metadata ownership from an internal failure
    emitter.instruction(&format!("jz {label}_success"));                        // restore the native caller only after the bridge accepted the operation
    emitter.instruction("mov edi, 1");                                          // preserve nonzero fatal status for a broken identity contract
    emitter.bl_c("exit");
    emitter.label(&format!("{label}_success"));
    if !dormant_gate(operation) { abi::emit_store_imm_to_symbol(emitter, "_mbstring_ini_native_active", 0, 1); }
    if operation == "reset" { abi::emit_store_zero_to_symbol(emitter, "_mbstring_ini_native_active", 0); }
    for register in 0..16 {
        emitter.instruction(&format!("movdqu xmm{register}, XMMWORD PTR [rsp + {}]", register * 16));// restore floating-point and vector temporaries exactly
    }
    emitter.instruction("lea rsp, [rbp - 112]");                                // recover the integer-save frame without assuming incoming alignment
    for register in registers.into_iter().rev() { emitter.instruction(&format!("pop {register}")); }// restore every saved integer register in reverse order
    emitter.instruction("pop rbp");                                             // restore original linkage and the caller's exact stack pointer
    if dormant_gate(operation) { emitter.label(&format!("{label}_inactive")); }
    emitter.instruction("ret");                                                 // preserve the complete native value convention on every exit
}

/// Gates retirement and legacy copies until a creation or explicit binding has recorded an origin.
fn dormant_gate(operation: &str) -> bool { matches!(operation, "copy" | "forget" | "reset") }
