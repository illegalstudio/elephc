//! Purpose:
//! Contains scalar bitwise-NOT exceptions beneath Magician's Rust interpreter frames.
//!
//! Called from:
//! - The shared eval value bridge emitter on every supported target.
//!
//! Key details:
//! - The value is borrowed and a successful result transfers one Mixed owner through an out slot.
//! - The shared boundary returns status two for TypeError or a throwing diagnostic handler.

use super::{abi, Emitter};
use crate::codegen_support::runtime::exceptions::emit_protected;
use crate::codegen_support::try_handlers::TRY_HANDLER_SLOT_SIZE;

/// Emits a versioned two-argument C entry that never unwinds through its Rust caller.
pub(super) fn emit(emitter: &mut Emitter) {
    let symbol = emitter.target.extern_symbol("__elephc_eval_value_bit_not_v1");
    emit_protected(emitter, &symbol, body);
}

/// Publishes the fresh result only after the throwing scalar helper has returned normally.
fn body(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let output = abi::symbol_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, result, TRY_HANDLER_SLOT_SIZE);
    abi::emit_call_label(emitter, "__rt_mixed_bit_not");
    abi::emit_load_temporary_stack_slot(emitter, output, TRY_HANDLER_SLOT_SIZE + 8);
    abi::emit_store_to_address(emitter, result, output, 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target catches native throws before returning control and result ownership to Rust.
    #[test]
    fn bit_not_callback_contains_native_throwables_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let asm = emitter.output();
            let entry = target.extern_symbol("__elephc_eval_value_bit_not_v1");
            let handler = asm.find(&target.extern_symbol("_setjmp")).unwrap();
            let operation = asm.find("__rt_mixed_bit_not").unwrap();
            let caught = asm.find(&format!("{entry}_throw:\n")).unwrap();
            assert!(handler < operation && operation < caught, "{name}: {asm}");
            assert!(asm.contains("_exc_handler_top") && asm.contains("_exc_call_frame_top"), "{name}");
            assert!(asm.contains("_rt_diag_suppression") && asm.contains("_gc_release_suppressed"), "{name}");
            assert!(!asm.contains("__rt_throw_current"), "{name}: a throw must return pending status");
        }
    }
}
