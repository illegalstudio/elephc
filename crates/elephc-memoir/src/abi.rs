//! Purpose:
//! Forwards memory-emitter ABI operations to the compiler host.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

use crate::Emitter;

/// Delegates emit branch if equal wide without duplicating target logic.
pub(crate) fn emit_branch_if_equal_wide(emitter: &mut impl Emitter, label: &str) { emitter.emit_branch_if_equal_wide(label) }

/// Delegates emit branch if int result nonzero without duplicating target logic.
pub(crate) fn emit_branch_if_int_result_nonzero(emitter: &mut impl Emitter, label: &str) { emitter.emit_branch_if_int_result_nonzero(label) }

/// Delegates emit branch if int result zero without duplicating target logic.
pub(crate) fn emit_branch_if_int_result_zero(emitter: &mut impl Emitter, label: &str) { emitter.emit_branch_if_int_result_zero(label) }

/// Delegates emit call label without duplicating target logic.
pub(crate) fn emit_call_label(emitter: &mut impl Emitter, label: &str) { emitter.emit_call_label(label) }

/// Delegates emit call reg without duplicating target logic.
pub(crate) fn emit_call_reg(emitter: &mut impl Emitter, reg: &str) { emitter.emit_call_reg(reg) }

/// Delegates emit frame prologue without duplicating target logic.
pub(crate) fn emit_frame_prologue(emitter: &mut impl Emitter, frame_size: usize) { emitter.emit_frame_prologue(frame_size) }

/// Delegates emit frame restore without duplicating target logic.
pub(crate) fn emit_frame_restore(emitter: &mut impl Emitter, frame_size: usize) { emitter.emit_frame_restore(frame_size) }

/// Delegates emit jump without duplicating target logic.
pub(crate) fn emit_jump(emitter: &mut impl Emitter, label: &str) { emitter.emit_jump(label) }

/// Delegates emit load from address without duplicating target logic.
pub(crate) fn emit_load_from_address(emitter: &mut impl Emitter, reg: &str, addr_reg: &str, byte_offset: usize) { emitter.emit_load_from_address(reg, addr_reg, byte_offset) }

/// Delegates emit load int immediate without duplicating target logic.
pub(crate) fn emit_load_int_immediate(emitter: &mut impl Emitter, reg: &str, value: i64) { emitter.emit_load_int_immediate(reg, value) }

/// Delegates emit load symbol to reg without duplicating target logic.
pub(crate) fn emit_load_symbol_to_reg(emitter: &mut impl Emitter, reg: &str, symbol: &str, byte_offset: usize) { emitter.emit_load_symbol_to_reg(reg, symbol, byte_offset) }

/// Delegates emit local symbol address without duplicating target logic.
pub(crate) fn emit_local_symbol_address(emitter: &mut impl Emitter, dest: &str, symbol: &str) { emitter.emit_local_symbol_address(dest, symbol) }

/// Delegates emit pop reg without duplicating target logic.
pub(crate) fn emit_pop_reg(emitter: &mut impl Emitter, reg: &str) { emitter.emit_pop_reg(reg) }

/// Delegates emit push reg without duplicating target logic.
pub(crate) fn emit_push_reg(emitter: &mut impl Emitter, reg: &str) { emitter.emit_push_reg(reg) }

/// Delegates emit reg move without duplicating target logic.
pub(crate) fn emit_reg_move(emitter: &mut impl Emitter, dst: &str, src: &str) { emitter.emit_reg_move(dst, src) }

/// Delegates emit return without duplicating target logic.
pub(crate) fn emit_return(emitter: &mut impl Emitter) { emitter.emit_return() }

/// Delegates emit store imm to symbol without duplicating target logic.
pub(crate) fn emit_store_imm_to_symbol(emitter: &mut impl Emitter, symbol: &str, byte_offset: usize, imm: i64) { emitter.emit_store_imm_to_symbol(symbol, byte_offset, imm) }

/// Delegates emit store reg to symbol without duplicating target logic.
pub(crate) fn emit_store_reg_to_symbol(emitter: &mut impl Emitter, reg: &str, symbol: &str, byte_offset: usize) { emitter.emit_store_reg_to_symbol(reg, symbol, byte_offset) }

/// Delegates emit store zero to local slot without duplicating target logic.
pub(crate) fn emit_store_zero_to_local_slot(emitter: &mut impl Emitter, offset: usize) { emitter.emit_store_zero_to_local_slot(offset) }

/// Delegates emit store zero to symbol without duplicating target logic.
pub(crate) fn emit_store_zero_to_symbol(emitter: &mut impl Emitter, symbol: &str, byte_offset: usize) { emitter.emit_store_zero_to_symbol(symbol, byte_offset) }

/// Delegates emit symbol address without duplicating target logic.
pub(crate) fn emit_symbol_address(emitter: &mut impl Emitter, dest: &str, symbol: &str) { emitter.emit_symbol_address(dest, symbol) }

/// Delegates int arg reg name without duplicating target logic.
pub(crate) fn int_arg_reg_name(emitter: &impl Emitter, idx: usize) -> &'static str { emitter.int_arg_reg_name(idx) }

/// Delegates int result reg without duplicating target logic.
pub(crate) fn int_result_reg(emitter: &impl Emitter) -> &'static str { emitter.int_result_reg() }

/// Delegates load at offset without duplicating target logic.
pub(crate) fn load_at_offset(emitter: &mut impl Emitter, reg: &str, offset: usize) { emitter.load_at_offset(reg, offset) }

/// Delegates secondary scratch reg without duplicating target logic.
pub(crate) fn secondary_scratch_reg(emitter: &impl Emitter) -> &'static str { emitter.secondary_scratch_reg() }

/// Delegates store at offset without duplicating target logic.
pub(crate) fn store_at_offset(emitter: &mut impl Emitter, reg: &str, offset: usize) { emitter.store_at_offset(reg, offset) }

/// Delegates string result regs without duplicating target logic.
pub(crate) fn string_result_regs(emitter: &impl Emitter) -> (&'static str, &'static str) { emitter.string_result_regs() }

/// Uses the authoritative host emit cdylib exit escape operation.
pub(crate) fn emit_cdylib_exit_escape(emitter: &mut impl Emitter) { emitter.emit_cdylib_exit_escape() }

/// Uses the authoritative host emit store to address operation.
pub(crate) fn emit_store_to_address(emitter: &mut impl Emitter, reg: &str, addr_reg: &str, byte_offset: usize) { emitter.emit_store_to_address(reg, addr_reg, byte_offset) }

/// Uses the authoritative host emit store zero to address operation.
pub(crate) fn emit_store_zero_to_address(emitter: &mut impl Emitter, addr_reg: &str, byte_offset: usize) { emitter.emit_store_zero_to_address(addr_reg, byte_offset) }

/// Uses the authoritative host tertiary scratch reg operation.
pub(crate) fn tertiary_scratch_reg(emitter: &impl Emitter) -> &'static str { emitter.tertiary_scratch_reg() }

/// Uses the authoritative host emit cmp reg to symbol operation.
pub(crate) fn emit_cmp_reg_to_symbol(emitter: &mut impl Emitter, reg: &str, symbol: &str) { emitter.emit_cmp_reg_to_symbol(reg, symbol) }

/// Uses the authoritative host emit unary cleanup preserving exception operation.
pub(crate) fn emit_unary_cleanup_preserving_exception(emitter: &mut impl Emitter, entry: &str, payload: &str) { emitter.emit_unary_cleanup_preserving_exception(entry, payload) }

/// Delegates local frame addressing to the authoritative compiler ABI.
pub(crate) fn emit_frame_slot_address(emitter: &mut impl Emitter, dest: &str, offset: usize) {
    emitter.emit_frame_slot_address(dest, offset)
}
