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
    abi::emit_teardown_call_alignment(emitter);
    emitter.bl_c("abort");
    abi::emit_jump(emitter, FAILURE);
    emit_bind_object(emitter);
}

/// Rejects a failed registry mutation before another instruction can expose stale weak metadata.
pub(in crate::codegen_support::runtime) fn require_success(emitter: &mut Emitter) {
    abi::emit_branch_if_int_result_nonzero(emitter, FAILURE);
}

/// Registers initialized owned cells from immutable physical rows before exposing a new object.
fn emit_bind_object(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let other = abi::tertiary_scratch_reg(emitter);
    let done = "__rt_reference_sources_bind_object_done";
    let next = "__rt_reference_sources_bind_object_next";
    let advance = "__rt_reference_sources_bind_object_advance";
    emitter.label_global("__rt_reference_sources_bind_object");
    abi::emit_frame_prologue(emitter, 80);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::emit_reg_move(emitter, result, abi::int_arg_reg_name(emitter.target, 0));
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_native_reference_source_class_count", 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {result}, {scratch}"));           // bound the class identifier before immutable row lookup
            emitter.instruction(&format!("b.hs {done}"));                       // synthetic and unknown objects have no owned binding rows
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {result}, {scratch}"));           // bound the class identifier before immutable row lookup
            emitter.instruction(&format!("jae {done}"));                        // synthetic and unknown objects have no owned binding rows
        }
    }
    abi::emit_symbol_address(emitter, scratch, "_native_reference_source_class_ptrs");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("ldr {result}, [{scratch}, {result}, lsl #3]")), // load this class's physical source rows
        Arch::X86_64 => emitter.instruction(&format!("mov {result}, QWORD PTR [{scratch} + {result} * 8]")), // load this class's physical source rows
    }
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_load_from_address(emitter, scratch, result, 0);
    abi::store_at_offset(emitter, scratch, 24);
    add_constant(emitter, result, 8);
    abi::store_at_offset(emitter, result, 16);
    emitter.label(next);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::load_at_offset(emitter, scratch, 16);
    abi::emit_load_from_address(emitter, result, scratch, 8);
    abi::store_at_offset(emitter, result, 56);
    abi::emit_load_from_address(emitter, result, scratch, 0);
    abi::load_at_offset(emitter, other, 8);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("add {result}, {other}, {result}")), // identify the exact physical property binding
        Arch::X86_64 => emitter.instruction(&format!("add {result}, {other}")), // identify the exact physical property binding
    }
    abi::store_at_offset(emitter, result, 40);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_branch_if_int_result_zero(emitter, advance);
    abi::store_at_offset(emitter, result, 48);
    for (index, offset) in [40, 48, 56].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    let entry = emitter.target.extern_symbol("__elephc_reference_sources_bind_v1");
    abi::emit_call_label(emitter, &entry);
    abi::emit_branch_if_int_result_zero(emitter, FAILURE);
    abi::load_at_offset(emitter, result, 48);
    associate_result(emitter);
    emitter.label(advance);
    abi::load_at_offset(emitter, result, 16);
    add_constant(emitter, result, 16);
    abi::store_at_offset(emitter, result, 16);
    abi::load_at_offset(emitter, result, 24);
    add_constant(emitter, result, -1);
    abi::store_at_offset(emitter, result, 24);
    abi::emit_jump(emitter, next);
    emitter.label(done);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_frame_restore(emitter, 80);
    abi::emit_return(emitter);
}

/// Advances a borrowed immutable row cursor or decreases its remaining count.
fn add_constant(emitter: &mut Emitter, register: &str, value: i64) {
    match emitter.target.arch {
        Arch::AArch64 if value >= 0 => emitter.instruction(&format!("add {register}, {register}, #{value}")), // advance the physical metadata cursor
        Arch::AArch64 => emitter.instruction(&format!("sub {register}, {register}, #{}", -value)), // consume one physical metadata row
        Arch::X86_64 => emitter.instruction(&format!("add {register}, {value}")), // advance the cursor or consume one metadata row
    }
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
            assert!(assembly.contains("_native_reference_source_class_ptrs")
                && assembly.contains("__elephc_reference_sources_bind_v1"),
                "{name}: initialized physical rows must register their own binding identities");
            assert!(!assembly.contains("__rt_incref") && !assembly.contains("__rt_decref"),
                "{name}: weak lifecycle operations cannot retain a value");
            assert!(assembly.contains(&target.extern_symbol("abort")),
                "{name}: rejected metadata must not silently disable validation");
            if target.arch == Arch::X86_64 {
                assert!(assembly.contains("and rsp, -16"),
                    "{name}: the non-returning failure entry aligns C calls from every incoming edge");
            }
        }
    }
}
