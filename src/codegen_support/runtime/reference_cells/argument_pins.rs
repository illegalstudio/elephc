//! Purpose:
//! Re-exports reference-root memory operations from Memoir.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

pub use elephc_memoir::reference_cells::argument_pins::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter};

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
