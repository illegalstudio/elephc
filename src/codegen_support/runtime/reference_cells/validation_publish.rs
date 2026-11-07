//! Purpose:
//! Publishes an original RHS through one canonical reference under native type constraints.
//!
//! Called from:
//! - Native reference publishers with the explicit validation capability enabled.
//!
//! Key details:
//! - The owned RHS is detached before callbacks and every source sees that same snapshot.
//! - The snapshot lease ends at publication, before displaced-value destructors can run.
//! - A conversion conflict owns its TypeError before the last RHS lease is retired.
//! - Every exit unlinks both roots and contains cleanup exceptions in the caller's context.

use super::*;

const FRAME: usize = 128;
const CONTEXT: usize = 8;
const WRAPPER: usize = 16;
const ORIGINAL: usize = 24;
const OUTCOME: usize = 32;
const STATUS: usize = 40;
const SNAPSHOT: usize = 48;
const WRAPPER_PIN: usize = 80;
const SNAPSHOT_PIN: usize = 112;

/// Emits publish(context, wrapper, owned_rhs, outcome) with the coordinator's status ABI.
/// Context and outcome must be valid; the RHS is consumed even on a refused wrapper.
/// The caller raises any error only after this returns and retains no artificial RHS owner.
/// Conflict status 3 carries an owned TypeError in context.pending; cleanup throws return 2.
pub(in crate::codegen_support::runtime) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let failed = "__rt_reference_publish_native_invalid";
    let refused = "__rt_reference_publish_native_refused";
    let candidate_ready = "__rt_reference_publish_native_candidate_ready";
    let validation_failure = "__rt_reference_publish_native_validation_failure";
    let cleanup = "__rt_reference_publish_native_cleanup";
    let done = "__rt_reference_publish_native_done";
    emitter.label_global("__rt_reference_publish_native");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [CONTEXT, WRAPPER, ORIGINAL, OUTCOME].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    for offset in [CONTEXT, OUTCOME] {
        abi::load_at_offset(emitter, result, offset);
        abi::emit_branch_if_int_result_zero(emitter, failed);
    }
    abi::load_at_offset(emitter, result, OUTCOME);
    abi::emit_store_zero_to_address(emitter, result, 0);
    abi::load_at_offset(emitter, result, WRAPPER);
    abi::emit_call_label(emitter, "__rt_reference_is");
    abi::emit_branch_if_int_result_zero(emitter, refused);
    abi::load_at_offset(emitter, result, ORIGINAL);
    abi::emit_branch_if_int_result_zero(emitter, refused);

    // Cloning peels an RHS reference now, before Stringable callbacks can replace its child.
    enter_pin(emitter, WRAPPER_PIN, WRAPPER);
    abi::load_at_offset(emitter, result, ORIGINAL);
    abi::emit_call_label(emitter, "__rt_mixed_clone");
    abi::store_at_offset(emitter, result, SNAPSHOT);
    enter_pin(emitter, SNAPSHOT_PIN, SNAPSHOT);
    abi::load_at_offset(emitter, result, SNAPSHOT);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    // Consume the input owner while the detached snapshot guarantees non-final payload release.
    abi::load_at_offset(emitter, result, ORIGINAL);
    abi::emit_call_label(emitter, "__rt_decref_mixed");

    for (index, offset) in [WRAPPER, SNAPSHOT, CONTEXT, OUTCOME].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_call_label(emitter, "__rt_reference_validate_native");
    abi::store_at_offset(emitter, result, STATUS);
    abi::emit_branch_if_int_result_nonzero(emitter, validation_failure);
    abi::load_at_offset(emitter, scratch, OUTCOME);
    abi::emit_load_from_address(emitter, result, scratch, 0);
    abi::emit_branch_if_int_result_nonzero(emitter, candidate_ready);
    // No type source constrained this assignment. The snapshot still needs a transfer owner.
    abi::load_at_offset(emitter, result, SNAPSHOT);
    abi::emit_call_label(emitter, "__rt_mixed_clone");
    emitter.label(candidate_ready);
    abi::load_at_offset(emitter, scratch, OUTCOME);
    abi::emit_store_zero_to_address(emitter, scratch, 0);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 2), result);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), CONTEXT);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), WRAPPER);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 3), SNAPSHOT_PIN);
    abi::emit_call_label(emitter, "__rt_reference_commit_native_rooted");
    abi::store_at_offset(emitter, result, STATUS);
    abi::emit_jump(emitter, cleanup);
    emitter.label(validation_failure);
    branch_unless_conflict(emitter, cleanup);
    // Establish the conflict before an RHS destructor can throw a replacement exception.
    abi::emit_call_label(emitter, "__rt_reference_conflict_error_new");
    abi::load_at_offset(emitter, scratch, CONTEXT);
    abi::emit_store_to_address(emitter, result, scratch, 8);
    emitter.label(cleanup);
    for (record, label) in [(SNAPSHOT_PIN, "snapshot"), (WRAPPER_PIN, "wrapper")] {
        abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), record);
        abi::emit_call_label(emitter, "__rt_reference_validation_pin_take");
        abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 1), result);
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), CONTEXT);
        abi::emit_call_label(emitter, "__rt_reference_validation_release");
        let released = format!("__rt_reference_publish_native_{label}_released");
        abi::emit_branch_if_int_result_zero(emitter, &released);
        abi::store_at_offset(emitter, result, STATUS);
        emitter.label(&released);
    }
    abi::load_at_offset(emitter, result, CONTEXT);
    abi::emit_load_from_address(emitter, result, result, 8);
    let status_ready = "__rt_reference_publish_native_status_ready";
    abi::emit_branch_if_int_result_zero(emitter, status_ready);
    abi::load_at_offset(emitter, result, STATUS);
    let throwable = "__rt_reference_publish_native_throwable";
    branch_unless_conflict(emitter, throwable);
    abi::emit_jump(emitter, status_ready);
    emitter.label(throwable);
    abi::emit_load_int_immediate(emitter, result, 2);
    abi::store_at_offset(emitter, result, STATUS);
    emitter.label(status_ready);
    abi::load_at_offset(emitter, result, STATUS);
    abi::emit_jump(emitter, done);
    emitter.label(refused);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), CONTEXT);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), ORIGINAL);
    abi::emit_call_label(emitter, "__rt_reference_validation_release");
    abi::emit_branch_if_int_result_nonzero(emitter, done);
    emitter.label(failed);
    abi::emit_load_int_immediate(emitter, result, 1);
    emitter.label(done);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    emit_reference_error_allocator(emitter, "__rt_reference_conflict_error_new",
        "_spl_type_error_class_id", "_native_reference_conflict_msg",
        NATIVE_REFERENCE_CONFLICT_MSG.len() as i64);
}

/// Selects coordinator status 3 without changing its conflict metadata or owned exception.
fn branch_unless_conflict(emitter: &mut Emitter, other: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #3");                                  // conflict is distinct from a new cleanup Throwable
            emitter.instruction(&format!("b.ne {other}"));                      // preserve all non-conflict coordinator statuses
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 3");                                  // conflict is distinct from a new cleanup Throwable
            emitter.instruction(&format!("jne {other}"));                       // preserve all non-conflict coordinator statuses
        }
    }
}

/// Acquires one invisible owner before any callback can detach a real PHP binding.
fn enter_pin(emitter: &mut Emitter, record: usize, value: usize) {
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), record);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), value);
    abi::emit_call_label(emitter, "__rt_reference_validation_pin_enter");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All native ABIs root the same original input and unlink every lease before returning status.
    #[test]
    fn native_reference_publisher_roots_original_rhs_and_contains_every_exit() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let assembly = emitter.output();
            let snapshot = assembly.find("__rt_mixed_clone").unwrap();
            let validate = assembly.find("__rt_reference_validate_native").unwrap();
            let commit = assembly.find("__rt_reference_commit_native").unwrap();
            let conflict = assembly.find("__rt_reference_conflict_error_new").unwrap();
            let cleanup = assembly.find("__rt_reference_publish_native_cleanup:").unwrap();
            assert!(snapshot < validate && validate < commit && commit < conflict && conflict < cleanup, "{name}");
            assert!(assembly.contains("_spl_type_error_class_id"), "{name}: conflict owns a TypeError before snapshot retirement");
            assert_eq!(assembly.matches("__rt_reference_validation_pin_enter").count(), 2, "{name}");
            assert_eq!(assembly.matches("__rt_reference_validation_pin_take").count(), 2, "{name}");
            assert_eq!(assembly.matches("__rt_reference_validation_release").count(), 3, "{name}");
            assert!(!assembly.contains("__rt_throw_current"), "{name}: the owner removes stack roots before a PHP exit");
        }
    }
}
