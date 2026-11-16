//! Purpose:
//! Re-exports reference-root memory operations from Memoir.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

pub use elephc_memoir::reference_cells::validation_pins::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch};

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
            let consumed = if name == "linux-x86_64" { "cmp r11, rax" }
                else { "cmp x11, x0" };
            assert!(take.find(consumed).unwrap() < take.find(TOP).unwrap(),
                "{name}: an already consumed staging record cannot change the root chain");
            assert!(take.contains("__rt_reference_validation_pin_take_return"),
                "{name}: repeated cleanup observes an empty owner");
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
