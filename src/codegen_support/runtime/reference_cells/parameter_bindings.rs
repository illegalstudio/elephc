//! Purpose:
//! Transfers a pending native reference argument binding into its formal parameter owner.
//!
//! Called from:
//! - Native by-reference parameter entry after ABI arguments have reached their local slots.
//!
//! Key details:
//! - An activated argument record identifies the native callee's incoming stack pointer.
//! - Claiming clears the caller owner before transferring it, then consumes eval staging leases.
//! - Ordinary variables and legacy callers still acquire an independent parameter owner.

pub(super) use elephc_memoir::reference_cells::parameter_bindings::emit;

#[cfg(test)]
use super::*;

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
            let sources = assembly.find("__rt_reference_argument_pins_activate").unwrap();
            let release = assembly.find("__rt_decref_any").unwrap();
            let staging = assembly.find("__rt_reference_validation_pin_take").unwrap();
            assert!(retain < sources && sources < release && release < staging,
                "{target}: own the parameter before consuming source and staging leases");
            assert_eq!(assembly.matches("__rt_decref_any").count(), 2,
                "{target}: retire source owners and the staging owner while the parameter keeps both alive");
            assert!(!assembly.contains("__rt_throw"), "{target}: formal acquisition cannot invoke PHP");
        }
    }
}
