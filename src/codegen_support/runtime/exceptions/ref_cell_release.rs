//! Purpose:
//! Retires an owned local reference cell even when its payload destructor throws.
//!
//! Called from:
//! - The typed ABI release helper used by explicit reference retirement and function epilogues.
//!
//! Key details:
//! - C arguments are a nullable unary payload-release entry and the owned cell pointer.
//! - The cell is freed before an accumulated exception rejoins the enclosing exception chain.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const FRAME: usize = 64;
const ENTRY: usize = 8;
const CELL: usize = 16;
const THROWN: usize = 24;

/// Emits bounded payload release followed by unconditional cell retirement on every target.
pub fn emit_local_ref_cell_release(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_local_ref_cell_release");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [ENTRY, CELL].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_store_zero_to_local_slot(emitter, THROWN);
    abi::load_at_offset(emitter, result, ENTRY);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_local_ref_cell_release_free");
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), ENTRY);
    abi::load_at_offset(emitter, scratch, CELL);
    abi::emit_load_from_address(emitter, abi::int_arg_reg_name(emitter.target, 1), scratch, 0);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 2), THROWN);
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
    emitter.label("__rt_local_ref_cell_release_free");
    abi::load_at_offset(emitter, result, CELL);
    abi::emit_call_label(emitter, "__rt_heap_free");
    abi::load_at_offset(emitter, result, THROWN);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_local_ref_cell_release_return");
    let older = match emitter.target.arch { Arch::AArch64 => "x1", Arch::X86_64 => "rdi" };
    abi::emit_load_symbol_to_reg(emitter, older, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, THROWN);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label("__rt_local_ref_cell_release_return");
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All target variants release the payload inside a boundary and free the cell before rethrowing.
    #[test]
    fn local_ref_cell_cleanup_finishes_before_exception_propagation() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_local_ref_cell_release(&mut emitter);
            let asm = emitter.output();
            let release = asm.find("__rt_cleanup_invoke").unwrap();
            let free = asm.find("__rt_heap_free").unwrap();
            let chain = asm.find("__rt_exception_chain").unwrap();
            let propagate = asm.find("__rt_throw_current").unwrap();
            assert!(release < free && free < chain && chain < propagate, "{name}");
            assert_eq!(asm.matches("__rt_heap_free").count(), 1, "{name}: retire the cell once");
        }
    }
}
