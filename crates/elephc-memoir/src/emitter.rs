//! Purpose:
//! Defines the host assembly and ABI operations required by memory emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

use crate::Arch;

/// Compiler-owned PHP object layout consumed by pure graph visitors.
#[derive(Clone, Copy)]
pub struct GraphLayout {
    /// Byte layout fact supplied by the compiler.
    pub fiber_callable_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_object_size: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_own_call_frame_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_pending_throw_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_stack_base_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_stack_top_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_start_args_max: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_start_args_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_transfer_value_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub fiber_user_arg_max_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub gen_last_key_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub gen_last_value_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub gen_return_value_offset: i32,
    /// Byte layout fact supplied by the compiler.
    pub spl_dll_storage_offset: i32,
}

/// EIR GC metric selectors supplied without a dependency on the compiler IR.
#[derive(Clone, Copy)]
pub struct GcStatusSelectors {
    /// Stable EIR selector for this runtime metric.
    pub application_time: i64,
    /// Stable EIR selector for this runtime metric.
    pub collected: i64,
    /// Stable EIR selector for this runtime metric.
    pub collector_time: i64,
    /// Stable EIR selector for this runtime metric.
    pub destructor_time: i64,
    /// Stable EIR selector for this runtime metric.
    pub free_time: i64,
    /// Stable EIR selector for this runtime metric.
    pub protected: i64,
    /// Stable EIR selector for this runtime metric.
    pub roots: i64,
    /// Stable EIR selector for this runtime metric.
    pub running: i64,
    /// Stable EIR selector for this runtime metric.
    pub runs: i64,
}

/// Host services used exclusively while generating assembly in the compiler.
pub trait Emitter {
    /// Provides the compiler arch contract.
    fn arch(&self) -> Arch;
    /// Provides the compiler extern symbol contract.
    fn extern_symbol(&self, name: &str) -> String;
    /// Provides the compiler line comment prefix contract.
    fn line_comment_prefix(&self) -> &'static str;
    /// Provides the compiler cdylib boundary contract.
    fn cdylib_boundary(&self) -> bool;
    /// Provides the compiler phpext output contract.
    fn phpext_output(&self) -> bool;
    /// Provides the compiler graph layout contract.
    fn graph_layout(&self) -> GraphLayout;
    /// Provides the compiler gc status selectors contract.
    fn gc_status_selectors(&self) -> GcStatusSelectors;
    /// Delegates instruction emission to the host.
    fn instruction(&mut self, text: &str);
    /// Delegates label emission to the host.
    fn label(&mut self, name: &str);
    /// Delegates label global emission to the host.
    fn label_global(&mut self, name: &str);
    /// Delegates label shared emission to the host.
    fn label_shared(&mut self, name: &str);
    /// Delegates blank emission to the host.
    fn blank(&mut self);
    /// Delegates comment emission to the host.
    fn comment(&mut self, text: &str);
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
    /// Contains throws while releasing a weak observer through the compiler boundary.
    fn emit_guarded_cleanup_call(&mut self, entry: &str, payload: &str, pending_offset: usize);
}
