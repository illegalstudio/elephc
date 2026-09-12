//! Purpose:
//! Reduces packed and associative PHP arrays using independently owned Mixed carries.
//!
//! Called from:
//! - The typed ArrayReduce backend through the C argument ABI.
//!
//! Key details:
//! - Consumes a callable descriptor and borrows validated source and initial-value cells.
//! - A payload snapshot protects iteration against callback writes through COW.
//! - A native exception boundary owns every intermediate, including a pending callback result.

use crate::codegen_support::{abi, arrays, emit::Emitter, platform::Arch, value_boxing};
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};
use crate::types::PhpType;

const FRAME: usize = TRY_HANDLER_SLOT_SIZE + 160;
const HANDLER: usize = FRAME - 16;
const CALLBACK: usize = 8;
const SOURCE: usize = 16;
const CARRY: usize = 24;
const INPUT: usize = 32;
const ARGUMENTS: usize = 40;
const NEXT: usize = 48;
const CURSOR: usize = 56;
const PAYLOAD: usize = 64;
const BORROWED_SOURCE: usize = 72;
const BORROWED_INITIAL: usize = 80;
const ENTRY: usize = 112;
const PREVIOUS: usize = 120;
const PENDING: usize = 128;

/// Emits the owned-descriptor, borrowed-source, borrowed-initial reduction entry on every target.
pub fn emit_array_reduce_boxed(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_array_reduce_boxed");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [CALLBACK, BORROWED_SOURCE, BORROWED_INITIAL].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    for offset in [SOURCE, CARRY, INPUT, ARGUMENTS, NEXT, CURSOR, PENDING] {
        clear_slot(emitter, offset);
    }
    install_boundary(emitter);

    // -- retain the source payload separately from the caller's mutable Mixed cell --
    abi::load_at_offset(emitter, result, BORROWED_SOURCE);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    abi::store_at_offset(emitter, value_low_reg(emitter), PAYLOAD);
    box_unboxed_value(emitter);
    abi::store_at_offset(emitter, result, SOURCE);
    abi::load_at_offset(emitter, result, BORROWED_INITIAL);
    acquire_borrowed_cell(emitter);
    abi::store_at_offset(emitter, result, CARRY);

    // -- the logical iterator handles packed widths, hash holes and insertion order --
    emitter.label("__rt_array_reduce_boxed_loop");
    for (index, offset) in [PAYLOAD, CURSOR].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_call_label(emitter, "__rt_array_iter_next");
    ins(emitter, "cmn x0, #1", "cmp rax, -1");
    ins(emitter, "b.eq __rt_array_reduce_boxed_cleanup", "je __rt_array_reduce_boxed_cleanup");
    abi::store_at_offset(emitter, result, CURSOR);
    let fields = if emitter.target.arch == Arch::AArch64 { ["x3", "x4", "x5"] } else { ["r8", "r9", "r10"] };
    for (index, reg) in fields.into_iter().enumerate() {
        abi::store_at_offset(emitter, reg, ENTRY - index * 8);
    }
    abi::emit_frame_slot_address(emitter, result, ENTRY);
    acquire_borrowed_cell(emitter);
    abi::store_at_offset(emitter, result, INPUT);
    prepare_callback_arguments(emitter);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), CALLBACK);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), ARGUMENTS);
    clear_slot(emitter, ARGUMENTS);
    abi::emit_call_label(emitter, "__rt_callable_invoke_owned_args");
    abi::store_at_offset(emitter, result, NEXT);
    release_slot(emitter, CARRY, "__rt_decref_mixed");
    abi::load_at_offset(emitter, result, NEXT);
    clear_slot(emitter, NEXT);
    abi::store_at_offset(emitter, result, CARRY);
    abi::emit_jump(emitter, "__rt_array_reduce_boxed_loop");

    // -- clear each owner before release so a destructor throw can resume this cleanup --
    emitter.label("__rt_array_reduce_boxed_cleanup");
    release_slot(emitter, INPUT, "__rt_decref_mixed");
    release_slot(emitter, ARGUMENTS, "__rt_decref_any");
    release_slot(emitter, NEXT, "__rt_decref_mixed");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_reduce_boxed_release_sources");
    release_slot(emitter, CARRY, "__rt_decref_mixed");
    emitter.label("__rt_array_reduce_boxed_release_sources");
    release_slot(emitter, SOURCE, "__rt_decref_mixed");
    release_slot(emitter, CALLBACK, "__rt_callable_descriptor_release");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_reduce_boxed_return");
    abi::load_at_offset(emitter, value_low_reg(emitter), PREVIOUS);
    clear_slot(emitter, PREVIOUS);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    restore_boundary(emitter);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_current");

    emitter.label("__rt_array_reduce_boxed_return");
    abi::load_at_offset(emitter, result, PREVIOUS);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    restore_boundary(emitter);
    abi::load_at_offset(emitter, result, CARRY);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);

    emitter.label("__rt_array_reduce_boxed_caught");
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::load_at_offset(emitter, value_low_reg(emitter), PENDING);
    abi::store_at_offset(emitter, result, PENDING);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::emit_jump(emitter, "__rt_array_reduce_boxed_cleanup");
}

/// Acquires real heap ownership from a stack triple without retaining the stack cell itself.
fn acquire_borrowed_cell(emitter: &mut Emitter) {
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    box_unboxed_value(emitter);
}

/// Adapts the unboxer's high payload register before invoking the retaining value constructor.
fn box_unboxed_value(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rsi, rdx");                                    // preserve string lengths and resource kinds across the constructor ABI
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
}

/// Builds an owned two-cell argument array, borrowing the carry and transferring the input owner.
fn prepare_callback_arguments(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_int_immediate(emitter, arg0, 2);
    abi::emit_load_int_immediate(emitter, arg1, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    arrays::emit_array_value_type_stamp(emitter, result, &PhpType::Mixed);
    abi::store_at_offset(emitter, result, ARGUMENTS);
    abi::load_at_offset(emitter, result, CARRY);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, arg1, CARRY);
    abi::load_at_offset(emitter, arg0, ARGUMENTS);
    abi::emit_call_label(emitter, "__rt_array_push_int");
    abi::store_at_offset(emitter, result, ARGUMENTS);
    abi::load_at_offset(emitter, arg1, INPUT);
    clear_slot(emitter, INPUT);
    abi::emit_reg_move(emitter, arg0, result);
    abi::emit_call_label(emitter, "__rt_array_push_int");
    abi::store_at_offset(emitter, result, ARGUMENTS);
    value_boxing::emit_box_current_owned_value_as_mixed(emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
    abi::store_at_offset(emitter, result, ARGUMENTS);
}

/// Publishes a local handler before snapshot or carry ownership can require unwinding.
fn install_boundary(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    for (symbol, offset) in [
        ("_exc_value", PREVIOUS),
        ("_exc_handler_top", HANDLER),
        ("_exc_call_frame_top", HANDLER - 8),
        ("_rt_diag_suppression", HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET),
    ] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, offset);
    }
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_frame_slot_address(emitter, result, HANDLER);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_handler_top", 0);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), HANDLER - TRY_HANDLER_JMP_BUF_OFFSET);
    emitter.bl_c("setjmp");                                                     // keep intermediate owners reachable when a callback or destructor throws
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_array_reduce_boxed_caught");
}

/// Restores the caller's handler and diagnostic depth after all reduction owners are retired.
fn restore_boundary(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    for (symbol, offset) in [
        ("_exc_handler_top", HANDLER),
        ("_rt_diag_suppression", HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET),
    ] {
        abi::load_at_offset(emitter, result, offset);
        abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    }
}

/// Clears an owner without clobbering either exception-chain or outgoing callback arguments.
fn clear_slot(emitter: &mut Emitter, offset: usize) {
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, scratch, 0);
    abi::store_at_offset(emitter, scratch, offset);
}

/// Retires an owner with zero-before-release ordering for resumable destructor cleanup.
fn release_slot(emitter: &mut Emitter, offset: usize, helper: &str) {
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset);
    clear_slot(emitter, offset);
    abi::emit_call_label(emitter, helper);
}

/// Returns the unboxed low-word register, also used as the exception-chain's older owner.
fn value_low_reg(emitter: &Emitter) -> &'static str {
    if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" }
}

/// Emits the equivalent instruction for each supported architecture.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 }); // keep reduction control flow identical across native ABIs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target roots the carry before callbacks and uses the storage-neutral iterator.
    #[test]
    fn boxed_reduce_roots_dynamic_carries_and_unwinds_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_array_reduce_boxed(&mut emitter);
            let asm = emitter.output();
            let boundary = asm.find(&target.extern_symbol("setjmp")).unwrap();
            let acquire = asm.find("__rt_mixed_from_value").unwrap();
            let invoke = asm.find("__rt_callable_invoke_owned_args").unwrap();
            assert!(boundary < acquire && acquire < invoke, "{name}");
            assert!(asm.contains("__rt_array_iter_next"), "{name}");
            assert!(asm.contains("__rt_array_reduce_boxed_caught:"), "{name}");
            assert!(asm.contains("__rt_exception_chain"), "{name}");
            assert!(asm.contains("__rt_callable_descriptor_release"), "{name}");
            assert!(asm.contains("__rt_throw_current"), "{name}");
            assert!(!asm.contains("__rt_mixed_clone"), "{name}: stack triples are borrowed");
            if target.arch == Arch::X86_64 {
                assert!(asm.contains("mov rsi, rdx"), "preserve high payload words");
            }
        }
    }
}
