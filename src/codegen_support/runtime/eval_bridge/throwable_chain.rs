//! Purpose:
//! Chains borrowed eval Throwable objects without exposing a native unwind to Rust.
//!
//! Called from:
//! - Shared eval runtime bridge emission on every supported target.
//!
//! Key details:
//! - The existing runtime preserves previous links, object identity, and cycle safety.
//! - One temporary raw predecessor owner transfers to that runtime; both boxed inputs stay borrowed.

use super::{abi, Arch, Emitter};
use crate::codegen_support::runtime::exceptions::emit_protected;
use crate::codegen_support::try_handlers::TRY_HANDLER_SLOT_SIZE;

/// Emits a versioned C entry borrowing two boxes and returning a protected status.
pub(super) fn emit(emitter: &mut Emitter) {
    let symbol = emitter.target.extern_symbol("__elephc_eval_throwable_chain_v1");
    emit_protected(emitter, &symbol, body);
}

/// Borrows raw identities, acquiring only the predecessor owner consumed by the shared helper.
fn body(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let payload = if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" };
    abi::emit_load_temporary_stack_slot(emitter, result, TRY_HANDLER_SLOT_SIZE + 8);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    abi::emit_reg_move(emitter, result, payload);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_store_to_sp(emitter, result, TRY_HANDLER_SLOT_SIZE + 8);
    abi::emit_load_temporary_stack_slot(emitter, result, TRY_HANDLER_SLOT_SIZE);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    abi::emit_reg_move(emitter, result, payload);
    abi::emit_load_temporary_stack_slot(emitter, payload, TRY_HANDLER_SLOT_SIZE + 8);
    abi::emit_call_label(emitter, "__rt_exception_chain");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All targets install protection before retaining and transferring the predecessor owner.
    #[test]
    fn eval_throwable_chain_borrows_boxes_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let asm = emitter.output();
            let protected = asm.find(&target.extern_symbol("_setjmp")).unwrap();
            let retained = asm.find("__rt_incref").unwrap();
            let chained = asm.find("__rt_exception_chain").unwrap();
            assert!(protected < retained && retained < chained, "{name}: {asm}");
            assert_eq!(asm.matches("__rt_mixed_unbox").count(), 2, "{name}: {asm}");
            assert!(!asm.contains("__rt_throw_current"), "{name}: {asm}");
        }
    }
}
