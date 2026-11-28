//! Purpose:
//! Contains destructor exceptions raised while Rust eval releases a native Mixed owner.
//!
//! Called from:
//! - The shared eval bridge emitter on every supported target.
//!
//! Key details:
//! - The versioned C ABI consumes one value and updates an owned, nullable Throwable accumulator.
//! - Cleanup installs its jump target below Rust and restores the enclosing native exception state.
//! - Its integer result distinguishes a new destructor throw from an earlier pending exception.
//! - Memoir emits contained retirement; versioned C ABI and hook selection remain here.

use super::{label_c_global, Emitter};

/// Emits bounded value release, preserving and chaining any exception already owned by the slot.
pub(super) fn emit(emitter: &mut Emitter) {
    emit_boundary(emitter, "__elephc_eval_value_release_v3", "__rt_eval_release", false);
}

/// Retires one foreign-owned pin through the same boxed Throwable accumulator as ordinary release.
pub(super) fn emit_reference_pin_release(emitter: &mut Emitter) {
    emit_boundary(emitter, "__elephc_eval_value_native_reference_pin_release_v1",
        "__rt_eval_reference_pin_release", true);
}

/// Selects the versioned C ABI and ownership hooks for Memoir's contained release body.
fn emit_boundary(emitter: &mut Emitter, entry: &str, prefix: &str, take_pin: bool) {
    label_c_global(emitter, entry);
    elephc_memoir::foreign_release::emit_body(emitter, elephc_memoir::foreign_release::Context {
        prefix,
        take_owner: take_pin.then_some("__rt_reference_validation_pin_take"),
        take_pending: "__rt_throwable_take_boxed",
        box_pending: "__rt_throwable_box_owned",
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target contains the native release before exporting one boxed exception owner to Rust.
    #[test]
    fn eval_value_release_boundary_returns_throwables_without_native_escape() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains(&target.extern_symbol("__elephc_eval_value_release_v3")), "{name}");
            assert!(asm.find("__rt_cleanup_invoke").unwrap() < asm.find("__rt_throwable_box_owned").unwrap(), "{name}");
            assert!(asm.find("__rt_throwable_take_boxed").unwrap() < asm.find("__rt_cleanup_invoke").unwrap(), "{name}");
            // Materializing one address mentions the symbol twice on AArch64 (page and offset).
            // Count calls instead, proving release cannot bypass the containing native boundary.
            let call = if target.arch == super::super::Arch::AArch64 { "bl" } else { "call" };
            assert!(asm.contains("__rt_decref_any"), "{name}");
            assert_eq!(asm.lines().filter(|line| line.trim() == format!("{call} __rt_cleanup_invoke")).count(), 1, "{name}");
            assert!(!asm.lines().any(|line| line.trim() == format!("{call} __rt_decref_any")), "{name}");
            assert!(!asm.contains("__rt_throw_current"), "{name}");
            let caught_flag = match target.arch {
                super::super::Arch::AArch64 => "ldur x0, [x29, #-32]",
                super::super::Arch::X86_64 => "mov rax, QWORD PTR [rbp - 32]",
            };
            assert!(asm.contains(caught_flag), "{name}: preserve the new-throw status across boxing");
        }
    }

    /// Foreign staging roots unlink before contained child destruction and return a boxed Throwable.
    #[test]
    fn native_staging_pin_release_contains_final_owner_destruction_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_reference_pin_release(&mut emitter);
            let assembly = emitter.output();
            let take = assembly.find("__rt_reference_validation_pin_take").unwrap();
            let invoke = assembly.find("__rt_cleanup_invoke").unwrap();
            let box_throwable = assembly.find("__rt_throwable_box_owned").unwrap();
            assert!(take < invoke && invoke < box_throwable, "{name}: unlink before protected retirement");
            assert!(!assembly.contains("__rt_reference_validation_pin_leave"),
                "{name}: nested cleanup cannot hide a newly caught Throwable from Rust");
            assert!(!assembly.contains("__rt_throw_current"), "{name}: never unwind across Rust");
        }
    }

}
