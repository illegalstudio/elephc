//! Purpose:
//! Exposes compiler imports and tests for Memoir storage assembly.
//!
//! Called from:
//! - Compiler runtime orchestration and focused memory integration tests.
//!
//! Key details:
//! - Preserves the compiler namespace while Memoir owns the assembly implementation.

pub use elephc_memoir::callables::descriptor_release::*;

#[cfg(test)]
use crate::codegen_support::emit::Emitter;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Reference captures release only validated cell owners through guarded cleanup on every ABI.
    #[test]
    fn descriptor_reference_capture_retirement_is_guarded_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_callable_descriptor_release(&mut emitter);
            let asm = emitter.output();
            let branch = asm.split_once("__rt_callable_descriptor_release_reference:\n").unwrap().1;
            let branch = branch.split("__rt_callable_descriptor_release_next").next().unwrap();
            let lookup = branch.find("__rt_reference_cell_owner").unwrap();
            let release = branch.find("__rt_decref_any").unwrap();
            let guard = branch.find("__rt_cleanup_invoke").unwrap();
            assert!(lookup < release && release < guard, "{target}: {branch}");
        }
    }
}
