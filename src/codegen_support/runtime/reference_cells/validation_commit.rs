//! Purpose:
//! Transfers one validated candidate into a persistent reference before retiring its old value.
//!
//! Called from:
//! - Native reference publishers after the Rust coordinator has returned successfully.
//!
//! Key details:
//! - The caller owns a valid context and keeps the wrapper alive until return.
//! - The candidate is consumed on every path, including pending exceptions and refused wrappers.
//! - Publication runs without PHP; old-value retirement contains Throwable exits in context.
//! - The rooted entry removes the original RHS lease after publication, before old retirement.

use super::*;

const FRAME: usize = 64;

/// Emits commit(context, wrapper, owned_candidate), returning success0, failure1 or Throwable2.
pub(in crate::codegen_support::runtime) fn emit(emitter: &mut Emitter) {
    emit_entry(emitter, "__rt_reference_commit_native", false);
    emit_entry(emitter, "__rt_reference_commit_native_rooted", true);
}

/// Optionally consumes the successful publisher's snapshot lease at the ownership transfer point.
fn emit_entry(emitter: &mut Emitter, entry: &str, take_snapshot: bool) {
    let result = abi::int_result_reg(emitter);
    let wrapper = abi::secondary_scratch_reg(emitter);
    let pending = format!("{entry}_pending");
    let invalid = format!("{entry}_invalid");
    let done = format!("{entry}_done");
    emitter.label_global(entry);
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [8, 16, 24].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    if take_snapshot {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 3), 40);
    }
    abi::load_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, result, result, 8);
    abi::emit_branch_if_int_result_nonzero(emitter, &pending);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_branch_if_int_result_zero(emitter, &invalid);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_reference_is");
    abi::emit_branch_if_int_result_zero(emitter, &invalid);
    abi::load_at_offset(emitter, wrapper, 16);
    abi::emit_load_from_address(emitter, result, wrapper, 8);
    abi::store_at_offset(emitter, result, 32);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_store_to_address(emitter, result, wrapper, 8);
    // The wrapper now owns the candidate. Never release that transferred owner again.
    if take_snapshot {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 40);
        abi::emit_call_label(emitter, "__rt_reference_validation_pin_take");
        abi::store_at_offset(emitter, result, 24);
        emit_release_slot(emitter, 24);
    }
    emit_release_slot(emitter, 32);
    if take_snapshot {
        // A snapshot destructor may throw even when the displaced child's release succeeds.
        abi::load_at_offset(emitter, result, 8);
        abi::emit_load_from_address(emitter, result, result, 8);
        let complete = format!("{entry}_complete");
        abi::emit_branch_if_int_result_zero(emitter, &complete);
        abi::emit_load_int_immediate(emitter, result, 2);
        abi::emit_jump(emitter, &done);
        emitter.label(&complete);
        abi::emit_load_int_immediate(emitter, result, 0);
    }
    abi::emit_jump(emitter, &done);
    emitter.label(&pending);
    emit_release_slot(emitter, 24);
    abi::emit_load_int_immediate(emitter, result, 2);
    abi::emit_jump(emitter, &done);
    emitter.label(&invalid);
    emit_release_slot(emitter, 24);
    abi::emit_branch_if_int_result_nonzero(emitter, &done);
    abi::emit_load_int_immediate(emitter, result, 1);
    emitter.label(&done);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Retires one owned frame value through the context's protected release operation.
fn emit_release_slot(emitter: &mut Emitter, offset: usize) {
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), offset);
    abi::emit_call_label(emitter, "__rt_reference_validation_release");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All targets publish before throwable retirement and consume refused candidates separately.
    #[test]
    fn native_reference_commit_transfers_before_protected_retirement_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let assembly = emitter.output();
            let (plain, rooted) = assembly.split_once("__rt_reference_commit_native_rooted:").unwrap();
            let (publish, refused) = plain.split_once("__rt_reference_commit_native_pending:").unwrap();
            let store = if name == "linux-x86_64" { "mov QWORD PTR [r10 + 8], rax" }
                else { "str x0, [x10, #8]" };
            assert!(publish.find(store).unwrap() < publish.find("__rt_reference_validation_release").unwrap(), "{name}: publish before cleanup");
            assert_eq!(refused.matches("__rt_reference_validation_release").count(), 2, "{name}: pending and invalid paths each consume their candidate");
            let success = rooted.split_once("__rt_reference_commit_native_rooted_pending:").unwrap().0;
            let publication = success.find(store).unwrap();
            let take = success.find("__rt_reference_validation_pin_take").unwrap();
            let first_release = success.find("__rt_reference_validation_release").unwrap();
            let last_release = success.rfind("__rt_reference_validation_release").unwrap();
            assert!(publication < take && take < first_release && first_release < last_release,
                "{name}: retire the snapshot after publication but before the displaced value");
            assert!(!assembly.contains("__rt_throw_current"), "{name}: the publisher retires roots before propagating a Throwable");
        }
    }
}
