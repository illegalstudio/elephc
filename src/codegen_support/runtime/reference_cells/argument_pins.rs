//! Purpose:
//! Keeps eval source leases visible until their exact native parameter binding is acquired.
//!
//! Called from:
//! - Eval argument pin creation, native parameter acquisition and staging-root retirement.
//!
//! Key details:
//! - Extended six-word roots append the pending native cell and its exact staging root.
//! - Only the matching cell can consume a root; callbacks using another cell cannot claim it.
//! - Retiring staging cancels pending associations before the cell address can be reused.

use super::*;

/// Pending eval argument roots still represent a PHP-visible binding during adapter coercion.
pub(super) const PENDING: i64 = 0x105;
/// Acquired eval argument records no longer own their former source value.
const ACTIVE: i64 = 0x205;

/// Emits pending-root creation and the two no-PHP association transitions.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.label_global("__rt_reference_argument_pin_enter");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 2), 8);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 3), 16);
    abi::emit_call_label(emitter, "__rt_reference_validation_pin_enter");
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_store_to_address(emitter, scratch, result, 32);
    abi::load_at_offset(emitter, scratch, 16);
    abi::emit_store_to_address(emitter, scratch, result, 40);
    abi::emit_load_int_immediate(emitter, scratch, PENDING);
    abi::emit_store_to_address(emitter, scratch, result, 24);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
    emit_transition(emitter, "activate", true);
    emit_transition(emitter, "forget", false);
    emit_staging_lookup(emitter);
}

/// Matches the exact cell for activation or the exact staging record for cancellation.
fn emit_transition(emitter: &mut Emitter, action: &str, activate: bool) {
    let prefix = format!("__rt_reference_argument_pins_{action}");
    let next = format!("{prefix}_next");
    let advance = format!("{prefix}_advance");
    let done = format!("{prefix}_done");
    let cell = abi::int_arg_reg_name(emitter.target, 0);
    let cursor = abi::secondary_scratch_reg(emitter);
    let (kind, expected) = if emitter.target.arch == Arch::AArch64 {
        ("x11", "x9")
    } else { ("r11", "r9") };
    emitter.label_global(&prefix);
    validation_pins::branch_zero(emitter, cell, &done);
    abi::emit_load_symbol_to_reg(emitter, cursor, "_rt_reference_validation_pins_top", 0);
    emitter.label(&next);
    validation_pins::branch_zero(emitter, cursor, &done);
    abi::emit_load_from_address(emitter, kind, cursor, 24);
    abi::emit_load_int_immediate(emitter, expected, PENDING);
    validation_pins::branch_not_equal(emitter, kind, expected, &advance);
    abi::emit_load_from_address(emitter, kind, cursor, if activate { 32 } else { 40 });
    validation_pins::branch_not_equal(emitter, kind, cell, &advance);
    if activate {
        abi::emit_load_int_immediate(emitter, kind, ACTIVE);
        abi::emit_store_to_address(emitter, kind, cursor, 24);
        abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), cursor, 16);
        abi::emit_store_zero_to_address(emitter, cursor, 16);
    }
    abi::emit_store_zero_to_address(emitter, cursor, 32);
    abi::emit_store_zero_to_address(emitter, cursor, 40);
    if activate { abi::emit_return(emitter); }
    emitter.label(&advance);
    abi::emit_load_from_address(emitter, cursor, cursor, 8);
    abi::emit_jump(emitter, &next);
    emitter.label(&done);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_return(emitter);
}

/// Finds the exact staging lease before source consumption clears its associations.
fn emit_staging_lookup(emitter: &mut Emitter) {
    let cell = abi::int_arg_reg_name(emitter.target, 0);
    let cursor = abi::secondary_scratch_reg(emitter);
    let (kind, expected) = if emitter.target.arch == Arch::AArch64 {
        ("x11", "x9")
    } else { ("r11", "r9") };
    let next = "__rt_reference_argument_pins_staging_next";
    let advance = "__rt_reference_argument_pins_staging_advance";
    let done = "__rt_reference_argument_pins_staging_done";
    emitter.label_global("__rt_reference_argument_pins_staging");
    abi::emit_load_symbol_to_reg(emitter, cursor, "_rt_reference_validation_pins_top", 0);
    emitter.label(next);
    validation_pins::branch_zero(emitter, cursor, done);
    abi::emit_load_from_address(emitter, kind, cursor, 24);
    abi::emit_load_int_immediate(emitter, expected, PENDING);
    validation_pins::branch_not_equal(emitter, kind, expected, advance);
    abi::emit_load_from_address(emitter, kind, cursor, 32);
    validation_pins::branch_not_equal(emitter, kind, cell, advance);
    abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), cursor, 40);
    abi::emit_return(emitter);
    emitter.label(advance);
    abi::emit_load_from_address(emitter, cursor, cursor, 8);
    abi::emit_jump(emitter, next);
    emitter.label(done);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Transitions authenticate the extended record before touching its extra word on all targets.
    #[test]
    fn native_argument_root_activation_matches_exact_cells_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(crate::codegen_support::platform::Target::parse(name).unwrap());
            emit(&mut emitter);
            let assembly = emitter.output();
            for action in ["activate", "forget"] {
                let body = assembly.split_once(&format!("__rt_reference_argument_pins_{action}:\n")).unwrap().1;
                let body = body.split_once(&format!("__rt_reference_argument_pins_{action}_done:")).unwrap().0;
                let kind = if name == "linux-x86_64" { "QWORD PTR [r10 + 24]" } else { "[x10, #24]" };
                let offset = if action == "activate" { 32 } else { 40 };
                let identity = if name == "linux-x86_64" { format!("QWORD PTR [r10 + {offset}]") }
                    else { format!("[x10, #{offset}]") };
                assert!(body.find(kind).unwrap() < body.find(&identity).unwrap(), "{name}: {action}");
                assert!(!body.contains("bl ") && !body.contains("call "), "{name}: {action} cannot execute PHP");
            }
        }
    }
}
