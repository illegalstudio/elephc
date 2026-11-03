//! Purpose:
//! Transfers a pending native reference argument binding into its formal parameter owner.
//!
//! Called from:
//! - Native by-reference parameter entry after ABI arguments have reached their local slots.
//!
//! Key details:
//! - An activated argument record identifies the native callee's incoming stack pointer.
//! - Claiming clears the caller owner before transferring it, without running PHP or cleanup.
//! - Ordinary variables and legacy callers still acquire an independent parameter owner.

use super::*;

/// Emits C-ABI acquire(cell, incoming_sp), accepting only exact live managed reference cells.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let (cursor, scratch, expected) = if emitter.target.arch == Arch::AArch64 {
        ("x10", "x11", "x9")
    } else { ("r10", "r11", "r9") };
    let next = "__rt_reference_parameter_acquire_next";
    let advance = "__rt_reference_parameter_acquire_advance";
    let retain = "__rt_reference_parameter_acquire_retain";
    let done = "__rt_reference_parameter_acquire_done";
    emitter.blank();
    emitter.label_global("__rt_reference_parameter_acquire");
    abi::emit_frame_prologue(emitter, 64);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), 16);
    abi::emit_reg_move(emitter, result, abi::int_arg_reg_name(emitter.target, 0));
    abi::emit_call_label(emitter, "__rt_reference_cell_owner");
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_symbol_address(emitter, scratch, "__rt_cleanup_call_reference_argument");
    abi::store_at_offset(emitter, scratch, 24);
    abi::emit_load_symbol_to_reg(emitter, cursor, "_exc_call_frame_top", 0);
    emitter.label(next);
    branch_zero(emitter, cursor, retain);
    abi::emit_load_from_address(emitter, scratch, cursor, 8);
    abi::load_at_offset(emitter, expected, 24);
    branch_not_equal(emitter, scratch, expected, advance);
    abi::emit_load_from_address(emitter, scratch, cursor, 32);
    abi::load_at_offset(emitter, expected, 16);
    branch_not_equal(emitter, scratch, expected, advance);
    abi::emit_load_from_address(emitter, scratch, cursor, 16);
    abi::emit_load_from_address(emitter, result, scratch, 0);
    abi::load_at_offset(emitter, expected, 8);
    branch_not_equal(emitter, result, expected, advance);
    abi::emit_store_zero_to_address(emitter, scratch, 0);
    abi::emit_store_zero_to_address(emitter, cursor, 32);
    abi::emit_jump(emitter, done);
    emitter.label(advance);
    abi::emit_load_from_address(emitter, cursor, cursor, 0);
    abi::emit_jump(emitter, next);
    emitter.label(retain);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.label(done);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), result);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_jump(emitter, "__rt_reference_argument_pins_activate");
}

/// Skips a record that belongs to another call activation or another cell owner.
fn branch_not_equal(emitter: &mut Emitter, left: &str, right: &str, target: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {left}, {right}"));               // compare exact native record identity
            emitter.instruction(&format!("b.ne {target}"));                     // unrelated records remain untouched
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {left}, {right}"));               // compare exact native record identity
            emitter.instruction(&format!("jne {target}"));                      // unrelated records remain untouched
        }
    }
}

/// Detects the end of the activation chain without borrowing an implicit scratch register.
fn branch_zero(emitter: &mut Emitter, left: &str, target: &str) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cbz {left}, {target}")), // older or ordinary callers need a retained parameter owner
        Arch::X86_64 => {
            emitter.instruction(&format!("test {left}, {left}"));               // inspect the current activation cursor
            emitter.instruction(&format!("jz {target}"));                       // older or ordinary callers need a retained parameter owner
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every ABI authenticates first, transfers by clearing its caller slot and otherwise retains.
    #[test]
    fn native_reference_parameter_acquire_has_safe_transfer_and_fallback_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(crate::codegen_support::platform::Target::parse(target).unwrap());
            emit(&mut emitter);
            let arch = emitter.target.arch;
            let assembly = emitter.output();
            let authenticate = assembly.find("__rt_reference_cell_owner").unwrap();
            let records = assembly.find("_exc_call_frame_top").unwrap();
            let retain = assembly.rfind("__rt_incref").unwrap();
            assert!(authenticate < records && records < retain, "{target}: authenticate before claim or retain");
            assert!(assembly.contains("__rt_cleanup_call_reference_argument"), "{target}: only argument owners transfer");
            let cleared = if arch == Arch::AArch64 {
                "str xzr, [x11]"
            } else { "mov QWORD PTR [r11], 0" };
            assert!(assembly.find(cleared).is_some_and(|position| position < retain),
                "{target}: clear the pending caller owner before returning its transferred cell");
            assert!(!assembly.contains("__rt_decref") && !assembly.contains("__rt_throw"),
                "{target}: acquisition cannot invoke PHP or retire the transferred owner");
        }
    }
}
