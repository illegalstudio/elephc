//! Purpose:
//! Roots boxed values and managed native cells without changing PHP alias sharing.
//!
//! Called from:
//! - Reference validation publishers and reference-sharing readers.
//!
//! Key details:
//! - Each caller-owned record contains previous/next links, the pinned value and authenticated kind.
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
    let restore = format!("{prefix}_pin_restore");
    let direct = format!("{prefix}_pin_direct");
    let candidates: &[&str] = if emitter.target.arch == Arch::AArch64 {
        &["x12", "x13", "x14", "x15", "x16", "x17", "x9", "x10", "x11"]
    } else { &["r8", "r9", "rdx", "rsi", "rdi", "rax", "rcx", "r10", "r11"] };
    let extra = candidates.iter().copied().filter(|register| !registers.contains(register))
        .take(3).collect::<Vec<_>>();
    abi::emit_symbol_address(emitter, cursor, TOP);
    abi::emit_load_from_address(emitter, cursor, cursor, 0);
    branch_zero(emitter, cursor, &done);
    for register in &extra { abi::emit_push_reg(emitter, register); }
    emitter.label(&next);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz {cursor}, {restore}"));           // inactive validation leaves ordinary sharing unchanged
            emitter.instruction(&format!("ldr {scratch}, [{cursor}, #16]"));    // inspect one stack-rooted validation lease
            emitter.instruction(&format!("cmp {scratch}, {reference}"));        // exclude only leases of this exact reference
            emitter.instruction(&format!("b.eq {direct}"));                     // a direct lease contributes exactly one invisible owner
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {cursor}, {cursor}"));           // inactive validation leaves ordinary sharing unchanged
            emitter.instruction(&format!("jz {restore}"));                      // stop after the final active validation lease
            emitter.instruction(&format!("mov {scratch}, QWORD PTR [{cursor} + 16]"));// inspect one stack-rooted validation lease
            emitter.instruction(&format!("cmp {scratch}, {reference}"));        // exclude only leases of this exact reference
            emitter.instruction(&format!("je {direct}"));                       // a direct lease contributes exactly one invisible owner
        }
    }
    subtract_unowned_cell_child(emitter, reference, cursor, scratch,
        [extra[0], extra[1], extra[2]], prefix, &direct, &skip);
    emitter.label(&direct);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("sub {count}, {count}, #1")),// subtract one invisible owner from PHP sharing
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
    emitter: &mut Emitter, reference: &str, outer: &str, scratch: &str,
    extra: [&str; 3], prefix: &str, subtract: &str, skip: &str,
) {
    let [cell, owners, node] = extra;
    let earlier = format!("{prefix}_pin_cell_earlier");
    let count = format!("{prefix}_pin_cell_count");
    let count_next = format!("{prefix}_pin_cell_count_next");
    let counted = format!("{prefix}_pin_cell_counted");
    branch_zero(emitter, scratch, skip);
    abi::emit_load_from_address(emitter, owners, outer, RECORD_BYTES - 8);
    match emitter.target.arch {
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
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldur {owners}, [{cell}, #-8]"));      // inspect the cell's physical descriptor
            emitter.instruction(&format!("ubfx {owners}, {owners}, #8, #7"));   // isolate the owned payload shape
            emitter.instruction(&format!("cmp {owners}, #7"));                  // only Mixed descriptor cells own a wrapper pointer
            emitter.instruction(&format!("b.ne {skip}"));                       // raw scalar bits are never interpreted as child owners
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {owners}, QWORD PTR [{cell} - 8]"));// inspect the cell's physical descriptor
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
    abi::emit_load_from_address(emitter, scratch, node, 16);
    branch_equal(emitter, scratch, cell, skip);
    abi::emit_load_from_address(emitter, node, node, 8);
    abi::emit_jump(emitter, &earlier);
    emitter.label(&count);
    let word = low_word(emitter, owners);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("ldur {word}, [{cell}, #-12]")),// read only the four-byte owner count
        Arch::X86_64 => emitter.instruction(&format!("mov {word}, DWORD PTR [{cell} - 12]")),// read only the four-byte owner count
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldur {scratch}, [{cell}, #-8]"));     // inspect the collector's separate temporary owner
            emitter.instruction(&format!("ubfx {scratch}, {scratch}, #18, #1"));// isolate its destructor pin
            emitter.instruction(&format!("sub {owners}, {owners}, {scratch}")); // keep collector ownership out of PHP sharing
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {scratch}, QWORD PTR [{cell} - 8]"));// inspect the collector's separate temporary owner
            emitter.instruction(&format!("shr {scratch}, 18"));                 // position its destructor pin
            emitter.instruction(&format!("and {scratch}, 1"));                  // isolate that one artificial owner
            emitter.instruction(&format!("sub {owners}, {scratch}"));           // keep collector ownership out of PHP sharing
        }
    }
    emitter.label(&count_next);
    branch_zero(emitter, node, &counted);
    abi::emit_load_from_address(emitter, scratch, node, 16);
    let advance = format!("{prefix}_pin_cell_advance");
    branch_not_equal(emitter, scratch, cell, &advance);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("sub {owners}, {owners}, #1")),// exclude each artificial cell lease exactly once
        Arch::X86_64 => emitter.instruction(&format!("sub {owners}, 1")),       // exclude each artificial cell lease exactly once
    }
    emitter.label(&advance);
    abi::emit_load_from_address(emitter, node, node, 8);
    abi::emit_jump(emitter, &count_next);
    emitter.label(&counted);
    branch_zero(emitter, owners, subtract);
    abi::emit_jump(emitter, skip);
}

/// Emits a register zero branch without implicit scratch registers or foreign calls.
fn branch_zero(emitter: &mut Emitter, register: &str, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cbz {register}, {label}")),// stop or skip when this borrowed register is empty
        Arch::X86_64 => {
            emitter.instruction(&format!("test {register}, {register}"));       // inspect the borrowed register without changing its value
            emitter.instruction(&format!("jz {label}"));                        // stop or skip when this borrowed register is empty
        }
    }
}

/// Compares two preserved registers and branches when they name the same owner.
fn branch_equal(emitter: &mut Emitter, left: &str, right: &str, label: &str) {
    match emitter.target.arch {
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
fn branch_not_equal(emitter: &mut Emitter, left: &str, right: &str, label: &str) {
    match emitter.target.arch {
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
fn low_word(emitter: &Emitter, register: &str) -> String {
    if emitter.target.arch == Arch::AArch64 {
        return format!("w{}", register.strip_prefix('x').expect("integer ARM register"));
    }
    match register {
        "rax" => "eax".into(), "rcx" => "ecx".into(), "rdx" => "edx".into(),
        "rsi" => "esi".into(), "rdi" => "edi".into(), other => format!("{other}d"),
    }
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
    // Static Mixed constants can be rooted too; authenticate before any later header inspection.
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_reference_validation_pin_enter_kind_ready");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("ldurb w0, [x0, #-8]"),            // remember the exact live allocation's physical kind
        Arch::X86_64 => emitter.instruction("movzx eax, BYTE PTR [rax - 8]"),   // remember the exact live allocation's physical kind
    }
    emitter.label("__rt_reference_validation_pin_enter_kind_ready");
    abi::store_at_offset(emitter, result, 24);
    abi::emit_symbol_address(emitter, previous, TOP);
    abi::emit_load_from_address(emitter, previous, previous, 0);
    abi::load_at_offset(emitter, record, 8);
    abi::emit_store_zero_to_address(emitter, record, 0);
    abi::emit_store_to_address(emitter, previous, record, 8);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_store_to_address(emitter, result, record, 16);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_store_to_address(emitter, result, record, RECORD_BYTES - 8);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x11, __rt_reference_validation_pin_enter_head");// the first lease has no successor to relink
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
    abi::emit_reg_move(emitter, record, abi::int_arg_reg_name(emitter.target, 0));
    abi::emit_load_from_address(emitter, result, record, 16);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, previous, record, 0);
    let next = if emitter.target.arch == Arch::AArch64 { "x12" } else { "r9" };
    abi::emit_load_from_address(emitter, next, record, 8);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x11, __rt_reference_validation_pin_leave_head");// an inner lease can retire while another fiber remains suspended
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
            emitter.instruction("cbz x12, __rt_reference_validation_pin_leave_release");// the tail has no successor backlink
        }
        Arch::X86_64 => {
            emitter.instruction("test r9, r9");                                 // the tail has no successor backlink
            emitter.instruction("jz __rt_reference_validation_pin_leave_release");// finish unlinking before cleanup can reenter PHP
        }
    }
    abi::emit_store_to_address(emitter, previous, next, 0);
    emitter.label("__rt_reference_validation_pin_leave_release");
    abi::emit_store_zero_to_address(emitter, record, 0);
    abi::emit_store_zero_to_address(emitter, record, 8);
    abi::emit_store_zero_to_address(emitter, record, 16);
    abi::emit_store_zero_to_address(emitter, record, RECORD_BYTES - 8);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);

    emitter.label_global("__rt_reference_validation_pin_leave");
    abi::emit_frame_prologue(emitter, 32);
    abi::emit_call_label(emitter, "__rt_reference_validation_pin_take");
    abi::emit_unary_cleanup_preserving_exception(emitter, "__rt_decref_any", result);
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
            let (enter, rest) = assembly.split_once("__rt_reference_validation_pin_take:").unwrap();
            let (take, leave) = rest.split_once("__rt_reference_validation_pin_leave:").unwrap();
            assert!(enter.find("__rt_incref").unwrap() < enter.find(TOP).unwrap(), "{name}: retain before publishing the record");
            assert!(enter.find("__rt_incref").unwrap() < enter.find("__rt_gc_candidate_live").unwrap()
                && enter.find("__rt_gc_candidate_live").unwrap() < enter.find(TOP).unwrap(),
                "{name}: authenticate a retained allocation before publishing its kind");
            assert!(take.contains(TOP) && !take.contains("__rt_decref"),
                "{name}: taking a root unlinks it without running PHP");
            assert!(leave.find("__rt_reference_validation_pin_take").unwrap() < leave.find("__rt_decref_any").unwrap(),
                "{name}: remove the record before protected generic release");
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
