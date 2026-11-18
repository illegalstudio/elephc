//! Purpose:
//! Preserves the shared concat cursor across nested C export calls.
//!
//! Called from:
//! - Scalar, owned-string and PHP extension export wrappers.
//!
//! Key details:
//! - Nested boundaries use the linked capacity to isolate outer scratch data.
//! - Entry and exit maintain the depth tracked by allocation failure handling.

use crate::layout::BOUNDARY_ACTIVE;
use crate::{Arch, Emitter, abi};

/// Saves concat state, isolates nested scratch use, and increments boundary depth.
pub fn emit_enter_boundary(emitter: &mut impl Emitter, concat_offset: usize, suffix: &str) {
    let nested = format!("L_cdylib_{suffix}_boundary_nested");
    let configured = format!("L_cdylib_{suffix}_boundary_concat_configured");
    match emitter.arch() {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", BOUNDARY_ACTIVE, 0);
            emitter.instruction(&format!("cbnz x9, {nested}"));                 // isolate concat scratch only for a nested host entry
            emitter.instruction("mov x10, #0");                                 // outer entries restore an empty concat arena on return
            abi::store_at_offset(emitter, "x10", concat_offset);
            abi::emit_store_imm_to_symbol(emitter, "_concat_off", 0, 0);
            emitter.instruction(&format!("b {configured}"));                    // join nested and outer concat setup
            emitter.label(&nested);
            abi::emit_load_symbol_to_reg(emitter, "x10", "_concat_off", 0);
            abi::store_at_offset(emitter, "x10", concat_offset);
            abi::emit_load_symbol_to_reg(emitter, "x10", "_concat_capacity", 0);
            abi::emit_store_reg_to_symbol(emitter, "x10", "_concat_off", 0);
            emitter.label(&configured);
            abi::emit_load_symbol_to_reg(emitter, "x9", BOUNDARY_ACTIVE, 0);
            emitter.instruction("add x9, x9, #1");                              // increment the re-entrant boundary depth
            abi::emit_store_reg_to_symbol(emitter, "x9", BOUNDARY_ACTIVE, 0);
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", BOUNDARY_ACTIVE, 0);
            emitter.instruction("test r10, r10");                               // distinguish an outer entry from a re-entrant call
            emitter.instruction(&format!("jnz {nested}"));                      // isolate concat scratch only for a nested host entry
            emitter.instruction("xor r11d, r11d");                              // outer entries restore an empty concat arena on return
            abi::store_at_offset(emitter, "r11", concat_offset);
            abi::emit_store_imm_to_symbol(emitter, "_concat_off", 0, 0);
            emitter.instruction(&format!("jmp {configured}"));                  // join nested and outer concat setup
            emitter.label(&nested);
            abi::emit_load_symbol_to_reg(emitter, "r11", "_concat_off", 0);
            abi::store_at_offset(emitter, "r11", concat_offset);
            abi::emit_load_symbol_to_reg(emitter, "r11", "_concat_capacity", 0);
            abi::emit_store_reg_to_symbol(emitter, "r11", "_concat_off", 0);
            emitter.label(&configured);
            abi::emit_load_symbol_to_reg(emitter, "r10", BOUNDARY_ACTIVE, 0);
            emitter.instruction("add r10, 1");                                  // increment the re-entrant boundary depth
            abi::emit_store_reg_to_symbol(emitter, "r10", BOUNDARY_ACTIVE, 0);
        }
    }
}

/// Decrements boundary depth and restores the caller's concat scratch cursor.
pub fn emit_leave_boundary(emitter: &mut impl Emitter, concat_offset: usize) {
    match emitter.arch() {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", BOUNDARY_ACTIVE, 0);
            emitter.instruction("sub x9, x9, #1");                              // leave exactly one nested host boundary
            abi::emit_store_reg_to_symbol(emitter, "x9", BOUNDARY_ACTIVE, 0);
            abi::load_at_offset(emitter, "x10", concat_offset);
            abi::emit_store_reg_to_symbol(emitter, "x10", "_concat_off", 0);
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", BOUNDARY_ACTIVE, 0);
            emitter.instruction("sub r10, 1");                                  // leave exactly one nested host boundary
            abi::emit_store_reg_to_symbol(emitter, "r10", BOUNDARY_ACTIVE, 0);
            abi::load_at_offset(emitter, "r11", concat_offset);
            abi::emit_store_reg_to_symbol(emitter, "r11", "_concat_off", 0);
        }
    }
}
