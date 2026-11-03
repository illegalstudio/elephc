//! Purpose:
//! Keeps weak native type-source indexes aligned with canonical reference transitions.
//!
//! Called from:
//! - Explicit native-validation runtime emission, Mixed promotion and property detachment.
//!
//! Key details:
//! - Association authenticates the current physical cell and owns no PHP values.
//! - Metadata updates run without PHP, after publication and before displaced retirement.
//! - A failed registry mutation terminates instead of continuing with stale type constraints.

use super::*;

const FAILURE: &str = "__rt_reference_sources_failure";

/// Emits the optional C-ABI associate(cell) adapter, preserving the borrowed cell as its result.
pub(in crate::codegen_support::runtime) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    emitter.label_global("__rt_reference_source_associate_native");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::emit_reg_move(emitter, result, abi::int_arg_reg_name(emitter.target, 0));
    abi::emit_call_label(emitter, "__rt_reference_cell_canonical");
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 1), result);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    let entry = emitter.target.extern_symbol("__elephc_reference_sources_set_reference_v1");
    abi::emit_call_label(emitter, &entry);
    require_success(emitter);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);

    // These callbacks perform no PHP and cannot legitimately fail during a live assignment.
    // Continuing after a rejected weak-index mutation would silently lose a type constraint.
    emitter.label_global(FAILURE);
    emitter.bl_c("abort");
    abi::emit_jump(emitter, FAILURE);
}

/// Rejects a failed registry mutation before another instruction can expose stale weak metadata.
pub(super) fn require_success(emitter: &mut Emitter) {
    abi::emit_branch_if_int_result_nonzero(emitter, FAILURE);
}

/// Updates all registered bindings of the current cell without changing PHP ownership.
pub(super) fn associate_result(emitter: &mut Emitter) {
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), abi::int_result_reg(emitter));
    abi::emit_call_label(emitter, "__rt_reference_source_associate_native");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported ABI authenticates before indexing, preserving identity without PHP owners.
    #[test]
    fn native_source_association_authenticates_and_never_retains_values() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let assembly = emitter.output();
            let canonical = assembly.find("__rt_reference_cell_canonical").unwrap();
            let index = assembly.find("__elephc_reference_sources_set_reference_v1").unwrap();
            assert!(canonical < index, "{name}: never index a guessed wrapper pointer");
            assert!(!assembly.contains("__rt_incref") && !assembly.contains("__rt_decref"),
                "{name}: weak lifecycle operations cannot retain a value");
            assert!(assembly.contains(&target.extern_symbol("abort")),
                "{name}: rejected metadata must not silently disable validation");
        }
    }
}
