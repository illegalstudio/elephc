//! Purpose:
//! Roots native reference values during coercion without changing PHP alias sharing.
//!
//! Called from:
//! - Reference validation publishers and reference-sharing readers.
//!
//! Key details:
//! - Each caller-owned record contains previous/next links, the pinned value and padding.
//! - Removal works in any activation order, including interleaved suspended fibers.
//! - Every native throw must be contained until the owning publisher removes its records.

use super::*;

const TOP: &str = "_rt_reference_validation_pins_top";
/// Native publishers reserve one aligned record per independently retained value.
pub(crate) const RECORD_BYTES: usize = 32;

/// Subtracts active validation leases using only the explicitly supplied scratch registers.
/// The reference remains borrowed; this leaf expansion never calls PHP or changes ownership.
pub(crate) fn subtract(
    emitter: &mut Emitter, reference: &str, count: &str, cursor: &str, scratch: &str,
    prefix: &str,
) {
    let registers = [reference, count, cursor, scratch];
    for (index, register) in registers.iter().enumerate() {
        debug_assert!(!registers[index + 1..].contains(register));
    }
    let next = format!("{prefix}_pin_next");
    let skip = format!("{prefix}_pin_skip");
    let done = format!("{prefix}_pin_done");
    abi::emit_symbol_address(emitter, cursor, TOP);
    abi::emit_load_from_address(emitter, cursor, cursor, 0);
    emitter.label(&next);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz {cursor}, {done}"));              // inactive validation leaves ordinary sharing unchanged
            emitter.instruction(&format!("ldr {scratch}, [{cursor}, #16]"));    // inspect one stack-rooted validation lease
            emitter.instruction(&format!("cmp {scratch}, {reference}"));        // exclude only leases of this exact reference
            emitter.instruction(&format!("b.ne {skip}"));                       // unrelated nested validations do not affect this owner count
            emitter.instruction(&format!("sub {count}, {count}, #1"));          // remove one artificial owner from PHP sharing
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {cursor}, {cursor}"));           // inactive validation leaves ordinary sharing unchanged
            emitter.instruction(&format!("jz {done}"));                         // stop after the final active validation lease
            emitter.instruction(&format!("mov {scratch}, QWORD PTR [{cursor} + 16]")); // inspect one stack-rooted validation lease
            emitter.instruction(&format!("cmp {scratch}, {reference}"));        // exclude only leases of this exact reference
            emitter.instruction(&format!("jne {skip}"));                        // unrelated nested validations do not affect this owner count
            emitter.instruction(&format!("sub {count}, 1"));                    // remove one artificial owner from PHP sharing
        }
    }
    emitter.label(&skip);
    abi::emit_load_from_address(emitter, cursor, cursor, 8);
    abi::emit_jump(emitter, &next);
    emitter.label(&done);
}

/// Emits C-ABI enter(record, value) and leave(record), permitting interleaved activations.
/// Enter returns the record. Leave returns the protected cleanup status and preserves exceptions.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let (record, previous) = if emitter.target.arch == Arch::AArch64 { ("x10", "x11") }
        else { ("r10", "r11") };
    emitter.blank();
    emitter.label_global("__rt_reference_validation_pin_enter");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::emit_reg_move(emitter, result, abi::int_arg_reg_name(emitter.target, 1));
    abi::store_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_symbol_address(emitter, previous, TOP);
    abi::emit_load_from_address(emitter, previous, previous, 0);
    abi::load_at_offset(emitter, record, 8);
    abi::emit_store_zero_to_address(emitter, record, 0);
    abi::emit_store_to_address(emitter, previous, record, 8);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_store_to_address(emitter, result, record, 16);
    abi::emit_store_zero_to_address(emitter, record, RECORD_BYTES - 8);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x11, __rt_reference_validation_pin_enter_head"); // the first lease has no successor to relink
        }
        Arch::X86_64 => {
            emitter.instruction("test r11, r11");                               // the first lease has no successor to relink
            emitter.instruction("jz __rt_reference_validation_pin_enter_head"); // skip the absent successor
        }
    }
    abi::emit_store_to_address(emitter, record, previous, 0);
    emitter.label("__rt_reference_validation_pin_enter_head");
    abi::emit_symbol_address(emitter, previous, TOP);
    abi::emit_store_to_address(emitter, record, previous, 0);
    abi::emit_reg_move(emitter, result, record);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);

    emitter.label_global("__rt_reference_validation_pin_leave");
    abi::emit_frame_prologue(emitter, 32);
    abi::emit_reg_move(emitter, record, abi::int_arg_reg_name(emitter.target, 0));
    abi::emit_load_from_address(emitter, result, record, 16);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, previous, record, 0);
    let next = if emitter.target.arch == Arch::AArch64 { "x12" } else { "r9" };
    abi::emit_load_from_address(emitter, next, record, 8);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x11, __rt_reference_validation_pin_leave_head"); // an inner lease can retire while another fiber remains suspended
        }
        Arch::X86_64 => {
            emitter.instruction("test r11, r11");                               // distinguish the head from a suspended fiber's inner lease
            emitter.instruction("jz __rt_reference_validation_pin_leave_head"); // update the global head only for its current record
        }
    }
    abi::emit_store_to_address(emitter, next, previous, 8);
    abi::emit_jump(emitter, "__rt_reference_validation_pin_leave_next");
    emitter.label("__rt_reference_validation_pin_leave_head");
    abi::emit_symbol_address(emitter, result, TOP);
    abi::emit_store_to_address(emitter, next, result, 0);
    emitter.label("__rt_reference_validation_pin_leave_next");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x12, __rt_reference_validation_pin_leave_release"); // the tail has no successor backlink
        }
        Arch::X86_64 => {
            emitter.instruction("test r9, r9");                                 // the tail has no successor backlink
            emitter.instruction("jz __rt_reference_validation_pin_leave_release"); // finish unlinking before cleanup can reenter PHP
        }
    }
    abi::emit_store_to_address(emitter, previous, next, 0);
    emitter.label("__rt_reference_validation_pin_leave_release");
    abi::emit_store_zero_to_address(emitter, record, 0);
    abi::emit_store_zero_to_address(emitter, record, 8);
    abi::emit_store_zero_to_address(emitter, record, 16);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_decref_preserving_exception(emitter, &PhpType::Mixed);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every ABI publishes a retained root and removes the record before throwable cleanup.
    #[test]
    fn validation_pin_lifetime_order_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let assembly = emitter.output();
            let (enter, leave) = assembly.split_once("__rt_reference_validation_pin_leave:").unwrap();
            assert!(enter.find("__rt_incref").unwrap() < enter.find(TOP).unwrap(), "{name}: retain before publishing the record");
            assert!(leave.find(TOP).unwrap() < leave.find("__rt_decref_mixed").unwrap(), "{name}: remove the record before protected release");
            assert!(leave.contains("__rt_cleanup"), "{name}: final retirement must contain PHP exceptions");
        }
    }

    /// Sharing checks count nested leases by exact identity without implicit calls or scratch.
    #[test]
    fn validation_pin_visibility_is_leaf_and_target_aware() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            let registers = if target.arch == Arch::AArch64 { ["x0", "x9", "x10", "x11"] }
                else { ["rax", "rcx", "r10", "r11"] };
            subtract(&mut emitter, registers[0], registers[1], registers[2], registers[3], "pin_visibility");
            let assembly = emitter.output();
            assert!(assembly.contains(TOP) && assembly.contains("pin_visibility_pin_next:"), "{name}: scan all nested leases");
            assert!(assembly.contains(if target.arch == Arch::AArch64 { "cmp x11, x0" } else { "cmp r11, rax" }), "{name}: exact value identity");
            assert!(!assembly.lines().any(|line| matches!(line.split_whitespace().next(), Some("bl" | "call"))), "{name}: sharing checks remain leaf operations");
        }
    }
}
