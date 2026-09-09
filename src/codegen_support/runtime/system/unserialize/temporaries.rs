//! Purpose:
//! Retains hydration data while the current wire stream can still reference its decoded values.
//!
//! Called from:
//! - Hydration-hook argument adaptation and parser-context completion.
//!
//! Key details:
//! - List nodes own their data payload and record the parser nesting depth that created them.
//! - A completed context detaches only its prefix before restoring any suspended outer parser.
//! - Bounded cleanup finishes every release and retires a discarded parse result before throwing.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::exceptions::emit_guarded_cleanup_call;

/// Emits context-owned data retention, prefix detachment and bounded completion helpers.
pub(super) fn emit_unserialize_temporaries(emitter: &mut Emitter) {
    emit_defer_data(emitter);
    emit_detach_data(emitter);
    emit_finish_data(emitter);
}

/// Consumes one data owner into a depth-stamped context node without changing its payload.
fn emit_defer_data(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_unserialize_defer_data");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_load_int_immediate(emitter, result, 24);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    abi::emit_load_symbol_to_reg(emitter, scratch, "_unser_temporaries", 0);
    abi::emit_store_to_address(emitter, scratch, result, 0);
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_store_to_address(emitter, scratch, result, 8);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_unser_active", 0);
    abi::emit_store_to_address(emitter, scratch, result, 16);
    abi::emit_store_reg_to_symbol(emitter, result, "_unser_temporaries", 0);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

/// Removes the current parser's list prefix, preserving every still-active outer context node.
fn emit_detach_data(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let depth = abi::tertiary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_unserialize_detach_data");
    abi::emit_frame_prologue(emitter, 48);
    abi::emit_load_symbol_to_reg(emitter, result, "_unser_temporaries", 0);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_store_zero_to_local_slot(emitter, 16);
    abi::emit_load_symbol_to_reg(emitter, depth, "_unser_active", 0);
    emitter.label("__rt_unserialize_detach_data_loop");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_unserialize_detach_data_done");
    abi::emit_load_from_address(emitter, scratch, result, 16);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x10, x11");                                // stop before a node belonging to a suspended outer parser
            emitter.instruction("b.lo __rt_unserialize_detach_data_done");      // leave the outer parser's owner list published
        }
        Arch::X86_64 => {
            emitter.instruction("cmp r10, r11");                                // stop before a node belonging to a suspended outer parser
            emitter.instruction("jb __rt_unserialize_detach_data_done");        // leave the outer parser's owner list published
        }
    }
    abi::store_at_offset(emitter, result, 16);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_jump(emitter, "__rt_unserialize_detach_data_loop");
    emitter.label("__rt_unserialize_detach_data_done");
    abi::store_at_offset(emitter, result, 24);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_unserialize_detach_data_return");
    abi::emit_store_zero_to_address(emitter, result, 0);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_store_reg_to_symbol(emitter, result, "_unser_temporaries", 0);
    abi::load_at_offset(emitter, result, 8);
    emitter.label("__rt_unserialize_detach_data_return");
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

/// Releases detached data after context restoration, returning the parsed owner or propagating cleanup.
fn emit_finish_data(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_unserialize_finish_data");
    abi::emit_frame_prologue(emitter, 64);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), 16);
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::store_at_offset(emitter, result, 24);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_store_zero_to_local_slot(emitter, 32);
    emitter.label("__rt_unserialize_finish_data_loop");
    abi::load_at_offset(emitter, result, 16);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_unserialize_finish_data_done");
    abi::emit_load_from_address(emitter, scratch, result, 0);
    abi::store_at_offset(emitter, scratch, 40);
    abi::emit_load_from_address(emitter, scratch, result, 8);
    abi::store_at_offset(emitter, scratch, 48);
    abi::emit_call_label(emitter, "__rt_heap_free");
    abi::load_at_offset(emitter, result, 48);
    emit_guarded_cleanup_call(emitter, "__rt_decref_any", result, 24);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_unserialize_finish_data_next");
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::store_at_offset(emitter, result, 32);
    emitter.label("__rt_unserialize_finish_data_next");
    abi::load_at_offset(emitter, result, 40);
    abi::store_at_offset(emitter, result, 16);
    abi::emit_jump(emitter, "__rt_unserialize_finish_data_loop");
    emitter.label("__rt_unserialize_finish_data_done");
    abi::load_at_offset(emitter, result, 32);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_unserialize_finish_data_return");
    abi::load_at_offset(emitter, result, 8);
    abi::emit_store_zero_to_local_slot(emitter, 8);
    emit_guarded_cleanup_call(emitter, "__rt_decref_mixed", result, 24);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label("__rt_unserialize_finish_data_return");
    abi::load_at_offset(emitter, result, 24);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Context cleanup is bounded and discards the otherwise-returned value before propagating a throw.
    #[test]
    fn hydration_temporary_cleanup_is_bounded_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_unserialize_temporaries(&mut emitter);
            let asm = emitter.output();
            let (defer, detach) = asm.split_once("__rt_unserialize_detach_data:").unwrap();
            let (detach, finish) = detach.split_once("__rt_unserialize_finish_data:").unwrap();
            let (defer_frame, detach_frame) = if name == "linux-x86_64" {
                ("sub rsp, 16", "sub rsp, 32")
            } else {
                ("sub sp, sp, #32", "sub sp, sp, #48")
            };
            assert!(defer.contains(defer_frame), "{name}: saved data must fit above the callee stack");
            assert!(detach.contains(detach_frame), "{name}: cursor spills must fit below the frame footer");
            assert!(detach.contains("_unser_active") && detach.contains("_unser_temporaries"), "{name}");
            assert!(!detach.contains("__rt_heap_free"), "{name}: detachment must not invoke PHP");
            assert_eq!(finish.matches("__rt_cleanup_invoke").count(), 2, "{name}");
            assert!(finish.find("__rt_decref_any").unwrap() < finish.find("__rt_decref_mixed").unwrap(), "{name}");
            assert!(finish.find("__rt_decref_mixed").unwrap() < finish.find("__rt_throw_current").unwrap(), "{name}");
        }
    }
}
