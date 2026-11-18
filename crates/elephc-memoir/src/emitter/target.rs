//! Purpose:
//! Delegates target conventions to the authoritative compiler ABI.
//!
//! Called from:
//! - Compiler memory emission and its host adapters.
//!
//! Key details:
//! - This capability runs only while the compiler emits assembly.

use crate::Arch;

/// Delegates target conventions to the authoritative compiler ABI.
pub trait TargetAbi {
    /// Returns the target platform flags for an anonymous private memory mapping.
    fn anonymous_private_map_flags(&self) -> i32;
    /// Provides the compiler arch contract.
    fn arch(&self) -> Arch;
    /// Provides the compiler extern symbol contract.
    fn extern_symbol(&self, name: &str) -> String;
    /// Provides the compiler cdylib boundary contract.
    fn cdylib_boundary(&self) -> bool;
    /// Provides the compiler phpext output contract.
    fn phpext_output(&self) -> bool;
    /// Delegates bl c emission to the host.
    fn bl_c(&mut self, name: &str);
    /// Delegates syscall emission to the host.
    fn syscall(&mut self, macos_num: u32);
    /// Delegates save exception handler emission to the host.
    fn save_exception_handler(&mut self);
    /// Delegates emit branch if equal wide to the authoritative target ABI.
    fn emit_branch_if_equal_wide(&mut self, label: &str);
    /// Delegates emit branch if int result nonzero to the authoritative target ABI.
    fn emit_branch_if_int_result_nonzero(&mut self, label: &str);
    /// Delegates emit branch if int result zero to the authoritative target ABI.
    fn emit_branch_if_int_result_zero(&mut self, label: &str);
    /// Delegates emit call label to the authoritative target ABI.
    fn emit_call_label(&mut self, label: &str);
    /// Delegates emit call reg to the authoritative target ABI.
    fn emit_call_reg(&mut self, reg: &str);
    /// Delegates emit frame prologue to the authoritative target ABI.
    fn emit_frame_prologue(&mut self, frame_size: usize);
    /// Delegates emit frame restore to the authoritative target ABI.
    fn emit_frame_restore(&mut self, frame_size: usize);
    /// Delegates emit jump to the authoritative target ABI.
    fn emit_jump(&mut self, label: &str);
    /// Delegates emit load from address to the authoritative target ABI.
    fn emit_load_from_address(&mut self, reg: &str, addr_reg: &str, byte_offset: usize);
    /// Delegates emit load int immediate to the authoritative target ABI.
    fn emit_load_int_immediate(&mut self, reg: &str, value: i64);
    /// Delegates emit load symbol to reg to the authoritative target ABI.
    fn emit_load_symbol_to_reg(&mut self, reg: &str, symbol: &str, byte_offset: usize);
    /// Delegates emit local symbol address to the authoritative target ABI.
    fn emit_local_symbol_address(&mut self, dest: &str, symbol: &str);
    /// Delegates emit pop reg to the authoritative target ABI.
    fn emit_pop_reg(&mut self, reg: &str);
    /// Delegates emit push reg to the authoritative target ABI.
    fn emit_push_reg(&mut self, reg: &str);
    /// Delegates emit reg move to the authoritative target ABI.
    fn emit_reg_move(&mut self, dst: &str, src: &str);
    /// Delegates emit return to the authoritative target ABI.
    fn emit_return(&mut self);
    /// Delegates emit store imm to symbol to the authoritative target ABI.
    fn emit_store_imm_to_symbol(&mut self, symbol: &str, byte_offset: usize, imm: i64);
    /// Delegates emit store reg to symbol to the authoritative target ABI.
    fn emit_store_reg_to_symbol(&mut self, reg: &str, symbol: &str, byte_offset: usize);
    /// Delegates emit store zero to local slot to the authoritative target ABI.
    fn emit_store_zero_to_local_slot(&mut self, offset: usize);
    /// Delegates emit store zero to symbol to the authoritative target ABI.
    fn emit_store_zero_to_symbol(&mut self, symbol: &str, byte_offset: usize);
    /// Delegates emit symbol address to the authoritative target ABI.
    fn emit_symbol_address(&mut self, dest: &str, symbol: &str);
    /// Delegates int arg reg name to the authoritative target ABI.
    fn int_arg_reg_name(&self, idx: usize) -> &'static str;
    /// Delegates int result reg to the authoritative target ABI.
    fn int_result_reg(&self) -> &'static str;
    /// Delegates load at offset to the authoritative target ABI.
    fn load_at_offset(&mut self, reg: &str, offset: usize);
    /// Delegates secondary scratch reg to the authoritative target ABI.
    fn secondary_scratch_reg(&self) -> &'static str;
    /// Delegates store at offset to the authoritative target ABI.
    fn store_at_offset(&mut self, reg: &str, offset: usize);
    /// Delegates string result regs to the authoritative target ABI.
    fn string_result_regs(&self) -> (&'static str, &'static str);
    /// Delegates emit cdylib exit escape to the established compiler ABI.
    fn emit_cdylib_exit_escape(&mut self);
    /// Delegates emit store to address to the established compiler ABI.
    fn emit_store_to_address(&mut self, reg: &str, addr_reg: &str, byte_offset: usize);
    /// Delegates emit store zero to address to the established compiler ABI.
    fn emit_store_zero_to_address(&mut self, addr_reg: &str, byte_offset: usize);
    /// Delegates tertiary scratch reg to the established compiler ABI.
    fn tertiary_scratch_reg(&self) -> &'static str;
    /// Delegates emit cmp reg to symbol to the established compiler ABI.
    fn emit_cmp_reg_to_symbol(&mut self, reg: &str, symbol: &str);
    /// Uses the host ABI to address a local slot in the active native frame.
    fn emit_frame_slot_address(&mut self, dest: &str, offset: usize);
    /// Selects the symbol-address scratch register without borrowing live result registers.
    fn symbol_scratch_reg(&self) -> &'static str;
    /// Stores a register in host-selected temporary stack storage.
    fn emit_store_to_sp(&mut self, reg: &str, offset: usize);
    /// Computes an address relative to the active temporary stack base.
    fn emit_temporary_stack_address(&mut self, dest: &str, offset: usize);
    /// Reserves aligned scratch through the host stack ABI.
    fn emit_reserve_temporary_stack(&mut self, amount: usize);
    /// Releases scratch through the host stack ABI.
    fn emit_release_temporary_stack(&mut self, amount: usize);
    /// Places two live registers into one aligned temporary stack record.
    fn emit_push_reg_pair(&mut self, low: &str, high: &str);
    /// Restores a register from host temporary stack storage.
    fn emit_load_temporary_stack_slot(&mut self, reg: &str, offset: usize);
}
