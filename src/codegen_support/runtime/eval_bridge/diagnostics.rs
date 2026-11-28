//! Purpose:
//! Contains PHP warning handlers beneath the Rust eval runtime adapter.
//!
//! Called from:
//! - Shared eval value-runtime emission on every supported target.
//!
//! Key details:
//! - Version two borrows a byte slice and returns a native success or pending-Throwable status.
//! - Existing raw diagnostic symbols remain available for native callers.

use super::{abi, Arch, Emitter};
use crate::codegen_support::runtime::exceptions::emit_protected;
use crate::codegen_support::try_handlers::TRY_HANDLER_SLOT_SIZE;

/// Emits a warning entry whose PHP callbacks cannot unwind through its Rust caller.
pub(super) fn emit(emitter: &mut Emitter) {
    let symbol = emitter.target.extern_symbol("__elephc_eval_warning_v2");
    emit_protected(emitter, &symbol, body);
}

/// Reloads the borrowed message only after installing the complete exception-handler record.
fn body(emitter: &mut Emitter) {
    let (pointer, length) = if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rdi", "rsi") };
    abi::emit_load_temporary_stack_slot(emitter, pointer, TRY_HANDLER_SLOT_SIZE);
    abi::emit_load_temporary_stack_slot(emitter, length, TRY_HANDLER_SLOT_SIZE + 8);
    abi::emit_call_label(emitter, "__rt_diag_warning");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target installs its jump boundary before warning dispatch and returns native exceptions.
    #[test]
    fn strict_eval_diagnostics_warning_boundary_covers_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains(&target.extern_symbol("__elephc_eval_warning_v2")), "{name}");
            assert!(asm.find(&target.extern_symbol("_setjmp")).unwrap() < asm.find("__rt_diag_warning").unwrap(), "{name}");
            assert!(asm.contains("_exc_handler_top") && asm.contains("_exc_call_frame_top"), "{name}");
            assert!(asm.contains("_rt_diag_suppression") && asm.contains("_gc_release_suppressed"), "{name}");
            assert!(!asm.contains("__rt_throw_current"), "{name}: PHP exceptions must return to Rust");
        }
    }
}
