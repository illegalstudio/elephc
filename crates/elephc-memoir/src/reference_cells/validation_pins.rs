//! Purpose:
//! Roots boxed values and managed native cells without changing PHP alias sharing.
//!
//! Called from:
//! - Reference validation publishers and reference-sharing readers.
//!
//! Key details:
//! - Each record contains links, the pinned value and its authenticated kind or argument phase.
//! - Removal works in any activation order, including interleaved suspended fibers.
//! - Every native throw must be contained until the owning publisher removes its records.

use super::*;
use crate::owner_layout::ValidationPin;

/// Head of the active reference-validation ownership leases.
pub const TOP: &str = "_rt_reference_validation_pins_top";
/// Native publishers reserve one aligned record per independently retained value.
pub const RECORD_BYTES: usize = ValidationPin::BYTES;

/// Subtracts active validation leases using only the explicitly supplied scratch registers.
/// The reference remains borrowed; this leaf expansion never calls PHP or changes ownership.
pub fn subtract(
    emitter: &mut impl Emitter, reference: &str, count: &str, cursor: &str, scratch: &str,
    prefix: &str,
) {
    let registers = [reference, count, cursor, scratch];
    for (index, register) in registers.iter().enumerate() {
        debug_assert!(!registers[index + 1..].contains(register));
    }
    let next = format!("{prefix}_pin_next");
    let skip = format!("{prefix}_pin_skip");
    let done = format!("{prefix}_pin_done");
    let restore = format!("{prefix}_pin_restore");
    let direct = format!("{prefix}_pin_direct");
    let candidates: &[&str] = if emitter.arch() == Arch::AArch64 {
        &["x12", "x13", "x14", "x15", "x16", "x17", "x9", "x10", "x11"]
    } else { &["r8", "r9", "rdx", "rsi", "rdi", "rax", "rcx", "r10", "r11"] };
    let extra = candidates.iter().copied().filter(|register| !registers.contains(register))
        .take(3).collect::<Vec<_>>();
    abi::emit_symbol_address(emitter, cursor, TOP);
    abi::emit_load_from_address(emitter, cursor, cursor, 0);
    branch_zero(emitter, cursor, &done);
    for register in &extra { abi::emit_push_reg(emitter, register); }
    emitter.label(&next);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz {cursor}, {restore}"));           // inactive validation leaves ordinary sharing unchanged
            emitter.instruction(&format!("ldr {scratch}, [{cursor}, #16]"));    // inspect one stack-rooted validation lease
            emitter.instruction(&format!("cmp {scratch}, {reference}"));        // exclude only leases of this exact reference
            emitter.instruction(&format!("b.eq {direct}"));                     // a direct lease contributes exactly one invisible owner
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {cursor}, {cursor}"));           // inactive validation leaves ordinary sharing unchanged
            emitter.instruction(&format!("jz {restore}"));                      // stop after the final active validation lease
            emitter.instruction(&format!("mov {scratch}, QWORD PTR [{cursor} + 16]")); // inspect one stack-rooted validation lease
            emitter.instruction(&format!("cmp {scratch}, {reference}"));        // exclude only leases of this exact reference
            emitter.instruction(&format!("je {direct}"));                       // a direct lease contributes exactly one invisible owner
        }
    }
    subtract_unowned_cell_child(emitter, reference, cursor, scratch,
        [extra[0], extra[1], extra[2]], prefix, &direct, &skip);
    emitter.label(&direct);
    abi::emit_load_from_address(emitter, scratch, cursor, 24);
    let pending = if emitter.arch() == Arch::AArch64 {
        format!("cmp {scratch}, #{}", argument_pins::PENDING)
    } else { format!("cmp {scratch}, {}", argument_pins::PENDING) };
    emitter.instruction(&pending);                                              // pending adapter arguments still contribute a PHP-visible binding
    abi::emit_branch_if_equal_wide(emitter, &skip);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("sub {count}, {count}, #1")), // subtract one invisible owner from PHP sharing
        Arch::X86_64 => emitter.instruction(&format!("sub {count}, 1")),        // subtract one invisible owner from PHP sharing
    }
    emitter.label(&skip);
    abi::emit_load_from_address(emitter, cursor, cursor, 8);
    abi::emit_jump(emitter, &next);
    emitter.label(&restore);
    for register in extra.iter().rev() { abi::emit_pop_reg(emitter, register); }
    emitter.label(&done);
}

/// Excludes one child owner held only by a staging cell after every real PHP binding detached.
fn subtract_unowned_cell_child(
    emitter: &mut impl Emitter, reference: &str, outer: &str, scratch: &str,
    extra: [&str; 3], prefix: &str, subtract: &str, skip: &str,
) {
    let [cell, owners, node] = extra;
    let earlier = format!("{prefix}_pin_cell_earlier");
    let count = format!("{prefix}_pin_cell_count");
    let count_next = format!("{prefix}_pin_cell_count_next");
    let counted = format!("{prefix}_pin_cell_counted");
    branch_zero(emitter, scratch, skip);
    abi::emit_load_from_address(emitter, owners, outer, ValidationPin::KIND);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {owners}, #8"));                  // the record authenticates an exact live managed cell
            emitter.instruction(&format!("b.ne {skip}"));                       // static or ordinary boxed roots grant no cell-header access
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {owners}, 8"));                   // the record authenticates an exact live managed cell
            emitter.instruction(&format!("jne {skip}"));                        // static or ordinary boxed roots grant no cell-header access
        }
    }
    abi::emit_reg_move(emitter, cell, scratch);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldur {owners}, [{cell}, #-8]"));      // inspect the cell's physical descriptor
            emitter.instruction(&format!("ubfx {owners}, {owners}, #8, #7"));   // isolate the owned payload shape
            emitter.instruction(&format!("cmp {owners}, #7"));                  // only Mixed descriptor cells own a wrapper pointer
            emitter.instruction(&format!("b.ne {skip}"));                       // raw scalar bits are never interpreted as child owners
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {owners}, QWORD PTR [{cell} - 8]")); // inspect the cell's physical descriptor
            emitter.instruction(&format!("shr {owners}, 8"));                   // position its owned payload shape
            emitter.instruction(&format!("and {owners}, 127"));                 // remove heap and collector flags
            emitter.instruction(&format!("cmp {owners}, 7"));                   // only Mixed descriptor cells own a wrapper pointer
            emitter.instruction(&format!("jne {skip}"));                        // raw scalar bits are never interpreted as child owners
        }
    }
    abi::emit_load_from_address(emitter, scratch, cell, 0);
    branch_not_equal(emitter, scratch, reference, skip);
    abi::emit_symbol_address(emitter, node, TOP);
    abi::emit_load_from_address(emitter, node, node, 0);
    emitter.label(&earlier);
    branch_equal(emitter, node, outer, &count);
    abi::emit_load_from_address(emitter, scratch, node, ValidationPin::OWNER);
    branch_equal(emitter, scratch, cell, skip);
    abi::emit_load_from_address(emitter, node, node, ValidationPin::NEXT);
    abi::emit_jump(emitter, &earlier);
    emitter.label(&count);
    let word = low_word(emitter, owners);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("ldur {word}, [{cell}, #-12]")), // read only the four-byte owner count
        Arch::X86_64 => emitter.instruction(&format!("mov {word}, DWORD PTR [{cell} - 12]")), // read only the four-byte owner count
    }
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldur {scratch}, [{cell}, #-8]"));     // inspect the collector's separate temporary owner
            emitter.instruction(&format!("ubfx {scratch}, {scratch}, #18, #1")); // isolate its destructor pin
            emitter.instruction(&format!("sub {owners}, {owners}, {scratch}")); // keep collector ownership out of PHP sharing
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {scratch}, QWORD PTR [{cell} - 8]")); // inspect the collector's separate temporary owner
            emitter.instruction(&format!("shr {scratch}, 18"));                 // position its destructor pin
            emitter.instruction(&format!("and {scratch}, 1"));                  // isolate that one artificial owner
            emitter.instruction(&format!("sub {owners}, {scratch}"));           // keep collector ownership out of PHP sharing
        }
    }
    emitter.label(&count_next);
    branch_zero(emitter, node, &counted);
    abi::emit_load_from_address(emitter, scratch, node, ValidationPin::OWNER);
    let advance = format!("{prefix}_pin_cell_advance");
    branch_not_equal(emitter, scratch, cell, &advance);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("sub {owners}, {owners}, #1")), // exclude each artificial cell lease exactly once
        Arch::X86_64 => emitter.instruction(&format!("sub {owners}, 1")),       // exclude each artificial cell lease exactly once
    }
    emitter.label(&advance);
    abi::emit_load_from_address(emitter, node, node, ValidationPin::NEXT);
    abi::emit_jump(emitter, &count_next);
    emitter.label(&counted);
    branch_zero(emitter, owners, subtract);
    abi::emit_jump(emitter, skip);
}

/// Emits a register zero branch without implicit scratch registers or foreign calls.
pub fn branch_zero(emitter: &mut impl Emitter, register: &str, label: &str) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("cbz {register}, {label}")), // stop or skip when this borrowed register is empty
        Arch::X86_64 => {
            emitter.instruction(&format!("test {register}, {register}"));       // inspect the borrowed register without changing its value
            emitter.instruction(&format!("jz {label}"));                        // stop or skip when this borrowed register is empty
        }
    }
}

/// Compares two preserved registers and branches when they name the same owner.
fn branch_equal(emitter: &mut impl Emitter, left: &str, right: &str, label: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {left}, {right}"));               // compare exact managed identities
            emitter.instruction(&format!("b.eq {label}"));                      // only identical owners share this path
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {left}, {right}"));               // compare exact managed identities
            emitter.instruction(&format!("je {label}"));                        // only identical owners share this path
        }
    }
}

/// Compares two preserved registers and skips an unrelated root or owned child.
pub fn branch_not_equal(emitter: &mut impl Emitter, left: &str, right: &str, label: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {left}, {right}"));               // compare exact managed identities
            emitter.instruction(&format!("b.ne {label}"));                      // unrelated identities contribute no owner to this reference
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {left}, {right}"));               // compare exact managed identities
            emitter.instruction(&format!("jne {label}"));                       // unrelated identities contribute no owner to this reference
        }
    }
}

/// Returns the 32-bit alias used to read an allocation's four-byte owner count.
fn low_word(emitter: &impl Emitter, register: &str) -> String {
    if emitter.arch() == Arch::AArch64 {
        return format!("w{}", register.strip_prefix('x').expect("integer ARM register"));
    }
    match register {
        "rax" => "eax".into(), "rcx" => "ecx".into(), "rdx" => "edx".into(),
        "rsi" => "esi".into(), "rdi" => "edi".into(), other => format!("{other}d"),
    }
}

/// Emits C-ABI enter(record, value) and leave(record), permitting interleaved activations.
/// Enter returns the record. Leave returns the protected cleanup status and preserves exceptions.
pub fn emit(emitter: &mut impl Emitter) {
    let result = abi::int_result_reg(emitter);
    let (record, previous) = if emitter.arch() == Arch::AArch64 { ("x10", "x11") }
        else { ("r10", "r11") };
    emitter.blank();
    emitter.label_global("__rt_reference_validation_pin_enter");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 8);
    abi::emit_reg_move(emitter, result, abi::int_arg_reg_name(emitter, 1));
    abi::store_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_incref");
    // Static Mixed constants can be rooted too; authenticate before any later header inspection.
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_reference_validation_pin_enter_kind_ready");
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction("ldurb w0, [x0, #-8]"),            // remember the exact live allocation's physical kind
        Arch::X86_64 => emitter.instruction("movzx eax, BYTE PTR [rax - 8]"),   // remember the exact live allocation's physical kind
    }
    emitter.label("__rt_reference_validation_pin_enter_kind_ready");
    abi::store_at_offset(emitter, result, 24);
    abi::emit_symbol_address(emitter, previous, TOP);
    abi::emit_load_from_address(emitter, previous, previous, 0);
    abi::load_at_offset(emitter, record, ValidationPin::NEXT);
    abi::emit_store_zero_to_address(emitter, record, ValidationPin::PREVIOUS);
    abi::emit_store_to_address(emitter, previous, record, ValidationPin::NEXT);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_store_to_address(emitter, result, record, ValidationPin::OWNER);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_store_to_address(emitter, result, record, ValidationPin::KIND);
    match emitter.arch() {
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

    // A foreign caller can take the owner before applying its own Throwable accumulator.
    emitter.label_global("__rt_reference_validation_pin_take");
    abi::emit_frame_prologue(emitter, 32);
    abi::emit_reg_move(emitter, record, abi::int_arg_reg_name(emitter, 0));
    abi::emit_load_from_address(emitter, result, record, ValidationPin::OWNER);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, previous, record, ValidationPin::KIND);
    abi::emit_load_int_immediate(emitter, result, 0x305);
    branch_equal(emitter, previous, result, "__rt_reference_validation_pin_take_return");
    abi::emit_load_int_immediate(emitter, result, 8);
    branch_not_equal(emitter, previous, result, "__rt_reference_validation_pin_take_unlink");
    abi::store_at_offset(emitter, record, ValidationPin::OWNER);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 16);
    abi::emit_call_label(emitter, "__rt_reference_argument_pins_forget");
    abi::load_at_offset(emitter, record, ValidationPin::OWNER);
    emitter.label("__rt_reference_validation_pin_take_unlink");
    abi::emit_load_from_address(emitter, previous, record, ValidationPin::PREVIOUS);
    let next = if emitter.arch() == Arch::AArch64 { "x12" } else { "r9" };
    abi::emit_load_from_address(emitter, next, record, ValidationPin::NEXT);
    match emitter.arch() {
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
    match emitter.arch() {
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
    abi::emit_store_zero_to_address(emitter, record, ValidationPin::PREVIOUS);
    abi::emit_store_zero_to_address(emitter, record, ValidationPin::NEXT);
    abi::emit_store_zero_to_address(emitter, record, ValidationPin::OWNER);
    // A consumed staging record can be retired again by its original foreign boundary.
    abi::emit_load_int_immediate(emitter, result, 0x305);
    abi::emit_store_to_address(emitter, result, record, ValidationPin::KIND);
    emitter.label("__rt_reference_validation_pin_take_return");
    abi::load_at_offset(emitter, result, 8);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);

    emitter.label_global("__rt_reference_validation_pin_leave");
    abi::emit_frame_prologue(emitter, 32);
    abi::emit_call_label(emitter, "__rt_reference_validation_pin_take");
    crate::cleanup_preserve::emit_unary_cleanup_preserving_exception(emitter, "__rt_decref_any", result);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}
