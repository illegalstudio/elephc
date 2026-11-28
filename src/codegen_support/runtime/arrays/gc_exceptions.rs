//! Purpose:
//! Preserves compiler integration tests and compatibility imports for Memoir emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

pub use elephc_memoir::gc_exceptions::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter};


#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All targets install the boundary before user code and publish the pending owner before chaining.
    #[test]
    fn collector_destructor_boundaries_preserve_handler_and_pending_owners_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_gc_exception_helpers(&mut emitter);
            let asm = emitter.output();
            assert!(asm.find(&target.extern_symbol("_setjmp")).unwrap() < asm.find("__rt_call_object_destructor").unwrap(), "{name}");
            let catch = asm.split("__rt_gc_destructor_caught:\n").nth(1).unwrap();
            assert!(catch.find("_gc_pending_throw").unwrap() < catch.find("__rt_exception_chain").unwrap(), "{name}");
            assert!(asm.contains("_rt_diag_suppression") && asm.contains("_exc_call_frame_top"), "{name}");
            assert!(asm.contains("__rt_gc_rethrow_pending:") && asm.contains("__rt_throw_current"), "{name}");
        }
    }
}
