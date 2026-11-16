//! Purpose:
//! Implements the Memoir emission boundary using the existing compiler target and ABI.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

use crate::codegen_support::{abi, emit::Emitter};
use elephc_memoir::{Arch, GraphLayout, GcStatusSelectors};

impl elephc_memoir::Emitter for Emitter {
    /// Keeps the PHP uninitialized-property marker authoritative in the compiler.
    fn uninitialized_property_sentinel(&self) -> i64 {
        crate::codegen_support::sentinels::UNINITIALIZED_TYPED_PROPERTY_SENTINEL
    }
    /// Supplies the compiler arch contract.
    fn arch(&self) -> Arch { self.target.arch }
    /// Supplies the compiler extern symbol contract.
    fn extern_symbol(&self, name: &str) -> String { self.target.extern_symbol(name) }
    /// Supplies the compiler line comment prefix contract.
    fn line_comment_prefix(&self) -> &'static str { self.target.line_comment_prefix() }
    /// Supplies the compiler cdylib boundary contract.
    fn cdylib_boundary(&self) -> bool { self.cdylib_boundary }
    /// Supplies the compiler phpext output contract.
    fn phpext_output(&self) -> bool { self.phpext_output }
    /// Supplies the compiler graph layout contract.
    fn graph_layout(&self) -> GraphLayout { GraphLayout {
            fiber_stack_size_offset: super::fibers::FIBER_STACK_SIZE_OFFSET,
            spl_fixed_storage_offset: super::spl::SPL_FIXED_STORAGE_OFFSET as i32,
            fiber_callable_offset: super::fibers::FIBER_CALLABLE_OFFSET,
            fiber_object_size: super::fibers::FIBER_OBJECT_SIZE,
            fiber_own_call_frame_offset: super::fibers::FIBER_OWN_CALL_FRAME_OFFSET,
            fiber_pending_throw_offset: super::fibers::FIBER_PENDING_THROW_OFFSET,
            fiber_stack_base_offset: super::fibers::FIBER_STACK_BASE_OFFSET,
            fiber_stack_top_offset: super::fibers::FIBER_STACK_TOP_OFFSET,
            fiber_start_args_max: super::fibers::FIBER_START_ARGS_MAX,
            fiber_start_args_offset: super::fibers::FIBER_START_ARGS_OFFSET,
            fiber_transfer_value_offset: super::fibers::FIBER_TRANSFER_VALUE_OFFSET,
            fiber_user_arg_max_offset: super::fibers::FIBER_USER_ARG_MAX_OFFSET,
            gen_last_key_offset: super::generators::coro::GEN_LAST_KEY_OFFSET,
            gen_last_value_offset: super::generators::coro::GEN_LAST_VALUE_OFFSET,
            gen_return_value_offset: super::generators::coro::GEN_RETURN_VALUE_OFFSET,
            spl_dll_storage_offset: super::spl::SPL_DLL_STORAGE_OFFSET as i32,
        } }
    /// Supplies the compiler gc status selectors contract.
    fn gc_status_selectors(&self) -> GcStatusSelectors { GcStatusSelectors {
            application_time: crate::ir::GcControlOp::ApplicationTime.as_i64(),
            collected: crate::ir::GcControlOp::Collected.as_i64(),
            collector_time: crate::ir::GcControlOp::CollectorTime.as_i64(),
            destructor_time: crate::ir::GcControlOp::DestructorTime.as_i64(),
            free_time: crate::ir::GcControlOp::FreeTime.as_i64(),
            protected: crate::ir::GcControlOp::Protected.as_i64(),
            roots: crate::ir::GcControlOp::Roots.as_i64(),
            running: crate::ir::GcControlOp::Running.as_i64(),
            runs: crate::ir::GcControlOp::Runs.as_i64(),
        } }
    /// Preserves the existing compiler instruction behavior.
    fn instruction(&mut self, text: &str) { Emitter::instruction(self, text); }
    /// Preserves the existing compiler label behavior.
    fn label(&mut self, name: &str) { Emitter::label(self, name); }
    /// Preserves the existing compiler label global behavior.
    fn label_global(&mut self, name: &str) { Emitter::label_global(self, name); }
    /// Preserves the existing compiler label shared behavior.
    fn label_shared(&mut self, name: &str) { Emitter::label_shared(self, name); }
    /// Preserves the existing compiler blank behavior.
    fn blank(&mut self) { Emitter::blank(self); }
    /// Preserves the existing compiler comment behavior.
    fn comment(&mut self, text: &str) { Emitter::comment(self, text); }
    /// Preserves the existing compiler bl c behavior.
    fn bl_c(&mut self, name: &str) { Emitter::bl_c(self, name); }
    /// Preserves the existing compiler syscall behavior.
    fn syscall(&mut self, macos_num: u32) { Emitter::syscall(self, macos_num); }
    /// Preserves the existing compiler save exception handler behavior.
    fn save_exception_handler(&mut self) { Emitter::save_exception_handler(self); }
    /// Uses the existing target ABI for emit branch if equal wide.
    fn emit_branch_if_equal_wide(&mut self, label: &str) { abi::emit_branch_if_equal_wide(self, label) }
    /// Uses the existing target ABI for emit branch if int result nonzero.
    fn emit_branch_if_int_result_nonzero(&mut self, label: &str) { abi::emit_branch_if_int_result_nonzero(self, label) }
    /// Uses the existing target ABI for emit branch if int result zero.
    fn emit_branch_if_int_result_zero(&mut self, label: &str) { abi::emit_branch_if_int_result_zero(self, label) }
    /// Uses the existing target ABI for emit call label.
    fn emit_call_label(&mut self, label: &str) { abi::emit_call_label(self, label) }
    /// Uses the existing target ABI for emit call reg.
    fn emit_call_reg(&mut self, reg: &str) { abi::emit_call_reg(self, reg) }
    /// Uses the existing target ABI for emit frame prologue.
    fn emit_frame_prologue(&mut self, frame_size: usize) { abi::emit_frame_prologue(self, frame_size) }
    /// Uses the existing target ABI for emit frame restore.
    fn emit_frame_restore(&mut self, frame_size: usize) { abi::emit_frame_restore(self, frame_size) }
    /// Uses the existing target ABI for emit jump.
    fn emit_jump(&mut self, label: &str) { abi::emit_jump(self, label) }
    /// Uses the existing target ABI for emit load from address.
    fn emit_load_from_address(&mut self, reg: &str, addr_reg: &str, byte_offset: usize) { abi::emit_load_from_address(self, reg, addr_reg, byte_offset) }
    /// Uses the existing target ABI for emit load int immediate.
    fn emit_load_int_immediate(&mut self, reg: &str, value: i64) { abi::emit_load_int_immediate(self, reg, value) }
    /// Uses the existing target ABI for emit load symbol to reg.
    fn emit_load_symbol_to_reg(&mut self, reg: &str, symbol: &str, byte_offset: usize) { abi::emit_load_symbol_to_reg(self, reg, symbol, byte_offset) }
    /// Uses the existing target ABI for emit local symbol address.
    fn emit_local_symbol_address(&mut self, dest: &str, symbol: &str) { abi::emit_local_symbol_address(self, dest, symbol) }
    /// Uses the existing target ABI for emit pop reg.
    fn emit_pop_reg(&mut self, reg: &str) { abi::emit_pop_reg(self, reg) }
    /// Uses the existing target ABI for emit push reg.
    fn emit_push_reg(&mut self, reg: &str) { abi::emit_push_reg(self, reg) }
    /// Uses the existing target ABI for emit reg move.
    fn emit_reg_move(&mut self, dst: &str, src: &str) { abi::emit_reg_move(self, dst, src) }
    /// Uses the existing target ABI for emit return.
    fn emit_return(&mut self) { abi::emit_return(self) }
    /// Uses the existing target ABI for emit store imm to symbol.
    fn emit_store_imm_to_symbol(&mut self, symbol: &str, byte_offset: usize, imm: i64) { abi::emit_store_imm_to_symbol(self, symbol, byte_offset, imm) }
    /// Uses the existing target ABI for emit store reg to symbol.
    fn emit_store_reg_to_symbol(&mut self, reg: &str, symbol: &str, byte_offset: usize) { abi::emit_store_reg_to_symbol(self, reg, symbol, byte_offset) }
    /// Uses the existing target ABI for emit store zero to local slot.
    fn emit_store_zero_to_local_slot(&mut self, offset: usize) { abi::emit_store_zero_to_local_slot(self, offset) }
    /// Uses the existing target ABI for emit store zero to symbol.
    fn emit_store_zero_to_symbol(&mut self, symbol: &str, byte_offset: usize) { abi::emit_store_zero_to_symbol(self, symbol, byte_offset) }
    /// Uses the existing target ABI for emit symbol address.
    fn emit_symbol_address(&mut self, dest: &str, symbol: &str) { abi::emit_symbol_address(self, dest, symbol) }
    /// Uses the existing target ABI for int arg reg name.
    fn int_arg_reg_name(&self, idx: usize) -> &'static str { abi::int_arg_reg_name(self.target, idx) }
    /// Uses the existing target ABI for int result reg.
    fn int_result_reg(&self) -> &'static str { abi::int_result_reg(self) }
    /// Uses the existing target ABI for load at offset.
    fn load_at_offset(&mut self, reg: &str, offset: usize) { abi::load_at_offset(self, reg, offset) }
    /// Uses the existing target ABI for secondary scratch reg.
    fn secondary_scratch_reg(&self) -> &'static str { abi::secondary_scratch_reg(self) }
    /// Uses the existing target ABI for store at offset.
    fn store_at_offset(&mut self, reg: &str, offset: usize) { abi::store_at_offset(self, reg, offset) }
    /// Uses the existing target ABI for string result regs.
    fn string_result_regs(&self) -> (&'static str, &'static str) { abi::string_result_regs(self) }
    /// Retains the existing protected weak-observer cleanup ABI.
    fn emit_guarded_cleanup_call(&mut self, entry: &str, payload: &str, pending_offset: usize) {
        super::exceptions::emit_guarded_cleanup_call(self, entry, payload, pending_offset);
    }
    /// Supplies the existing scratch capacity selected for this compilation.
    fn concat_capacity(&self) -> usize { self.concat_capacity }
    /// Preserves raw assembly directives.
    fn raw(&mut self, text: &str) { Emitter::raw(self, text); }
    /// Preserves compiler translation of native reference-source failures.
    fn emit_reference_source_success(&mut self) { super::reference_cells::require_source_success(self); }

    /// Delegates emit cdylib exit escape to the established compiler ABI.
    fn emit_cdylib_exit_escape(&mut self) { abi::emit_cdylib_exit_escape(self) }
    /// Delegates emit store to address to the established compiler ABI.
    fn emit_store_to_address(&mut self, reg: &str, addr_reg: &str, byte_offset: usize) { abi::emit_store_to_address(self, reg, addr_reg, byte_offset) }
    /// Delegates emit store zero to address to the established compiler ABI.
    fn emit_store_zero_to_address(&mut self, addr_reg: &str, byte_offset: usize) { abi::emit_store_zero_to_address(self, addr_reg, byte_offset) }
    /// Delegates tertiary scratch reg to the established compiler ABI.
    fn tertiary_scratch_reg(&self) -> &'static str { abi::tertiary_scratch_reg(self) }
    /// Delegates emit cmp reg to symbol to the established compiler ABI.
    fn emit_cmp_reg_to_symbol(&mut self, reg: &str, symbol: &str) { abi::emit_cmp_reg_to_symbol(self, reg, symbol) }
    /// Delegates emit unary cleanup preserving exception to the established compiler ABI.
    fn emit_unary_cleanup_preserving_exception(&mut self, entry: &str, payload: &str) { abi::emit_unary_cleanup_preserving_exception(self, entry, payload) }

}

impl From<crate::codegen_support::RuntimeFeatures> for elephc_memoir::ValueFeatures {
    /// Copies the exact feature gates used by value-retirement emitters.
    fn from(value: crate::codegen_support::RuntimeFeatures) -> Self {
        Self {
            eval_bridge: value.eval_bridge,
            fiber: value.fiber,
            generator: value.generator,
            native_reference_validation: value.native_reference_validation,
            directory_resource: value.directory_resource,
            popen_resource: value.popen_resource,
        }
    }
}

impl elephc_memoir::data::DataTarget for crate::codegen_support::platform::Target {
    /// Uses the compiler's established common-symbol spelling.
    fn common(&self, name: &str, bytes: usize) -> String {
        crate::codegen_support::data_section::comm_directive(name, bytes, *self)
    }
    /// Uses the compiler's established explicit-alignment spelling.
    fn aligned_common(&self, name: &str, bytes: usize, alignment: usize) -> String {
        crate::codegen_support::data_section::comm_directive_aligned(name, bytes, alignment, *self)
    }
}
