//! Purpose:
//! Boxes a persistent native callable descriptor for lazy eval Reflection metadata.
//!
//! Called from:
//! - Magician ReflectionParameter lazy factory construction through RuntimeValueOps.
//!
//! Key details:
//! - Native default factories carry no captures or eval-context pointers.
//! - Borrowed descriptor boxing follows the ordinary Callable ownership contract.

use super::*;

/// Emits the C ABI that boxes a borrowed persistent native factory as an owned Mixed callable.
pub(super) fn emit(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_callable_from_raw");
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_reg_move(emitter, abi::int_result_reg(emitter),
        abi::int_arg_reg_name(emitter.target, 0));
    let done = "__elephc_eval_value_callable_from_raw_done";
    abi::emit_branch_if_int_result_zero(emitter, done);
    crate::codegen_support::value_boxing::emit_box_current_value_as_mixed(
        emitter, &crate::types::PhpType::Callable,
    );
    emitter.label(done);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All five targets box the descriptor without depending on an eval-context callback.
    #[test]
    fn eval_native_callable_factory_emits_for_all_supported_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen_support::platform::Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.contains(&target.extern_symbol("__elephc_eval_value_callable_from_raw")), "{name}");
            assert!(assembly.contains("__rt_mixed_from_value"), "{name}");
            assert!(!assembly.contains("_elephc_eval_wrap_callback_fn"), "{name}");
            assert!(!assembly.contains("__rt_callable_descriptor_release"), "{name}: boxing borrows the persistent descriptor");
        }
    }
}
